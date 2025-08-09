use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::TryStreamExt;
use gola_ag_ui_types::Message;
use reqwest::Client;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;
use tokio_util::io::StreamReader;
use uuid::Uuid;

// Import gola-term types
use gola_term::domain::models::{
    AgentClient as TermAgentClient, AgentName, AgentPrompt, AgentResponse, Author, Event,
};

/// HTTP-based AgentClient that connects to a remote gola server
pub struct RemoteGolaTermClient {
    server_url: String,
    http_client: Client,
}

impl RemoteGolaTermClient {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            http_client: Client::new(),
        }
    }

    pub async fn health_check_server(&self) -> Result<()> {
        let health_url = format!("{}/health", self.server_url);
        let response = self.http_client.get(&health_url).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("Server health check failed: {}", response.status())
        }
    }
}

#[async_trait]
impl TermAgentClient for RemoteGolaTermClient {
    fn name(&self) -> AgentName {
        AgentName::GolaAgUI
    }

    async fn health_check(&self) -> Result<()> {
        self.health_check_server().await
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

        // Add the current user message (including "gola-connect-HACK" if that's what was sent)
        messages.push(Message::User {
            id: Uuid::new_v4().to_string(),
            content: prompt.text,
            name: None,
        });

        // Send request to remote server
        let stream_url = format!("{}/agents/stream", self.server_url);
        let thread_id = format!(
            "th_{}",
            Uuid::new_v4().to_string().replace('-', "")[..8].to_string()
        );
        let run_id = Uuid::new_v4().to_string();
        let request_body = json!({
            "threadId": thread_id,
            "runId": run_id,
            "state": {},
            "messages": messages.clone(),
            "tools": [],
            "context": [],
            "forwardedProps": {}
        });

        let response = self
            .http_client
            .post(&stream_url)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Server request failed: {}", response.status());
        }

        // Parse SSE stream and extract message content
        let stream = response
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
        let mut lines_reader = StreamReader::new(stream).lines();

        let mut assistant_message = String::new();
        let mut updated_messages = messages;

        while let Ok(line) = lines_reader.next_line().await {
            if line.is_none() {
                break;
            }

            let line = line.unwrap();
            if line.starts_with("data: ") {
                let data_content = line[6..].trim(); // Remove "data: " prefix
                if let Ok(event_data) = serde_json::from_str::<serde_json::Value>(data_content) {
                    if let Some(event_type) = event_data.get("type").and_then(|t| t.as_str()) {
                        match event_type {
                            "TEXT_MESSAGE_CONTENT" => {
                                if let Some(delta) =
                                    event_data.get("delta").and_then(|d| d.as_str())
                                {
                                    assistant_message.push_str(delta);
                                    let response = AgentResponse {
                                        author: Author::Gola,
                                        text: delta.to_string(),
                                        done: false,
                                        context: None,
                                    };
                                    tx.send(Event::AgentPromptResponse(response))?;
                                }
                            }
                            "RUN_FINISHED" => {
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
                            "RUN_ERROR" => {
                                if let Some(error_msg) =
                                    event_data.get("message").and_then(|m| m.as_str())
                                {
                                    anyhow::bail!("Server error: {}", error_msg);
                                }
                            }
                            _ => {
                                // Ignore other event types for now
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn clear_memory(&self) -> Result<()> {
        let clear_url = format!("{}/agents/clear-memory", self.server_url);
        let response = self.http_client.post(&clear_url).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!("Clear memory request failed: {}", response.status())
        }
    }
}
