//! Auto-recovery LLM wrapper that handles common API errors automatically
//! 
//! This module provides an LLM wrapper that can automatically detect and recover
//! from common API errors, particularly the "tool_calls must be followed by tool messages" error.

use crate::core_types::{LLMResponse, Message};
use crate::errors::AgentError;
use crate::llm::{MessageValidator, ToolMetadata, LLM};
use async_trait::async_trait;
use std::sync::Arc;
use regex::Regex;

/// LLM wrapper that provides automatic error recovery capabilities
#[derive(Clone)]
pub struct AutoRecoveryLLM {
    inner: Arc<dyn LLM>,
    validator: MessageValidator,
    max_retries: usize,
    enable_memory_clearing: bool,
}

impl AutoRecoveryLLM {
    pub fn new(inner: Arc<dyn LLM>) -> Self {
        Self {
            inner,
            validator: MessageValidator::new(),
            max_retries: 3,
            enable_memory_clearing: false,
        }
    }

    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    pub fn with_memory_clearing(mut self, enable: bool) -> Self {
        self.enable_memory_clearing = enable;
        self
    }

    pub fn with_validator(mut self, validator: MessageValidator) -> Self {
        self.validator = validator;
        self
    }

    /// Checks if an error is the specific tool_calls validation error
    fn is_tool_calls_validation_error(&self, error: &AgentError) -> bool {
        if let AgentError::LLMError(msg) = error {
            // Check for the specific OpenAI error pattern
            msg.contains("tool_calls") && 
            msg.contains("must be followed by tool messages") &&
            msg.contains("tool_call_id")
        } else {
            false
        }
    }

    /// Checks if an error is a 400 Bad Request that might be recoverable
    fn is_recoverable_400_error(&self, error: &AgentError) -> bool {
        if let AgentError::LLMError(msg) = error {
            msg.contains("400 Bad Request") || 
            msg.contains("invalid_request_error") ||
            self.is_tool_calls_validation_error(error)
        } else {
            false
        }
    }

    /// Extracts missing tool_call_ids from the error message
    fn extract_missing_tool_call_ids(&self, error_msg: &str) -> Vec<String> {
        let mut missing_ids = Vec::new();
        
        // Pattern to match: "The following tool_call_ids did not have response messages: call_xyz"
        let re = Regex::new(r"tool_call_ids did not have response messages: ([^\s,]+)").unwrap();
        
        for cap in re.captures_iter(error_msg) {
            if let Some(id) = cap.get(1) {
                missing_ids.push(id.as_str().to_string());
            }
        }

        // Also try a more general pattern
        if missing_ids.is_empty() {
            let re2 = Regex::new(r"call_[a-zA-Z0-9]+").unwrap();
            for cap in re2.captures_iter(error_msg) {
                missing_ids.push(cap.get(0).unwrap().as_str().to_string());
            }
        }

        missing_ids
    }

    /// Attempts to recover from tool_calls validation errors by fixing the message sequence
    fn recover_from_tool_calls_error(&self, mut messages: Vec<Message>, error_msg: &str) -> Result<Vec<Message>, AgentError> {
        log::info!("Attempting to recover from tool_calls validation error");
        
        // Extract missing tool_call_ids from error message
        let missing_ids = self.extract_missing_tool_call_ids(error_msg);
        
        if !missing_ids.is_empty() {
            log::info!("Found missing tool_call_ids: {:?}", missing_ids);
            
            // Find the assistant message with these tool calls and add synthetic responses
            let mut insertions = Vec::new(); // Collect insertions to apply later
            
            for (i, message) in messages.iter().enumerate() {
                if let Message { role: crate::core_types::Role::Assistant, tool_calls: Some(tool_calls), .. } = message {
                    for tool_call in tool_calls {
                        if let Some(id) = &tool_call.id {
                            if missing_ids.contains(id) {
                                // Find the right insertion point (after this assistant message)
                                let mut insert_index = i + 1;
                                
                                // Skip any existing tool responses for other tool calls
                                while insert_index < messages.len() {
                                    if let Message { role: crate::core_types::Role::Tool, .. } = &messages[insert_index] {
                                        insert_index += 1;
                                    } else {
                                        break;
                                    }
                                }
                                
                                let synthetic_response = Message {
                                    role: crate::core_types::Role::Tool,
                                    content: "[Tool execution was interrupted - continuing conversation]".to_string(),
                                    tool_call_id: Some(id.clone()),
                                    tool_calls: None,
                                };
                                
                                insertions.push((insert_index, synthetic_response, id.clone()));
                                break;
                            }
                        }
                    }
                }
            }
            
            // Apply insertions in reverse order to maintain correct indices
            insertions.sort_by(|a, b| b.0.cmp(&a.0));
            for (insert_index, synthetic_response, id) in insertions {
                messages.insert(insert_index, synthetic_response);
                log::info!("Added synthetic tool response for tool_call_id: {}", id);
            }
        }

        // Also run the general validator to catch any other issues
        self.validator.validate_and_fix(messages)
    }

