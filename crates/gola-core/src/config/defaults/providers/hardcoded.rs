//! Hardcoded default providers
//!
//! These providers offer the most basic, universally applicable defaults
//! that work in any environment with minimal configuration.

use crate::config::defaults::traits::{DefaultProvider, DefaultPriority, DefaultsContext};
use crate::config::types::*;
use crate::errors::AgentError;
use std::collections::HashMap;
use std::path::PathBuf;

/// Hardcoded defaults for the complete Gola configuration
pub struct HardcodedGolaProvider;

impl HardcodedGolaProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<GolaConfig> for HardcodedGolaProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Hardcoded
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true // Always available as fallback
    }
    
    fn provide_defaults(&self, context: &DefaultsContext) -> Result<GolaConfig, AgentError> {
        Ok(GolaConfig {
            agent: AgentDefinition {
                name: context.get_project_name(),
                description: "AI agent powered by Gola".to_string(),
                max_steps: 10,
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
                    tool_timeout: 30,
                    continue_on_error: false,
                    memory: MemoryConfig::default(),
                },
            },
            llm: Some(HardcodedLlmProvider::new().provide_defaults(context)?),
            tools: HardcodedToolsProvider::new().provide_defaults(context)?,
            rag: Some(HardcodedRagProvider::new().provide_defaults(context)?),
            prompts: Some(HardcodedPromptProvider::new().provide_defaults(context)?),
            mcp_servers: vec![],
            environment: HardcodedEnvironmentProvider::new().provide_defaults(context)?,
            logging: HardcodedLoggingProvider::new().provide_defaults(context)?,
            tracing: TracingConfig {
                enabled: false,
                trace_file: "gola_trace.jsonl".to_string(),
                model_provider: "openai".to_string(),
            },
        })
    }
    
    fn description(&self) -> &'static str {
        "Hardcoded universal defaults for Gola configuration"
    }
}

/// Hardcoded defaults for LLM configuration
pub struct HardcodedLlmProvider;

impl HardcodedLlmProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<LlmConfig> for HardcodedLlmProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Hardcoded
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true
    }
    
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<LlmConfig, AgentError> {
        Ok(LlmConfig {
            provider: LlmProvider::OpenAI,
            model: "gpt-4.1-mini".to_string(),
            auth: LlmAuth {
                api_key: None,
                api_key_env: Some("OPENAI_API_KEY".to_string()),
                headers: HashMap::new(),
            },
            parameters: ModelParameters {
                temperature: 0.7,
                max_tokens: 16000,
                top_p: 1.0,
                frequency_penalty: 0.0,
                presence_penalty: 0.0,
                stop_sequences: Vec::new(),
                // Provider-specific defaults
                anthropic_version: None,
                safety_settings: None,
                system_message: None,
            },
        })
    }
    
    fn description(&self) -> &'static str {
        "Hardcoded OpenAI GPT-4o-mini defaults"
    }
}

/// Hardcoded defaults for tools configuration
pub struct HardcodedToolsProvider;

impl HardcodedToolsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<ToolsConfig> for HardcodedToolsProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Hardcoded
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true
    }
    
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<ToolsConfig, AgentError> {
        Ok(ToolsConfig {
            calculator: true,
            web_search: None,
            code_execution: None,
        })
    }
    
    fn description(&self) -> &'static str {
        "Hardcoded basic tools configuration"
    }
}

/// Hardcoded defaults for RAG system configuration
pub struct HardcodedRagProvider;

impl HardcodedRagProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<RagSystemConfig> for HardcodedRagProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Hardcoded
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true
    }
    
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<RagSystemConfig, AgentError> {
        Ok(RagSystemConfig {
            enabled: false,
            embeddings: EmbeddingConfig {
                provider: EmbeddingProvider::OpenAI,
                model: "text-embedding-ada-002".to_string(),
                dimension: 1536,
                batch_size: 100,
                auth: EmbeddingAuth::default(),
            },
            text_processing: TextProcessingConfig {
                chunk_size: 1000,
                chunk_overlap: 200,
                splitter_type: TextSplitterType::Basic,
            },
            vector_store: VectorStoreConfig {
                store_type: VectorStoreType::InMemory,
                persistence: Some(PersistenceConfig {
                    system_path: PathBuf::from("./data/rag_system"),
                    vector_store_path: PathBuf::from("./data/vector_store"),
                    mode: PersistenceMode::CreateOrLoad,
                }),
            },
            document_sources: vec![],
            retrieval: RetrievalConfig {
                top_k: 5,
                similarity_threshold: 0.7,
                enable_reranking: false,
                reranker_model: None,
            },
            embedding_cache: crate::rag::cache::EmbeddingCacheConfig::default(),
        })
    }
    
    fn description(&self) -> &'static str {
        "Hardcoded RAG system defaults (disabled)"
    }
}

/// Hardcoded defaults for prompts configuration
pub struct HardcodedPromptProvider;

impl HardcodedPromptProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<PromptConfig> for HardcodedPromptProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Hardcoded
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true
    }
    
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<PromptConfig, AgentError> {
        let mut templates = HashMap::new();
        templates.insert(
            "system_default".to_string(),
            "You are a helpful AI assistant. Use the available tools when needed to provide accurate and helpful responses.".to_string(),
        );
        templates.insert(
            "error_handling".to_string(),
            "I encountered an error: {{error}}. Let me try a different approach.".to_string(),
        );
        
        Ok(PromptConfig {
            template_vars: Some(HashMap::new()),
            fragments: Some(templates),
            roles: Some(RolePrompts::default()),
            purposes: Some(HashMap::new()),
        })
    }
    
    fn description(&self) -> &'static str {
        "Hardcoded basic prompt templates"
    }
}

/// Hardcoded defaults for environment configuration
pub struct HardcodedEnvironmentProvider;

impl HardcodedEnvironmentProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<EnvironmentConfig> for HardcodedEnvironmentProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Hardcoded
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true
    }
    
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<EnvironmentConfig, AgentError> {
        Ok(EnvironmentConfig {
            variables: HashMap::new(),
            env_files: vec![],
            load_system_env: true,
        })
    }
    
    fn description(&self) -> &'static str {
        "Hardcoded environment configuration defaults"
    }
}

/// Hardcoded defaults for logging configuration
pub struct HardcodedLoggingProvider;

impl HardcodedLoggingProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<LoggingConfig> for HardcodedLoggingProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Hardcoded
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        true
    }
    
    fn provide_defaults(&self, context: &DefaultsContext) -> Result<LoggingConfig, AgentError> {
        let level = if context.is_development() {
            "debug".to_string()
        } else if context.is_production() {
            "warn".to_string()
        } else {
            "info".to_string()
        };
        
        Ok(LoggingConfig {
            level,
            format: if context.is_development() { "pretty".to_string() } else { "json".to_string() },
            file: None,
            colored: context.is_development(),
        })
    }
    
    fn description(&self) -> &'static str {
        "Hardcoded logging configuration defaults"
    }
}