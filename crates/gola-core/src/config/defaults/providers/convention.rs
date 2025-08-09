//! Convention-based default providers
//!
//! These providers analyze project structure and patterns to provide
//! intelligent defaults based on detected conventions.

use crate::config::defaults::traits::{DefaultProvider, DefaultPriority, DefaultsContext};
use crate::config::types::*;
use crate::errors::AgentError;
use std::collections::HashMap;

/// Convention-based defaults for the complete Gola configuration
pub struct ConventionGolaProvider;

impl ConventionGolaProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<GolaConfig> for ConventionGolaProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Convention
    }
    
    fn can_provide(&self, context: &DefaultsContext) -> bool {
        // Can provide if we detect any project structure patterns
        context.project_info.is_rust_project 
            || context.project_info.is_node_project 
            || context.project_info.is_python_project
    }
    
    fn provide_defaults(&self, context: &DefaultsContext) -> Result<GolaConfig, AgentError> {
        // Build configuration based on detected project type
        let config = GolaConfig {
            agent: AgentDefinition {
                name: context.project_info.project_name.clone().unwrap_or_else(|| context.get_project_name()),
                description: format!("AI agent for {}", context.get_project_name()),
                max_steps: if context.project_info.is_rust_project { 15 } else { 10 },
                schema: SchemaConfig {
                    enabled: false,
                    input: None,
                    output: None,
                    validation: SchemaValidationConfig {
                        log_errors: true,
                        include_schema_in_errors: false,
                        max_validation_attempts: 3,
                        validate_intermediate_steps: false,
                    },
                },
                behavior: AgentBehavior {
                    verbose: context.is_development(),
                    show_reasoning: context.is_development(),
                    tool_timeout: if context.project_info.is_rust_project { 60 } else { 30 },
                    continue_on_error: false,
                    memory: MemoryConfig::default(),
                },
            },
            llm: Some(ConventionLlmProvider::new().provide_defaults(context)?),
            tools: ConventionToolsProvider::new().provide_defaults(context)?,
            rag: Some(ConventionRagProvider::new().provide_defaults(context)?),
            prompts: Some(ConventionPromptProvider::new().provide_defaults(context)?),
            mcp_servers: vec![],
            environment: ConventionEnvironmentProvider::new().provide_defaults(context)?,
            logging: ConventionLoggingProvider::new().provide_defaults(context)?,
            tracing: TracingConfig {
                enabled: false,
                trace_file: "gola_trace.jsonl".to_string(),
                model_provider: "openai".to_string(),
            },
        };
        
        Ok(config)
    }
    
    fn description(&self) -> &'static str {
        "Convention-based defaults using project structure analysis"
    }
}

impl ConventionGolaProvider {
    #[allow(dead_code)]
    fn get_project_tags(&self, context: &DefaultsContext) -> Vec<String> {
        let mut tags = vec!["ai".to_string(), "assistant".to_string()];
        
        if context.project_info.is_rust_project {
            tags.push("rust".to_string());
        }
        if context.project_info.is_node_project {
            tags.push("nodejs".to_string());
        }
        if context.project_info.is_python_project {
            tags.push("python".to_string());
        }
        
        tags
    }
    
    #[allow(dead_code)]
    fn get_project_capabilities(&self, context: &DefaultsContext) -> Vec<String> {
        let mut capabilities = vec!["reasoning".to_string(), "tool_usage".to_string()];
        
        if context.project_info.is_rust_project {
            capabilities.push("systems_programming".to_string());
            capabilities.push("performance_analysis".to_string());
        }
        if context.project_info.is_node_project {
            capabilities.push("web_development".to_string());
            capabilities.push("api_integration".to_string());
        }
        if context.project_info.is_python_project {
            capabilities.push("data_analysis".to_string());
            capabilities.push("machine_learning".to_string());
        }
        
        capabilities
    }
}

/// Convention-based defaults for LLM configuration
pub struct ConventionLlmProvider;

impl ConventionLlmProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<LlmConfig> for ConventionLlmProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Convention
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true
    }
    
    fn provide_defaults(&self, context: &DefaultsContext) -> Result<LlmConfig, AgentError> {
        // Choose model based on project complexity
        let (provider, model) = if context.project_info.is_rust_project {
            (LlmProvider::Anthropic, "claude-3-5-sonnet-latest".to_string())
        } else {
            (LlmProvider::OpenAI, "gpt-4.1-mini".to_string())
        };
        
        let api_key_env = self.get_api_key_env(&provider);
        let anthropic_version = if matches!(provider, LlmProvider::Anthropic) {
            Some("2023-06-01".to_string())
        } else {
            None
        };
        
        Ok(LlmConfig {
            provider,
            model,
            auth: LlmAuth {
                api_key: None,
                api_key_env: Some(api_key_env),
                headers: HashMap::new(),
            },
            parameters: ModelParameters {
                temperature: 0.7,
                max_tokens: if context.project_info.is_rust_project { 4096 } else { 2048 },
                top_p: 0.9,
                frequency_penalty: 0.0,
                presence_penalty: 0.0,
                stop_sequences: Vec::new(),
                anthropic_version,
                safety_settings: None,
                system_message: None,
            },
        })
    }
    
    fn description(&self) -> &'static str {
        "Convention-based LLM defaults using project analysis"
    }
}

impl ConventionLlmProvider {
    fn get_api_key_env(&self, provider: &LlmProvider) -> String {
        match provider {
            LlmProvider::OpenAI => "OPENAI_API_KEY".to_string(),
            LlmProvider::Anthropic => "ANTHROPIC_API_KEY".to_string(),
            LlmProvider::Gemini => "GEMINI_API_KEY".to_string(),
            LlmProvider::Custom { .. } => "CUSTOM_API_KEY".to_string(),
        }
    }
}

// Stub implementations for other convention providers
pub struct ConventionToolsProvider;
impl ConventionToolsProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<ToolsConfig> for ConventionToolsProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Convention }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { true }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<ToolsConfig, AgentError> {
        // TODO: Implement based on project type
        Err(AgentError::ConfigError("Convention tools provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Convention-based tools configuration" }
}

pub struct ConventionRagProvider;
impl ConventionRagProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<RagSystemConfig> for ConventionRagProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Convention }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { true }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<RagSystemConfig, AgentError> {
        // TODO: Implement based on project documentation structure
        Err(AgentError::ConfigError("Convention RAG provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Convention-based RAG configuration" }
}

pub struct ConventionPromptProvider;
impl ConventionPromptProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<PromptConfig> for ConventionPromptProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Convention }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { true }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<PromptConfig, AgentError> {
        // TODO: Implement based on project type and README analysis
        Err(AgentError::ConfigError("Convention prompt provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Convention-based prompt configuration" }
}

pub struct ConventionEnvironmentProvider;
impl ConventionEnvironmentProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<EnvironmentConfig> for ConventionEnvironmentProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Convention }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { true }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<EnvironmentConfig, AgentError> {
        // TODO: Implement based on .env file detection
        Err(AgentError::ConfigError("Convention environment provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Convention-based environment configuration" }
}

pub struct ConventionLoggingProvider;
impl ConventionLoggingProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<LoggingConfig> for ConventionLoggingProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Convention }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { true }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<LoggingConfig, AgentError> {
        // TODO: Implement based on project logging patterns
        Err(AgentError::ConfigError("Convention logging provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Convention-based logging configuration" }
}