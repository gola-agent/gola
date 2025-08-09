//! Agent orchestration and lifecycle management.
//!
//! Provides the core `Agent` implementation responsible for conversation flow,
//! tool execution coordination, memory management, and loop detection. Agents
//! integrate language models with external tools while maintaining conversation
//! context and enforcing authorization policies.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::config::types::{MemoryConfig, MemoryEvictionStrategy};
use crate::core_types::{HistoryStep, Message, Observation, Role};
use crate::errors::AgentError;
use crate::executors::CodeExecutor;
use crate::guardrails::{
    AuthorizationContext, AuthorizationHandler, AuthorizationMode, AuthorizationRequest,
    AuthorizationResponse,
};
use crate::llm::{ToolMetadata, LLM};
use crate::memory::{
    AgentMemory, ConversationMemory, ConversationSummaryBufferMemory, MemoryStats, ConversationSummaryMemory
};
use crate::rag::{Rag, RagConfig, RetrievedContext};
use crate::memory::SlidingWindowMemory;
use crate::tools::{Tool, ControlPlaneServer};
use crate::loop_detection::{PatternDetector, LoopDetectionConfig, LoopPattern};
use crate::trace::{AgentTraceHandler, AgentStep, AgentExecution};
use async_trait::async_trait;

#[async_trait]
pub trait GolaAgent: Send + Sync {
    async fn run(&mut self, initial_task: String) -> Result<String, AgentError>;
    fn clear_memory(&mut self);
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub max_steps: usize,
    pub enable_rag: bool,
    pub rag_config: Option<RagConfig>,
    pub system_prompt: Option<String>,
    pub memory_config: Option<MemoryConfig>,
    pub authorization_mode: AuthorizationMode,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            enable_rag: false,
            rag_config: None,
            system_prompt: None,
            memory_config: None,
            authorization_mode: AuthorizationMode::Allow,
        }
    }
}

pub struct Agent {
    llm: Arc<dyn LLM>,
    tools: HashMap<String, Arc<dyn Tool>>,
    code_executor: Option<Arc<dyn CodeExecutor>>,
    memory: Box<dyn ConversationMemory>,
    history: AgentMemory,
    config: AgentConfig,
    rag_system: Option<Box<dyn Rag>>,
    authorization_handler: Option<Arc<dyn AuthorizationHandler>>,
    all_tools_approved: bool,
    consecutive_tool_failures: HashMap<String, u32>,
    last_tool_error: Option<String>,
    consecutive_error_count: u32,
    trace_handler: Option<Box<dyn AgentTraceHandler>>,
    trace_handles: Vec<tokio::task::JoinHandle<()>>,
    control_plane: ControlPlaneServer,
    loop_detector: PatternDetector,
}

#[async_trait]
impl GolaAgent for Agent {
    async fn run(&mut self, initial_task: String) -> Result<String, AgentError> {
        Agent::run(self, initial_task).await
    }

    fn clear_memory(&mut self) {
        Agent::clear_memory(self);
    }
}

impl Agent {
    pub fn new(
        llm: Arc<dyn LLM>,
        tools: HashMap<String, Arc<dyn Tool>>,
        code_executor: Option<Arc<dyn CodeExecutor>>,
        config: AgentConfig,
    ) -> Self {
        let memory: Box<dyn ConversationMemory> =
            if let Some(memory_config) = &config.memory_config {
                match memory_config.eviction_strategy {
                    MemoryEvictionStrategy::Summarize => Box::new(
                        ConversationSummaryBufferMemory::new(llm.clone(), memory_config.max_history_steps),
                    ),
                    MemoryEvictionStrategy::ConversationSummary => Box::new(
                        ConversationSummaryMemory::new(llm.clone()),
                    ),
                    _ => Box::new(SlidingWindowMemory::new(memory_config.max_history_steps)),
                }
            } else {
                Box::new(SlidingWindowMemory::new(10))
            };

        Agent {
            llm,
            tools,
            code_executor,
            memory,
            history: AgentMemory::new(),
            config,
            rag_system: None,
            authorization_handler: None,
            all_tools_approved: false,
            consecutive_tool_failures: HashMap::new(),
            last_tool_error: None,
            consecutive_error_count: 0,
            trace_handler: None,
            trace_handles: Vec::new(),
            control_plane: ControlPlaneServer::new(),
            loop_detector: PatternDetector::new(LoopDetectionConfig::default()),
        }
    }

    pub fn with_authorization(
        llm: Arc<dyn LLM>,
        tools: HashMap<String, Arc<dyn Tool>>,
        code_executor: Option<Arc<dyn CodeExecutor>>,
        config: AgentConfig,
        authorization_handler: Arc<dyn AuthorizationHandler>,
    ) -> Self {
        let mut agent = Self::new(llm, tools, code_executor, config);
        agent.authorization_handler = Some(authorization_handler);
        agent
    }

    pub fn with_rag(
        llm: Arc<dyn LLM>,
        tools: HashMap<String, Arc<dyn Tool>>,
        code_executor: Option<Arc<dyn CodeExecutor>>,
        mut config: AgentConfig,
        rag_system: Box<dyn Rag>,
    ) -> Self {
        config.enable_rag = true;
        let mut agent = Self::new(llm, tools, code_executor, config);
        agent.rag_system = Some(rag_system);
        agent
    }

