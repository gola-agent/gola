//! Conversation memory management with multiple retention strategies.
//!
//! Provides various memory implementations for maintaining conversation context
//! while balancing token limits, relevance, and computational efficiency. Supports
//! sliding windows, summarization, and hybrid approaches for optimal context retention.

pub mod agent;
pub mod conversation_summary;
pub mod sliding_window;
pub mod summary_buffer;

use crate::core_types::{Message};
use crate::errors::AgentError;
use async_trait::async_trait;
pub use agent::AgentMemory;
pub use conversation_summary::ConversationSummaryMemory;
pub use sliding_window::SlidingWindowMemory;
pub use summary_buffer::ConversationSummaryBufferMemory;


#[async_trait]
pub trait ConversationMemory: Send + Sync {
    async fn add_message(&mut self, message: Message) -> Result<(), AgentError>;
    fn get_context(&self) -> Vec<Message>;
    fn clear(&mut self);
    fn stats(&self) -> MemoryStats {
        MemoryStats::default()
    }
}

#[derive(Debug, Default)]
pub struct MemoryStats {
    pub total_steps: usize,
    pub user_tasks: usize,
    pub thoughts: usize,
    pub actions: usize,
    pub observations: usize,
    pub errors: usize,
    pub successful_observations: usize,
    pub failed_observations: usize,
    pub message_count: usize,
    pub token_count: usize,
}

impl MemoryStats {
    pub fn utilization_percentage(&self, max_steps: usize) -> f64 {
        if max_steps == 0 {
            return 0.0;
        }
        (self.total_steps as f64 / max_steps as f64) * 100.0
    }
}


