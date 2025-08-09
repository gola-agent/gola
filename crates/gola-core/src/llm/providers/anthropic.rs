//! Anthropic Claude LLM client implementation
//! 
//! This module provides a native implementation of the Anthropic Claude API,
//! supporting the Messages API with proper tool calling and streaming.

use std::sync::Arc;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{LlmConfig, ModelParameters};
use crate::core_types::{LLMResponse, Message, Role, ToolCall, Usage};
use crate::errors::AgentError;
use crate::llm::{LLM, ToolMetadata};

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    client: Client,
    api_key: String,
    api_base: String,
    model: String,
    parameters: ModelParameters,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum AnthropicContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    #[serde(rename = "id")]
    _id: String,
    #[serde(rename = "type")]
    _type: String,
    #[serde(rename = "role")]
    _role: String,
    content: Vec<AnthropicResponseContent>,
    #[serde(rename = "model")]
    _model: String,
    stop_reason: Option<String>,
    #[serde(rename = "stop_sequence")]
    _stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    #[serde(rename = "type")]
    _type: String,
    message: String,
}

impl AnthropicClient {
    pub fn new(
        api_key: String,
        model: String,
        parameters: ModelParameters,
        api_base: Option<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            api_base: api_base.unwrap_or_else(|| ANTHROPIC_API_BASE.to_string()),
            model,
            parameters,
        }
    }

    fn convert_messages(&self, messages: Vec<Message>) -> Result<(Option<String>, Vec<AnthropicMessage>), AgentError> {
        let mut system_message = None;
        let mut anthropic_messages = Vec::new();
        let mut current_user_content = Vec::new();
        let mut current_assistant_content = Vec::new();
        let mut _last_role = None;

        for message in messages {
            match message.role {
                Role::System => {
                    // Anthropic handles system messages separately
                    system_message = Some(message.content);
                }
                Role::User => {
                    // If we have pending assistant content, flush it first
                    if !current_assistant_content.is_empty() {
                        anthropic_messages.push(AnthropicMessage {
                            role: "assistant".to_string(),
                            content: current_assistant_content.drain(..).collect(),
                        });
                    }

                    // Add to current user content
                    current_user_content.push(AnthropicContent::Text {
                        text: message.content,
                    });
                    _last_role = Some(Role::User);
                }
                Role::Assistant => {
                    // If we have pending user content, flush it first
                    if !current_user_content.is_empty() {
                        anthropic_messages.push(AnthropicMessage {
                            role: "user".to_string(),
                            content: current_user_content.drain(..).collect(),
                        });
                    }

                    // Add to current assistant content
                    current_assistant_content.push(AnthropicContent::Text {
                        text: message.content,
                    });

                    // Handle tool calls
                    if let Some(tool_calls) = message.tool_calls {
                        for tool_call in tool_calls {
                            current_assistant_content.push(AnthropicContent::ToolUse {
                                id: tool_call.id.unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4())),
                                name: tool_call.name,
                                input: tool_call.arguments,
                            });
                        }
                    }
                    _last_role = Some(Role::Assistant);
                }
                Role::Tool => {
                    // Tool results should be added to user messages in Anthropic
                    current_user_content.push(AnthropicContent::ToolResult {
                        tool_use_id: message.tool_call_id.unwrap_or_default(),
                        content: message.content,
                    });
                }
            }
        }

        // Flush any remaining content
        if !current_user_content.is_empty() {
            anthropic_messages.push(AnthropicMessage {
                role: "user".to_string(),
                content: current_user_content,
            });
        }
        if !current_assistant_content.is_empty() {
            anthropic_messages.push(AnthropicMessage {
                role: "assistant".to_string(),
                content: current_assistant_content,
            });
        }

        // Use system message from parameters if provided and no system message in conversation
        if system_message.is_none() {
            system_message = self.parameters.system_message.clone();
        }

        Ok((system_message, anthropic_messages))
    }

    fn convert_tools(&self, tools: Option<Vec<ToolMetadata>>) -> Vec<AnthropicTool> {
        tools
            .unwrap_or_default()
            .into_iter()
            .map(|tool| AnthropicTool {
                name: tool.name,
                description: tool.description,
                input_schema: tool.input_schema,
            })
            .collect()
    }

    fn convert_response(&self, response: AnthropicResponse) -> Result<LLMResponse, AgentError> {
        let mut content = String::new();
        let mut tool_calls = Vec::new();

        for content_block in response.content {
            match content_block {
                AnthropicResponseContent::Text { text } => {
                    content.push_str(&text);
                }
                AnthropicResponseContent::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id: Some(id),
                        name,
                        arguments: input,
                    });
                }
            }
        }

        Ok(LLMResponse {
            content: if content.is_empty() { None } else { Some(content) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            finish_reason: response.stop_reason,
            usage: Some(Usage {
                prompt_tokens: response.usage.input_tokens,
                completion_tokens: response.usage.output_tokens,
                total_tokens: response.usage.input_tokens + response.usage.output_tokens,
            }),
        })
    }
}

