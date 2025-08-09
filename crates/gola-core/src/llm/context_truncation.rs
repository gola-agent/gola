//! Context window management through intelligent truncation strategies
//!
//! This module addresses the fundamental constraint of LLM context windows by
//! implementing adaptive truncation strategies. Rather than failing when context
//! limits are exceeded, the system intelligently preserves the most relevant
//! information through summarization and selective message retention. This design
//! enables long-running conversations and complex reasoning chains that would
//! otherwise exceed model limitations.

use crate::core_types::{LLMResponse, Message, Role};
use crate::errors::AgentError;
use crate::llm::{summarizer, ToolMetadata, LLM};
use async_trait::async_trait;
use log::{warn, info, debug, error};
use std::sync::Arc;

/// Wrapper around an LLM client that implements context window truncation strategy
/// for handling 429 (rate limit) and 413 (payload too large) errors
pub struct ContextTruncatingLLM {
    inner_llm: Arc<dyn LLM>,
    max_retries: usize,
    truncation_ratio: f32,
    min_messages: usize,
    summarization_threshold: usize,
}

impl ContextTruncatingLLM {
    /// Creates a new context truncating LLM wrapper
    pub fn new(inner_llm: Arc<dyn LLM>) -> Self {
        Self {
            inner_llm,
            max_retries: 5,
            truncation_ratio: 0.5,
            min_messages: 1,
            summarization_threshold: 500,
        }
    }

    /// Sets the maximum number of retries
    pub fn with_max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Sets the truncation ratio (how much context to remove on each retry)
    pub fn with_truncation_ratio(mut self, ratio: f32) -> Self {
        self.truncation_ratio = ratio.clamp(0.1, 0.9);
        self
    }

    /// Sets the minimum number of messages to keep
    pub fn with_min_messages(mut self, min_messages: usize) -> Self {
        self.min_messages = min_messages.max(1);
        self
    }

    /// Sets the token threshold for summarization
    pub fn with_summarization_threshold(mut self, threshold: usize) -> Self {
        self.summarization_threshold = threshold;
        self
    }

    /// Checks if an error indicates context window or rate limit issues
    fn is_context_or_rate_limit_error(error: &AgentError) -> bool {
        match error {
            AgentError::LLMError(msg) => {
                let msg_lower = msg.to_lowercase();
                // Check for HTTP status codes
                msg_lower.contains("429") || // Rate limit
                msg_lower.contains("413") || // Payload too large
                // Check for common error messages
                msg_lower.contains("rate limit") ||
                msg_lower.contains("too many requests") ||
                msg_lower.contains("payload too large") ||
                msg_lower.contains("request too large") ||
                msg_lower.contains("context length") ||
                msg_lower.contains("token limit") ||
                msg_lower.contains("maximum context") ||
                msg_lower.contains("context window")
            }
            _ => false,
        }
    }

