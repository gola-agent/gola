//! Mock Agent implementations for testing

use async_trait::async_trait;
use gola_ag_ui_types::{AgentHandler, Event, Message, RunAgentInput};
use anyhow::Result;
use tokio::sync::mpsc;

/// Mock agent handler for testing
pub struct MockAgent {
    response_message: String,
    should_fail: bool,
}

impl MockAgent {
    pub fn new() -> Self {
        Self {
            response_message: "Mock agent response".to_string(),
            should_fail: false,
        }
    }

    pub fn with_response(response: String) -> Self {
        Self {
            response_message: response,
            should_fail: false,
        }
    }

    pub fn with_error() -> Self {
        Self {
            response_message: String::new(),
            should_fail: true,
        }
    }
}

#[async_trait]
impl AgentHandler for MockAgent {
    async fn handle_run_agent(
        &self,
        input: RunAgentInput,
        tx: mpsc::UnboundedSender<Result<Event>>,
    ) -> Result<()> {
        if self.should_fail {
            tx.send(Err(anyhow::anyhow!("Mock agent error")))?;
            return Ok(());
        }

        // Send a mock response
        let message = Message {
            role: gola_ag_ui_types::Role::Assistant,
            content: self.response_message.clone(),
        };
        
        tx.send(Ok(Event::TextMessageStart(Default::default())))?;
        tx.send(Ok(Event::TextMessageContent(gola_ag_ui_types::TextMessageContent {
            delta: self.response_message.clone(),
        })))?;
        tx.send(Ok(Event::TextMessageEnd(Default::default())))?;
        tx.send(Ok(Event::RunComplete(Default::default())))?;

        Ok(())
    }
}