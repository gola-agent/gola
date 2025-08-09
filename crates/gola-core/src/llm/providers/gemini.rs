//! Google Gemini API client implementation
//!
//! This module provides a native Google Gemini API client that directly integrates
//! with Google's Generative AI API endpoints.

use crate::config::{LlmConfig, LlmProvider};
use crate::core_types::{LLMResponse, Message, Role, ToolCall, Usage};
use crate::errors::AgentError;
use crate::llm::{ToolMetadata, LLM};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::sync::Arc;

/// Google Gemini API client
pub struct GeminiClient {
    api_key: String,
    model: String,
    client: Client,
    base_url: String,
}

impl GeminiClient {
    /// Create a new Gemini client
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            client: Client::new(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
        }
    }

    /// Create a new Gemini client with custom base URL
    pub fn with_base_url(api_key: String, model: String, base_url: String) -> Self {
        Self {
            api_key,
            model,
            client: Client::new(),
            base_url,
        }
    }
}

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
    #[serde(rename = "safetySettings", skip_serializing_if = "Option::is_none")]
    safety_settings: Option<Value>,
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: Value,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    temperature: f32,
    #[serde(rename = "maxOutputTokens")]
    max_output_tokens: u32,
    #[serde(rename = "topP")]
    top_p: f32,
    #[serde(rename = "stopSequences", skip_serializing_if = "Vec::is_empty")]
    stop_sequences: Vec<String>,
}

#[derive(Debug, Serialize)]
struct GeminiTool {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata", default)]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsage {
    #[serde(rename = "promptTokenCount", default)]
    prompt_token_count: Option<i32>,
    #[serde(rename = "candidatesTokenCount", default)]
    candidates_token_count: Option<i32>,
    #[serde(rename = "totalTokenCount", default)]
    total_token_count: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct GeminiError {
    error: GeminiErrorDetails,
}

#[derive(Debug, Deserialize)]
struct GeminiErrorDetails {
    code: u16,
    message: String,
}

impl GeminiClient {
    fn convert_messages_to_gemini_contents(
        &self,
        messages: Vec<Message>,
    ) -> (Option<GeminiContent>, Vec<GeminiContent>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for message in messages {
            match message.role {
                Role::System => {
                    // Use the last system message as system instruction
                    system_instruction = Some(GeminiContent {
                        role: None,
                        parts: vec![GeminiPart::Text {
                            text: message.content,
                        }],
                    });
                }
                Role::User => {
                    contents.push(GeminiContent {
                        role: Some("user".to_string()),
                        parts: vec![GeminiPart::Text {
                            text: message.content,
                        }],
                    });
                }
                Role::Assistant => {
                    let mut parts = Vec::new();
                    
                    if !message.content.is_empty() {
                        parts.push(GeminiPart::Text {
                            text: message.content,
                        });
                    }
                    
                    if let Some(tool_calls) = &message.tool_calls {
                        for tool_call in tool_calls {
                            parts.push(GeminiPart::FunctionCall {
                                function_call: GeminiFunctionCall {
                                    name: tool_call.name.clone(),
                                    args: tool_call.arguments.clone(),
                                },
                            });
                        }
                    }
                    
                    contents.push(GeminiContent {
                        role: Some("model".to_string()),
                        parts,
                    });
                }
                Role::Tool => {
                    // Convert tool response to function response
                    let tool_call_id = message.tool_call_id.as_ref().unwrap_or(&message.content);
                    let response_value = serde_json::json!({ "content": message.content });
                    
                    contents.push(GeminiContent {
                        role: Some("function".to_string()),
                        parts: vec![GeminiPart::FunctionResponse {
                            function_response: GeminiFunctionResponse {
                                name: tool_call_id.clone(),
                                response: response_value,
                            },
                        }],
                    });
                }
            }
        }

        (system_instruction, contents)
    }

    fn convert_tools_to_gemini(&self, tools: Vec<ToolMetadata>) -> Vec<GeminiTool> {
        if tools.is_empty() {
            return vec![];
        }
        
        let function_declarations = tools
            .into_iter()
            .map(|tool| GeminiFunctionDeclaration {
                name: tool.name,
                description: tool.description,
                parameters: tool.input_schema,
            })
            .collect();
        
        vec![GeminiTool {
            function_declarations,
        }]
    }

    fn convert_gemini_response_to_llm(&self, response: GeminiResponse) -> Result<LLMResponse, AgentError> {
        let candidate = response
            .candidates
            .into_iter()
            .next()
            .ok_or_else(|| AgentError::LLMError("No candidates in Gemini response".to_string()))?;

        let mut content_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for part in candidate.content.parts {
            match part {
                GeminiPart::Text { text } => {
                    content_parts.push(text);
                }
                GeminiPart::FunctionCall { function_call } => {
                    tool_calls.push(ToolCall {
                        id: Some(format!("call_{}", uuid::Uuid::new_v4().simple())),
                        name: function_call.name,
                        arguments: function_call.args,
                    });
                }
                GeminiPart::FunctionResponse { .. } => {
                    // Function responses shouldn't appear in the final response
                    continue;
                }
            }
        }

        let content = if content_parts.is_empty() {
            None
        } else {
            Some(content_parts.join(" "))
        };

        let tool_calls = if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        };