    pub fn with_rag_and_authorization(
        llm: Arc<dyn LLM>,
        tools: HashMap<String, Arc<dyn Tool>>,
        code_executor: Option<Arc<dyn CodeExecutor>>,
        mut config: AgentConfig,
        rag_system: Box<dyn Rag>,
        authorization_handler: Arc<dyn AuthorizationHandler>,
    ) -> Self {
        config.enable_rag = true;
        let mut agent = Self::new(llm, tools, code_executor, config);
        agent.rag_system = Some(rag_system);
        agent.authorization_handler = Some(authorization_handler);
        agent
    }

    pub fn set_authorization_handler(&mut self, handler: Arc<dyn AuthorizationHandler>) {
        self.authorization_handler = Some(handler);
    }

    pub fn set_trace_handler(&mut self, handler: Box<dyn AgentTraceHandler>) {
        self.trace_handler = Some(handler);
    }

    /// Remove authorization handler
    pub fn remove_authorization_handler(&mut self) {
        self.authorization_handler = None;
    }

    /// Check if authorization is enabled
    pub fn is_authorization_enabled(&self) -> bool {
        matches!(self.config.authorization_mode, AuthorizationMode::Ask)
            && self.authorization_handler.is_some()
    }

    /// Reset authorization state (clear "all approved" flag)
    pub fn reset_authorization_state(&mut self) {
        self.all_tools_approved = false;
    }

    /// Check if tool execution is authorized
    async fn check_tool_authorization(
        &mut self,
        tool_name: &str,
        tool_description: &str,
        tool_arguments: &Value,
        tool_call_id: Option<String>,
        step_number: usize,
    ) -> Result<bool, AgentError> {
        match self.config.authorization_mode {
            AuthorizationMode::Allow => Ok(true),
            AuthorizationMode::Deny => {
                log::info!("Tool execution denied by authorization mode: {}", tool_name);
                Err(AgentError::AuthorizationDenied(format!(
                    "Tool execution denied by authorization mode: {}",
                    tool_name
                )))
            }
            AuthorizationMode::Ask => {
                if self.all_tools_approved {
                    log::info!("Tool execution auto-approved (all mode): {}", tool_name);
                    return Ok(true);
                }

                if let Some(handler) = &self.authorization_handler {
                    let context = AuthorizationContext {
                        tool_name: tool_name.to_string(),
                        tool_description: tool_description.to_string(),
                        tool_arguments: tool_arguments.clone(),
                        tool_call_id,
                    };

                    let request = AuthorizationRequest {
                        context,
                        step_number,
                        max_steps: self.config.max_steps,
                    };

                    log::info!("Requesting authorization for tool: {}", tool_name);

                    match handler.request_authorization(request).await {
                        Ok(AuthorizationResponse::Yes) => {
                            log::info!("Tool execution authorized: {}", tool_name);
                            self.history.add_step(HistoryStep::Thought(format!(
                                "User authorized execution of tool: {}",
                                tool_name
                            )));
                            Ok(true)
                        }
                        Ok(AuthorizationResponse::No) => {
                            log::info!("Tool execution denied by user: {}", tool_name);
                            self.history.add_step(HistoryStep::Thought(format!(
                                "User denied execution of tool: {}",
                                tool_name
                            )));
                            Ok(false)
                        }
                        Ok(AuthorizationResponse::All) => {
                            log::info!("Tool execution authorized (all mode): {}", tool_name);
                            self.all_tools_approved = true;
                            self.history.add_step(HistoryStep::Thought(format!(
                                "User authorized execution of tool '{}' and all future tools",
                                tool_name
                            )));
                            Ok(true)
                        }
                        Err(e) => {
                            log::error!(
                                "Authorization request failed for tool {}: {}",
                                tool_name,
                                e
                            );
                            Err(AgentError::AuthorizationFailed(format!(
                                "Authorization request failed for tool '{}': {}",
                                tool_name, e
                            )))
                        }
                    }
                } else {
                    log::warn!(
                        "Authorization mode is Ask but no handler is configured, denying tool: {}",
                        tool_name
                    );
                    Err(AgentError::AuthorizationFailed(
                        "Authorization mode is Ask but no authorization handler is configured"
                            .to_string(),
                    ))
                }
            }
        }
    }

    pub fn enable_rag(&mut self, rag_system: Box<dyn Rag>) {
        self.config.enable_rag = true;
        self.rag_system = Some(rag_system);
    }

    pub fn disable_rag(&mut self) {
        self.config.enable_rag = false;
        self.rag_system = None;
    }

    pub fn is_rag_enabled(&self) -> bool {
        self.config.enable_rag && self.rag_system.is_some()
    }

    pub fn rag_system(&self) -> Option<&dyn Rag> {
        self.rag_system.as_ref().map(|r| r.as_ref())
    }

