//! Configuration builder for the agent framework
//! 
//! This module provides a fluent builder API for constructing agent configurations
//! programmatically, including support for JSON schema validation.

use crate::config::types::*;
use crate::errors::AgentError;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Builder for creating agent configurations
#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    config: GolaConfig,
}

impl ConfigBuilder {
    /// Create a new configuration builder with default values
    pub fn new() -> Self {
        Self {
            config: GolaConfig {
                agent: AgentDefinition {
                    name: "default_agent".to_string(),
                    description: String::new(),
                    max_steps: 10,
                    behavior: AgentBehavior::default(),
                    schema: SchemaConfig::default(),
                },
                prompts: None,
                llm: Some(LlmConfig {
                    provider: LlmProvider::OpenAI,
                    model: "gpt-4.1-mini".to_string(),
                    parameters: ModelParameters::default(),
                    auth: LlmAuth::default(),
                }),
                rag: None,
                mcp_servers: Vec::new(),
                tools: ToolsConfig::default(),
                environment: EnvironmentConfig::default(),
                logging: LoggingConfig::default(),
                tracing: TracingConfig::default(),
            },
        }
    }

    /// Set the agent name
    pub fn agent_name(mut self, name: impl Into<String>) -> Self {
        self.config.agent.name = name.into();
        self
    }

    /// Set the agent description
    pub fn agent_description(mut self, description: impl Into<String>) -> Self {
        self.config.agent.description = description.into();
        self
    }

    /// Set the prompt configuration
    pub fn prompts(mut self, prompts: PromptConfig) -> Self {
        self.config.prompts = Some(prompts);
        self
    }

    /// Set the maximum number of steps
    pub fn max_steps(mut self, steps: usize) -> Self {
        self.config.agent.max_steps = steps;
        self
    }

    /// Enable schema validation
    pub fn enable_schema_validation(mut self) -> Self {
        self.config.agent.schema.enabled = true;
        self
    }

    /// Disable schema validation
    pub fn disable_schema_validation(mut self) -> Self {
        self.config.agent.schema.enabled = false;
        self
    }

    /// Set input schema from JSON value
    pub fn input_schema(mut self, schema: Value) -> Self {
        self.config.agent.schema.input = Some(InputSchemaConfig {
            schema,
            strict: true,
            error_message: None,
            source: SchemaSource::default(),
        });
        self.config.agent.schema.enabled = true;
        self
    }

    /// Set input schema from JSON string
    pub fn input_schema_from_str(self, schema_str: &str) -> Result<Self, AgentError> {
        let schema: Value = serde_json::from_str(schema_str)
            .map_err(|e| AgentError::ConfigError(format!("Invalid JSON schema: {}", e)))?;
        Ok(self.input_schema(schema))
    }

    /// Set input schema from file
    pub fn input_schema_from_file(mut self, file_path: impl Into<PathBuf>) -> Self {
        let path = file_path.into();
        self.config.agent.schema.input = Some(InputSchemaConfig {
            schema: Value::Null,
            strict: true,
            error_message: None,
            source: SchemaSource {
                source_type: SchemaSourceType::File,
                location: Some(path.to_string_lossy().to_string()),
                version: None,
                description: None,
            },
        });
        self.config.agent.schema.enabled = true;
        self
    }

    /// Set output schema from JSON value
    pub fn output_schema(mut self, schema: Value) -> Self {
        self.config.agent.schema.output = Some(OutputSchemaConfig {
            schema,
            strict: true,
            error_message: None,
            source: SchemaSource::default(),
            auto_correct: false,
        });
        self.config.agent.schema.enabled = true;
        self
    }

    /// Set output schema from JSON string
    pub fn output_schema_from_str(self, schema_str: &str) -> Result<Self, AgentError> {
        let schema: Value = serde_json::from_str(schema_str)
            .map_err(|e| AgentError::ConfigError(format!("Invalid JSON schema: {}", e)))?;
        Ok(self.output_schema(schema))
    }

    /// Set output schema from file
    pub fn output_schema_from_file(mut self, file_path: impl Into<PathBuf>) -> Self {
        let path = file_path.into();
        self.config.agent.schema.output = Some(OutputSchemaConfig {
            schema: Value::Null,
            strict: true,
            error_message: None,
            source: SchemaSource {
                source_type: SchemaSourceType::File,
                location: Some(path.to_string_lossy().to_string()),
                version: None,
                description: None,
            },
            auto_correct: false,
        });
        self.config.agent.schema.enabled = true;
        self
    }

    /// Configure schema validation behavior
    pub fn schema_validation(mut self, config: SchemaValidationConfig) -> Self {
        self.config.agent.schema.validation = config;
        self
    }

    /// Set whether to use strict validation for input
    pub fn input_schema_strict(mut self, strict: bool) -> Self {
        if let Some(ref mut input) = self.config.agent.schema.input {
            input.strict = strict;
        }
        self
    }

    /// Set whether to use strict validation for output
    pub fn output_schema_strict(mut self, strict: bool) -> Self {
        if let Some(ref mut output) = self.config.agent.schema.output {
            output.strict = strict;
        }
        self
    }