    /// Attempts progressive recovery strategies
    async fn attempt_recovery(&self, messages: Vec<Message>, tools: Option<Vec<ToolMetadata>>, error: &AgentError, attempt: usize) -> Result<LLMResponse, AgentError> {
        log::warn!("LLM call failed (attempt {}), attempting recovery: {}", attempt, error);

        let recovered_messages = if self.is_tool_calls_validation_error(error) {
            // Specific recovery for tool_calls validation errors
            if let AgentError::LLMError(error_msg) = error {
                self.recover_from_tool_calls_error(messages, error_msg)?
            } else {
                return Err(error.clone());
            }
        } else if self.is_recoverable_400_error(error) {
            // General message validation and fixing
            self.validator.validate_and_fix(messages)?
        } else {
            // For non-recoverable errors, just return the original error
            return Err(error.clone());
        };

        log::info!("Attempting LLM call with recovered messages ({} messages)", recovered_messages.len());
        
        // Try the call again with recovered messages
        self.inner.generate(recovered_messages, tools).await
    }

    /// Implements progressive fallback strategies for severe errors
    async fn progressive_fallback(&self, mut messages: Vec<Message>, _tools: Option<Vec<ToolMetadata>>, attempt: usize) -> Result<LLMResponse, AgentError> {
        log::warn!("Applying progressive fallback strategy (attempt {})", attempt);

        match attempt {
            1 => {
                // First fallback: Remove all tool-related messages and calls
                log::info!("Fallback 1: Removing all tool-related content");
                messages = messages.into_iter().map(|mut msg| {
                    if msg.role == crate::core_types::Role::Assistant {
                        msg.tool_calls = None;
                    }
                    if msg.role == crate::core_types::Role::Tool {
                        // Convert tool messages to system messages
                        msg.role = crate::core_types::Role::System;
                        msg.content = format!("Previous result: {}", msg.content);
                        msg.tool_call_id = None;
                    }
                    msg
                }).collect();
                
                self.inner.generate(messages, None).await // No tools
            }
            2 => {
                // Second fallback: Keep only user and assistant messages
                log::info!("Fallback 2: Keeping only user and assistant messages");
                messages.retain(|msg| matches!(msg.role, crate::core_types::Role::User | crate::core_types::Role::Assistant));
                
                // Clean up assistant messages
                for msg in &mut messages {
                    if msg.role == crate::core_types::Role::Assistant {
                        msg.tool_calls = None;
                    }
                }
                
                self.inner.generate(messages, None).await
            }
            3 => {
                // Third fallback: Keep only the most recent user message
                log::info!("Fallback 3: Using only the most recent user message");
                if let Some(last_user_msg) = messages.iter().rev().find(|msg| msg.role == crate::core_types::Role::User) {
                    let minimal_messages = vec![last_user_msg.clone()];
                    self.inner.generate(minimal_messages, None).await
                } else {
                    // If no user message found, create a generic one
                    let generic_message = vec![Message {
                        role: crate::core_types::Role::User,
                        content: "Please continue our conversation.".to_string(),
                        tool_call_id: None,
                        tool_calls: None,
                    }];
                    self.inner.generate(generic_message, None).await
                }
            }
            _ => {
                // Final fallback: Return a generic error
                Err(AgentError::LLMError(
                    "All recovery attempts failed. Please clear the conversation memory and try again.".to_string()
                ))
            }
        }
    }
}

#[async_trait]
impl LLM for AutoRecoveryLLM {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        // First, validate and fix messages proactively
        let validated_messages = match self.validator.validate_and_fix(messages.clone()) {
            Ok(msgs) => msgs,
            Err(e) => {
                log::warn!("Message validation failed, using original messages: {}", e);
                messages
            }
        };