    async fn retrieve_rag_context(
        &self,
        query: &str,
    ) -> Result<Option<RetrievedContext>, AgentError> {
        if !self.is_rag_enabled() {
            return Ok(None);
        }

        if let Some(rag) = &self.rag_system {
            log::info!("Retrieving RAG context for query: {}", query);
            let context = rag.retrieve(query, None).await?;

            if context.is_empty() {
                log::info!("No relevant context found in RAG system");
                Ok(None)
            } else {
                log::info!("Retrieved {} relevant documents from RAG", context.len());
                Ok(Some(context))
            }
        } else {
            Ok(None)
        }
    }

    async fn format_task_with_rag_context(&self, task: &str) -> Result<String, AgentError> {
        if let Some(context) = self.retrieve_rag_context(task).await? {
            let formatted_context = context.format_for_llm();
            Ok(format!(
                "Task: {}\n\n{}\n\nPlease use the above context to help answer the task if relevant.",
                task, formatted_context
            ))
        } else {
            Ok(task.to_string())
        }
    }

    pub async fn run(&mut self, initial_task: String) -> Result<String, AgentError> {
        log::info!("Agent run started with task: {}", initial_task);

        self.add_user_task_to_memory(&initial_task).await?;

        let mut steps = vec![];
        for step_num in 0..self.config.max_steps {
            log::info!("Agent Step #{}", step_num + 1);

            match self.run_step(step_num).await {
                Ok((Some(final_answer), step)) => {
                    steps.push(step);
                    if let Some(handler) = &mut self.trace_handler {
                        let execution = AgentExecution {
                            steps,
                            final_result: Some(final_answer.clone()),
                            error: None,
                        };
                        handler.on_execution_complete(&execution);
                    }
                    return Ok(final_answer);
                }
                Ok((None, step)) => {
                    steps.push(step);
                    continue; // No final answer yet, continue loop
                }
                Err(AgentError::LoopDetection(loop_msg)) => {
                    log::error!("Loop detection triggered, propagating error to UI handler: {}", loop_msg);
                    
                    // Propagate LoopDetection error to UI handler for graceful recovery
                    // Don't attempt recovery here - let the UI handler manage it
                    if let Some(handler) = &mut self.trace_handler {
                        let execution = AgentExecution {
                            steps,
                            final_result: None,
                            error: Some(loop_msg.clone()),
                        };
                        handler.on_execution_complete(&execution);
                    }
                    return Err(AgentError::LoopDetection(loop_msg));
                }
                Err(e) => {
                    let err_msg = format!("Agent step failed: {}", e);
                    log::error!("{}", err_msg);
                    if let Some(handler) = &mut self.trace_handler {
                        let execution = AgentExecution {
                            steps,
                            final_result: None,
                            error: Some(err_msg.clone()),
                        };
                        handler.on_execution_complete(&execution);
                    }
                    return Err(AgentError::InternalError(err_msg));
                }
            }
        }

        log::warn!(
            "Agent reached max_steps ({}) without a final answer.",
            self.config.max_steps
        );
        if let Some(handler) = &mut self.trace_handler {
            let execution = AgentExecution {
                steps,
                final_result: None,
                error: Some("Max steps reached".to_string()),
            };
            handler.on_execution_complete(&execution);
        }
        for handle in self.trace_handles.drain(..) {
            handle.await.unwrap();
        }
        Err(AgentError::MaxStepsReached)
    }

