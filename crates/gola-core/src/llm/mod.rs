//! Language model provider abstractions and integrations.
//!
//! Defines the core LLM trait and implementations for various providers including
//! Anthropic, OpenAI, Gemini, and custom HTTP endpoints. Includes utilities for
//! response parsing, context management, and automatic error recovery.

pub use crate::core_types::{LLMResponse, Message};
use crate::errors::AgentError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod providers;
pub mod response_parser;
pub mod context_truncation;
pub mod utils;
pub mod message_validator;
pub mod auto_recovery_llm;
pub mod summarizer;

pub use response_parser::ResponseParser;
pub use context_truncation::ContextTruncatingLLM;
pub use utils::LLMFactory;
pub use message_validator::MessageValidator;
pub use auto_recovery_llm::AutoRecoveryLLM;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[async_trait]
pub trait LLM: Send + Sync {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError>;
}

use reqwest::Client;

pub struct HttpLLMClient {
    pub endpoint_url: String,
    client: Client,
}

impl HttpLLMClient {
    pub fn new(endpoint_url: String) -> Self {
        Self {
            endpoint_url,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LLM for HttpLLMClient {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        #[derive(Serialize)]
        struct RequestPayload<'a> {
            messages: &'a Vec<Message>,
            tools: Option<&'a Vec<ToolMetadata>>,
        }

        let payload = RequestPayload {
            messages: &messages,
            tools: tools.as_ref(),
        };

        let request_url = format!("{}/v1/chat/completions", self.endpoint_url);
        log::debug!(
            "HttpLLMClient sending request to {}: {:?}",
            request_url,
            payload.messages
        );
        if payload.tools.is_some() {
            log::debug!("HttpLLMClient tools: {:?}", payload.tools);
        }

        match self.client.post(&request_url).json(&payload).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<LLMResponse>().await {
                        Ok(llm_response) => {
                            log::debug!(
                                "HttpLLMClient received LLMResponse content: {:?}",
                                llm_response.content
                            );
                            if llm_response.tool_calls.is_some() {
                                log::debug!(
                                    "HttpLLMClient received tool_calls: {:?}",
                                    llm_response.tool_calls
                                );
                            }
                            Ok(llm_response)
                        }
                        Err(e) => {
                            let err_msg = format!("Failed to parse LLM response JSON: {}", e);
                            log::error!("{}", err_msg);
                            Err(AgentError::ParsingError(err_msg))
                        }
                    }
                } else {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_else(|_| {
                        "Unknown error while reading error response body".to_string()
                    });
                    let err_msg = format!(
                        "LLM API request failed with status {}: {}",
                        status, error_text
                    );
                    log::error!("{}", err_msg);
                    Err(AgentError::LLMError(err_msg))
                }
            }
            Err(e) => {
                let err_msg = format!("HTTP request to LLM endpoint failed: {}", e);
                log::error!("{}", err_msg);
                Err(AgentError::LLMError(err_msg))
            }
        }
    }
}