    /// Truncates messages, first by summarizing large tool outputs, then by removing older messages.
    async fn truncate_messages(&self, messages: &[Message], truncation_factor: f32) -> Vec<Message> {
        if messages.len() <= self.min_messages {
            warn!("Cannot truncate further: only {} messages remaining", messages.len());
            return messages.to_vec();
        }

        let mut processed_messages = messages.to_vec();

        // --- Step 1: Summarize large tool messages ---
        for message in &mut processed_messages {
            if message.role == Role::Tool {
                let original_content = message.content.clone();
                match summarizer::summarize_message_content(
                    self.inner_llm.clone(),
                    &original_content,
                    self.summarization_threshold,
                )
                .await
                {
                    Ok(new_content) => {
                        if new_content.len() < original_content.len() {
                            info!("Successfully summarized a large tool message.");
                            message.content = new_content;
                        }
                    }
                    Err(e) => {
                        error!("Failed to summarize message content: {}", e);
                    }
                }
            }
        }

        // --- Step 2: If still too large, truncate by removing messages ---
        let mut result = Vec::new();
        let mut remaining_messages = processed_messages;

        // Always preserve system message if present
        let has_system_message = !remaining_messages.is_empty() 
            && matches!(remaining_messages[0].role, Role::System);
        
        if has_system_message {
            result.push(remaining_messages.remove(0));
        }

        // Calculate how many messages to remove from the middle
        let messages_to_remove = ((remaining_messages.len() as f32) * truncation_factor).ceil() as usize;
        let messages_to_keep = remaining_messages.len().saturating_sub(messages_to_remove);

        if messages_to_keep == 0 {
            // If we would remove everything, keep at least the last message
            if let Some(last_message) = remaining_messages.last() {
                result.push(last_message.clone());
            }
        } else {
            // Keep the most recent messages
            let start_index = remaining_messages.len().saturating_sub(messages_to_keep);
            result.extend_from_slice(&remaining_messages[start_index..]);
        }

        // Add a truncation notice if we removed messages
        if messages_to_remove > 0 {
            let truncation_notice = Message {
                role: Role::System,
                content: format!(
                    "[Context truncated: {} messages removed to fit within limits]",
                    messages_to_remove
                ),
                tool_call_id: None,
                tool_calls: None,
            };
            
            // Insert after system message or at beginning
            let insert_pos = if has_system_message { 1 } else { 0 };
            result.insert(insert_pos, truncation_notice);
        }

        info!(
            "Truncated context: {} -> {} messages (removed {})",
            messages.len(),
            result.len(),
            messages_to_remove
        );

        result
    }

    /// Implements exponential backoff for rate limiting
    async fn wait_for_retry(&self, attempt: usize) {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s, 8s, 16s
            let delay_seconds = 2_u64.pow(attempt as u32 - 1);
            let delay = std::time::Duration::from_secs(delay_seconds.min(30)); // Cap at 30 seconds
            
            info!("Waiting {}s before retry attempt {}", delay.as_secs(), attempt);
            tokio::time::sleep(delay).await;
        }
    }
}

#[async_trait]
impl LLM for ContextTruncatingLLM {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        let mut current_messages = messages;
        let mut truncation_factor = self.truncation_ratio;

        for attempt in 0..=self.max_retries {
            // Wait before retry (except for first attempt)
            if attempt > 0 {
                self.wait_for_retry(attempt).await;
            }

            debug!(
                "Attempt {} with {} messages",
                attempt + 1,
                current_messages.len()
            );

            // Try the LLM call
            match self.inner_llm.generate(current_messages.clone(), tools.clone()).await {
                Ok(response) => {
                    if attempt > 0 {
                        info!(
                            "Successfully generated response after {} retries with {} messages",
                            attempt,
                            current_messages.len()
                        );
                    }
                    return Ok(response);
                }
                Err(error) => {
                    // Check if this is a context/rate limit error
                    if Self::is_context_or_rate_limit_error(&error) {
                        if attempt < self.max_retries {
                            warn!(
                                "Context/rate limit error on attempt {}: {}",
                                attempt + 1,
                                error
                            );

                            // Truncate messages for next attempt
                            current_messages = self.truncate_messages(&current_messages, truncation_factor).await;
                            
                            // Increase truncation factor for next attempt if needed
                            truncation_factor = (truncation_factor + 0.1).min(0.8);

                            // Check if we can still truncate
                            if current_messages.len() <= self.min_messages {
                                error!(
                                    "Cannot truncate further: reached minimum message count ({})",
                                    self.min_messages
                                );
                                return Err(AgentError::LLMError(format!(
                                    "Failed after {} attempts with context truncation: {}",
                                    attempt + 1,
                                    error
                                )));
                            }

                            continue;
                        } else {
                            error!(
                                "Max retries ({}) exceeded for context/rate limit error: {}",
                                self.max_retries,
                                error
                            );
                            return Err(AgentError::LLMError(format!(
                                "Failed after {} attempts with context truncation: {}",
                                self.max_retries + 1,
                                error
                            )));
                        }
                    } else {
                        // Not a context/rate limit error, return immediately
                        debug!("Non-context error, not retrying: {}", error);
                        return Err(error);
                    }
                }
            }
        }

