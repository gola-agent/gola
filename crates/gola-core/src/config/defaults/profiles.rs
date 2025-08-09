//! Configuration profile management with inheritance chains
//!
//! This module implements a hierarchical profile system that enables configuration
//! reuse through inheritance. The design allows for base profiles (e.g., development,
//! production) that can be extended and specialized by child profiles. This approach
//! reduces configuration duplication and promotes consistency across environments.
//!
//! The profile resolution follows a depth-first inheritance chain, where child
//! configurations override parent values. This enables progressive refinement of
//! settings from general to specific use cases.

use crate::config::types::*;
use crate::errors::AgentError;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigProfile {
    pub name: String,
    pub description: Option<String>,
    pub inherits_from: Option<Vec<String>>,
    #[serde(flatten)]
    pub config: GolaConfig,
    pub metadata: Option<ProfileMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub tags: Option<Vec<String>>,
}

pub struct ProfileManager {
    profile_dir: PathBuf,
    profile_cache: HashMap<String, ConfigProfile>,
}

impl ProfileManager {
    /// Create a new profile manager with the default profile directory
    pub fn new() -> Result<Self, AgentError> {
        let profile_dir = Self::get_default_profile_dir()?;
        Self::with_profile_dir(profile_dir)
    }
    
    /// Create a new profile manager with a specific profile directory
    pub fn with_profile_dir(profile_dir: PathBuf) -> Result<Self, AgentError> {
        // Create the profile directory if it doesn't exist
        if !profile_dir.exists() {
            fs::create_dir_all(&profile_dir)
                .map_err(|e| AgentError::ConfigError(format!("Failed to create profile directory: {}", e)))?;
        }
        
        Ok(Self {
            profile_dir,
            profile_cache: HashMap::new(),
        })
    }
    
    /// Get the default profile directory
    pub fn get_default_profile_dir() -> Result<PathBuf, AgentError> {
        // Try to use XDG config directory first, then fall back to home directory
        let config_dir = dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .ok_or_else(|| AgentError::ConfigError("Could not determine config directory".to_string()))?;
        
        Ok(config_dir.join("gola").join("profiles"))
    }
    
    /// Load a profile by name
    pub fn load_profile(&mut self, name: &str) -> Result<ConfigProfile, AgentError> {
        // Check cache first
        if let Some(profile) = self.profile_cache.get(name) {
            return Ok(profile.clone());
        }
        
        // Load from file
        let profile_path = self.profile_dir.join(format!("{}.yaml", name));
        if !profile_path.exists() {
            return Err(AgentError::ConfigError(format!("Profile '{}' not found", name)));
        }
        
        let profile_content = fs::read_to_string(&profile_path)
            .map_err(|e| AgentError::ConfigError(format!("Failed to read profile '{}': {}", name, e)))?;
        
        let mut profile: ConfigProfile = serde_yaml::from_str(&profile_content)
            .map_err(|e| AgentError::ConfigError(format!("Failed to parse profile '{}': {}", name, e)))?;
        
        // Set the name if not already set
        if profile.name.is_empty() {
            profile.name = name.to_string();
        }
        
        // Cache the profile
        self.profile_cache.insert(name.to_string(), profile.clone());
        
        Ok(profile)
    }
    
    /// Resolve a profile with all its inheritance
    pub fn resolve_profile(&mut self, name: &str) -> Result<GolaConfig, AgentError> {
        let profile = self.load_profile(name)?;
        self.resolve_profile_inheritance(&profile)
    }
    
    /// Resolve profile inheritance recursively
    fn resolve_profile_inheritance(&mut self, profile: &ConfigProfile) -> Result<GolaConfig, AgentError> {
        let mut visited = HashSet::new();
        self.resolve_profile_inheritance_recursive(profile, &mut visited)
    }
    
