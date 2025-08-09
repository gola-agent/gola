//! Mock LLM implementations for testing

use async_trait::async_trait;
use gola_core::llm::{LLM, ToolMetadata};
use gola_core::core_types::{Message, LLMResponse, ToolCall, Usage};
use gola_core::errors::AgentError;
use std::sync::Arc;
use std::sync::Mutex;

/// A configurable mock LLM for testing
pub struct MockLLM {
    responses: Arc<Mutex<Vec<LLMResponse>>>,
    call_count: Arc<Mutex<usize>>,
    should_fail: bool,
    error_message: String,
}

impl MockLLM {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(vec![
                LLMResponse {
                    content: Some("Mock response".to_string()),
                    tool_calls: None,
                    finish_reason: Some("stop".to_string()),
                    usage: Some(Usage {
                        prompt_tokens: 10,
                        completion_tokens: 20,
                        total_tokens: 30,
                    }),
                }
            ])),
            call_count: Arc::new(Mutex::new(0)),
            should_fail: false,
            error_message: "Mock error".to_string(),
        }
    }

    pub fn with_responses(responses: Vec<LLMResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
            call_count: Arc::new(Mutex::new(0)),
            should_fail: false,
            error_message: "Mock error".to_string(),
        }
    }

    pub fn with_error(error_message: String) -> Self {
        Self {
            responses: Arc::new(Mutex::new(vec![])),
            call_count: Arc::new(Mutex::new(0)),
            should_fail: true,
            error_message,
        }
    }

    pub fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl LLM for MockLLM {
    async fn generate(
        &self,
        _messages: Vec<Message>,
        _tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        
        if self.should_fail {
            return Err(AgentError::LLMError(self.error_message.clone()));
        }

        let responses = self.responses.lock().unwrap();
        let index = (*count - 1) % responses.len();
        Ok(responses[index].clone())
    }
}

/// A mock LLM that simulates streaming responses
pub struct StreamingMockLLM {
    base_mock: MockLLM,
}

impl StreamingMockLLM {
    pub fn new() -> Self {
        Self {
            base_mock: MockLLM::new(),
        }
    }
}

#[async_trait]
impl LLM for StreamingMockLLM {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        // Simulate some delay for streaming
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        self.base_mock.generate(messages, tools).await
    }
}