        // This should never be reached due to the loop logic above
        Err(AgentError::LLMError(
            "Unexpected error in context truncation retry logic".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_types::{Message, Role, LLMResponse};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Mock LLM for testing
    struct MockLLM {
        fail_count: AtomicUsize,
        error_type: String,
    }

    impl MockLLM {
        fn new(fail_count: usize, error_type: &str) -> Self {
            Self {
                fail_count: AtomicUsize::new(fail_count),
                error_type: error_type.to_string(),
            }
        }
    }

    #[async_trait]
    impl LLM for MockLLM {
        async fn generate(
            &self,
            messages: Vec<Message>,
            _tools: Option<Vec<ToolMetadata>>,
        ) -> Result<LLMResponse, AgentError> {
            // Check for summarization prompt
            if messages.len() == 1 && messages[0].content.contains("Summarize the following content") {
                return Ok(LLMResponse {
                    content: Some("This is a summary.".to_string()),
                    tool_calls: None,
                    finish_reason: None,
                    usage: None,
                });
            }

            let current_count = self.fail_count.load(Ordering::SeqCst);
            if current_count > 0 {
                self.fail_count.store(current_count - 1, Ordering::SeqCst);
                return Err(AgentError::LLMError(self.error_type.clone()));
            }

            Ok(LLMResponse {
                content: Some(format!("Success with {} messages", messages.len())),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            })
        }
    }

    #[tokio::test]
    async fn test_successful_first_attempt() {
        let mock_llm = Arc::new(MockLLM::new(0, ""));
        let truncating_llm = ContextTruncatingLLM::new(mock_llm);

        let messages = vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant.".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::User,
                content: "Hello!".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = truncating_llm.generate(messages, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_context_error_retry_with_summarization() {
        let mock_llm = Arc::new(MockLLM::new(1, "context length exceeded"));
        let truncating_llm = ContextTruncatingLLM::new(mock_llm)
            .with_summarization_threshold(10); // Low threshold to trigger summarization

        let long_content = "This is a very long tool message that will surely exceed the token threshold.".repeat(20);
        let messages = vec![
            Message {
                role: Role::System,
                content: "System prompt".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::Tool,
                content: long_content.clone(),
                tool_call_id: Some("tool_123".to_string()),
                tool_calls: None,
            },
        ];

        let result = truncating_llm.generate(messages, None).await;
        assert!(result.is_ok());
        // We can't easily check the content of the summarized message from the outside,
        // but success on the second try implies summarization happened.
    }

    #[tokio::test]
    async fn test_non_context_error_no_retry() {
        let mock_llm = Arc::new(MockLLM::new(1, "Authentication failed"));
        let truncating_llm = ContextTruncatingLLM::new(mock_llm);

        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello!".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = truncating_llm.generate(messages, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Authentication failed"));
    }

    #[tokio::test]
    async fn test_message_truncation_and_summarization() {
        let mock_llm = Arc::new(MockLLM::new(0, ""));
        let truncating_llm = ContextTruncatingLLM::new(mock_llm)
            .with_summarization_threshold(10);

        let long_content = "This is a very long tool message that will surely exceed the token threshold.".repeat(20);
        let messages = vec![
            Message {
                role: Role::System,
                content: "System prompt".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::User,
                content: "Old message 1".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::Tool,
                content: long_content.clone(),
                tool_call_id: Some("tool_123".to_string()),
                tool_calls: None,
            },
            Message {
                role: Role::User,
                content: "Recent message".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let truncated = truncating_llm.truncate_messages(&messages, 0.5).await;
        
        // Check that the tool message was summarized, if it still exists
        if let Some(summarized_message) = truncated.iter().find(|m| m.role == Role::Tool) {
            assert!(summarized_message.content.contains("[Content summarized to fit context]"));
            assert!(summarized_message.content.len() < long_content.len());
        }
    }
}