    /// Recursive helper for resolving profile inheritance
    fn resolve_profile_inheritance_recursive(
        &mut self,
        profile: &ConfigProfile,
        visited: &mut HashSet<String>,
    ) -> Result<GolaConfig, AgentError> {
        // Detect cycles
        if visited.contains(&profile.name) {
            return Err(AgentError::ConfigError(format!(
                "Circular dependency detected in profile inheritance: {}",
                profile.name
            )));
        }
        visited.insert(profile.name.clone());
        
        let mut resolved_config = profile.config.clone();
        
        // Process parent profiles in order
        if let Some(parents) = &profile.inherits_from {
            for parent_name in parents {
                let parent_profile = self.load_profile(parent_name)?;
                let parent_config = self.resolve_profile_inheritance_recursive(&parent_profile, visited)?;
                
                // Merge parent config with current (current takes precedence)
                resolved_config = self.merge_configs(parent_config, resolved_config)?;
            }
        }
        
        visited.remove(&profile.name);
        Ok(resolved_config)
    }
    
    /// Merge two configurations, with the second taking precedence
    fn merge_configs(&self, base: GolaConfig, override_config: GolaConfig) -> Result<GolaConfig, AgentError> {
        // This is a simplified merge - a full implementation would need to handle
        // deep merging of nested structures
        Ok(GolaConfig {
            agent: override_config.agent,
            llm: override_config.llm,
            tools: override_config.tools,
            rag: override_config.rag.or(base.rag),
            prompts: override_config.prompts.or(base.prompts),
            mcp_servers: override_config.mcp_servers,
            environment: override_config.environment,
            logging: override_config.logging,
            tracing: override_config.tracing,
        })
    }
    
    /// Save a profile to disk
    pub fn save_profile(&mut self, profile: &ConfigProfile) -> Result<(), AgentError> {
        let profile_path = self.profile_dir.join(format!("{}.yaml", profile.name));
        
        let profile_content = serde_yaml::to_string(profile)
            .map_err(|e| AgentError::ConfigError(format!("Failed to serialize profile '{}': {}", profile.name, e)))?;
        
        fs::write(&profile_path, profile_content)
            .map_err(|e| AgentError::ConfigError(format!("Failed to write profile '{}': {}", profile.name, e)))?;
        
        // Update cache
        self.profile_cache.insert(profile.name.clone(), profile.clone());
        
        Ok(())
    }
    
