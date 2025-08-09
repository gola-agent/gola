use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use gola_ag_ui_types::Message;
use gola_agent_client::{AgentClient as DirectAgentClient, StreamEvent, StreamRequest};
use tokio::sync::mpsc;
use uuid::Uuid;

// Import gola-term types
use gola_term::domain::models::{
    AgentClient as TermAgentClient, AgentName, AgentPrompt, AgentResponse, Author, Event,
};

/// Channel-based AgentClient that bridges direct agent handler to gola-term interface
pub struct DirectGolaTermClient {
    direct_client: Box<dyn DirectAgentClient>,
}

impl DirectGolaTermClient {
    pub fn new(direct_client: Box<dyn DirectAgentClient>) -> Self {
        Self { direct_client }
    }
}

#[async_trait]
impl TermAgentClient for DirectGolaTermClient {
    fn name(&self) -> AgentName {
        AgentName::GolaAgUI
    }

    async fn health_check(&self) -> Result<()> {
        self.direct_client.health_check().await
    }

    async fn send_prompt(
        &self,
        prompt: AgentPrompt,
        tx: &mpsc::UnboundedSender<Event>,
    ) -> Result<()> {
        // Let all messages, including "gola-connect-HACK", go through to the actual agent
        // This ensures we get proper icebreaker messages from the agent handler

        // Parse existing context or create new message history
        let mut messages: Vec<Message> = vec![];
        if !prompt.agent_context.is_empty() {
            // Try to parse existing context
            if let Ok(context_messages) =
                serde_json::from_str::<Vec<Message>>(&prompt.agent_context)
            {
                messages = context_messages;
            }
        }

        // Add the current user message
        messages.push(Message::User {
            id: Uuid::new_v4().to_string(),
            content: prompt.text,
            name: None,
        });

        // Create stream request using direct client interface
        let request = StreamRequest::new(messages.clone());
        let mut stream = self.direct_client.stream_request(request).await?;

        let mut assistant_message = String::new();
        let mut updated_messages = messages;

        // Process streaming events and convert to gola-term format
        while let Some(event) = stream.next().await {
            match event? {
                StreamEvent::TextDelta(text) => {
                    assistant_message.push_str(&text);

                    // Send as gola-term AgentResponse
                    let response = AgentResponse {
                        author: Author::Gola,
                        text,
                        done: false,
                        context: None,
                    };
                    tx.send(Event::AgentPromptResponse(response))?;
                }
                StreamEvent::ToolCall(tool_name) => {
                    let tool_text = format!("[using tool: {}]", tool_name);
                    assistant_message.push_str(&tool_text);

                    let response = AgentResponse {
                        author: Author::Gola,
                        text: tool_text,
                        done: false,
                        context: None,
                    };
                    tx.send(Event::AgentPromptResponse(response))?;
                }
                StreamEvent::RunFinished => {
                    // Add assistant response to message history
                    if !assistant_message.is_empty() {
                        updated_messages.push(Message::Assistant {
                            id: Uuid::new_v4().to_string(),
                            content: Some(assistant_message),
                            name: None,
                            tool_calls: None,
                        });
                    }

                    // Send final completion with updated context
                    let final_response = AgentResponse {
                        author: Author::Gola,
                        text: String::new(),
                        done: true,
                        context: Some(serde_json::to_string(&updated_messages)?),
                    };
                    tx.send(Event::AgentPromptResponse(final_response))?;
                    break;
                }
                StreamEvent::RunError(error) => {
                    anyhow::bail!("Agent run error: {}", error);
                }
                StreamEvent::Other(_) => {
                    // Ignore other events for now
                }
            }
        }

        Ok(())
    }

    async fn clear_memory(&self) -> Result<()> {
        self.direct_client.clear_memory().await
    }
}
