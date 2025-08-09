//! Fixed-size sliding window memory for recent context retention.
//!
//! This module implements the simplest yet most predictable memory strategy: keeping
//! only the N most recent messages. While this approach may lose important early
//! context, it guarantees consistent token usage and zero computational overhead.
//!
//! The sliding window is optimal for scenarios where recency matters more than
//! completeness - customer service, real-time assistance, or any interaction where
//! the immediate context dominates relevance. The FIFO eviction ensures no message
//! permanently consumes memory budget regardless of its perceived importance.

use crate::core_types::Message;
use crate::errors::AgentError;
use crate::memory::{ConversationMemory, MemoryStats};
use async_trait::async_trait;
use std::collections::VecDeque;

pub struct SlidingWindowMemory {
    messages: VecDeque<Message>,
    max_messages: usize,
}

impl SlidingWindowMemory {
    pub fn new(max_messages: usize) -> Self {
        Self {
            messages: VecDeque::with_capacity(max_messages),
            max_messages,
        }
    }
}

#[async_trait]
impl ConversationMemory for SlidingWindowMemory {
    async fn add_message(&mut self, message: Message) -> Result<(), AgentError> {
        if self.messages.len() >= self.max_messages {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
        Ok(())
    }

    fn get_context(&self) -> Vec<Message> {
        let mut last_assistant_tool_call_idx = None;
        for (i, msg) in self.messages.iter().enumerate().rev() {
            if msg.role == crate::core_types::Role::Assistant && msg.tool_calls.is_some() {
                last_assistant_tool_call_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = last_assistant_tool_call_idx {
            let mut context = Vec::new();
            // Add messages before the last tool-calling assistant message, filtering out tool results
            for i in 0..idx {
                if self.messages[i].role != crate::core_types::Role::Tool {
                    context.push(self.messages[i].clone());
                }
            }
            // Add all messages from the last tool-calling assistant message onwards
            for i in idx..self.messages.len() {
                context.push(self.messages[i].clone());
            }
            context
        } else {
            // No tool calls yet, so just return everything
            self.messages.iter().cloned().collect()
        }
    }

    fn clear(&mut self) {
        self.messages.clear();
    }

    fn stats(&self) -> MemoryStats {
        MemoryStats {
            total_steps: self.messages.len(),
            ..Default::default()
        }
    }
}
