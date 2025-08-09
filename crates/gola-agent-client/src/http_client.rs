use anyhow::{bail, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use futures::Stream;
use futures_util::stream::TryStreamExt;
use gola_ag_ui_types::Event as GolaEvent;
use std::pin::Pin;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio_util::io::StreamReader;

use crate::{AgentClient, StreamEvent, StreamRequest};

/// HTTP client for communicating with remote Gola agent servers
pub struct HttpAgentClient {
    base_url: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl HttpAgentClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[async_trait]
impl AgentClient for HttpAgentClient {
    async fn stream_request(
        &self,
        request: StreamRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let stream_url = format!("{}/stream", self.base_url);
        let request_payload = request.to_run_agent_input();

        let response = self
            .client
            .post(&stream_url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .timeout(self.timeout)
            .json(&request_payload)
            .send()
            .await?;

        if !response.status().is_success() {
            bail!(
                "Failed to connect to agent stream endpoint: {}",
                response.status()
            );
        }

        let stream = response
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Interrupted, e.to_string()));

        let mut lines_reader = StreamReader::new(stream).lines();

        let event_stream = try_stream! {
            let mut current_event_name = "message".to_string();
            let mut current_event_data = String::new();

            while let Ok(line) = lines_reader.next_line().await {
                if line.is_none() {
                    break;
                }

                let line = line.unwrap();
                if line.is_empty() {
                    // Process complete SSE event
                    if !current_event_data.is_empty() {
                        if let Ok(event) = GolaEvent::from_sse(&current_event_name, &current_event_data) {
                            yield StreamEvent::from_gola_event(event);
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
        };

        Ok(Box::pin(event_stream))
    }

    async fn health_check(&self) -> Result<()> {
        let health_url = format!("{}/health", self.base_url);
        let response = self
            .client
            .get(&health_url)
            .timeout(self.timeout)
            .send()
            .await?;

        if !response.status().is_success() {
            bail!("Health check failed: {}", response.status());
        }

        Ok(())
    }

    async fn clear_memory(&self) -> Result<()> {
        let clear_url = format!("{}/memory/clear", self.base_url);
        let response = self
            .client
            .delete(&clear_url)
            .timeout(self.timeout)
            .send()
            .await?;

        if !response.status().is_success() {
            bail!("Clear memory failed: {}", response.status());
        }

        Ok(())
    }
}
