//! Hybrid memory combining summarization with recent message buffer.
//!
//! This module provides the best of both worlds: maintaining a compressed summary
//! of older conversations while keeping recent messages verbatim. This hybrid approach
//! addresses the key weakness of pure summarization (loss of recent detail) and pure
//! windowing (loss of all historical context).
//!
//! The strategy is particularly effective for multi-turn problem-solving where both
//! long-term goals and immediate tactical decisions matter. The summary preserves
//! strategic context while the buffer ensures precise recall of recent exchanges,
//! critical for maintaining conversation flow and avoiding repetitive clarifications.

use crate::core_types::{Message, Role};
use crate::errors::AgentError;
use crate::llm::LLM;
use crate::memory::{ConversationMemory, MemoryStats};
use async_trait::async_trait;
use std::sync::Arc;

const SUMMARIZATION_PROMPT: &str = "Progressively summarize the lines of conversation provided, adding onto the previous summary returning a new summary.\n\nEXAMPLE\nCurrent summary:\nThe human asks what the AI thinks of artificial intelligence. The AI thinks artificial intelligence is a force for good.\n\nNew lines of conversation:\nHuman: Why do you think artificial intelligence is a force for good?\nAI: Because artificial intelligence will help humans reach their full potential.\n\nNew summary:\nThe human asks what the AI thinks of artificial intelligence. The AI thinks artificial intelligence is a force for good, because it will help humans reach their full potential.\nEND OF EXAMPLE\n\nCurrent summary:\n{summary}\n\nNew lines of conversation:\n{new_lines}\n\nNew summary:";

pub struct ConversationSummaryBufferMemory {
    llm: Arc<dyn LLM>,
    max_token_limit: usize,
    moving_summary_buffer: String,
    messages: Vec<Message>,
}

impl ConversationSummaryBufferMemory {
    pub fn new(llm: Arc<dyn LLM>, max_token_limit: usize) -> Self {
        Self {
            llm,
            max_token_limit,
            moving_summary_buffer: String::new(),
            messages: Vec::new(),
        }
    }

    pub fn count_tokens(&self, messages: &[Message]) -> Result<usize, AgentError> {
        // Simple token counting based on character length
        // In a real implementation, you'd use a proper tokenizer
        let mut token_count = 0;
        for message in messages {
            // Rough approximation: 4 characters per token
            token_count += message.content.len() / 4;
        }
        Ok(token_count)
    }

    async fn prune(&mut self) -> Result<(), AgentError> {
        // Check if we need to summarize based on message count or token count
        let current_tokens = self.count_tokens(&self.messages)?;
        let should_summarize = current_tokens > self.max_token_limit;
        log::info!("Pruning memory. Messages: {}, Tokens: {}, Should Summarize: {}", self.messages.len(), current_tokens, should_summarize);

        if should_summarize && !self.messages.is_empty() {
            let mut pruned_memory = Vec::new();
            
            // Take half of the messages for summarization
            let messages_to_summarize = self.messages.len() / 2;
            log::info!("Summarizing {} messages", messages_to_summarize);
            for _ in 0..messages_to_summarize {
                if !self.messages.is_empty() {
                    pruned_memory.push(self.messages.remove(0));
                }
            }

            if !pruned_memory.is_empty() {
                let new_summary = self.predict_new_summary(pruned_memory).await?;
                log::info!("New summary: {}", new_summary);
                self.moving_summary_buffer = new_summary;
            }
        }

        Ok(())
    }

    async fn predict_new_summary(&self, messages: Vec<Message>) -> Result<String, AgentError> {
        let new_lines = messages
            .iter()
            .map(|m| format!("{:?}: {}", m.role, m.content))
            .collect::<Vec<String>>()
            .join("\n");

        let prompt = SUMMARIZATION_PROMPT
            .replace("{summary}", &self.moving_summary_buffer)
            .replace("{new_lines}", &new_lines);

        let messages = vec![Message {
            role: Role::System,
            content: prompt,
            tool_call_id: None,
            tool_calls: None,
        }];

        let response = self.llm.generate(messages, None).await?;
        
        Ok(response.content.unwrap_or_default())
    }
}

#[async_trait]
impl ConversationMemory for ConversationSummaryBufferMemory {
    async fn add_message(&mut self, message: Message) -> Result<(), AgentError> {
        self.messages.push(message);
        self.prune().await
    }

    fn get_context(&self) -> Vec<Message> {
        let mut context = Vec::new();
        if !self.moving_summary_buffer.is_empty() {
            context.push(Message {
                role: Role::System,
                content: self.moving_summary_buffer.clone(),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        let mut last_assistant_tool_call_idx = None;
        for (i, msg) in self.messages.iter().enumerate().rev() {
            if msg.role == Role::Assistant && msg.tool_calls.is_some() {
                last_assistant_tool_call_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = last_assistant_tool_call_idx {
            // Add messages before the last tool-calling assistant message, filtering out tool results
            for i in 0..idx {
                if self.messages[i].role != Role::Tool {
                    context.push(self.messages[i].clone());
                }
            }
            // Add all messages from the last tool-calling assistant message onwards
            for i in idx..self.messages.len() {
                context.push(self.messages[i].clone());
            }
        } else {
            // No tool calls yet, so just add all messages
            context.extend(self.messages.clone());
        }
        
        context
    }

    fn clear(&mut self) {
        self.messages.clear();
        self.moving_summary_buffer.clear();
    }

    fn stats(&self) -> MemoryStats {
        MemoryStats {
            total_steps: self.messages.len(),
            ..Default::default()
        }
    }
}