        let usage = response.usage_metadata.map(|u| Usage {
            prompt_tokens: u.prompt_token_count.unwrap_or(0) as u32,
            completion_tokens: u.candidates_token_count.unwrap_or(0) as u32,
            total_tokens: u.total_token_count.unwrap_or(0) as u32,
        });

        Ok(LLMResponse {
            content,
            tool_calls,
            finish_reason: candidate.finish_reason,
            usage,
        })
    }
}

#[async_trait]
impl LLM for GeminiClient {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        let (system_instruction, contents) = self.convert_messages_to_gemini_contents(messages);
        
        let generation_config = GeminiGenerationConfig {
            temperature: 0.7,
            max_output_tokens: 4096,
            top_p: 0.9,
            stop_sequences: Vec::new(),
        };

        let tools_gemini = tools.map(|t| self.convert_tools_to_gemini(t));

        let request = GeminiRequest {
            contents,
            generation_config,
            safety_settings: None,
            system_instruction,
            tools: tools_gemini,
        };

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::LLMError(format!("Gemini API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            
            if let Ok(gemini_error) = serde_json::from_str::<GeminiError>(&error_text) {
                return Err(AgentError::LLMError(format!(
                    "Gemini API error {}: {}",
                    gemini_error.error.code, gemini_error.error.message
                )));
            }
            
            return Err(AgentError::LLMError(format!(
                "Gemini API request failed with status {}: {}",
                status,
                error_text
            )));
        }

        let gemini_response: GeminiResponse = response
            .json()
            .await
            .map_err(|e| AgentError::ParsingError(format!("Failed to parse Gemini response: {}", e)))?;

        self.convert_gemini_response_to_llm(gemini_response)
    }
}

/// Create a Gemini LLM client from configuration
pub fn create_client(config: &LlmConfig) -> Result<Arc<dyn LLM>, AgentError> {
    let api_key = match &config.auth.api_key {
        Some(key) => key.clone(),
        None => match &config.auth.api_key_env {
            Some(env_var) => env::var(env_var).map_err(|_| {
                AgentError::ConfigError(format!(
                    "Environment variable {} not found for Gemini API key",
                    env_var
                ))
            })?,
            None => env::var("GEMINI_API_KEY").map_err(|_| {
                AgentError::ConfigError("No API key found for Gemini. Set GEMINI_API_KEY environment variable or provide api_key in config".to_string())
            })?,
        },
    };

    let client = match &config.provider {
        LlmProvider::Gemini => GeminiClient::new(api_key, config.model.clone()),
        LlmProvider::Custom { base_url } => {
            GeminiClient::with_base_url(api_key, config.model.clone(), base_url.clone())
        }
        _ => {
            return Err(AgentError::ConfigError(
                "Invalid provider for Gemini client".to_string(),
            ))
        }
    };

    Ok(Arc::new(client))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LlmAuth, ModelParameters};
    use std::collections::HashMap;

    #[test]
    fn test_gemini_client_creation() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-pro".to_string());
        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model, "gemini-pro");
        assert_eq!(client.base_url, "https://generativelanguage.googleapis.com/v1beta");
    }

    #[test]
    fn test_message_conversion_simple() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-pro".to_string());
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let (system_instruction, contents) = client.convert_messages_to_gemini_contents(messages);
        assert!(system_instruction.is_none());
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].role, Some("user".to_string()));
    }

    #[test]
    fn test_message_conversion_with_system() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-pro".to_string());
        let messages = vec![
            Message {
                role: Role::System,
                content: "You are helpful".to_string(),
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

        let (system_instruction, contents) = client.convert_messages_to_gemini_contents(messages);
        assert!(system_instruction.is_some());
        assert_eq!(contents.len(), 1);
    }

    #[test]
    fn test_tool_conversion_empty() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-pro".to_string());
        let tools = vec![];
        let gemini_tools = client.convert_tools_to_gemini(tools);
        assert!(gemini_tools.is_empty());
    }

    #[test]
    fn test_tool_conversion() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-pro".to_string());
        let tools = vec![ToolMetadata {
            name: "calculator".to_string(),
            description: "Perform calculations".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {"type": "string"}
                }
            }),
        }];

        let gemini_tools = client.convert_tools_to_gemini(tools);
        assert_eq!(gemini_tools.len(), 1);
        assert_eq!(gemini_tools[0].function_declarations.len(), 1);
        assert_eq!(gemini_tools[0].function_declarations[0].name, "calculator");
    }

    #[test]
    fn test_create_client_from_config() {
        let config = LlmConfig {
            provider: LlmProvider::Gemini,
            model: "gemini-pro".to_string(),
            auth: LlmAuth {
                api_key: Some("test-key".to_string()),
                api_key_env: None,
                headers: HashMap::new(),
            },
            parameters: ModelParameters::default(),
        };

        let result = create_client(&config);
        assert!(result.is_ok());
    }
}