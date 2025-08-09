use std::time::Duration;

use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::stream::TryStreamExt;
use gola_ag_ui_types::{Event as GolaEvent, Message, RunAgentInput, Tool};
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;
use tokio_util::io::StreamReader;
use uuid::Uuid;

use crate::configuration::{Config, ConfigKey};
use crate::domain::models::{AgentClient, AgentName, AgentPrompt, AgentResponse, Author, Event};

pub struct GolaAgUI {
    url: String,
    timeout: String,
}

impl Default for GolaAgUI {
    fn default() -> GolaAgUI {
        GolaAgUI {
            url: Config::get(ConfigKey::GolaAgUIURL),
            timeout: "1000".to_string(),
        }
    }
}

#[async_trait]
impl AgentClient for GolaAgUI {
    fn name(&self) -> AgentName {
        AgentName::GolaAgUI
    }

    async fn health_check(&self) -> Result<()> {
        if self.url.is_empty() {
            bail!("GolaAgUI URL is not defined");
        }

        let health_url = format!("{}/health", self.url);
        let res = reqwest::Client::new()
            .get(&health_url)
            .timeout(Duration::from_millis(self.timeout.parse::<u64>()?))
            .send()
            .await;

        if res.is_err() {
            tracing::error!(error = ?res.unwrap_err(), "GolaAgUI is not reachable");
            bail!("GolaAgUI is not reachable");
        }

        let status = res.unwrap().status().as_u16();
        if status >= 400 {
            tracing::error!(status = status, "GolaAgUI health check failed");
            bail!("GolaAgUI health check failed");
        }

        Ok(())
    }

    async fn send_prompt(
        &self,
        prompt: AgentPrompt,
        tx: &mpsc::UnboundedSender<Event>,
    ) -> Result<()> {
        // Convert gola-term prompt to GolaAgUI format
        let thread_id = format!(
            "th_{}",
            Uuid::new_v4().to_string().replace('-', "")[..8].to_string()
        );
        let run_id = Uuid::new_v4().to_string();

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

        let request_payload = RunAgentInput {
            thread_id: thread_id.clone(),
            run_id,
            state: Value::Object(serde_json::Map::new()),
            messages,
            tools: Vec::<Tool>::new(), // Empty tools for now
            context: Vec::new(),
            forwarded_props: Value::Object(serde_json::Map::new()),
        };

        let stream_url = format!("{}/stream", self.url);

        let response = reqwest::Client::new()
            .post(&stream_url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&request_payload)
            .send()
            .await?;

        if !response.status().is_success() {
            bail!("Failed to connect to GolaAgUI stream endpoint");
        }

        fn convert_err(err: reqwest::Error) -> std::io::Error {
            let err_msg = err.to_string();
            std::io::Error::new(std::io::ErrorKind::Interrupted, err_msg)
        }

        let stream = response.bytes_stream().map_err(convert_err);
        let mut lines_reader = StreamReader::new(stream).lines();

        let mut current_event_name = "message".to_string();
        let mut current_event_data = String::new();
        let mut assistant_message = String::new();
        let mut updated_messages = request_payload.messages.clone();

        while let Ok(line) = lines_reader.next_line().await {
            if line.is_none() {
                break;
            }

            let line = line.unwrap();
            if line.is_empty() {
                // Process complete SSE event
                if !current_event_data.is_empty() {
                    if let Ok(event) = GolaEvent::from_sse(&current_event_name, &current_event_data)
                    {
                        self.handle_gola_event(event, &mut assistant_message, tx)
                            .await?;
                    }
                }
                current_event_name = "message".to_string();
                current_event_data.clear();
            } else if line.starts_with("event:") {
                current_event_name = line["event:".len()..].trim().to_string();
            } else if line.starts_with("data:") {
                let data_content = line["data:".len()..].trim();
                if !current_event_data.is_empty() {
                    current_event_data.push('\n');
                }
                current_event_data.push_str(data_content);
            }
        }

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

        Ok(())
    }

    async fn clear_memory(&self) -> Result<()> {
        let clear_url = format!("{}/memory/clear", self.url);
        let res = reqwest::Client::new()
            .delete(&clear_url)
            .timeout(Duration::from_millis(self.timeout.parse::<u64>()?))
            .send()
            .await;

        if res.is_err() {
            tracing::error!(error = ?res.unwrap_err(), "GolaAgUI clear memory failed");
            bail!("GolaAgUI clear memory failed");
        }

        let response = res.unwrap();
        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Failed to read error body".to_string());
            tracing::error!(status = status_code, body = %body, "GolaAgUI clear memory failed");
            bail!("GolaAgUI clear memory failed");
        }

        Ok(())
    }
}

impl GolaAgUI {
    async fn handle_gola_event(
        &self,
        event: GolaEvent,
        assistant_message: &mut String,
        tx: &mpsc::UnboundedSender<Event>,
    ) -> Result<()> {
        match event {
            GolaEvent::TextMessageContent(msg_event) => {
                assistant_message.push_str(&msg_event.delta);
                let response = AgentResponse {
                    author: Author::Gola,
                    text: msg_event.delta,
                    done: false,
                    context: None,
                };
                tx.send(Event::AgentPromptResponse(response))?;
            }
            GolaEvent::TextMessageChunk(chunk_event) => {
                if let Some(delta) = &chunk_event.delta {
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
            GolaEvent::RunFinished(_) => {
                // Run completed - handled by caller
            }
            GolaEvent::RunError(error_event) => {
                bail!("GolaAgUI run error: {}", error_event.message);
            }
            _ => {
                // Other events (tool calls, etc.) - ignore for minimal implementation
            }
        }
        Ok(())
    }
}
