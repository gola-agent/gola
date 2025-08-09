use anyhow::Result;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::Stream;
use futures_util::StreamExt;
use gola_ag_ui_server::AgentHandler;
use std::pin::Pin;
use std::sync::Arc;

use crate::{AgentClient, StreamEvent, StreamRequest};

pub struct DirectAgentClient<T: AgentHandler> {
    agent_handler: Arc<T>,
}

impl<T: AgentHandler> DirectAgentClient<T> {
    pub fn new(agent_handler: Arc<T>) -> Self {
        Self { agent_handler }
    }
}

#[async_trait]
impl<T: AgentHandler> AgentClient for DirectAgentClient<T> {
    async fn stream_request(
        &self,
        request: StreamRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let agent_handler = self.agent_handler.clone();
        let run_input = request.to_run_agent_input();

        let event_stream = try_stream! {
            let mut agent_stream = agent_handler.handle_input(run_input).await
                .map_err(|e| anyhow::anyhow!("Agent handler error: {}", e))?;

            while let Some(event) = agent_stream.next().await {
                yield StreamEvent::from_gola_event(event);
            }
        };

        Ok(Box::pin(event_stream))
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    async fn clear_memory(&self) -> Result<()> {
        self.agent_handler
            .clear_memory()
            .await
            .map_err(|e| anyhow::anyhow!("Clear memory error: {}", e))?;
        Ok(())
    }
}