#[async_trait]
impl LLM for AnthropicClient {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        let (system_message, anthropic_messages) = self.convert_messages(messages)?;
        let anthropic_tools = self.convert_tools(tools);

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.parameters.max_tokens,
            messages: anthropic_messages,
            system: system_message,
            temperature: if self.parameters.temperature > 0.0 {
                Some(self.parameters.temperature)
            } else {
                None
            },
            top_p: if self.parameters.top_p < 1.0 {
                Some(self.parameters.top_p)
            } else {
                None
            },
            stop_sequences: self.parameters.stop_sequences.clone(),
            tools: anthropic_tools,
        };

        let anthropic_version = self.parameters.anthropic_version
            .as_deref()
            .unwrap_or(DEFAULT_ANTHROPIC_VERSION);

        let response = self
            .client
            .post(&format!("{}/v1/messages", self.api_base))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", anthropic_version)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LLMError(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Try to parse as Anthropic error
            if let Ok(anthropic_error) = serde_json::from_str::<AnthropicError>(&error_text) {
                return Err(AgentError::LLMError(format!(
                    "Anthropic API error ({}): {}",
                    status, anthropic_error.message
                )));
            }

            return Err(AgentError::LLMError(format!(
                "HTTP {} error: {}",
                status, error_text
            )));
        }

        let anthropic_response: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| AgentError::LLMError(format!("Failed to parse response: {}", e)))?;

        self.convert_response(anthropic_response)
    }
}

