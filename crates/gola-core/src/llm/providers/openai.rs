use crate::core_types::{LLMResponse, Message, Role, ToolCall};
use crate::errors::AgentError;
use crate::llm::{ToolMetadata, LLM};
use async_trait::async_trait;
use reqwest::Client;

use serde_json::{json, Value};


#[derive(Debug, Clone)]
pub struct OpenAIClient {
    client: Client,
    api_key: String,
    api_base: String,
    model: String,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

impl OpenAIClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            api_base: "https://api.openai.com/v1".to_string(),
            model,
            temperature: None,
            max_tokens: None,
        }
    }

    pub fn with_api_base(mut self, api_base: String) -> Self {
        self.api_base = api_base.trim_end_matches('/').to_string();
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    fn build_request_body(
        &self,
        messages: &[Message],
        tools: Option<&[ToolMetadata]>,
    ) -> Value {
        let mut body = json!({
            "model": self.model,
            "messages": self.format_messages(messages),
        });

        if let Some(temp) = self.temperature {
            body["temperature"] = temp.into();
        }

        if let Some(max_tokens) = self.max_tokens {
            body["max_tokens"] = max_tokens.into();
        }

        if let Some(tools) = tools {
            if !tools.is_empty() {
                log::info!("=== TOOL METADATA DUMP ===");
                log::info!("Total tools being sent to OpenAI: {}", tools.len());
                
                // Log each tool in detail
                for (idx, tool) in tools.iter().enumerate() {
                    log::info!("Tool #{}: {}", idx + 1, tool.name);
                    log::info!("  Description: {}", tool.description);
                    log::info!("  Input schema: {}", serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default());
                    
                    // Check if it's a control plane tool
                    if tool.name == "await_human" || tool.name == "assistant_done" {
                        log::info!("  -> This is a CONTROL PLANE tool");
                    } else if tool.name.contains(":") {
                        log::info!("  -> This is an MCP tool (contains ':' in name)");
                    }
                }
                log::info!("=== END TOOL METADATA DUMP ===");
                
                let formatted_tools: Vec<Value> = tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema
                            }
                        })
                    })
                    .collect();
                body["tools"] = formatted_tools.clone().into();
                body["tool_choice"] = "auto".into();
                
                // Log the exact formatted tools array being sent
                log::info!("=== FORMATTED TOOLS JSON ===");
                log::info!("{}", serde_json::to_string_pretty(&formatted_tools).unwrap_or_default());
                log::info!("=== END FORMATTED TOOLS JSON ===");
            }
        }

        body
    }

    fn format_messages(&self, messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|msg| {
                let mut message = json!({
                    "role": self.format_role(&msg.role),
                    "content": msg.content
                });
                
                // Add tool_call_id for tool messages if present
                if let Role::Tool = msg.role {
                    if let Some(tool_call_id) = &msg.tool_call_id {
                        message["tool_call_id"] = json!(tool_call_id);
                    }
                }
                
                // Add tool_calls for assistant messages if present
                if let Role::Assistant = msg.role {
                    if let Some(tool_calls) = &msg.tool_calls {
                        if !tool_calls.is_empty() {
                            let formatted_tool_calls: Vec<Value> = tool_calls
                                .iter()
                                .map(|tc| {
                                let call = json!({
                                        "id": tc.id.clone().unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4().to_string().replace("-", ""))),
                                        "type": "function",
                                        "function": {
                                            "name": tc.name,
                                            "arguments": tc.arguments.to_string()
                                        }
                                    });
                                    call
                                })
                                .collect();
                            message["tool_calls"] = json!(formatted_tool_calls);
                        }
                    }
                }
                
                message
            })
            .collect()
    }

    fn format_role(&self, role: &Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }
}

#[async_trait]
impl LLM for OpenAIClient {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        let url = format!("{}/chat/completions", self.api_base);
        let body = self.build_request_body(&messages, tools.as_deref());