    /// Executes a single step of the agent's reasoning loop.
    /// This is suitable for turn-by-turn conversational interactions.
    /// It returns `Ok((Some(final_answer), step))` if the agent provides a final answer,
    pub async fn run_step(&mut self, step_number: usize) -> Result<(Option<String>, AgentStep), AgentError> {
        let conversation_messages = self.memory.get_context();
        let mut messages_for_llm = Vec::new();

        // Prepend system prompt if it exists
        if let Some(system_prompt) = &self.config.system_prompt {
            if !system_prompt.is_empty() {
                messages_for_llm.push(Message {
                    role: Role::System,
                    content: system_prompt.clone(),
                    tool_call_id: None,
                    tool_calls: None,
                });
            }
        }

        messages_for_llm.extend(conversation_messages);

        let mut tool_metadata: Vec<ToolMetadata> =
            self.tools.values().map(|t| t.metadata()).collect();
        
        // Add control plane tools to the available tools
        // Add all control plane tools to the available tools
        let control_tools = self.control_plane.list_tools();
        log::info!("Control plane tools available: {:?}", control_tools);
        for tool_name in control_tools {
            if let Some(control_tool) = self.control_plane.get_tool(&tool_name) {
                tool_metadata.push(control_tool.metadata());
                log::info!("Added control plane tool to LLM: {}", tool_name);
            }
        }
        log::info!("Total tools available to LLM: {}", tool_metadata.len());

        log::info!("Generating LLM response");
        let llm_response = match self
            .llm
            .generate(messages_for_llm, Some(tool_metadata))
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let err_msg = format!("LLM generation failed: {}", e);
                log::error!("{}", err_msg);
                self.history.add_step(HistoryStep::LLMError(err_msg.clone()));
                return Err(AgentError::LLMError(err_msg));
            }
        };
        log::info!("LLM response generated");

        let mut thought = None;
        if let Some(content) = &llm_response.content {
            log::info!("Adding assistant message to memory");
            self.memory
                .add_message(Message {
                    role: Role::Assistant,
                    content: content.clone(),
                    tool_call_id: None,
                    tool_calls: None,
                })
                .await?;
            log::info!("Assistant message added to memory");
            thought = Some(content.clone());
        }

        if let Some(t) = &thought {
            if !t.trim().is_empty() {
                log::info!("Thought: {}", t);
                self.history.add_step(HistoryStep::Thought(t.clone()));

                if llm_response.tool_calls.is_none() {
                     if t.contains("Final Answer:") {
                        log::info!("Final Answer (assumed): {}", t);
                        let final_answer = t
                            .split("Final Answer:")
                            .last()
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        let step = AgentStep {
                            step_number,
                            thought: thought.clone(),
                            tool_calls: llm_response.tool_calls.clone(),
                            tool_results: None,
                        };
                        if let Some(handler) = &mut self.trace_handler {
                            handler.on_step_complete(&step);
                        }
                        return Ok((Some(final_answer), step));
                    }
                    // If there are no tool calls and no "Final Answer",
                    // we assume it's a conversational response and yield control.
                    let step = AgentStep {
                        step_number,
                        thought: thought.clone(),
                        tool_calls: llm_response.tool_calls.clone(),
                        tool_results: None,
                    };
                    if let Some(handler) = &mut self.trace_handler {
                        handler.on_step_complete(&step);
                    }
                    return Ok((Some(t.clone()), step));
                }
            }
        }

        let mut tool_results = vec![];
        if let Some(tool_calls) = &llm_response.tool_calls {
            if tool_calls.is_empty() && llm_response.content.is_none() {
                let no_action_msg =
                    "LLM did not provide content or tool calls. Ending run.".to_string();
                log::warn!("{}", no_action_msg);
                self.history
                    .add_step(HistoryStep::LLMError(no_action_msg.clone()));
                return Err(AgentError::LLMError(no_action_msg));
            }

            for tool_call in tool_calls {
                self.history
                    .add_step(HistoryStep::Action(tool_call.clone()));
                log::info!(
                    "Action: Calling tool '{}' with args {:?}",
                    tool_call.name,
                    tool_call.arguments
                );

                // Refactored tool execution logic into a separate function
                let observation = self.execute_tool(tool_call.clone(), step_number).await?;
                tool_results.push(observation.clone());

                // Check if this was a control plane signal
                if ControlPlaneServer::is_control_tool(&tool_call.name) && observation.success {
                    if tool_call.name == "assistant_done" {
                        log::info!("Control plane completion detected, parsing response");
                        
                        // Parse the completion response to get the summary
                        if let Ok(response) = serde_json::from_str::<serde_json::Value>(&observation.content) {
                            if let Some(summary) = response.get("summary").and_then(|s| s.as_str()) {
                                log::info!("Agent execution completed with summary: {}", summary);
                                
                                let step = AgentStep {
                                    step_number,
                                    thought: thought.clone(),
                                    tool_calls: llm_response.tool_calls.clone(),
                                    tool_results: Some(tool_results),
                                };
                                if let Some(handler) = &mut self.trace_handler {
                                    if let Some(handle) = handler.on_step_complete(&step) {
                                        self.trace_handles.push(handle);
                                    }
                                }
                                
                                // Return the summary as the final answer
                                return Ok((Some(summary.to_string()), step));
                            }
                        }
                        
                        // Fallback: use the observation content as final answer
                        log::info!("Agent execution completed via control plane");
                        let step = AgentStep {
                            step_number,
                            thought: thought.clone(),
                            tool_calls: llm_response.tool_calls.clone(),
                            tool_results: Some(tool_results),
                        };
                        if let Some(handler) = &mut self.trace_handler {
                            if let Some(handle) = handler.on_step_complete(&step) {
                                self.trace_handles.push(handle);
                            }
                        }
                        return Ok((Some("Agent completed task via control plane".to_string()), step));
                    } else if tool_call.name == "report_progress" {
                        // Parse the report_progress response to log the reason
                        let mut should_stop = false;
                        if let Ok(response) = serde_json::from_str::<serde_json::Value>(&observation.content) {
                            if let Some(reason) = response.get("reason").and_then(|r| r.as_str()) {
                                log::info!("Control plane report_progress detected with reason: {}", reason);
                                
                                // Stop execution for reasons that require user input
                                match reason {
                                    "awaiting_input" | "pending_choice" | "need_clarification" => {
                                        should_stop = true;
                                        log::info!("Stopping execution - waiting for user input");
                                    }
                                    _ => {
                                        log::info!("Continuing execution - no user input needed");
                                    }
                                }
                            }
                        }
                        
                        if should_stop {
                            let step = AgentStep {
                                step_number,
                                thought: thought.clone(),
                                tool_calls: llm_response.tool_calls.clone(),
                                tool_results: Some(tool_results),
                            };
                            if let Some(handler) = &mut self.trace_handler {
                                if let Some(handle) = handler.on_step_complete(&step) {
                                    self.trace_handles.push(handle);
                                }
                            }
                            return Ok((Some("Waiting for user input".to_string()), step));
                        }
                    }
                }
            }
        }

        let step = AgentStep {
            step_number,
            thought: thought.clone(),
            tool_calls: llm_response.tool_calls.clone(),
            tool_results: Some(tool_results),
        };
        if let Some(handler) = &mut self.trace_handler {
            if let Some(handle) = handler.on_step_complete(&step) {
                self.trace_handles.push(handle);
            }
        }

        // No final answer yet in this step
        Ok((None, step))
    }

    pub async fn add_user_task_to_memory(&mut self, task: &str) -> Result<(), AgentError> {
        log::info!("Formatting task with RAG context");
        let enhanced_task = self
            .format_task_with_rag_context(task)
            .await?;
        log::info!("Task formatted");

        self.history
            .add_step(HistoryStep::UserTask(enhanced_task.clone()));
        log::info!("Adding user message to memory");
        self.memory
            .add_message(Message {
                role: Role::User,
                content: enhanced_task,
                tool_call_id: None,
                tool_calls: None,
            })
            .await?;
        log::info!("User message added to memory");
        Ok(())
    }

    async fn execute_tool(&mut self, tool_call: crate::core_types::ToolCall, step_num: usize) -> Result<Observation, AgentError> {
        let loop_pattern = self.loop_detector.add_tool_call(
            tool_call.name.clone(),
            tool_call.arguments.clone(),
            step_num
        );
        
        if loop_pattern.is_problematic() {
            log::warn!("Loop pattern detected: {:?}", loop_pattern);
            return self.handle_loop_detection(loop_pattern, tool_call, step_num).await;
        }
        
        // Check if this is a control plane tool first
        if ControlPlaneServer::is_control_tool(&tool_call.name) {
            self.execute_control_plane_tool(tool_call, step_num).await
        } else if tool_call.name == "execute_code" {
            self.execute_code_tool(tool_call, step_num).await
        } else if tool_call.name == "rag_search" {
            self.execute_rag_search_tool(tool_call, step_num).await
        } else {
            self.execute_generic_tool(tool_call, step_num).await
        }
    }

    async fn execute_code_tool(&mut self, tool_call: crate::core_types::ToolCall, step_num: usize) -> Result<Observation, AgentError> {
        const MAX_CONSECUTIVE_FAILURES: u32 = 2;
        if let Some(failures) = self.consecutive_tool_failures.get("execute_code") {
            if *failures >= MAX_CONSECUTIVE_FAILURES {
                let err_msg = format!("Tool 'execute_code' has failed {} times in a row. Skipping.", failures);
                log::error!("{}", err_msg);
                return self.add_tool_observation(tool_call.id, err_msg, false).await;
            }
        }

        let executor_option = self.code_executor.clone();
        let lang = tool_call
            .arguments
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("python")
            .to_string();
        let code = tool_call
            .arguments
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tool_args = tool_call.arguments.clone();
        let tool_id = tool_call.id.clone();

        if let Some(executor) = executor_option {
            let is_authorized = self.check_tool_authorization(
                "execute_code",
                "Execute code in a sandbox",
                &tool_args,
                tool_id.clone(),
                step_num,
            ).await.unwrap_or(false);

            if is_authorized {
                match executor.execute_code(&lang, &code).await {
                    Ok(exec_result) => {
                        self.consecutive_tool_failures.remove("execute_code");
                        let obs_content = format!(
                            "Stdout: {}\nStderr: {}",
                            exec_result.stdout, exec_result.stderr
                        );
                        self.add_tool_observation(tool_id, obs_content, true).await
                    }
                    Err(e) => {
                        let entry = self.consecutive_tool_failures.entry("execute_code".to_string()).or_insert(0);
                        *entry += 1;
                        let err_msg = format!("Code execution failed: {}", e);
                        log::error!("{}", err_msg);
                        self.history.add_step(HistoryStep::ExecutorError(err_msg.clone()));
                        self.add_tool_observation(tool_id, err_msg, false).await
                    }
                }
            } else {
                let err_msg = "Code execution denied by user".to_string();
                log::info!("{}", err_msg);
                self.history.add_step(HistoryStep::ToolError(err_msg.clone()));
                self.add_tool_observation(tool_id, err_msg, false).await
            }
        } else {
            let err_msg = "Code execution requested, but no code executor is configured.".to_string();
            log::error!("{}", err_msg);
            self.history.add_step(HistoryStep::ExecutorError(err_msg.clone()));
            self.add_tool_observation(tool_id, err_msg, false).await
        }
    }

    async fn execute_rag_search_tool(&mut self, tool_call: crate::core_types::ToolCall, step_num: usize) -> Result<Observation, AgentError> {
        const MAX_CONSECUTIVE_FAILURES: u32 = 2;
        if let Some(failures) = self.consecutive_tool_failures.get("rag_search") {
            if *failures >= MAX_CONSECUTIVE_FAILURES {
                let err_msg = format!("Tool 'rag_search' has failed {} times in a row. Skipping.", failures);
                log::error!("{}", err_msg);
                return self.add_tool_observation(tool_call.id, err_msg, false).await;
            }
        }

        if self.is_rag_enabled() {
            let query = tool_call
                .arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tool_args = tool_call.arguments.clone();
            let tool_id = tool_call.id.clone();

            let is_authorized = self.check_tool_authorization(
                "rag_search",
                "Search for information in the RAG system",
                &tool_args,
                tool_id.clone(),
                step_num,
            ).await.unwrap_or(false);

            if is_authorized {
                match self.retrieve_rag_context(&query).await {
                    Ok(Some(context)) => {
                        self.consecutive_tool_failures.remove("rag_search");
                        let formatted_context = context.format_for_llm();
                        self.add_tool_observation(tool_id, formatted_context, true).await
                    }
                    Ok(None) => {
                        self.consecutive_tool_failures.remove("rag_search");
                        let no_results_msg = "No relevant documents found in RAG system.".to_string();
                        self.add_tool_observation(tool_id, no_results_msg, true).await
                    }
                    Err(e) => {
                        let entry = self.consecutive_tool_failures.entry("rag_search".to_string()).or_insert(0);
                        *entry += 1;
                        let err_msg = format!("RAG search failed: {}", e);
                        self.add_tool_observation(tool_id, err_msg, false).await
                    }
                }
            } else {
                let err_msg = "RAG search denied by user".to_string();
                self.add_tool_observation(tool_id, err_msg, false).await
            }
        } else {
            let err_msg = "RAG search requested, but RAG is not enabled.".to_string();
            self.add_tool_observation(tool_call.id, err_msg, false).await
        }
    }

    async fn execute_generic_tool(&mut self, tool_call: crate::core_types::ToolCall, step_num: usize) -> Result<Observation, AgentError> {
        let tool_name = tool_call.name.clone();
        let tool_args = tool_call.arguments.clone();
        let tool_id = tool_call.id.clone();

        const MAX_CONSECUTIVE_FAILURES: u32 = 2;

        if let Some(failures) = self.consecutive_tool_failures.get(&tool_name) {
            if *failures >= MAX_CONSECUTIVE_FAILURES {
                let err_msg = format!("Tool '{}' has failed {} times in a row. Skipping.", tool_name, failures);
                log::error!("{}", err_msg);
                return self.add_tool_observation(tool_id, err_msg, false).await;
            }
        }

        if let Some(tool) = self.tools.get(&tool_name).cloned() {
            let tool_metadata = tool.metadata();
            let is_authorized = self.check_tool_authorization(
                &tool_name,
                &tool_metadata.description,
                &tool_args,
                tool_id.clone(),
                step_num,
            ).await.unwrap_or(false);

            if is_authorized {
                match tool.execute(tool_args).await {
                    Ok(content) => {
                        self.consecutive_tool_failures.remove(&tool_name);
                        self.add_tool_observation(tool_id, content, true).await
                    }
                    Err(e) => {
                        let entry = self.consecutive_tool_failures.entry(tool_name.clone()).or_insert(0);
                        *entry += 1;
                        let err_msg = format!("Tool '{}' execution failed: {}", tool_name, e);
                        self.add_tool_observation(tool_id, err_msg, false).await
                    }
                }
            } else {
                let err_msg = format!("Tool execution denied by user: {}", tool_name);
                self.add_tool_observation(tool_id, err_msg, false).await
            }
        } else {
            let err_msg = format!("Unknown tool: {}", tool_name);
            self.add_tool_observation(tool_id, err_msg, false).await
        }
    }

    async fn handle_loop_detection(&mut self, pattern: LoopPattern, tool_call: crate::core_types::ToolCall, step_num: usize) -> Result<Observation, AgentError> {
        match pattern {
            LoopPattern::ExactLoop { tool_name, count, .. } => {
                log::error!("TERMINATING: Exact loop detected - {} called {} times", tool_name, count);
                
                // Don't attempt recovery - terminate execution immediately
                let error_msg = format!(
                    "Agent execution terminated due to infinite loop: '{}' called {} times consecutively. \
                    This indicates the agent is stuck and cannot make progress.",
                    tool_name, count
                );
                
                return Err(AgentError::LoopDetection(error_msg));
            }
            LoopPattern::SimilarLoop { tool_name, count, similarity_score, .. } => {
                log::error!("TERMINATING: Similar loop detected - {} called {} times with {:.1}% similarity", 
                           tool_name, count, similarity_score * 100.0);
                
                let error_msg = format!(
                    "Agent execution terminated due to similar loop pattern: '{}' called {} times with {:.1}% argument similarity. \
                    This indicates the agent is stuck in a repetitive pattern.",
                    tool_name, count, similarity_score * 100.0
                );
                
                return Err(AgentError::LoopDetection(error_msg));
            }
            _ => {
                // This shouldn't happen since we check is_problematic() before calling this method
                log::warn!("Non-problematic pattern passed to handle_loop_detection: {:?}", pattern);
                
                // Execute normally as fallback
                self.execute_generic_tool(tool_call, step_num).await
            }
        }
    }
    
    #[allow(dead_code)]
    async fn attempt_tier1_recovery(&mut self, tool_name: &str, _arguments: &serde_json::Value) -> Result<Option<String>, AgentError> {
        // Generic recovery approach: For any tool being called in a loop,
        // we should NOT provide successful recovery as this allows the loop to continue.
        // Instead, return None to force an error observation that stops the loop.
        log::warn!("Tier 1 recovery: Tool '{}' called in loop - no recovery provided to force loop termination", tool_name);
        Ok(None)
    }

    async fn execute_control_plane_tool(&mut self, tool_call: crate::core_types::ToolCall, _step_num: usize) -> Result<Observation, AgentError> {
        let tool_name = tool_call.name.clone();
        let tool_args = tool_call.arguments.clone();
        let tool_id = tool_call.id.clone();

        log::info!("Executing control plane tool: {}", tool_name);

        match self.control_plane.execute_tool(&tool_name, tool_args).await {
            Ok(content) => {
                // For control plane tools, we should check if this is a completion signal
                if tool_name == "assistant_done" {
                    log::info!("Assistant completion signal received");
                    // We'll handle the completion in the observation
                } else if tool_name == "report_progress" {
                    // Parse the report_progress response to extract context and reason
                    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(context) = response.get("context").and_then(|c| c.as_str()) {
                            // Log the context in a user-friendly way
                            log::info!("📊 Progress update: {}", context);
                            
                            // Return the context as the observation so it shows in the terminal
                            return self.add_tool_observation(tool_id, context.to_string(), true).await;
                        } else {
                            // No context provided - create a default message based on reason
                            let default_message = if let Some(reason) = response.get("reason").and_then(|r| r.as_str()) {
                                match reason {
                                    "awaiting_input" => "Waiting for your input",
                                    "pending_choice" => "Waiting for your selection", 
                                    "need_clarification" => "Need clarification",
                                    "response_complete" => "Response complete",
                                    "results_displayed" => "Results displayed",
                                    _ => "Progress update"
                                }
                            } else {
                                "Progress update"
                            };
                            
                            return self.add_tool_observation(tool_id, default_message.to_string(), true).await;
                        }
                    }
                }
                
                self.add_tool_observation(tool_id, content, true).await
            }
            Err(e) => {
                let err_msg = format!("Control plane tool '{}' execution failed: {}", tool_name, e);
                log::error!("{}", err_msg);
                self.add_tool_observation(tool_id, err_msg, false).await
            }
        }
    }

    async fn add_tool_observation(&mut self, tool_call_id: Option<String>, content: String, success: bool) -> Result<Observation, AgentError> {
        if !success {
            if self.last_tool_error.as_deref() == Some(&content) {
                self.consecutive_error_count += 1;
            } else {
                self.last_tool_error = Some(content.clone());
                self.consecutive_error_count = 1;
            }
        } else {
            self.last_tool_error = None;
            self.consecutive_error_count = 0;
        }

        log::info!("Observation: {}", content);
        let observation = Observation {
            tool_call_id: tool_call_id.clone(),
            content: content.clone(),
            success,
        };
        self.history.add_step(HistoryStep::Observation(observation.clone()));
        self.memory
            .add_message(Message {
                role: Role::Tool,
                content,
                tool_call_id,
                tool_calls: None,
            })
            .await?;
        Ok(observation)
    }

    #[allow(dead_code)]
    async fn recover_from_loop(&mut self, original_task: &str, loop_msg: &str) -> Result<String, AgentError> {
        log::info!("Starting loop recovery process");
        
        // Reset conversation memory but preserve the original task
        self.clear_memory();
        
        self.loop_detector.clear();
        
        let recovery_message = format!(
            "I encountered a technical issue where I was repeating the same action multiple times. \
            Let me approach your request differently.\n\nYour original request: {}\n\n\
            I'll now try a different approach to help you with this task.",
            original_task
        );
        
        // Add recovery context to memory
        self.add_user_task_to_memory(&format!(
            "{}. Please try a different approach and avoid repeating the same tool calls. \
            Focus on making progress toward the goal through varied methods.",
            original_task
        )).await?;
        
        // Add recovery explanation as system context
        self.memory.add_message(Message {
            role: Role::System,
            content: format!(
                "Previous attempt failed due to repetitive behavior. Loop detected: {}. \
                Try alternative approaches and avoid calling the same tool repeatedly with identical parameters.",
                loop_msg
            ),
            tool_call_id: None,
            tool_calls: None,
        }).await?;
        
        log::info!("Recovery context added, returning explanation to user");
        
        // Return recovery message to user
        Ok(recovery_message)
    }

    pub fn history(&self) -> &AgentMemory {
        &self.history
    }

    pub fn memory(&self) -> &Box<dyn ConversationMemory> {
        &self.memory
    }

    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: AgentConfig) {
        self.config = config;
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> MemoryStats {
        self.history.get_stats()
    }

    pub fn update_memory_config(&mut self, memory_config: MemoryConfig) {
        self.config.memory_config = Some(memory_config);
    }

    /// Clear agent memory
    pub fn clear_memory(&mut self) {
        self.memory.clear();
        self.history.clear();
    }

    /// Get the tools available to this agent
    pub fn tools(&self) -> &HashMap<String, Arc<dyn Tool>> {
        &self.tools
    }
    
    /// Clear the loop detector state (for recovery purposes)
    pub fn clear_loop_detector(&mut self) {
        self.loop_detector.clear();
    }
    
    pub async fn add_recovery_message(&mut self, message: Message) -> Result<(), AgentError> {
        self.memory.add_message(message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_types::LLMResponse;

    // Mock LLM for testing
    struct MockLLM;

    #[async_trait]
    impl LLM for MockLLM {
        async fn generate(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<ToolMetadata>>,
        ) -> Result<LLMResponse, AgentError> {
            Ok(LLMResponse {
                finish_reason: None,
                usage: None,
                content: Some("Final Answer: Test complete".to_string()),
                tool_calls: None,
            })
        }
    }

    #[tokio::test]
    async fn test_clear_memory() {
        // 1. Create an agent with a mock LLM
        let llm = Arc::new(MockLLM);
        let tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        let config = AgentConfig::default();
        let mut agent = Agent::new(llm, tools, None, config);

        // 2. Run a simple task to populate memory
        let result = agent.run("test task".to_string()).await;
        assert!(result.is_ok());

        // 3. Assert that history is not empty
        assert!(!agent.history().get_history().is_empty(), "History should not be empty after a run");
        assert!(!agent.memory().get_context().is_empty(), "ConversationMemory should not be empty after a run");


        // 4. Call clear_memory
        agent.clear_memory();

        // 5. Assert that history is now empty
        assert!(agent.history().get_history().is_empty(), "History should be empty after clearing memory");
        assert!(agent.memory().get_context().is_empty(), "ConversationMemory should be empty after clearing memory");
    }

    // Mock LLM that simulates looping behavior
    struct LoopingMockLLM {
        call_count: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl LoopingMockLLM {
        fn new() -> Self {
            Self {
                call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl LLM for LoopingMockLLM {
        async fn generate(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<ToolMetadata>>,
        ) -> Result<LLMResponse, AgentError> {
            let mut count = self.call_count.lock().unwrap();
            *count += 1;

            // First few calls simulate looping tool calls
            if *count <= 3 {
                Ok(LLMResponse {
                    finish_reason: None,
                    usage: None,
                    content: Some("I need to get the current time.".to_string()),
                    tool_calls: Some(vec![crate::core_types::ToolCall {
                        id: Some(format!("call_{}", *count)),
                        name: "get_current_time".to_string(),
                        arguments: serde_json::json!({"timezone": "UTC"}),
                    }]),
                })
            } else {
                // After loop recovery, should provide final answer
                Ok(LLMResponse {
                    finish_reason: None,
                    usage: None,
                    content: Some("Final Answer: Recovery successful".to_string()),
                    tool_calls: None,
                })
            }
        }
    }

    // Mock tool for testing
    struct MockTimeTool;

    #[async_trait]
    impl Tool for MockTimeTool {
        fn metadata(&self) -> ToolMetadata {
            ToolMetadata {
                name: "get_current_time".to_string(),
                description: "Get current time".to_string(),
                input_schema: serde_json::json!({}),
            }
        }

        async fn execute(&self, _args: serde_json::Value) -> Result<String, AgentError> {
            Ok(serde_json::to_string(&serde_json::json!({
                "datetime": "2025-08-04T12:00:00Z",
                "timezone": "UTC"
            })).unwrap())
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_loop_recovery() {
        // 1. Create an agent with looping mock LLM and a mock tool
        let llm = Arc::new(LoopingMockLLM::new());
        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        tools.insert("get_current_time".to_string(), Arc::new(MockTimeTool));
        
        let config = AgentConfig::default();
        let mut agent = Agent::new(llm, tools, None, config);

        // 2. Add initial user task to memory
        agent.add_user_task_to_memory("What time is it?").await.unwrap();

        // 3. Run multiple steps to trigger loop detection within a single step
        // Since loop detector resets between steps, we need a step that makes 3+ identical calls
        let result = agent.run_step(1).await;

        // 4. The test should either succeed with recovery or fail with loop detection
        // Since our mock makes 3 identical calls within one step, it should trigger loop detection
        match result {
            Ok((Some(response), _)) => {
                // If we get a response, it should be from recovery
                assert!(
                    response.contains("technical issue") || response.contains("different approach") || response.contains("Recovery successful"),
                    "Response should indicate recovery occurred: {}",
                    response
                );
            }
            Err(crate::errors::AgentError::LoopDetection(_)) => {
                // This is also acceptable - loop was detected correctly
                println!("Loop detection working correctly");
            }
            other => {
                panic!("Unexpected result: {:?}", other);
            }
        }

        // 5. Verify that loop detector was reset (it should be empty at start of next step)
        let stats = agent.loop_detector.get_statistics();
        assert_eq!(stats.total_calls, 0, "Loop detector should be cleared at start of steps");
    }
}