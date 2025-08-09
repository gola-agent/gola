#[cfg(test)]
mod tests {
    use crate::config::types::TracingConfig;
    use crate::core_types::{Observation, ToolCall};
    use crate::tracing::TracingTraceHandler;
    use crate::llm::{LLMResponse, LLM};
    use crate::trace::{AgentStep, AgentTraceHandler};
    use async_trait::async_trait;
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tempfile::tempdir;

    struct MockLLM {
        response: Result<LLMResponse, crate::errors::AgentError>,
    }

    #[async_trait]
    impl LLM for MockLLM {
        async fn generate(
            &self,
            _messages: Vec<crate::core_types::Message>,
            _tools: Option<Vec<crate::llm::ToolMetadata>>,
        ) -> Result<LLMResponse, crate::errors::AgentError> {
            self.response.clone()
        }
    }

    #[tokio::test]
    async fn test_tracing_disabled() {
        let dir = tempdir().unwrap();
        let trace_file_path = dir.path().join("trace.jsonl");

        let config = TracingConfig {
            enabled: false,
            trace_file: trace_file_path.to_str().unwrap().to_string(),
            model_provider: "default".to_string(),
        };

        let llm = Arc::new(MockLLM {
            response: Ok(LLMResponse {
                content: Some("summary".to_string()),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            }),
        });

        let mut handler = TracingTraceHandler::new(config, llm).unwrap();
        let step = AgentStep {
            step_number: 1,
            thought: None,
            tool_calls: None,
            tool_results: None,
        };

        if let Some(handle) = handler.on_step_complete(&step) {
            handle.await.unwrap();
        }

        assert!(!trace_file_path.exists());
    }

    #[tokio::test]
    async fn test_tracing_summarization_and_file_output() {
        let dir = tempdir().unwrap();
        let trace_file_path = dir.path().join("trace.jsonl");

        let config = TracingConfig {
            enabled: true,
            trace_file: trace_file_path.to_str().unwrap().to_string(),
            model_provider: "default".to_string(),
        };

        let llm = Arc::new(MockLLM {
            response: Ok(LLMResponse {
                content: Some("This is a summary.".to_string()),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            }),
        });

        let mut handler = TracingTraceHandler::new(config, llm).unwrap();
        let step = AgentStep {
            step_number: 1,
            thought: Some("I should use a tool".to_string()),
            tool_calls: Some(vec![ToolCall {
                id: Some("1".to_string()),
                name: "test_tool".to_string(),
                arguments: Value::Null,
            }]),
            tool_results: Some(vec![Observation {
                tool_call_id: Some("1".to_string()),
                content: "Success".to_string(),
                success: true,
            }]),
        };

        if let Some(handle) = handler.on_step_complete(&step) {
            handle.await.unwrap();
        }

        let content = fs::read_to_string(trace_file_path).unwrap();
        let trace: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(trace["step_number"], 1);
        assert_eq!(trace["content"], "This is a summary.");
        assert!(trace["timestamp"].is_string());
        assert_eq!(trace["tool_call"]["name"], "test_tool");
        assert_eq!(trace["result"]["content"], "Success");
    }

    #[tokio::test]
    async fn test_tracing_error_handling() {
        let dir = tempdir().unwrap();
        let trace_file_path = dir.path().join("trace.jsonl");

        let config = TracingConfig {
            enabled: true,
            trace_file: trace_file_path.to_str().unwrap().to_string(),
            model_provider: "default".to_string(),
        };

        let llm = Arc::new(MockLLM {
            response: Err(crate::errors::AgentError::LLMError("LLM is down".to_string())),
        });

        let mut handler = TracingTraceHandler::new(config, llm).unwrap();
        let step = AgentStep {
            step_number: 1,
            thought: Some("I should use a tool".to_string()),
            tool_calls: Some(vec![ToolCall {
                id: Some("1".to_string()),
                name: "test_tool".to_string(),
                arguments: Value::Null,
            }]),
            tool_results: Some(vec![Observation {
                tool_call_id: Some("1".to_string()),
                content: "Success".to_string(),
                success: true,
            }]),
        };

        if let Some(handle) = handler.on_step_complete(&step) {
            handle.await.unwrap();
        }

        let content = fs::read_to_string(trace_file_path).unwrap();
        let trace: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(trace["content"], "No summary available");
    }

