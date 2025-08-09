//! Tracing integration for agent execution tracing and observability
//!
//! This module provides structured logging and tracing capabilities for agent
//! executions, enabling post-hoc analysis and debugging of agent behavior. The
//! design prioritizes machine-readable trace formats that can be ingested by
//! observability platforms. This approach is critical for understanding agent
//! decision patterns, identifying failure modes, and optimizing prompt engineering
//! based on empirical execution data.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use crate::config::types::TracingConfig;
use crate::core_types::{Message, Role};
use crate::llm::LLM;
use crate::trace::{AgentTraceHandler, AgentStep, AgentExecution};

#[derive(Debug, Serialize)]
struct TracingTrace {
    timestamp: String,
    step_number: usize,
    trace_type: String,
    content: String,
    tool_call: Option<serde_json::Value>,
    result: Option<serde_json::Value>,
}

pub struct TracingTraceHandler {
    config: TracingConfig,
    llm_client: Arc<dyn LLM>,
    trace_file: Option<Arc<Mutex<File>>>,
}

impl TracingTraceHandler {
    pub fn new(config: TracingConfig, llm_client: Arc<dyn LLM>) -> Result<Self, std::io::Error> {
        Ok(Self {
            config,
            llm_client,
            trace_file: None,
        })
    }

    async fn get_summary_static(config: &TracingConfig, step: &AgentStep, llm_client: Arc<dyn LLM>) -> Option<String> {
        if !config.enabled || step.tool_calls.is_none() || step.tool_results.is_none() {
            return None;
        }

        let tool_calls = step.tool_calls.as_ref().unwrap();
        let tool_results = step.tool_results.as_ref().unwrap();

        if tool_calls.is_empty() || tool_results.is_empty() {
            return None;
        }

        let tool_call = &tool_calls[0];
        let tool_result = &tool_results[0];

        let tool_call_str = serde_json::to_string_pretty(&tool_call).unwrap_or_default();

        let prompt = format!(
            r#"Based on the following tool call and its result, provide a very short, one-sentence, user-facing summary of what the agent just did.
For example, if the tool call was to create a file and the result was success, a good summary would be 'Creating the main application file.'
If the tool call was to get the weather and the result was "25°C and sunny", a good summary would be "The weather is 25°C and sunny."
Keep it concise and in the present tense. Do not add any preamble or extra text.

Tool Call:
{}

Tool Result:
{}"#,
            tool_call_str,
            tool_result.content
        );

        let messages = vec![Message {
            role: Role::User,
            content: prompt,
            tool_call_id: None,
            tool_calls: None,
        }];

        match llm_client.generate(messages, None).await {
            Ok(response) => response.content.map(|s| s.trim().to_string()),
            Err(e) => {
                log::error!("Tracing summarization failed: {}", e);
                None
            }
        }
    }
}

use tokio::task::JoinHandle;
impl AgentTraceHandler for TracingTraceHandler {
    fn on_step_complete(&mut self, step: &AgentStep) -> Option<JoinHandle<()>> {
        if !self.config.enabled {
            return None;
        }

        if self.trace_file.is_none() {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.config.trace_file)
                .unwrap();
            self.trace_file = Some(Arc::new(Mutex::new(file)));
        }

        let step = step.clone();
        let llm_client = self.llm_client.clone();
        let trace_file = self.trace_file.clone();
        let config = self.config.clone();

        Some(tokio::spawn(async move {
            let (content, trace_type) = if step.tool_calls.is_some() && !step.tool_calls.as_ref().unwrap().is_empty() {
                if let Some(summary) = Self::get_summary_static(&config, &step, llm_client).await {
                    (summary, "summary".to_string())
                } else {
                    ("No summary available".to_string(), "summary".to_string())
                }
            } else {
                (step.thought.clone().unwrap_or_default(), "thought".to_string())
            };

            let trace = TracingTrace {
                timestamp: Utc::now().to_rfc3339(),
                step_number: step.step_number,
                trace_type,
                content,
                tool_call: step.tool_calls.as_ref().and_then(|tc| tc.first()).map(|tc| json!(tc)),
                result: step.tool_results.as_ref().and_then(|tr| tr.first()).map(|tr| json!(tr)),
            };

            if let Some(file_mutex) = trace_file {
                let mut file = file_mutex.lock().await;
                if let Ok(json_trace) = serde_json::to_string(&trace) {
                    if writeln!(*file, "{}", json_trace).is_err() {
                        log::error!("Failed to write Tracing trace to file");
                    }
                }
            }
        }))
    }

    fn on_execution_complete(&mut self, _execution: &AgentExecution) {
        if !self.config.enabled {
            return;
        }
        // Finalization logic can go here if needed
    }
}

#[cfg(test)]
mod tests;