/// Create an Anthropic LLM client from configuration
pub fn create_client(config: &LlmConfig) -> Result<Arc<dyn LLM>, AgentError> {
    let api_key = config.auth.api_key.clone()
        .or_else(|| {
            config.auth.api_key_env.as_ref()
                .and_then(|env_var| std::env::var(env_var).ok())
        })
        .ok_or_else(|| AgentError::ConfigError(
            "No API key found for Anthropic. Set api_key or api_key_env".to_string()
        ))?;

    let client = AnthropicClient::new(
        api_key,
        config.model.clone(),
        config.parameters.clone(),
        None,
    );

    Ok(Arc::new(client))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_types::{Message, Role, ToolCall};
    use crate::llm::ToolMetadata;
    use serde_json::json;

    fn create_test_client() -> AnthropicClient {
        AnthropicClient::new(
            "test-key".to_string(),
            "claude-3-5-sonnet-latest".to_string(),
            ModelParameters::default(),
            None,
        )
    }

    fn create_test_client_with_params(params: ModelParameters) -> AnthropicClient {
        AnthropicClient::new(
            "test-key".to_string(),
            "claude-3-5-sonnet-latest".to_string(),
            params,
            None,
        )
    }

    #[test]
    fn test_client_creation() {
        let client = create_test_client();
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model, "claude-3-5-sonnet-latest");
        assert_eq!(client.api_base, ANTHROPIC_API_BASE);
    }

    #[test]
    fn test_client_creation_with_custom_api_base() {
        let client = AnthropicClient::new(
            "test-key".to_string(),
            "claude-3-5-sonnet-latest".to_string(),
            ModelParameters::default(),
            Some("https://custom.api.com".to_string()),
        );
        assert_eq!(client.api_base, "https://custom.api.com");
    }

    #[test]
    fn test_simple_message_conversion() {
        let client = create_test_client();

        let messages = vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let (system, anthropic_messages) = client.convert_messages(messages).unwrap();
        
        assert_eq!(system, Some("You are a helpful assistant".to_string()));
        assert_eq!(anthropic_messages.len(), 1);
        assert_eq!(anthropic_messages[0].role, "user");
        assert_eq!(anthropic_messages[0].content.len(), 1);
        
        if let AnthropicContent::Text { text } = &anthropic_messages[0].content[0] {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_message_conversion_with_assistant_response() {
        let client = create_test_client();

        let messages = vec![
            Message {
                role: Role::User,
                content: "What's 2+2?".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::Assistant,
                content: "2+2 equals 4".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let (system, anthropic_messages) = client.convert_messages(messages).unwrap();
        
        assert_eq!(system, None);
        assert_eq!(anthropic_messages.len(), 2);
        assert_eq!(anthropic_messages[0].role, "user");
        assert_eq!(anthropic_messages[1].role, "assistant");
        
        if let AnthropicContent::Text { text } = &anthropic_messages[1].content[0] {
            assert_eq!(text, "2+2 equals 4");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn test_message_conversion_with_tool_calls() {
        let client = create_test_client();

        let messages = vec![
            Message {
                role: Role::User,
                content: "Calculate 5 * 3".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::Assistant,
                content: "I'll calculate that for you.".to_string(),
                tool_calls: Some(vec![ToolCall {
                    id: Some("call_123".to_string()),
                    name: "calculator".to_string(),
                    arguments: json!({"operation": "multiply", "a": 5, "b": 3}),
                }]),
                tool_call_id: None,
            },
        ];

        let (system, anthropic_messages) = client.convert_messages(messages).unwrap();
        
        assert_eq!(system, None);
        assert_eq!(anthropic_messages.len(), 2);
        assert_eq!(anthropic_messages[1].role, "assistant");
        assert_eq!(anthropic_messages[1].content.len(), 2); // Text + tool use
        
        // Check text content
        if let AnthropicContent::Text { text } = &anthropic_messages[1].content[0] {
            assert_eq!(text, "I'll calculate that for you.");
        } else {
            panic!("Expected text content");
        }

        // Check tool use content
        if let AnthropicContent::ToolUse { id, name, input } = &anthropic_messages[1].content[1] {
            assert_eq!(id, "call_123");
            assert_eq!(name, "calculator");
            assert_eq!(input, &json!({"operation": "multiply", "a": 5, "b": 3}));
        } else {
            panic!("Expected tool use content");
        }
    }

    #[test]
    fn test_message_conversion_with_tool_result() {
        let client = create_test_client();

        let messages = vec![
            Message {
                role: Role::User,
                content: "Calculate 5 * 3".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::Tool,
                content: "15".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_123".to_string()),
            },
        ];

        let (system, anthropic_messages) = client.convert_messages(messages).unwrap();
        
        assert_eq!(system, None);
        assert_eq!(anthropic_messages.len(), 1);
        assert_eq!(anthropic_messages[0].role, "user");
        assert_eq!(anthropic_messages[0].content.len(), 2); // Original text + tool result
        
        // Check original user message
        if let AnthropicContent::Text { text } = &anthropic_messages[0].content[0] {
            assert_eq!(text, "Calculate 5 * 3");
        } else {
            panic!("Expected text content");
        }

        // Check tool result
        if let AnthropicContent::ToolResult { tool_use_id, content } = &anthropic_messages[0].content[1] {
            assert_eq!(tool_use_id, "call_123");
            assert_eq!(content, "15");
        } else {
            panic!("Expected tool result content");
        }
    }

    #[test]
    fn test_message_conversion_with_system_message_from_params() {
        let mut params = ModelParameters::default();
        params.system_message = Some("You are a math tutor".to_string());
        
        let client = create_test_client_with_params(params);

        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let (system, anthropic_messages) = client.convert_messages(messages).unwrap();
        
        assert_eq!(system, Some("You are a math tutor".to_string()));
        assert_eq!(anthropic_messages.len(), 1);
    }

    #[test]
    fn test_message_conversion_system_message_priority() {
        // System message in conversation should take priority over params
        let mut params = ModelParameters::default();
        params.system_message = Some("You are a math tutor".to_string());
        
        let client = create_test_client_with_params(params);

        let messages = vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let (system, _) = client.convert_messages(messages).unwrap();
        
        assert_eq!(system, Some("You are a helpful assistant".to_string()));
    }

    #[test]
    fn test_tool_conversion() {
        let client = create_test_client();

        let tools = vec![
            ToolMetadata {
                name: "calculator".to_string(),
                description: "Perform basic arithmetic".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "operation": {"type": "string"},
                        "a": {"type": "number"},
                        "b": {"type": "number"}
                    }
                }),
            },
            ToolMetadata {
                name: "search".to_string(),
                description: "Search the web".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"}
                    }
                }),
            },
        ];

        let anthropic_tools = client.convert_tools(Some(tools));
        
        assert_eq!(anthropic_tools.len(), 2);
        assert_eq!(anthropic_tools[0].name, "calculator");
        assert_eq!(anthropic_tools[0].description, "Perform basic arithmetic");
        assert_eq!(anthropic_tools[1].name, "search");
        assert_eq!(anthropic_tools[1].description, "Search the web");
    }

    #[test]
    fn test_tool_conversion_empty() {
        let client = create_test_client();
        let anthropic_tools = client.convert_tools(None);
        assert!(anthropic_tools.is_empty());
    }

    #[test]
    fn test_response_conversion_text_only() {
        let client = create_test_client();

        let anthropic_response = AnthropicResponse {
            _id: "msg_123".to_string(),
            _type: "message".to_string(),
            _role: "assistant".to_string(),
            content: vec![AnthropicResponseContent::Text {
                text: "Hello! How can I help you today?".to_string(),
            }],
            _model: "claude-3-5-sonnet-latest".to_string(),
            stop_reason: Some("end_turn".to_string()),
            _stop_sequence: None,
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 25,
            },
        };

        let llm_response = client.convert_response(anthropic_response).unwrap();
        
        assert_eq!(llm_response.content, Some("Hello! How can I help you today?".to_string()));
        assert!(llm_response.tool_calls.is_none());
        assert_eq!(llm_response.finish_reason, Some("end_turn".to_string()));
        
        let usage = llm_response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 25);
        assert_eq!(usage.total_tokens, 35);
    }

    #[test]
    fn test_response_conversion_with_tool_calls() {
        let client = create_test_client();

        let anthropic_response = AnthropicResponse {
            _id: "msg_123".to_string(),
            _type: "message".to_string(),
            _role: "assistant".to_string(),
            content: vec![
                AnthropicResponseContent::Text {
                    text: "I'll calculate that for you.".to_string(),
                },
                AnthropicResponseContent::ToolUse {
                    id: "call_456".to_string(),
                    name: "calculator".to_string(),
                    input: json!({"operation": "add", "a": 5, "b": 3}),
                },
            ],
            _model: "claude-3-5-sonnet-latest".to_string(),
            stop_reason: Some("tool_use".to_string()),
            _stop_sequence: None,
            usage: AnthropicUsage {
                input_tokens: 15,
                output_tokens: 30,
            },
        };

        let llm_response = client.convert_response(anthropic_response).unwrap();
        
        assert_eq!(llm_response.content, Some("I'll calculate that for you.".to_string()));
        assert!(llm_response.tool_calls.is_some());
        
        let tool_calls = llm_response.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, Some("call_456".to_string()));
        assert_eq!(tool_calls[0].name, "calculator");
        assert_eq!(tool_calls[0].arguments, json!({"operation": "add", "a": 5, "b": 3}));
    }

    #[test]
    fn test_response_conversion_multiple_text_blocks() {
        let client = create_test_client();

        let anthropic_response = AnthropicResponse {
            _id: "msg_123".to_string(),
            _type: "message".to_string(),
            _role: "assistant".to_string(),
            content: vec![
                AnthropicResponseContent::Text {
                    text: "First part. ".to_string(),
                },
                AnthropicResponseContent::Text {
                    text: "Second part.".to_string(),
                },
            ],
            _model: "claude-3-5-sonnet-latest".to_string(),
            stop_reason: Some("end_turn".to_string()),
            _stop_sequence: None,
            usage: AnthropicUsage {
                input_tokens: 5,
                output_tokens: 10,
            },
        };

        let llm_response = client.convert_response(anthropic_response).unwrap();
        
        assert_eq!(llm_response.content, Some("First part. Second part.".to_string()));
        assert!(llm_response.tool_calls.is_none());
    }

    #[test]
    fn test_create_client_from_config() {
        use crate::config::{LlmAuth, LlmConfig, LlmProvider, ModelParameters};
        use std::collections::HashMap;
        
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            model: "claude-3-5-sonnet-latest".to_string(),
            auth: LlmAuth {
                api_key: Some("test-api-key".to_string()),
                api_key_env: None,
                headers: HashMap::new(),
            },
            parameters: ModelParameters::default(),
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_client_from_config_with_env_key() {
        use crate::config::{LlmAuth, LlmConfig, LlmProvider, ModelParameters};
        use std::collections::HashMap;
        use std::env;
        
        // Set environment variable for test
        env::set_var("TEST_ANTHROPIC_KEY", "env-test-key");
        
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            model: "claude-3-5-sonnet-latest".to_string(),
            auth: LlmAuth {
                api_key: None,
                api_key_env: Some("TEST_ANTHROPIC_KEY".to_string()),
                headers: HashMap::new(),
            },
            parameters: ModelParameters::default(),
        };

        let result = create_client(&config);
        assert!(result.is_ok());
        
        // Clean up
        env::remove_var("TEST_ANTHROPIC_KEY");
    }

    #[test]
    fn test_create_client_missing_api_key() {
        use crate::config::{LlmAuth, LlmConfig, LlmProvider, ModelParameters};
        use std::collections::HashMap;
        
        let config = LlmConfig {
            provider: LlmProvider::Anthropic,
            model: "claude-3-5-sonnet-latest".to_string(),
            auth: LlmAuth {
                api_key: None,
                api_key_env: None,
                headers: HashMap::new(),
            },
            parameters: ModelParameters::default(),
        };

        let result = create_client(&config);
        assert!(result.is_err());
        
        if let Err(AgentError::ConfigError(msg)) = result {
            assert!(msg.contains("No API key found for Anthropic"));
        } else {
            panic!("Expected ConfigError");
        }
    }

    #[test]
    fn test_anthropic_request_serialization() {
        let request = AnthropicRequest {
            model: "claude-3-5-sonnet-latest".to_string(),
            max_tokens: 1000,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContent::Text {
                    text: "Hello".to_string(),
                }],
            }],
            system: Some("You are helpful".to_string()),
            temperature: Some(0.7),
            top_p: None,
            stop_sequences: vec![],
            tools: vec![],
        };

        let serialized = serde_json::to_value(&request).unwrap();
        
        assert_eq!(serialized["model"], "claude-3-5-sonnet-latest");
        assert_eq!(serialized["max_tokens"], 1000);
        assert_eq!(serialized["system"], "You are helpful");
        // Handle f32 precision - check approximately equal to 0.7
        let temp_value = serialized["temperature"].as_f64().unwrap();
        assert!((temp_value - 0.7).abs() < 0.001, "Temperature {} is not approximately 0.7", temp_value);
        assert!(serialized["top_p"].is_null());
        assert_eq!(serialized["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_anthropic_response_deserialization() {
        let response_json = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "text",
                    "text": "Hello there!"
                }
            ],
            "model": "claude-3-5-sonnet-latest",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5
            }
        });

        let response: AnthropicResponse = serde_json::from_value(response_json).unwrap();
        
        assert_eq!(response._id, "msg_123");
        assert_eq!(response._type, "message");
        assert_eq!(response._role, "assistant");
        assert_eq!(response.content.len(), 1);
        assert_eq!(response.stop_reason, Some("end_turn".to_string()));
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
        
        if let AnthropicResponseContent::Text { text } = &response.content[0] {
            assert_eq!(text, "Hello there!");
        } else {
            panic!("Expected text content");
        }
    }
}