    /// List available profiles
    pub fn list_profiles(&self) -> Result<Vec<String>, AgentError> {
        let mut profiles = Vec::new();
        
        for entry in fs::read_dir(&self.profile_dir)
            .map_err(|e| AgentError::ConfigError(format!("Failed to read profile directory: {}", e)))?
        {
            let entry = entry
                .map_err(|e| AgentError::ConfigError(format!("Failed to read directory entry: {}", e)))?;
            
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "yaml" || ext == "yml") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    profiles.push(name.to_string());
                }
            }
        }
        
        profiles.sort();
        Ok(profiles)
    }
    
    /// Delete a profile
    pub fn delete_profile(&mut self, name: &str) -> Result<(), AgentError> {
        let profile_path = self.profile_dir.join(format!("{}.yaml", name));
        
        if profile_path.exists() {
            fs::remove_file(&profile_path)
                .map_err(|e| AgentError::ConfigError(format!("Failed to delete profile '{}': {}", name, e)))?;
        }
        
        // Remove from cache
        self.profile_cache.remove(name);
        
        Ok(())
    }
    
    /// Create default built-in profiles
    pub fn create_builtin_profiles(&mut self) -> Result<(), AgentError> {
        let profiles = vec![
            self.create_development_profile(),
            self.create_production_profile(),
            self.create_research_profile(),
        ];
        
        for profile in profiles {
            if !self.profile_dir.join(format!("{}.yaml", profile.name)).exists() {
                self.save_profile(&profile)?;
            }
        }
        
        Ok(())
    }
    
    /// Create a development profile
    fn create_development_profile(&self) -> ConfigProfile {
        ConfigProfile {
            name: "development".to_string(),
            description: Some("Development environment with debug logging and relaxed settings".to_string()),
            inherits_from: None,
            config: GolaConfig {
                agent: AgentDefinition {
                    name: "dev-agent".to_string(),
                    description: "Development AI agent".to_string(),
                    max_steps: 15,
                    schema: SchemaConfig {
                        enabled: false,
                        input: None,
                        output: None,
                        validation: SchemaValidationConfig {
                            log_errors: true,
                            include_schema_in_errors: true,
                            max_validation_attempts: 3,
                            validate_intermediate_steps: false,
                        },
                    },
                    behavior: AgentBehavior {
                        verbose: true,
                        show_reasoning: true,
                        tool_timeout: 60,
                        continue_on_error: false,
                        memory: MemoryConfig::default(),
                    },
                },
                llm: Some(LlmConfig {
                    provider: LlmProvider::OpenAI,
                    model: "gpt-4.1-mini".to_string(),
                    auth: LlmAuth {
                        api_key: None,
                        api_key_env: Some("OPENAI_API_KEY".to_string()),
                        headers: HashMap::new(),
                    },
                    parameters: ModelParameters::default(),
                }),
                tools: ToolsConfig {
                    calculator: true,
                    web_search: None,
                    code_execution: None,
                },
                rag: None,
                prompts: None,
                mcp_servers: vec![],
                environment: EnvironmentConfig {
                    variables: HashMap::new(),
                    env_files: vec![],
                    load_system_env: true,
                },
                logging: LoggingConfig {
                    level: "debug".to_string(),
                    format: "pretty".to_string(),
                    file: None,
                    colored: true,
                },
                tracing: TracingConfig {
                    enabled: false,
                    trace_file: "gola_trace.jsonl".to_string(),
                    model_provider: "openai".to_string(),
                },
            },
            metadata: Some(ProfileMetadata {
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                modified_at: None,
                author: Some("Gola".to_string()),
                version: Some("1.0.0".to_string()),
                tags: Some(vec!["builtin".to_string(), "development".to_string()]),
            }),
        }
    }
    
    /// Create a production profile
    fn create_production_profile(&self) -> ConfigProfile {
        ConfigProfile {
            name: "production".to_string(),
            description: Some("Production environment with optimized settings and structured logging".to_string()),
            inherits_from: None,
            config: GolaConfig {
                agent: AgentDefinition {
                    name: "prod-agent".to_string(),
                    description: "Production AI agent".to_string(),
                    max_steps: 10,
                    schema: SchemaConfig {
                        enabled: true,
                        input: None,
                        output: None,
                        validation: SchemaValidationConfig {
                            log_errors: true,
                            include_schema_in_errors: false,
                            max_validation_attempts: 1,
                            validate_intermediate_steps: false,
                        },
                    },
                    behavior: AgentBehavior {
                        verbose: false,
                        show_reasoning: false,
                        tool_timeout: 30,
                        continue_on_error: false,
                        memory: MemoryConfig::default(),
                    },
                },
                llm: Some(LlmConfig {
                    provider: LlmProvider::Anthropic,
                    model: "claude-3-5-sonnet-latest".to_string(),
                    auth: LlmAuth {
                        api_key: None,
                        api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
                        headers: HashMap::new(),
                    },
                    parameters: ModelParameters {
                        temperature: 0.3,
                        max_tokens: 4096,
                        anthropic_version: Some("2023-06-01".to_string()),
                        ..Default::default()
                    },
                }),
                tools: ToolsConfig {
                    calculator: true,
                    web_search: None,
                    code_execution: None,
                },
                rag: None,
                prompts: None,
                mcp_servers: vec![],
                environment: EnvironmentConfig {
                    variables: HashMap::new(),
                    env_files: vec![],
                    load_system_env: true,
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                    format: "json".to_string(),
                    file: Some(PathBuf::from("./logs/gola.log")),
                    colored: false,
                },
                tracing: TracingConfig {
                    enabled: false,
                    trace_file: "gola_trace.jsonl".to_string(),
                    model_provider: "openai".to_string(),
                },
            },
            metadata: Some(ProfileMetadata {
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                modified_at: None,
                author: Some("Gola".to_string()),
                version: Some("1.0.0".to_string()),
                tags: Some(vec!["builtin".to_string(), "production".to_string()]),
            }),
        }
    }
    
    /// Create a research profile
    fn create_research_profile(&self) -> ConfigProfile {
        ConfigProfile {
            name: "research".to_string(),
            description: Some("Research environment with RAG enabled and comprehensive tools".to_string()),
            inherits_from: Some(vec!["development".to_string()]),
            config: GolaConfig {
                agent: AgentDefinition {
                    name: "research-agent".to_string(),
                    description: "Research AI agent with knowledge retrieval".to_string(),
                    max_steps: 20,
                    schema: SchemaConfig {
                        enabled: false,
                        input: None,
                        output: None,
                        validation: SchemaValidationConfig {
                            log_errors: true,
                            include_schema_in_errors: true,
                            max_validation_attempts: 3,
                            validate_intermediate_steps: false,
                        },
                    },
                    behavior: AgentBehavior {
                        verbose: true,
                        show_reasoning: true,
                        tool_timeout: 120,
                        continue_on_error: false,
                        memory: MemoryConfig::default(),
                    },
                },
                llm: Some(LlmConfig {
                    provider: LlmProvider::Anthropic,
                    model: "claude-3-5-sonnet-latest".to_string(),
                    auth: LlmAuth {
                        api_key: None,
                        api_key_env: Some("ANTHROPIC_API_KEY".to_string()),
                        headers: HashMap::new(),
                    },
                    parameters: ModelParameters {
                        temperature: 0.7,
                        max_tokens: 8192,
                        anthropic_version: Some("2023-06-01".to_string()),
                        ..Default::default()
                    },
                }),
                tools: ToolsConfig {
                    calculator: true,
                    web_search: Some(WebSearchConfig {
                        enabled: true,
                        provider: WebSearchProvider::DuckDuckGo,
                        auth: WebSearchAuth::default(),
                        max_results: 10,
                    }),
                    code_execution: Some(CodeExecutionConfig {
                        enabled: true,
                        backend: CodeExecutionBackend::Docker,
                        timeout: 120,
                        allowed_languages: vec!["python".to_string(), "javascript".to_string(), "bash".to_string()],
                    }),
                },
                rag: Some(RagSystemConfig {
                    enabled: true,
                    embeddings: EmbeddingConfig {
                        provider: EmbeddingProvider::OpenAI,
                        model: "text-embedding-ada-002".to_string(),
                        dimension: 1536,
                        batch_size: 50,
                        auth: EmbeddingAuth::default(),
                    },
                    text_processing: TextProcessingConfig {
                        chunk_size: 1500,
                        chunk_overlap: 300,
                        splitter_type: TextSplitterType::Basic,
                    },
                    vector_store: VectorStoreConfig {
                        store_type: VectorStoreType::InMemory,
                        persistence: Some(PersistenceConfig {
                            system_path: PathBuf::from("./data/research_rag_system"),
                            vector_store_path: PathBuf::from("./data/research_vector_store"),
                            mode: PersistenceMode::CreateOrLoad,
                        }),
                    },
                    document_sources: vec![],
                    retrieval: RetrievalConfig {
                        top_k: 10,
                        similarity_threshold: 0.6,
                        enable_reranking: true,
                        reranker_model: None,
                    },
                    embedding_cache: crate::rag::cache::EmbeddingCacheConfig::default(),
                }),
                prompts: None,
                mcp_servers: vec![],
                environment: EnvironmentConfig {
                    variables: HashMap::new(),
                    env_files: vec![],
                    load_system_env: true,
                },
                logging: LoggingConfig {
                    level: "info".to_string(),
                    format: "pretty".to_string(),
                    file: None,
                    colored: true,
                },
                tracing: TracingConfig {
                    enabled: false,
                    trace_file: "gola_trace.jsonl".to_string(),
                    model_provider: "openai".to_string(),
                },
            },
            metadata: Some(ProfileMetadata {
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                modified_at: None,
                author: Some("Gola".to_string()),
                version: Some("1.0.0".to_string()),
                tags: Some(vec!["builtin".to_string(), "research".to_string(), "rag".to_string()]),
            }),
        }
    }
}

impl Default for ProfileManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default ProfileManager")
    }
}