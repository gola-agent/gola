use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::domain::models::AgentName;
use crate::domain::models::AgentPrompt;
use crate::domain::models::Event;

#[async_trait]
pub trait AgentClient: Send + Sync {
    fn name(&self) -> AgentName;
    async fn health_check(&self) -> Result<()>;
    async fn send_prompt(
        &self,
        prompt: AgentPrompt,
        event_tx: &mpsc::UnboundedSender<Event>,
    ) -> Result<()>;
    async fn clear_memory(&self) -> Result<()>;
}

pub type AgentClientBox = Box<dyn AgentClient>;