    #[tokio::test]
    async fn test_no_summary_for_empty_tools() {
        let dir = tempdir().unwrap();
        let trace_file_path = dir.path().join("trace.jsonl");

        let config = TracingConfig {
            enabled: true,
            trace_file: trace_file_path.to_str().unwrap().to_string(),
            model_provider: "default".to_string(),
        };

        let llm = Arc::new(MockLLM {
            response: Ok(LLMResponse {
                content: Some("This should not be called".to_string()),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            }),
        });

        let mut handler = TracingTraceHandler::new(config, llm).unwrap();
        let step = AgentStep {
            step_number: 1,
            thought: Some("Just a thought, no tools".to_string()),
            tool_calls: None,
            tool_results: None,
        };

        if let Some(handle) = handler.on_step_complete(&step) {
            handle.await.unwrap();
        }

        let content = fs::read_to_string(trace_file_path).unwrap();
        let trace: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(trace["content"], "Just a thought, no tools");
        assert!(trace["tool_call"].is_null());
    }

    #[tokio::test]
    async fn test_tracing_concurrent_writes() {
        let dir = tempdir().unwrap();
        let trace_file_path = dir.path().join("trace.jsonl");

        let config = TracingConfig {
            enabled: true,
            trace_file: trace_file_path.to_str().unwrap().to_string(),
            model_provider: "default".to_string(),
        };

        let llm = Arc::new(MockLLM {
            response: Ok(LLMResponse {
                content: Some("Concurrent summary".to_string()),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            }),
        });

        let handler = Arc::new(Mutex::new(TracingTraceHandler::new(config, llm).unwrap()));
        let mut handles = vec![];

        for i in 0..10 {
            let handler_clone = Arc::clone(&handler);
            let handle = tokio::spawn(async move {
                let step = AgentStep {
                    step_number: i,
                    thought: Some(format!("Thought {}", i)),
                    tool_calls: Some(vec![ToolCall {
                        id: Some(i.to_string()),
                        name: "concurrent_tool".to_string(),
                        arguments: Value::Null,
                    }]),
                    tool_results: Some(vec![Observation {
                        tool_call_id: Some(i.to_string()),
                        content: "Success".to_string(),
                        success: true,
                    }]),
                };
                let mut handler_guard = handler_clone.lock().await;
                if let Some(handle) = handler_guard.on_step_complete(&step) {
                    handle.await.unwrap();
                }
            });
            handles.push(handle);
        }

        futures::future::join_all(handles).await;

        let content = fs::read_to_string(trace_file_path).unwrap();
        let lines: Vec<_> = content.lines().collect();
        assert_eq!(lines.len(), 10);
    }

    #[tokio::test]
    async fn test_thought_logged_when_no_tool_call() {
        let dir = tempdir().unwrap();
        let trace_file_path = dir.path().join("trace.jsonl");

        let config = TracingConfig {
            enabled: true,
            trace_file: trace_file_path.to_str().unwrap().to_string(),
            model_provider: "default".to_string(),
        };

        let llm = Arc::new(MockLLM {
            response: Ok(LLMResponse {
                content: Some("This is a conversational response.".to_string()),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            }),
        });

        let mut handler = TracingTraceHandler::new(config, llm).unwrap();
        let step = AgentStep {
            step_number: 1,
            thought: Some("This is a conversational response.".to_string()),
            tool_calls: None,
            tool_results: None,
        };

        if let Some(handle) = handler.on_step_complete(&step) {
            handle.await.unwrap();
        }

        let content = fs::read_to_string(trace_file_path).unwrap();
        let trace: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(trace["trace_type"], "thought");
        assert_eq!(trace["content"], "This is a conversational response.");
        assert!(trace["tool_call"].is_null());
    }
}