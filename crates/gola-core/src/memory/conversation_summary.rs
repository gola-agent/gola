//! Conversation memory using LLM-based progressive summarization.
//!
//! This module addresses the fundamental limitation of fixed context windows by
//! compressing conversation history into evolving summaries. Rather than losing
//! old context entirely, the LLM distills key information into a compact narrative
//! that preserves semantic meaning while drastically reducing token usage.
//!
//! The summarization approach is ideal for long-running conversations where early
//! context remains relevant but verbatim retention is impractical. The tradeoff
//! is potential loss of specific details in favor of maintaining overall coherence.

use std::sync::Arc;
use async_trait::async_trait;
use crate::core_types::{Message, Role};
use crate::errors::AgentError;
use crate::llm::LLM;
use crate::memory::{ConversationMemory, MemoryStats};

const CONVERSATION_SUMMARY_PROMPT: &str = "Concisely summarize the following conversation. The summary should be a single, evolving paragraph that represents the entire conversation so far.\n\n---\n\nPREVIOUS SUMMARY:\n{summary}\n\nNEW LINES:\n{new_lines}\n\n---\n\nNEW SUMMARY:";

pub struct ConversationSummaryMemory {
    llm: Arc<dyn LLM>,
    summary: String,
    messages: Vec<Message>,
}

impl ConversationSummaryMemory {
    pub fn new(llm: Arc<dyn LLM>) -> Self {
        Self {
            llm,
            summary: String::new(),
            messages: Vec::new(),
        }
    }

    async fn predict_new_summary(&self) -> Result<String, AgentError> {
        let new_lines = self.messages.iter()
            .map(|m| format!("{:?}: {}", m.role, m.content))
            .collect::<Vec<String>>()
            .join("\n");

        let prompt = CONVERSATION_SUMMARY_PROMPT
            .replace("{summary}", &self.summary)
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
impl ConversationMemory for ConversationSummaryMemory {
    async fn add_message(&mut self, message: Message) -> Result<(), AgentError> {
        self.messages.push(message);
        self.summary = self.predict_new_summary().await?;
        // After summarizing, we can clear the messages as they are now part of the summary
        self.messages.clear();
        Ok(())
    }

    fn get_context(&self) -> Vec<Message> {
        vec![Message {
            role: Role::System,
            content: self.summary.clone(),
            tool_call_id: None,
            tool_calls: None,
        }]
    }

    fn clear(&mut self) {
        self.summary.clear();
        self.messages.clear();
    }

    fn stats(&self) -> MemoryStats {
        MemoryStats {
            total_steps: self.summary.split_whitespace().count(),
            ..Default::default()
        }
    }
}