        // More detailed logging of the messages
        log::debug!("OpenAI API request to {}", url);
        log::debug!("Request messages:");
        for (i, msg) in messages.iter().enumerate() {
            log::info!("  Message #{}: role={:?}, content={}, tool_call_id={:?}", 
                      i, msg.role, msg.content, msg.tool_call_id);
        }
        log::debug!("Request body: {}", serde_json::to_string_pretty(&body).unwrap_or_default());

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::LLMError(format!("HTTP request failed: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .map_err(|e| AgentError::LLMError(format!("Failed to read response: {}", e)))?;

        log::debug!("OpenAI API response ({}): {}", status, response_text);

        if !status.is_success() {
            return Err(AgentError::LLMError(format!(
                "API request failed with status {}: {}",
                status, response_text
            )));
        }

        let response_json: Value = serde_json::from_str(&response_text)
            .map_err(|e| AgentError::ParsingError(format!("Invalid JSON response: {}", e)))?;

        self.parse_response(response_json)
    }
}

impl OpenAIClient {
    fn parse_response(&self, response: Value) -> Result<LLMResponse, AgentError> {
        log::info!("=== OPENAI RESPONSE PARSING ===");
        log::info!("Full response: {}", serde_json::to_string_pretty(&response).unwrap_or_default());
        
        let choices = response["choices"]
            .as_array()
            .ok_or_else(|| AgentError::ParsingError("No choices in response".to_string()))?;

        if choices.is_empty() {
            return Err(AgentError::ParsingError("Empty choices array".to_string()));
        }

        let choice = &choices[0];
        let message = &choice["message"];

        let content = message["content"].as_str().map(|s| s.to_string());

        let tool_calls = if let Some(calls) = message["tool_calls"].as_array() {
            let mut parsed_calls = Vec::new();
            for call in calls {
                if let (Some(id), Some(function)) = (call["id"].as_str(), call["function"].as_object()) {
                    if let (Some(name), Some(arguments_str)) = (
                        function["name"].as_str(),
                        function["arguments"].as_str(),
                    ) {
                        let arguments: Value = serde_json::from_str(arguments_str)
                            .map_err(|e| {
                                AgentError::ParsingError(format!(
                                    "Invalid tool call arguments JSON: {}",
                                    e
                                ))
                            })?;

                        parsed_calls.push(ToolCall {
                            id: Some(id.to_string()),
                            name: name.to_string(),
                            arguments,
                        });
                    }
                }
            }
            if parsed_calls.is_empty() {
                None
            } else {
                Some(parsed_calls)
            }
        } else {
            None
        };

        // Validate that we have either content or tool calls
        if content.is_none() && tool_calls.is_none() {
            return Err(AgentError::ParsingError(
                "Response has neither content nor tool calls".to_string(),
            ));
        }

        Ok(LLMResponse { 
            content, 
            tool_calls,
            finish_reason: None,
            usage: None,
        })
    }
}

// Gemini-compatible client (uses OpenAI protocol but different endpoint)
#[derive(Debug, Clone)]
pub struct GeminiClient {
    openai_client: OpenAIClient,
}

impl GeminiClient {
    pub fn new(api_key: String, model: String) -> Self {
        let openai_client = OpenAIClient::new(api_key, model)
            .with_api_base("https://generativelanguage.googleapis.com/v1beta".to_string());
        
        Self { openai_client }
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.openai_client = self.openai_client.with_temperature(temperature);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.openai_client = self.openai_client.with_max_tokens(max_tokens);
        self
    }
}

#[async_trait]
impl LLM for GeminiClient {
    async fn generate(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        self.openai_client.generate(messages, tools).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_types::{Message, Role};

    #[test]
    fn test_openai_client_creation() {
        let client = OpenAIClient::new("test-key".to_string(), "gpt-4.1-mini".to_string())
            .with_temperature(0.7)
            .with_max_tokens(1000);

        assert_eq!(client.api_key, "test-key");
        assert_eq!(client.model, "gpt-4.1-mini");
        assert_eq!(client.temperature, Some(0.7));
        assert_eq!(client.max_tokens, Some(1000));
    }

    #[test]
    fn test_message_formatting() {
        let client = OpenAIClient::new("test-key".to_string(), "gpt-4.1-mini".to_string());
        
        let messages = vec![
            Message {
                role: Role::System,
                content: "You are a helpful assistant.".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::User,
                content: "Hello!".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let formatted = client.format_messages(&messages);
        assert_eq!(formatted.len(), 2);
        assert_eq!(formatted[0]["role"], "system");
        assert_eq!(formatted[0]["content"], "You are a helpful assistant.");
        assert_eq!(formatted[1]["role"], "user");
        assert_eq!(formatted[1]["content"], "Hello!");
    }

    #[test]
    fn test_gemini_client_creation() {
        let client = GeminiClient::new("test-key".to_string(), "gemini-2.0-flash".to_string())
            .with_temperature(0.8);

        assert!(client.openai_client.api_base.contains("generativelanguage.googleapis.com"));
    }
}

/// Create an OpenAI LLM client from configuration
pub fn create_client(config: &crate::config::LlmConfig) -> Result<std::sync::Arc<dyn LLM>, AgentError> {
    
    
    let api_key = config.auth.api_key.clone()
        .or_else(|| {
            config.auth.api_key_env.as_ref()
                .and_then(|env_var| std::env::var(env_var).ok())
        })
        .ok_or_else(|| AgentError::ConfigError(
            "No API key found for OpenAI. Set api_key or api_key_env".to_string()
        ))?;

    let mut client = OpenAIClient::new(api_key, config.model.clone());
    
    if config.parameters.temperature > 0.0 {
        client = client.with_temperature(config.parameters.temperature);
    }
    if config.parameters.max_tokens > 0 {
        client = client.with_max_tokens(config.parameters.max_tokens);
    }

    Ok(std::sync::Arc::new(client))
}

/// Create an OpenAI-compatible client for custom endpoints
pub fn create_custom_client(config: &crate::config::LlmConfig, base_url: &str) -> Result<std::sync::Arc<dyn LLM>, AgentError> {
    let api_key = config.auth.api_key.clone()
        .or_else(|| {
            config.auth.api_key_env.as_ref()
                .and_then(|env_var| std::env::var(env_var).ok())
        })
        .ok_or_else(|| AgentError::ConfigError(
            "No API key found for custom OpenAI-compatible provider. Set api_key or api_key_env".to_string()
        ))?;

    let mut client = OpenAIClient::new(api_key, config.model.clone())
        .with_api_base(base_url.to_string());
    
    if config.parameters.temperature > 0.0 {
        client = client.with_temperature(config.parameters.temperature);
    }
    if config.parameters.max_tokens > 0 {
        client = client.with_max_tokens(config.parameters.max_tokens);
    }

    Ok(std::sync::Arc::new(client))
}