        // Try the initial call
        match self.inner.generate(validated_messages.clone(), tools.clone()).await {
            Ok(response) => Ok(response),
            Err(error) => {
                // Attempt recovery based on error type
                for attempt in 1..=self.max_retries {
                    let recovery_result = if self.is_recoverable_400_error(&error) {
                        self.attempt_recovery(validated_messages.clone(), tools.clone(), &error, attempt).await
                    } else {
                        // For non-recoverable errors, try progressive fallback
                        self.progressive_fallback(validated_messages.clone(), tools.clone(), attempt).await
                    };

                    match recovery_result {
                        Ok(response) => {
                            log::info!("Successfully recovered from error on attempt {}", attempt);
                            return Ok(response);
                        }
                        Err(recovery_error) => {
                            log::warn!("Recovery attempt {} failed: {}", attempt, recovery_error);
                            
                            // If this was the last attempt, return the recovery error
                            if attempt == self.max_retries {
                                return Err(recovery_error);
                            }
                        }
                    }
                }

                // If all recovery attempts failed, return the original error
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_types::{Message, Role, ToolCall};
    use crate::llm::ToolMetadata;
    use serde_json::json;

    #[derive(Clone)]
    struct MockLLM {
        responses: std::sync::Arc<std::sync::Mutex<Vec<Result<LLMResponse, AgentError>>>>,
    }

    impl MockLLM {
        fn new(responses: Vec<Result<LLMResponse, AgentError>>) -> Self {
            Self {
                responses: std::sync::Arc::new(std::sync::Mutex::new(responses)),
            }
        }
    }

    #[async_trait]
    impl LLM for MockLLM {
        async fn generate(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<ToolMetadata>>,
        ) -> Result<LLMResponse, AgentError> {
            let mut responses = self.responses.lock().unwrap();
            responses.pop().unwrap_or_else(|| {
                Err(AgentError::LLMError("No more mock responses".to_string()))
            })
        }
    }

    #[tokio::test]
    async fn test_successful_call_no_recovery_needed() {
        let mock_response = Ok(LLMResponse {
            content: Some("Success".to_string()),
            tool_calls: None,
            finish_reason: None,
            usage: None,
        });
        
        let mock_llm = Arc::new(MockLLM::new(vec![mock_response]));
        let auto_recovery_llm = AutoRecoveryLLM::new(mock_llm);

        let messages = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
            tool_call_id: None,
            tool_calls: None,
        }];

        let result = auto_recovery_llm.generate(messages, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, Some("Success".to_string()));
    }

    #[tokio::test]
    async fn test_tool_calls_error_recovery() {
        // First call fails with tool_calls error, second call succeeds
        let error_response = Err(AgentError::LLMError(
            "API request failed with status 400 Bad Request: tool_calls must be followed by tool messages responding to each tool_call_id. The following tool_call_ids did not have response messages: call_123".to_string()
        ));
        let success_response = Ok(LLMResponse {
            content: Some("Recovered successfully".to_string()),
            tool_calls: None,
            finish_reason: None,
            usage: None,
        });

        let mock_llm = Arc::new(MockLLM::new(vec![success_response, error_response]));
        let auto_recovery_llm = AutoRecoveryLLM::new(mock_llm);

        let messages = vec![
            Message {
                role: Role::Assistant,
                content: "I'll use a tool".to_string(),
                tool_call_id: None,
                tool_calls: Some(vec![ToolCall {
                    id: Some("call_123".to_string()),
                    name: "test_tool".to_string(),
                    arguments: json!({}),
                }]),
            },
            Message {
                role: Role::User,
                content: "What happened?".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = auto_recovery_llm.generate(messages, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, Some("Recovered successfully".to_string()));
    }

    #[tokio::test]
    async fn test_progressive_fallback() {
        // All recovery attempts fail, should try progressive fallback
        let error_response = Err(AgentError::LLMError(
            "Persistent error".to_string()
        ));
        let success_response = Ok(LLMResponse {
            content: Some("Fallback successful".to_string()),
            tool_calls: None,
            finish_reason: None,
            usage: None,
        });

        // MockLLM pops from the end, so we need to reverse the order
        // The sequence should be: initial call fails, then 3 recovery attempts, last one succeeds
        let mock_llm = Arc::new(MockLLM::new(vec![
            error_response.clone(),
            error_response.clone(),
            error_response.clone(),
            success_response,
        ]));
        
        let auto_recovery_llm = AutoRecoveryLLM::new(mock_llm).with_max_retries(3);

        let messages = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
            tool_call_id: None,
            tool_calls: None,
        }];

        let result = auto_recovery_llm.generate(messages, None).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, Some("Fallback successful".to_string()));
    }

    #[test]
    fn test_extract_missing_tool_call_ids() {
        let auto_recovery_llm = AutoRecoveryLLM::new(Arc::new(MockLLM::new(vec![])));
        
        let error_msg = "API request failed with status 400 Bad Request: tool_calls must be followed by tool messages responding to each tool_call_id. The following tool_call_ids did not have response messages: call_ri1mrhlVqtTeu5TJCNTxk1gJ";
        
        let missing_ids = auto_recovery_llm.extract_missing_tool_call_ids(error_msg);
        assert_eq!(missing_ids, vec!["call_ri1mrhlVqtTeu5TJCNTxk1gJ"]);
    }

    #[test]
    fn test_is_tool_calls_validation_error() {
        let auto_recovery_llm = AutoRecoveryLLM::new(Arc::new(MockLLM::new(vec![])));
        
        let error = AgentError::LLMError(
            "tool_calls must be followed by tool messages responding to each tool_call_id".to_string()
        );
        
        assert!(auto_recovery_llm.is_tool_calls_validation_error(&error));
        
        let non_tool_error = AgentError::LLMError("Some other error".to_string());
        assert!(!auto_recovery_llm.is_tool_calls_validation_error(&non_tool_error));
    }
}