    /// Enable auto-correction for output schema validation
    pub fn enable_output_auto_correct(mut self) -> Self {
        if let Some(ref mut output) = self.config.agent.schema.output {
            output.auto_correct = true;
        }
        self
    }

    /// Set custom error message for input validation
    pub fn input_error_message(mut self, message: impl Into<String>) -> Self {
        if let Some(ref mut input) = self.config.agent.schema.input {
            input.error_message = Some(message.into());
        }
        self
    }

    /// Set custom error message for output validation
    pub fn output_error_message(mut self, message: impl Into<String>) -> Self {
        if let Some(ref mut output) = self.config.agent.schema.output {
            output.error_message = Some(message.into());
        }
        self
    }

    /// Set LLM provider and model
    pub fn llm(mut self, provider: LlmProvider, model: impl Into<String>) -> Self {
        if let Some(ref mut llm_config) = self.config.llm {
            llm_config.provider = provider;
            llm_config.model = model.into();
        } else {
            self.config.llm = Some(LlmConfig {
                provider,
                model: model.into(),
                parameters: ModelParameters::default(),
                auth: LlmAuth::default(),
            });
        }
        self
    }

    /// Set LLM API key
    pub fn llm_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.ensure_llm_config();
        if let Some(ref mut llm_config) = self.config.llm {
            llm_config.auth.api_key = Some(api_key.into());
        }
        self
    }

    /// Set LLM API key from environment variable
    pub fn llm_api_key_env(mut self, env_var: impl Into<String>) -> Self {
        self.ensure_llm_config();
        if let Some(ref mut llm_config) = self.config.llm {
            llm_config.auth.api_key_env = Some(env_var.into());
        }
        self
    }

    /// Configure LLM parameters
    pub fn llm_parameters(mut self, params: ModelParameters) -> Self {
        self.ensure_llm_config();
        if let Some(ref mut llm_config) = self.config.llm {
            llm_config.parameters = params;
        }
        self
    }

    /// Set LLM temperature
    pub fn temperature(mut self, temperature: f32) -> Self {
        self.ensure_llm_config();
        if let Some(ref mut llm_config) = self.config.llm {
            llm_config.parameters.temperature = temperature;
        }
        self
    }

    /// Set LLM max tokens
    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.ensure_llm_config();
        if let Some(ref mut llm_config) = self.config.llm {
            llm_config.parameters.max_tokens = max_tokens;
        }
        self
    }

    /// Ensure LLM config exists, creating a default one if needed
    fn ensure_llm_config(&mut self) {
        if self.config.llm.is_none() {
            self.config.llm = Some(LlmConfig {
                provider: LlmProvider::OpenAI,
                model: "gpt-4.1-mini".to_string(),
                parameters: ModelParameters::default(),
                auth: LlmAuth::default(),
            });
        }
    }

    /// Enable RAG with configuration
    pub fn enable_rag(mut self, rag_config: RagSystemConfig) -> Self {
        self.config.rag = Some(rag_config);
        self
    }

    /// Add MCP server
    pub fn add_mcp_server(mut self, server: McpServerConfig) -> Self {
        self.config.mcp_servers.push(server);
        self
    }

    /// Configure tools
    pub fn tools(mut self, tools_config: ToolsConfig) -> Self {
        self.config.tools = tools_config;
        self
    }

    /// Set environment variables
    pub fn environment_variables(mut self, vars: HashMap<String, String>) -> Self {
        self.config.environment.variables = vars;
        self
    }

    /// Add environment file
    pub fn add_env_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.environment.env_files.push(path.into());
        self
    }

    /// Configure logging
    pub fn logging(mut self, logging_config: LoggingConfig) -> Self {
        self.config.logging = logging_config;
        self
    }

    /// Set log level
    pub fn log_level(mut self, level: impl Into<String>) -> Self {
        self.config.logging.level = level.into();
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<GolaConfig, AgentError> {
        self.config.validate()?;
        Ok(self.config)
    }

    /// Build the configuration without validation
    pub fn build_unchecked(self) -> GolaConfig {
        self.config
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Schema builder for creating JSON schemas programmatically
#[derive(Debug, Clone)]
pub struct SchemaBuilder {
    schema: Value,
}

impl SchemaBuilder {
    /// Create a new schema builder
    pub fn new() -> Self {
        Self {
            schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    /// Set schema type
    pub fn schema_type(mut self, schema_type: &str) -> Self {
        self.schema["type"] = Value::String(schema_type.to_string());
        self
    }

    /// Add a property to the schema
    pub fn add_property(mut self, name: &str, property_schema: Value) -> Self {
        if let Some(properties) = self.schema["properties"].as_object_mut() {
            properties.insert(name.to_string(), property_schema);
        }
        self
    }

    /// Add a required field
    pub fn add_required(mut self, field: &str) -> Self {
        if let Some(required) = self.schema["required"].as_array_mut() {
            required.push(Value::String(field.to_string()));
        }
        self
    }

    /// Add string property
    pub fn add_string_property(self, name: &str, description: Option<&str>) -> Self {
        let mut prop = serde_json::json!({
            "type": "string"
        });
        if let Some(desc) = description {
            prop["description"] = Value::String(desc.to_string());
        }
        self.add_property(name, prop)
    }

    /// Add number property
    pub fn add_number_property(self, name: &str, description: Option<&str>) -> Self {
        let mut prop = serde_json::json!({
            "type": "number"
        });
        if let Some(desc) = description {
            prop["description"] = Value::String(desc.to_string());
        }
        self.add_property(name, prop)
    }

    /// Add boolean property
    pub fn add_boolean_property(self, name: &str, description: Option<&str>) -> Self {
        let mut prop = serde_json::json!({
            "type": "boolean"
        });
        if let Some(desc) = description {
            prop["description"] = Value::String(desc.to_string());
        }
        self.add_property(name, prop)
    }

    /// Add array property
    pub fn add_array_property(self, name: &str, items_schema: Value, description: Option<&str>) -> Self {
        let mut prop = serde_json::json!({
            "type": "array",
            "items": items_schema
        });
        if let Some(desc) = description {
            prop["description"] = Value::String(desc.to_string());
        }
        self.add_property(name, prop)
    }

    /// Add object property
    pub fn add_object_property(self, name: &str, properties: Value, description: Option<&str>) -> Self {
        let mut prop = serde_json::json!({
            "type": "object",
            "properties": properties
        });
        if let Some(desc) = description {
            prop["description"] = Value::String(desc.to_string());
        }
        self.add_property(name, prop)
    }

    /// Set schema title
    pub fn title(mut self, title: &str) -> Self {
        self.schema["title"] = Value::String(title.to_string());
        self
    }

    /// Set schema description
    pub fn description(mut self, description: &str) -> Self {
        self.schema["description"] = Value::String(description.to_string());
        self
    }

    /// Build the schema
    pub fn build(self) -> Value {
        self.schema
    }
}

impl Default for SchemaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_config_builder_basic() {
        let config = ConfigBuilder::new()
            .agent_name("test_agent")
            .agent_description("A test agent")
            .max_steps(5)
            .temperature(0.8)
            .build()
            .unwrap();

        assert_eq!(config.agent.name, "test_agent");
        assert_eq!(config.agent.description, "A test agent");
        assert_eq!(config.agent.max_steps, 5);
        assert_eq!(config.llm.as_ref().unwrap().parameters.temperature, 0.8);
    }

    #[test]
    fn test_config_builder_with_schema() {
        let input_schema = json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The user query"
                }
            },
            "required": ["query"]
        });

        let output_schema = json!({
            "type": "object",
            "properties": {
                "response": {
                    "type": "string",
                    "description": "The agent response"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1
                }
            },
            "required": ["response"]
        });

        let config = ConfigBuilder::new()
            .agent_name("schema_agent")
            .input_schema(input_schema)
            .output_schema(output_schema)
            .input_schema_strict(true)
            .output_schema_strict(false)
            .enable_output_auto_correct()
            .build()
            .unwrap();

        assert!(config.agent.schema.enabled);
        assert!(config.agent.schema.input.is_some());
        assert!(config.agent.schema.output.is_some());
        assert!(config.agent.schema.input.as_ref().unwrap().strict);
        assert!(!config.agent.schema.output.as_ref().unwrap().strict);
        assert!(config.agent.schema.output.as_ref().unwrap().auto_correct);
    }

    #[test]
    fn test_schema_builder() {
        let schema = SchemaBuilder::new()
            .title("User Input Schema")
            .description("Schema for validating user input")
            .add_string_property("name", Some("User's name"))
            .add_number_property("age", Some("User's age"))
            .add_boolean_property("active", Some("Whether user is active"))
            .add_required("name")
            .add_required("age")
            .build();

        assert_eq!(schema["title"], "User Input Schema");
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["name"].is_object());
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert!(schema["required"].as_array().unwrap().contains(&json!("name")));
        assert!(schema["required"].as_array().unwrap().contains(&json!("age")));
    }

    #[test]
    fn test_input_schema_from_str() {
        let schema_str = r#"{
            "type": "object",
            "properties": {
                "message": {"type": "string"}
            },
            "required": ["message"]
        }"#;

        let config = ConfigBuilder::new()
            .input_schema_from_str(schema_str)
            .unwrap()
            .build()
            .unwrap();

        assert!(config.agent.schema.enabled);
        assert!(config.agent.schema.input.is_some());
        let input_schema = config.agent.schema.input.unwrap();
        assert_eq!(input_schema.schema["type"], "object");
    }

    #[test]
    fn test_schema_validation_config() {
        let validation_config = SchemaValidationConfig {
            log_errors: true,
            include_schema_in_errors: true,
            max_validation_attempts: 5,
            validate_intermediate_steps: true,
        };

        let config = ConfigBuilder::new()
            .enable_schema_validation()
            .schema_validation(validation_config.clone())
            .build()
            .unwrap();

        assert!(config.agent.schema.enabled);
        assert_eq!(config.agent.schema.validation.max_validation_attempts, 5);
        assert!(config.agent.schema.validation.include_schema_in_errors);
        assert!(config.agent.schema.validation.validate_intermediate_steps);
    }
}
