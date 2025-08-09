//! Agent UI handler for real-time streaming communication with frontend clients
//!
//! This module bridges the agent execution engine with web-based user interfaces
//! through server-sent events (SSE) and streaming protocols. The design prioritizes
//! real-time feedback and progressive disclosure of agent reasoning, enabling users
//! to understand and intervene in agent decision-making. This transparency is crucial
//! for building trust in autonomous systems and enabling human-in-the-loop workflows.

use gola_ag_ui_server::agent::{streams, AgentHandler, AgentMetadata, AgentStream};
use gola_ag_ui_server::error::ServerError;
use gola_ag_ui_types::{
    Event, Role, RunAgentInput, RunErrorEvent, RunFinishedEvent,
    RunStartedEvent, TextMessageContentEvent, TextMessageEndEvent, TextMessageStartEvent,
    AuthorizationConfig, ToolAuthorizationResponseEvent, PendingAuthorization,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::agent::Agent;
use crate::config::GolaConfig;
use crate::errors::AgentError as GolaAgentError;
use crate::polling_authorization_handler::PollingAuthorizationHandler;
use crate::guardrails::AuthorizationMode;

const GOLA_CONNECT_MESSAGE: &str = "gola-connect-HACK";

#[derive(Clone)]
pub struct GolaAgentHandler {
    agent: Arc<Mutex<Agent>>,
    config: Arc<GolaConfig>,
    authorization_handler: Option<Arc<PollingAuthorizationHandler>>,
}

impl GolaAgentHandler {
    /// Create a new GolaAgentHandler with authorization support
    pub fn new(agent: Arc<Mutex<Agent>>, config: Arc<GolaConfig>) -> Self {
        // Create polling authorization handler
        let polling_auth_handler = Arc::new(PollingAuthorizationHandler::new());
        
        Self {
            agent,
            config,
            authorization_handler: Some(polling_auth_handler),
        }
    }

    /// Create a new GolaAgentHandler without authorization support (for testing)
    pub fn new_without_authorization(agent: Arc<Mutex<Agent>>, config: Arc<GolaConfig>) -> Self {
        Self {
            agent,
            config,
            authorization_handler: None,
        }
    }

    /// Runs a task directly using the underlying agent, returning a single response.
    /// This is for non-streaming, direct execution.
    pub async fn run_task_directly(&self, task: String) -> Result<String, GolaAgentError> {
        let mut agent_guard = self.agent.lock().await;
        agent_guard.run(task).await
    }

    /// Set up authorization for the agent if authorization handler is available
    async fn setup_agent_authorization(&self) -> Result<(), ServerError> {
        if let Some(auth_handler) = &self.authorization_handler {
            let mut agent_guard = self.agent.lock().await;
            
            // Set the authorization handler on the agent
            agent_guard.set_authorization_handler(auth_handler.clone());
            
            // Configure authorization mode to Ask so that the agent will use the handler
            let mut agent_config = agent_guard.config().clone();
            agent_config.authorization_mode = AuthorizationMode::Ask;
            agent_guard.set_config(agent_config);
            
            log::info!("Authorization handler configured for agent");
        }
        Ok(())
    }
}

#[async_trait]
impl AgentHandler for GolaAgentHandler {
    async fn handle_input(&self, input: RunAgentInput) -> Result<AgentStream, ServerError> {
        let initial_task = input
            .messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role(), Role::User))
            .and_then(|msg| msg.content())
            .map(|s| s.to_string());

        let task_to_run = match initial_task {
            Some(task) if !task.is_empty() => task,
            _ => {
                if input.messages.is_empty() {
                    return Err(ServerError::invalid_input("Messages cannot be empty"));
                } else {
                    return Err(ServerError::invalid_input(
                        "No user message with content found in input",
                    ));
                }
            }
        };


        if task_to_run == GOLA_CONNECT_MESSAGE {
            // Check if we have an ice breaker prompt configured
            let icebreaker_content = if let Some(prompts) = &self.config.prompts {
                if let Some(purposes) = &prompts.purposes {
                    if let Some(ice_breaker) = purposes.get("ice_breaker") {
                        if let Some(assembly) = &ice_breaker.assembly {
                            if let Some(prompt_source) = assembly.get(0) {
                                match prompt_source {
                                    crate::config::PromptSource::File { file } => {
                                        // The file content should have been loaded during config resolution
                                        Some(file.clone())
                                    },
                                    crate::config::PromptSource::Fragment { fragment } => {
                                        Some(fragment.clone())
                                    },
                                }
                            } else { None }
                        } else { None }
                    } else { None }
                } else { None }
            } else { None };

            let message = icebreaker_content.unwrap_or_else(|| {
                "Hey there! What can I do for you?".to_string()
            });
            
            return Ok(streams::text_response(message));
        }

        let run_id = input.run_id.clone();
        let thread_id = input.thread_id.clone();

        // Set up authorization before running the agent
        self.setup_agent_authorization().await?;

        // Clone the agent Arc to move into the stream
        let agent_clone = self.agent.clone();

        let stream = async_stream::stream! {
            yield Event::RunStarted(RunStartedEvent::new(thread_id.clone(), run_id.clone()));

            let mut agent_guard = agent_clone.lock().await;
            let mut error_occurred = false;

            // Add the user's message to memory before starting the loop
            if let Err(e) = agent_guard.add_user_task_to_memory(&task_to_run).await {
                let error_message = format!("Failed to add task to memory: {}", e);
                log::error!("{}", error_message);
                yield Event::RunError(RunErrorEvent::new(error_message));
                error_occurred = true;
            }

            if !error_occurred {
                for step_num in 0..agent_guard.config().max_steps {
                    match agent_guard.run_step(step_num).await {
                        Ok((Some(agent_response_content), step)) => {
                            // Send tool observations first if any
                            if let Some(tool_results) = &step.tool_results {
                                for observation in tool_results {
                                    if let Some(tool_calls) = &step.tool_calls {
                                        for tool_call in tool_calls {
                                            if tool_call.name == "report_progress" && observation.tool_call_id == tool_call.id {
                                                // Check if this is a progress report that should be shown
                                                if let Some(_reason) = tool_call.arguments.get("reason").and_then(|r| r.as_str()) {
                                                    // Always show progress messages with context
                                                    // The context provides useful information to users
                                                    // Send report_progress as a complete separate message
                                                    let obs_message_id = Uuid::new_v4().to_string();
                                                    yield Event::TextMessageStart(TextMessageStartEvent::new(obs_message_id.clone()));
                                                    yield Event::TextMessageContent(TextMessageContentEvent::new(obs_message_id.clone(), format!("{}\n\n", observation.content)));
                                                    yield Event::TextMessageEnd(TextMessageEndEvent::new(obs_message_id));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            
                            // Check if report_progress was called with a reason that should stop auto-continuing
                            let mut should_stop_for_report_progress = false;
                            if let Some(tool_calls) = &step.tool_calls {
                                for tool_call in tool_calls {
                                    if tool_call.name == "report_progress" {
                                        // Check the arguments directly (already a Value)
                                        if let Some(reason) = tool_call.arguments.get("reason").and_then(|r| r.as_str()) {
                                            // Stop auto-continuing for these reasons
                                            if reason == "awaiting_input" || 
                                               reason == "pending_choice" || 
                                               reason == "need_clarification" {
                                                should_stop_for_report_progress = true;
                                                log::info!("Stopping auto-continue due to report_progress with reason: {}", reason);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            
                            // Check if the response indicates the agent wants to continue
                            let response_lower = agent_response_content.to_lowercase();
                            let should_auto_continue = !should_stop_for_report_progress && (
                                response_lower.contains("please hold") 
                                || response_lower.contains("hold on")
                                || response_lower.contains("one moment")
                                || response_lower.contains("just a moment")
                                || response_lower.contains("let me")
                                || response_lower.contains("i'll search")
                                || response_lower.contains("i'll find")
                                || response_lower.contains("i'll update")
                                || response_lower.contains("i'll proceed")
                                || response_lower.contains("i'll determine")
                                || response_lower.contains("i'll now")
                                || response_lower.contains("now i'll")
                                || response_lower.contains("let me summarize")
                            );
                            
                            // Send the main response as a separate message
                            let message_id = Uuid::new_v4().to_string();
                            yield Event::TextMessageStart(TextMessageStartEvent::new(message_id.clone()));
                            yield Event::TextMessageContent(TextMessageContentEvent::new(message_id.clone(), agent_response_content));
                            yield Event::TextMessageEnd(TextMessageEndEvent::new(message_id));
                            
                            if should_auto_continue {
                                log::info!("Auto-continuing based on continuation hints in response");
                                // Continue the loop instead of breaking
                                continue;
                            } else {
                                // Final answer received, break the loop.
                                break;
                            }
                        }
                        Ok((None, step)) => {
                            // Send tool observations if any
                            if let Some(tool_results) = &step.tool_results {
                                for observation in tool_results {
                                    if let Some(tool_calls) = &step.tool_calls {
                                        for tool_call in tool_calls {
                                            if tool_call.name == "report_progress" && observation.tool_call_id == tool_call.id {
                                                // Check if this is a progress report that should be shown
                                                if let Some(_reason) = tool_call.arguments.get("reason").and_then(|r| r.as_str()) {
                                                    // Always show progress messages with context
                                                    // The context provides useful information to users
                                                    // Send report_progress as a complete message
                                                    let obs_message_id = Uuid::new_v4().to_string();
                                                    yield Event::TextMessageStart(TextMessageStartEvent::new(obs_message_id.clone()));
                                                    yield Event::TextMessageContent(TextMessageContentEvent::new(obs_message_id.clone(), format!("{}\n\n", observation.content)));
                                                    yield Event::TextMessageEnd(TextMessageEndEvent::new(obs_message_id));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            
                            // Check if report_progress was called with a reason that should stop auto-continuing
                            let mut should_stop_for_report_progress = false;
                            if let Some(tool_calls) = &step.tool_calls {
                                for tool_call in tool_calls {
                                    if tool_call.name == "report_progress" {
                                        // Check the arguments directly (already a Value)
                                        if let Some(reason) = tool_call.arguments.get("reason").and_then(|r| r.as_str()) {
                                            // Stop auto-continuing for these reasons
                                            if reason == "awaiting_input" || 
                                               reason == "pending_choice" || 
                                               reason == "need_clarification" {
                                                should_stop_for_report_progress = true;
                                                log::info!("Stopping auto-continue due to report_progress with reason: {}", reason);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            
                            if should_stop_for_report_progress {
                                // Stop execution when waiting for user input
                                break;
                            } else {
                                // Agent took an action, continue the loop to process the result.
                                continue;
                            }
                        }
                        Err(GolaAgentError::LoopDetection(loop_msg)) => {
                            log::warn!("Loop detected, attempting automated recovery: {}", loop_msg);
                            
                            // Reset the loop detector to prevent immediate re-detection
                            agent_guard.clear_loop_detector();
                            
                            // Add recovery context to agent memory to guide different approach
                            let recovery_context = format!(
                                "Loop detected: {}. Try a different approach - avoid repeating the same tool call with identical parameters. \
                                Consider alternative methods or skip problematic tools if possible.",
                                loop_msg
                            );
                            
                            // Add the recovery message directly to agent memory using core types
                            let recovery_message = crate::core_types::Message {
                                role: crate::core_types::Role::System,
                                content: recovery_context,
                                tool_call_id: None,
                                tool_calls: None,
                            };
                            
                            if let Err(e) = agent_guard.add_recovery_message(recovery_message).await {
                                log::error!("Failed to add recovery context to memory: {}", e);
                                // Continue anyway - the loop detector reset should still help
                            }
                            
                            log::info!("Loop detector reset and recovery context added, continuing execution");
                            
                            // Continue the execution loop instead of breaking
                            // This allows the agent to try a different approach
                            continue;
                        }
                        Err(gola_err) => {
                            let error_message = format!("Agent execution failed: {}", gola_err);
                            log::error!("{}", error_message);
                            yield Event::RunError(RunErrorEvent::new(error_message));
                            error_occurred = true;
                            // Error occurred, break the loop.
                            break;
                        }
                    }
                }
            }

            if !error_occurred {
                yield Event::RunFinished(RunFinishedEvent::new(thread_id.clone(), run_id.clone()));
            }
        };

        Ok(Box::pin(stream))
    }

    async fn validate_input(&self, input: &RunAgentInput) -> Result<(), ServerError> {
        if input.messages.is_empty() {
            return Err(ServerError::invalid_input("Messages cannot be empty"));
        }
        if !input.messages.iter().any(|msg| {
            matches!(msg.role(), Role::User) && msg.content().map_or(false, |c| !c.is_empty())
        }) {
            return Err(ServerError::invalid_input(
                "No user message with content found in input",
            ));
        }
        Ok(())
    }

    async fn get_metadata(&self) -> Result<AgentMetadata, ServerError> {
        let gola_config = &self.config;
        
        let agent_guard = self.agent.lock().await;
        let tools_map = agent_guard.tools();
        
        let mut ui_tools = Vec::new();
        for (_tool_name, tool_arc) in tools_map.iter() {
            let tool_metadata = tool_arc.metadata();
            let ui_tool = gola_ag_ui_types::Tool::new(
                tool_metadata.name.clone(),
                tool_metadata.description.clone(),
                tool_metadata.input_schema.clone(),
            );
            ui_tools.push(ui_tool);
        }

        let metadata = AgentMetadata {
            name: gola_config.agent.name.clone(),
            description: gola_config.agent.description.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec!["text_generation".to_string()],
            max_context_length: None,
            supports_streaming: true,
            supports_tools: !ui_tools.is_empty(),
            supports_authorization: self.authorization_handler.is_some(),
            available_tools: ui_tools,
        };
        Ok(metadata)
    }

    async fn on_connect(&self) -> Result<(), ServerError> {
        log::info!("GolaAgentHandler: Client connected.");
        Ok(())
    }

    async fn on_disconnect(&self) -> Result<(), ServerError> {
        log::info!("GolaAgentHandler: Client disconnected.");
        Ok(())
    }

    // Authorization methods implementation
    async fn get_authorization_config(&self) -> Result<AuthorizationConfig, ServerError> {
        if let Some(auth_handler) = &self.authorization_handler {
            Ok(auth_handler.get_config().await)
        } else {
            Err(ServerError::invalid_input("Authorization not supported by this agent"))
        }
    }

    async fn set_authorization_config(&self, config: AuthorizationConfig) -> Result<(), ServerError> {
        if let Some(auth_handler) = &self.authorization_handler {
            auth_handler.set_config(config).await;
            log::info!("Authorization configuration updated");
            Ok(())
        } else {
            Err(ServerError::invalid_input("Authorization not supported by this agent"))
        }
    }

    async fn handle_authorization_response(&self, response: ToolAuthorizationResponseEvent) -> Result<(), ServerError> {
        if let Some(auth_handler) = &self.authorization_handler {
            auth_handler.handle_response(response.tool_call_id, response.response).await
                .map_err(|e| ServerError::internal(format!("Failed to handle authorization response: {}", e)))?;
            log::info!("Authorization response processed successfully");
            Ok(())
        } else {
            Err(ServerError::invalid_input("Authorization not supported by this agent"))
        }
    }

    async fn get_pending_authorizations(&self) -> Result<Vec<PendingAuthorization>, ServerError> {
        if let Some(auth_handler) = &self.authorization_handler {
            Ok(auth_handler.get_pending_authorizations().await)
        } else {
            Ok(vec![])
        }
    }

    async fn cancel_authorization(&self, tool_call_id: String) -> Result<(), ServerError> {
        if let Some(auth_handler) = &self.authorization_handler {
            auth_handler.cancel_authorization(tool_call_id).await
                .map_err(|e| ServerError::internal(format!("Failed to cancel authorization: {}", e)))?;
            log::info!("Authorization cancelled successfully");
            Ok(())
        } else {
            Err(ServerError::invalid_input("Authorization not supported by this agent"))
        }
    }

    async fn get_memory_stats(&self) -> Result<Option<serde_json::Value>, ServerError> {
        let agent_guard = self.agent.lock().await;
        let stats = agent_guard.memory_stats();
        let config = agent_guard.config();
        
        let default_memory_config = crate::config::types::MemoryConfig::default();
        let memory_config = config.memory_config.as_ref().unwrap_or(&default_memory_config);
        
        let stats_json = serde_json::json!({
            "total_steps": stats.total_steps,
            "user_tasks": stats.user_tasks,
            "thoughts": stats.thoughts,
            "actions": stats.actions,
            "observations": stats.observations,
            "errors": stats.errors,
            "successful_observations": stats.successful_observations,
            "failed_observations": stats.failed_observations,
            "utilization_percentage": stats.utilization_percentage(memory_config.max_history_steps),
            "config": {
                "max_history_steps": memory_config.max_history_steps,
                "eviction_strategy": format!("{:?}", memory_config.eviction_strategy),
                "min_recent_steps": memory_config.min_recent_steps,
                "preserve_strategy": {
                    "preserve_initial_task": memory_config.preserve_strategy.preserve_initial_task,
                    "preserve_successful_observations": memory_config.preserve_strategy.preserve_successful_observations,
                    "preserve_errors": memory_config.preserve_strategy.preserve_errors,
                    "preserve_recent_count": memory_config.preserve_strategy.preserve_recent_count
                }
            }
        });
        
        Ok(Some(stats_json))
    }
    async fn clear_memory(&self) -> Result<(), ServerError> {
        let mut agent_guard = self.agent.lock().await;
        
        // Clear the agent's memory
        agent_guard.clear_memory();
        
        log::info!("Agent memory cleared successfully");
        Ok(())
    }

    async fn get_available_tools(&self) -> Result<Vec<gola_ag_ui_types::Tool>, ServerError> {
        let metadata = self.get_metadata().await?;
        Ok(metadata.available_tools)
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    
    use crate::builder::ConfigBuilder;
    use crate::config::{
        LlmProvider as GolaLlmProvider, ToolsConfig,
    };
    use crate::core_types::{LLMResponse as CoreLLMResponse, Message as CoreMessage};
    use crate::llm::{ToolMetadata as CoreToolMetadata, LLM as CoreLLM};
    
    use gola_ag_ui_types::Message;
    use futures_util::StreamExt;

    // Mock LLM for testing Agent behavior
    #[derive(Clone)]
    struct MockLLM {
        response_fn: Arc<dyn Fn() -> Result<CoreLLMResponse, GolaAgentError> + Send + Sync>,
    }

    impl MockLLM {
        fn new<F>(response_fn: F) -> Self
        where
            F: Fn() -> Result<CoreLLMResponse, GolaAgentError> + Send + Sync + 'static,
        {
            Self {
                response_fn: Arc::new(response_fn),
            }
        }
    }

    #[async_trait]
    impl CoreLLM for MockLLM {
        async fn generate(
            &self,
            _messages: Vec<CoreMessage>,
            _tools: Option<Vec<CoreToolMetadata>>,
        ) -> Result<CoreLLMResponse, GolaAgentError> {
            (self.response_fn)()
        }
    }

    // Helper to create a GolaConfig for testing
    fn create_test_gola_config_for_handler() -> GolaConfig {
        ConfigBuilder::new()
            .agent_name("TestGolaAgentForHandler")
            .agent_description("A Gola agent for handler testing")
            .llm(
                GolaLlmProvider::Custom {
                    base_url: "http://localhost:65432".to_string(),
                },
                "mock-handler-model".to_string(),
            )
            .llm_api_key("handler-test-key")
            .build_unchecked()
    }

    // Helper to create a GolaAgentHandler with a MockLLM for testing handle_input
    async fn create_handler_with_mock_llm_behavior<FLlm>(
        llm_behavior_fn: FLlm,
    ) -> GolaAgentHandler
    where
        FLlm: Fn() -> Result<CoreLLMResponse, GolaAgentError> + Send + Sync + 'static,
    {
        let mock_llm = Arc::new(MockLLM::new(llm_behavior_fn));

        let core_agent_config = crate::agent::AgentConfig::default();
        let agent_instance = crate::agent::Agent::new(
            mock_llm,
            Default::default(),
            None,
            core_agent_config,
        );

        let gola_config = create_test_gola_config_for_handler();

        GolaAgentHandler::new(Arc::new(Mutex::new(agent_instance)), Arc::new(gola_config))
    }

    #[tokio::test]
    async fn test_handle_input_success_event_structure() {
        let agent_response_content = "Successful agent response".to_string();
        let response_clone = format!("Final Answer: {}", agent_response_content);

        let handler = create_handler_with_mock_llm_behavior(move || {
            Ok(CoreLLMResponse {
                content: Some(response_clone.clone()),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            })
        })
        .await;

        let run_input = RunAgentInput::new(
            "thread-1".to_string(),
            "run-1".to_string(),
            serde_json::json!({}),
            vec![Message::new_user("msg-1".to_string(), "Hello".to_string())],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        let stream = handler.handle_input(run_input.clone()).await.unwrap();
        let events: Vec<Event> = stream.collect().await;

        assert_eq!(events.len(), 5);
        assert!(
            matches!(events[0], Event::RunStarted(ref e) if e.thread_id == "thread-1" && e.run_id == "run-1")
        );
        assert!(matches!(events[1], Event::TextMessageStart(_)));
        assert!(
            matches!(events[2], Event::TextMessageContent(ref c) if c.delta == agent_response_content)
        );
        assert!(matches!(events[3], Event::TextMessageEnd(_)));
        assert!(
            matches!(events[4], Event::RunFinished(ref e) if e.thread_id == "thread-1" && e.run_id == "run-1")
        );
    }

    #[tokio::test]
    async fn test_authorization_config() {
        let handler = create_handler_with_mock_llm_behavior(|| {
            Ok(CoreLLMResponse {
                content: Some("test".to_string()),
                tool_calls: None,
                    finish_reason: None,
                    usage: None,
            })
        })
        .await;

        // Test getting default authorization config
        let config = handler.get_authorization_config().await.unwrap();
        assert!(config.enabled);

        // Test setting authorization config
        let new_config = AuthorizationConfig::disabled();
        handler.set_authorization_config(new_config.clone()).await.unwrap();

        let retrieved_config = handler.get_authorization_config().await.unwrap();
        assert!(!retrieved_config.enabled);
    }

    #[tokio::test]
    async fn test_pending_authorizations() {
        let handler = create_handler_with_mock_llm_behavior(|| {
            Ok(CoreLLMResponse {
                content: Some("test".to_string()),
                tool_calls: None,
                    finish_reason: None,
                    usage: None,
            })
        })
        .await;

        let pending = handler.get_pending_authorizations().await.unwrap();
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_metadata_with_authorization() {
        let handler = create_handler_with_mock_llm_behavior(|| {
            Ok(CoreLLMResponse {
                content: Some("test".to_string()),
                tool_calls: None,
                    finish_reason: None,
                    usage: None,
            })
        })
        .await;

        let metadata = handler.get_metadata().await.unwrap();
        assert!(metadata.supports_authorization);
    }

    // Include other existing tests...
    #[tokio::test]
    async fn test_handle_input_agent_run_error() {
        let error_message_from_llm = "LLM failed internally".to_string();
        let error_clone = error_message_from_llm.clone();

        let handler = create_handler_with_mock_llm_behavior(move || {
            Err(GolaAgentError::InternalError(error_clone.clone()))
        })
        .await;

        let run_input = RunAgentInput::new(
            "thread-error".to_string(),
            "run-error".to_string(),
            serde_json::json!({}),
            vec![Message::new_user(
                "msg-err".to_string(),
                "Trigger error".to_string(),
            )],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        let stream = handler.handle_input(run_input.clone()).await.unwrap();
        let events: Vec<Event> = stream.collect().await;

        assert_eq!(events.len(), 2);
        assert!(
            matches!(events[0], Event::RunStarted(ref e) if e.thread_id == "thread-error" && e.run_id == "run-error")
        );

        let expected_error_substring = format!("Agent execution failed: LLM interaction failed: LLM generation failed: Internal error: {}", error_message_from_llm);
        assert!(
            matches!(events[1], Event::RunError(ref e) if e.message.contains(&expected_error_substring))
        );
    }

    #[tokio::test]
    async fn test_validate_input_valid() {
        let gola_config = create_test_gola_config_for_handler();
        let mock_agent = crate::agent::Agent::new(
            Arc::new(MockLLM::new(|| {
                Ok(CoreLLMResponse {
                    content: None,
                    tool_calls: None,
                    finish_reason: None,
                    usage: None,
                })
            })),
            Default::default(),
            None,
            Default::default(),
        );
        let handler =
            GolaAgentHandler::new(Arc::new(Mutex::new(mock_agent)), Arc::new(gola_config));

        let input = RunAgentInput::new(
            "t1".to_string(),
            "r1".to_string(),
            serde_json::json!({}),
            vec![Message::new_user("m1".to_string(), "Test".to_string())],
            vec![],
            vec![],
            serde_json::json!({}),
        );
        assert!(handler.validate_input(&input).await.is_ok());
    }

    #[tokio::test]
    async fn test_validate_input_empty_messages() {
        let gola_config = create_test_gola_config_for_handler();
        let mock_agent = crate::agent::Agent::new(
            Arc::new(MockLLM::new(|| {
                Ok(CoreLLMResponse {
                    content: None,
                    tool_calls: None,
                    finish_reason: None,
                    usage: None,
                })
            })),
            Default::default(),
            None,
            Default::default(),
        );
        let handler =
            GolaAgentHandler::new(Arc::new(Mutex::new(mock_agent)), Arc::new(gola_config));
        let input = RunAgentInput::new(
            "t1".to_string(),
            "r1".to_string(),
            serde_json::json!({}),
            vec![],
            vec![],
            vec![],
            serde_json::json!({}),
        );
        let result = handler.validate_input(&input).await;
        assert!(result.is_err());
        match result.err().unwrap() {
            ServerError::InvalidInput(msg) => assert_eq!(msg, "Messages cannot be empty"),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_validate_input_no_user_message_with_content() {
        let gola_config = create_test_gola_config_for_handler();
        let mock_agent = crate::agent::Agent::new(
            Arc::new(MockLLM::new(|| {
                Ok(CoreLLMResponse {
                    content: None,
                    tool_calls: None,
                    finish_reason: None,
                    usage: None,
                })
            })),
            Default::default(),
            None,
            Default::default(),
        );
        let handler =
            GolaAgentHandler::new(Arc::new(Mutex::new(mock_agent)), Arc::new(gola_config));

        let input_no_user = RunAgentInput::new(
            "t1".to_string(),
            "r1".to_string(),
            serde_json::json!({}),
            vec![Message::new_assistant("m1".to_string(), "Hi".to_string())],
            vec![],
            vec![],
            serde_json::json!({}),
        );
        let result_no_user = handler.validate_input(&input_no_user).await;
        assert!(result_no_user.is_err());
        match result_no_user.err().unwrap() {
            ServerError::InvalidInput(msg) => {
                assert_eq!(msg, "No user message with content found in input")
            }
            _ => panic!("Expected InvalidInput error for no user message"),
        }
    }

    #[tokio::test]
    async fn test_get_metadata_basic() {
        let mut gola_config = create_test_gola_config_for_handler();
        gola_config.agent.name = "MetaAgent".to_string();
        gola_config.agent.description = "Agent for metadata test".to_string();
        gola_config.tools = ToolsConfig {
            calculator: false,
            web_search: None,
            code_execution: None,
        };
        gola_config.mcp_servers = vec![];

        let mock_agent = crate::agent::Agent::new(
            Arc::new(MockLLM::new(|| {
                Ok(CoreLLMResponse {
                    content: None,
                    tool_calls: None,
                    finish_reason: None,
                    usage: None,
                })
            })),
            Default::default(),
            None,
            Default::default(),
        );
        let handler =
            GolaAgentHandler::new(Arc::new(Mutex::new(mock_agent)), Arc::new(gola_config));
        let metadata = handler.get_metadata().await.unwrap();

        assert_eq!(metadata.name, "MetaAgent");
        assert_eq!(metadata.description, "Agent for metadata test");
    }

    #[tokio::test]
    async fn test_handle_input_icebreaker() {
        let icebreaker_message = "Welcome to Gola!".to_string();
        let mut gola_config = create_test_gola_config_for_handler();
        
        // Set up the icebreaker prompt in the config
        let mut purposes = std::collections::HashMap::new();
        purposes.insert("ice_breaker".to_string(), crate::config::PurposePrompt {
            role: "user".to_string(),
            assembly: Some(vec![crate::config::PromptSource::File { file: icebreaker_message.clone() }]),
        });
        gola_config.prompts = Some(crate::config::PromptConfig {
            purposes: Some(purposes),
            ..Default::default()
        });

        let mock_agent = crate::agent::Agent::new(
            Arc::new(MockLLM::new(|| {
                Ok(CoreLLMResponse {
                content: Some("This should not be called".to_string()),
                tool_calls: None,
                    finish_reason: None,
                    usage: None,
            })
            })),
            Default::default(),
            None,
            Default::default(),
        );
        let handler = GolaAgentHandler::new(Arc::new(Mutex::new(mock_agent)), Arc::new(gola_config));

        let run_input = RunAgentInput::new(
            "thread-icebreaker".to_string(),
            "run-icebreaker".to_string(),
            serde_json::json!({}),
            vec![Message::new_user("msg-connect".to_string(), GOLA_CONNECT_MESSAGE.to_string())],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        let stream = handler.handle_input(run_input).await.unwrap();
        let events: Vec<Event> = stream.collect().await;

        assert!(!events.is_empty());
        let text_content_event = events.iter().find_map(|e| match e {
            Event::TextMessageContent(content) => Some(content),
            _ => None,
        });

        assert!(text_content_event.is_some());
        assert_eq!(text_content_event.unwrap().delta, icebreaker_message);
    }

    #[tokio::test]
    async fn test_handle_input_default_icebreaker() {
        let gola_config = create_test_gola_config_for_handler();

        let mock_agent = crate::agent::Agent::new(
            Arc::new(MockLLM::new(|| {
                Ok(CoreLLMResponse {
                content: Some("This should not be called".to_string()),
                tool_calls: None,
                    finish_reason: None,
                    usage: None,
            })
            })),
            Default::default(),
            None,
            Default::default(),
        );
        let handler = GolaAgentHandler::new(Arc::new(Mutex::new(mock_agent)), Arc::new(gola_config));

        let run_input = RunAgentInput::new(
            "thread-icebreaker".to_string(),
            "run-icebreaker".to_string(),
            serde_json::json!({}),
            vec![Message::new_user("msg-connect".to_string(), GOLA_CONNECT_MESSAGE.to_string())],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        let stream = handler.handle_input(run_input).await.unwrap();
        let events: Vec<Event> = stream.collect().await;

        assert!(!events.is_empty());
        let text_content_event = events.iter().find_map(|e| match e {
            Event::TextMessageContent(content) => Some(content),
            _ => None,
        });

        assert!(text_content_event.is_some());
        assert_eq!(text_content_event.unwrap().delta, "Hey there! What can I do for you?");
    }

    #[tokio::test]
    #[ignore]
    async fn test_automated_loop_recovery() {
        // This test verifies that the automated recovery logic exists and compiles correctly
        // by testing the UI handler's response to a loop detection error
        
        let mock_llm = Arc::new(MockLLM::new(|| {
            // Return a simple error to simulate loop detection being triggered
            Err(crate::errors::AgentError::LoopDetection("Test loop detected".to_string()))
        }));

        let tools = std::collections::HashMap::new();
        let config = crate::agent::AgentConfig::default();
        let agent = crate::agent::Agent::new(mock_llm, tools, None, config);
        let handler = GolaAgentHandler::new(Arc::new(Mutex::new(agent)), Arc::new(create_test_gola_config_for_handler()));

        let run_input = RunAgentInput::new(
            "thread-recovery".to_string(),
            "run-recovery".to_string(),
            serde_json::json!({}),
            vec![Message::new_user("msg-recovery".to_string(), "Test recovery".to_string())],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        let stream = handler.handle_input(run_input).await.unwrap();
        let events: Vec<Event> = stream.collect().await;

        println!("Events received ({} total):", events.len());
        for (i, event) in events.iter().enumerate() {
            match event {
                Event::RunStarted(_) => println!("  {}: RunStarted", i),
                Event::TextMessageStart(_) => println!("  {}: TextMessageStart", i),
                Event::TextMessageContent(e) => println!("  {}: TextMessageContent: {}", i, e.delta),
                Event::TextMessageEnd(_) => println!("  {}: TextMessageEnd", i),
                Event::RunFinished(_) => println!("  {}: RunFinished", i),
                Event::RunError(e) => println!("  {}: RunError: {}", i, e.message),
                _ => println!("  {}: Other event: {:?}", i, event),
            }
        }

        // The key test: automated recovery should prevent loop detection errors from causing termination
        // The system should either complete successfully or continue processing, but NOT terminate with loop errors
        let has_run_error = events.iter().any(|e| matches!(e, Event::RunError(_)));
        
        if has_run_error {
            let error_events: Vec<_> = events.iter()
                .filter_map(|e| match e {
                    Event::RunError(err) => Some(&err.message),
                    _ => None,
                })
                .collect();
            
            // If there are errors, they should NOT be about loop detection (automated recovery should handle those)
            for error_msg in &error_events {
                assert!(!error_msg.to_lowercase().contains("loop"), 
                    "Expected automated recovery to handle loop detection, but got loop error: {}", error_msg);
            }
        }
        
        // Test passes if we get here without loop-related errors
        println!("Test completed - automated recovery handling verified");
    }
}
