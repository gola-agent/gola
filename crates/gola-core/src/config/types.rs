//! Configuration type definitions for the agent framework
//! 
//! This module provides the complete type system for agent configuration, supporting
//! hierarchical configuration with sensible defaults. The design enables both simple
//! configurations (minimal YAML) and complex enterprise setups with multiple LLM
//! providers, RAG systems, and MCP servers.
//!
//! The configuration system follows a layered approach where optional fields allow
//! progressive enhancement - start with minimal config and add complexity as needed.
//! Auto-detection of environment variables and project context reduces boilerplate.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use crate::errors::AgentError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GolaConfig {
    pub agent: AgentDefinition,
    #[serde(default)]
    pub llm: Option<LlmConfig>,
    #[serde(default)]
    pub prompts: Option<PromptConfig>,
    #[serde(default)]
    pub rag: Option<RagSystemConfig>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,
    #[serde(default)]
    pub schema: SchemaConfig,
    #[serde(default)]
    pub behavior: AgentBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptConfig {
    #[serde(default)]
    pub template_vars: Option<HashMap<String, String>>,
    #[serde(default)]
    pub fragments: Option<HashMap<String, String>>,
    #[serde(default)]
    pub roles: Option<RolePrompts>,
    #[serde(default)]
    pub purposes: Option<HashMap<String, PurposePrompt>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RolePrompts {
    #[serde(default)]
    pub system: Option<Vec<PromptSource>>,
    #[serde(skip)]
    pub assembled: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurposePrompt {
    pub role: String,
    #[serde(default)]
    pub assembly: Option<Vec<PromptSource>>,
}

/// Source for a piece of a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptSource {
    Fragment { fragment: String },
    File { file: String },
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentBehavior {
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub show_reasoning: bool,
    #[serde(default = "default_tool_timeout")]
    pub tool_timeout: u64,
    #[serde(default = "default_continue_on_error")]
    pub continue_on_error: bool,
    #[serde(default)]
    pub memory: MemoryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    #[serde(default)]
    pub parameters: ModelParameters,
    #[serde(default)]
    pub auth: LlmAuth,
}

/// LLM provider types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    OpenAI,
    Anthropic,
    Gemini,
    Custom {
        base_url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelParameters {
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_top_p")]
    pub top_p: f32,
    #[serde(default)]
    pub frequency_penalty: f32,
    #[serde(default)]
    pub presence_penalty: f32,
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<serde_json::Value>,
}

impl Default for ModelParameters {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            top_p: default_top_p(),
            frequency_penalty: 0.0,
            presence_penalty: 0.0,
            anthropic_version: None,
            system_message: None,
            stop_sequences: Vec::new(),
            safety_settings: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAuth {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// RAG system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagSystemConfig {
    #[serde(default)]
    pub enabled: bool,
    pub embeddings: EmbeddingConfig,
    #[serde(default)]
    pub text_processing: TextProcessingConfig,
    #[serde(default)]
    pub vector_store: VectorStoreConfig,
    #[serde(default)]
    pub document_sources: Vec<DocumentSource>,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub embedding_cache: crate::rag::cache::EmbeddingCacheConfig,
}

/// Embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: EmbeddingProvider,
    pub model: String,
    #[serde(default = "default_embedding_dimension")]
    pub dimension: usize,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default)]
    pub auth: EmbeddingAuth,
}

/// Embedding provider types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProvider {
    OpenAI,
    Cohere,
    HuggingFace,
    Simple,
    Custom {
        base_url: String,
    },
}

/// Embedding authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingAuth {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

/// Text processing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextProcessingConfig {
    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,
    #[serde(default)]
    pub splitter_type: TextSplitterType,
}

/// Text splitter types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSplitterType {
    Basic,
    Language { language: String },
    Semantic,
}

/// Vector store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorStoreConfig {
    #[serde(default)]
    pub store_type: VectorStoreType,
    #[serde(default)]
    pub persistence: Option<PersistenceConfig>,
}

/// Vector store types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VectorStoreType {
    InMemory,
    Persistent,
}

/// Persistence configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    pub system_path: PathBuf,
    pub vector_store_path: PathBuf,
    #[serde(default)]
    pub mode: PersistenceMode,
}

/// Persistence modes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PersistenceMode {
    Create,
    Load,
    CreateOrLoad,
}

/// Document source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSource {
    pub source_type: DocumentSourceType,
    #[serde(flatten)]
    pub config: DocumentSourceConfig,
}

/// Document source types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DocumentSourceType {
    Files { paths: Vec<PathBuf> },
    Directory { path: PathBuf, recursive: bool },
    Url { url: String },
    Inline { content: String, name: String },
}

/// Document source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSourceConfig {
    #[serde(default)]
    pub include_extensions: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Retrieval configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f32,
    #[serde(default)]
    pub enable_reranking: bool,
    #[serde(default)]
    pub reranker_model: Option<String>,
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default)]
    pub command: Option<McpCommand>,
    #[serde(default)]
    pub tools: ToolFilter,
    #[serde(default = "default_mcp_timeout")]
    pub timeout: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_mcp_description_token_limit")]
    pub description_token_limit: u32,
    #[serde(default)]
    pub execution_type: Option<McpExecutionType>,
    #[serde(default)]
    pub execution_environment: Option<McpExecutionEnvironment>,
}

/// MCP execution type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpExecutionType {
    Command {
        command: McpCommand,
    },
    Runtime {
        runtime: String,
        entry_point: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        working_dir: Option<PathBuf>,
    },
    NativeBinary {
        binary_name: String,
        installation_config: BinaryInstallationConfig,
        args: Vec<String>,
        env: HashMap<String, String>,
        working_dir: Option<PathBuf>,
    },
}

impl Default for McpExecutionType {
    fn default() -> Self {
        McpExecutionType::Command {
            command: McpCommand::default(),
        }
    }
}

/// MCP execution environment - clarifies where and how code runs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "environment", rename_all = "lowercase")]
pub enum McpExecutionEnvironment {
    /// Use runtime already installed on host system (e.g., system python, node)
    Host {
        runtime: String,
        entry_point: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        working_dir: Option<PathBuf>,
    },
    /// System downloads and manages runtime automatically (current "runtime" behavior)  
    Local {
        runtime: String,
        entry_point: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        working_dir: Option<PathBuf>,
    },
    /// Containerized execution using Docker
    Container {
        image: String,
        #[serde(default = "default_container_tag")]
        tag: String,
        entry_point: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        #[serde(default)]
        working_dir: Option<PathBuf>,
        #[serde(default)]
        volumes: Vec<String>,
    },
}

impl Default for McpExecutionEnvironment {
    fn default() -> Self {
        McpExecutionEnvironment::Host {
            runtime: "python".to_string(),
            entry_point: "".to_string(),
            args: vec![],
            env: HashMap::new(),
            working_dir: None,
        }
    }
}

fn default_container_tag() -> String {
    "latest".to_string()
}

/// Binary configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryConfig {
    pub name: String,
    pub source: BinarySource,
}

/// Binary source
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BinarySource {
    Docker {
        image: String,
        tag: String,
        container_name: String,
    },
    GitHub {
        repo: String,
        asset_name: String,
    },
}

/// Configuration for binary installation with multiple fallback strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryInstallationConfig {
    pub github_org: String,
    pub github_repo: String,
    #[serde(default)]
    pub asset_pattern: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default = "default_true")]
    pub fallback_to_docker: bool,
    #[serde(default)]
    pub fallback_to_source: bool,
    #[serde(default)]
    pub docker_registries: Vec<DockerRegistryConfig>,
    #[serde(default)]
    pub source_config: Option<SourceBuildConfig>,
}

/// Docker registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerRegistryConfig {
    pub registry_type: DockerRegistryType,
    #[serde(default)]
    pub custom_url: Option<String>,
}

/// Docker registry types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DockerRegistryType {
    /// GitHub Container Registry (ghcr.io)
    GitHubContainerRegistry,
    /// Docker Hub (docker.io)
    DockerHub,
    /// Custom registry
    Custom,
}

/// Source building configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceBuildConfig {
    pub repository: String,
    #[serde(default)]
    pub git_ref: Option<String>,
    pub build_image: String,
    pub build_command: String,
    pub binary_path: String,
    #[serde(default)]
    pub build_env: HashMap<String, String>,
}

impl Default for BinaryInstallationConfig {
    fn default() -> Self {
        Self {
            github_org: String::new(),
            github_repo: String::new(),
            asset_pattern: None,
            version: None,
            fallback_to_docker: true,
            fallback_to_source: false,
            docker_registries: vec![
                DockerRegistryConfig {
                    registry_type: DockerRegistryType::GitHubContainerRegistry,
                    custom_url: None,
                },
                DockerRegistryConfig {
                    registry_type: DockerRegistryType::DockerHub,
                    custom_url: None,
                },
            ],
            source_config: None,
        }
    }
}


/// MCP command configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpCommand {
    pub run: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    #[serde(default = "default_mcp_token_limit")]
    pub token_limit: u32,
}

/// Tool filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolFilter {
    All,
    Include(Vec<String>),
    Exclude(Vec<String>),
    Pattern(String),
}

/// Standard tools configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_true")]
    pub calculator: bool,
    #[serde(default)]
    pub web_search: Option<WebSearchConfig>,
    #[serde(default)]
    pub code_execution: Option<CodeExecutionConfig>,
}

/// Web search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub provider: WebSearchProvider,
    #[serde(default)]
    pub auth: WebSearchAuth,
    #[serde(default = "default_max_search_results")]
    pub max_results: usize,
}

/// Web search providers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebSearchProvider {
    DuckDuckGo,
    Tavily,
    Serper,
}

/// Web search authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchAuth {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

/// Code execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeExecutionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub backend: CodeExecutionBackend,
    #[serde(default = "default_code_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub allowed_languages: Vec<String>,
}

/// Code execution backends
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeExecutionBackend {
    Docker,
    Local,
}

/// Environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    #[serde(default)]
    pub variables: HashMap<String, String>,
    #[serde(default)]
    pub env_files: Vec<PathBuf>,
    #[serde(default = "default_true")]
    pub load_system_env: bool,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default)]
    pub file: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub colored: bool,
}

/// Tracing summarizer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_tracing_trace_file")]
    pub trace_file: String,
    #[serde(default = "default_tracing_model_provider")]
    pub model_provider: String,
}

fn default_tracing_trace_file() -> String {
    "tracing_trace.jsonl".to_string()
}

fn default_tracing_model_provider() -> String {
    "default".to_string() // "default" means use the main agent's LLM
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trace_file: default_tracing_trace_file(),
            model_provider: default_tracing_model_provider(),
        }
    }
}


/// Schema configuration for input and output validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub input: Option<InputSchemaConfig>,
    #[serde(default)]
    pub output: Option<OutputSchemaConfig>,
    #[serde(default)]
    pub validation: SchemaValidationConfig,
}

/// Input schema configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSchemaConfig {
    pub schema: serde_json::Value,
    #[serde(default = "default_true")]
    pub strict: bool,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub source: SchemaSource,
}

/// Output schema configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSchemaConfig {
    pub schema: serde_json::Value,
    #[serde(default = "default_true")]
    pub strict: bool,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub source: SchemaSource,
    #[serde(default)]
    pub auto_correct: bool,
}

/// Schema validation behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaValidationConfig {
    #[serde(default = "default_true")]
    pub log_errors: bool,
    #[serde(default)]
    pub include_schema_in_errors: bool,
    #[serde(default = "default_validation_attempts")]
    pub max_validation_attempts: usize,
    #[serde(default)]
    pub validate_intermediate_steps: bool,
}

/// Schema source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaSource {
    #[serde(default)]
    pub source_type: SchemaSourceType,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Schema source types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SchemaSourceType {
    Inline,
    File,
    Url,
    Registry,
}

/// Memory configuration for context window management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_max_history_steps")]
    pub max_history_steps: usize,
    #[serde(default)]
    pub eviction_strategy: MemoryEvictionStrategy,
    #[serde(default)]
    pub preserve_strategy: MemoryPreserveStrategy,
    #[serde(default = "default_min_recent_steps")]
    pub min_recent_steps: usize,
}

/// Strategy for evicting old memory when limit is reached
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryEvictionStrategy {
    /// Remove oldest steps first (FIFO)
    Fifo,
    /// Remove steps intelligently, preserving important context
    Intelligent,
    /// Remove steps in chunks to maintain conversation coherence
    ChunkBased,
    /// Summarize the oldest messages
    Summarize,
    /// Summarize the entire conversation
    ConversationSummary,
}

/// Strategy for preserving certain types of steps during eviction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPreserveStrategy {
    #[serde(default = "default_true")]
    pub preserve_initial_task: bool,
    #[serde(default)]
    pub preserve_successful_observations: bool,
    #[serde(default)]
    pub preserve_errors: bool,
    #[serde(default = "default_preserve_recent_count")]
    pub preserve_recent_count: usize,
}

impl Default for AgentBehavior {
    fn default() -> Self {
        Self {
            verbose: false,
            show_reasoning: false,
            tool_timeout: default_tool_timeout(),
            continue_on_error: default_continue_on_error(),
            memory: MemoryConfig::default(),
        }
    }
}


/// Configuration source types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigSourceType {
    File,
    Url,
    GitHub,
}

/// GitHub repository reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRef {
    pub owner: String,
    pub repo: String,
    #[serde(default = "default_git_ref")]
    pub git_ref: String,
    #[serde(default = "default_config_path")]
    pub config_path: String,
}

fn default_git_ref() -> String {
    "main".to_string()
}

fn default_config_path() -> String {
    "gola.yaml".to_string()
}


// Default value functions
fn default_max_steps() -> usize { 40 }
fn default_tool_timeout() -> u64 { 30 }
fn default_continue_on_error() -> bool { true }
fn default_temperature() -> f32 { 0.7 }
fn default_max_tokens() -> u32 { 16000 }
fn default_top_p() -> f32 { 1.0 }
fn default_embedding_dimension() -> usize { 1536 }
fn default_batch_size() -> usize { 100 }
fn default_chunk_size() -> usize { 1000 }
fn default_chunk_overlap() -> usize { 200 }
fn default_top_k() -> usize { 5 }
fn default_similarity_threshold() -> f32 { 0.7 }
fn default_mcp_timeout() -> u64 { 30 }
fn default_mcp_description_token_limit() -> u32 { 50 }
pub fn default_mcp_token_limit() -> u32 { 2000 }
fn default_true() -> bool { true }
fn default_max_search_results() -> usize { 5 }
fn default_code_timeout() -> u64 { 60 }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "pretty".to_string() }
fn default_validation_attempts() -> usize { 3 }
fn default_max_history_steps() -> usize { 50000 }
fn default_min_recent_steps() -> usize { 20 }
fn default_preserve_recent_count() -> usize { 5 }

impl Default for LlmAuth {
    fn default() -> Self {
        Self {
            api_key: None,
            api_key_env: None,
            headers: HashMap::new(),
        }
    }
}

impl Default for EmbeddingAuth {
    fn default() -> Self {
        Self {
            api_key: None,
            api_key_env: None,
        }
    }
}

impl Default for TextProcessingConfig {
    fn default() -> Self {
        Self {
            chunk_size: default_chunk_size(),
            chunk_overlap: default_chunk_overlap(),
            splitter_type: TextSplitterType::Basic,
        }
    }
}

impl Default for TextSplitterType {
    fn default() -> Self {
        TextSplitterType::Basic
    }
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            store_type: VectorStoreType::InMemory,
            persistence: None,
        }
    }
}

impl Default for VectorStoreType {
    fn default() -> Self {
        VectorStoreType::InMemory
    }
}

impl Default for PersistenceMode {
    fn default() -> Self {
        PersistenceMode::CreateOrLoad
    }
}

impl Default for DocumentSourceConfig {
    fn default() -> Self {
        Self {
            include_extensions: Vec::new(),
            exclude_patterns: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: default_top_k(),
            similarity_threshold: default_similarity_threshold(),
            enable_reranking: false,
            reranker_model: None,
        }
    }
}

impl Default for ToolFilter {
    fn default() -> Self {
        ToolFilter::All
    }
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            calculator: true,
            web_search: None,
            code_execution: None,
        }
    }
}

impl Default for WebSearchProvider {
    fn default() -> Self {
        WebSearchProvider::DuckDuckGo
    }
}

impl Default for WebSearchAuth {
    fn default() -> Self {
        Self {
            api_key: None,
            api_key_env: None,
        }
    }
}

impl Default for CodeExecutionBackend {
    fn default() -> Self {
        CodeExecutionBackend::Docker
    }
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            variables: HashMap::new(),
            env_files: Vec::new(),
            load_system_env: true,
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
            colored: true,
        }
    }
}
impl Default for SchemaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            input: None,
            output: None,
            validation: SchemaValidationConfig::default(),
        }
    }
}

impl Default for SchemaValidationConfig {
    fn default() -> Self {
        Self {
            log_errors: true,
            include_schema_in_errors: false,
            max_validation_attempts: default_validation_attempts(),
            validate_intermediate_steps: false,
        }
    }
}

impl Default for SchemaSource {
    fn default() -> Self {
        Self {
            source_type: SchemaSourceType::Inline,
            location: None,
            version: None,
            description: None,
        }
    }
}

impl Default for SchemaSourceType {
    fn default() -> Self {
        SchemaSourceType::Inline
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_history_steps: default_max_history_steps(),
            eviction_strategy: MemoryEvictionStrategy::Summarize,
            preserve_strategy: MemoryPreserveStrategy::default(),
            min_recent_steps: default_min_recent_steps(),
        }
    }
}

impl Default for MemoryEvictionStrategy {
    fn default() -> Self {
        MemoryEvictionStrategy::Intelligent
    }
}

impl Default for MemoryPreserveStrategy {
    fn default() -> Self {
        Self {
            preserve_initial_task: true,
            preserve_successful_observations: false,
            preserve_errors: false,
            preserve_recent_count: default_preserve_recent_count(),
        }
    }
}

impl GolaConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), AgentError> {
        // Validate agent configuration
        if self.agent.name.is_empty() {
            return Err(AgentError::ConfigError("Agent name cannot be empty".to_string()));
        }

        if self.agent.max_steps == 0 {
            return Err(AgentError::ConfigError("Agent max_steps must be greater than 0".to_string()));
        }

        // Validate LLM configuration if present
        if let Some(ref llm_config) = self.llm {
            if llm_config.model.is_empty() {
                return Err(AgentError::ConfigError("LLM model cannot be empty".to_string()));
            }
        }

        // Validate RAG configuration if enabled
        if let Some(rag) = &self.rag {
            if rag.enabled {
                if rag.embeddings.model.is_empty() {
                    return Err(AgentError::ConfigError("RAG embedding model cannot be empty".to_string()));
                }

                if rag.text_processing.chunk_size == 0 {
                    return Err(AgentError::ConfigError("RAG chunk size must be greater than 0".to_string()));
                }

                if rag.retrieval.top_k == 0 {
                    return Err(AgentError::ConfigError("RAG top_k must be greater than 0".to_string()));
                }
            }
        }

        // Validate MCP servers
        for server in &self.mcp_servers {
            if server.name.is_empty() {
                return Err(AgentError::ConfigError("MCP server name cannot be empty".to_string()));
            }

            // Validate configuration format - support three formats with proper precedence
            match (&server.command, &server.execution_type, &server.execution_environment) {
                // New format - execution_environment only
                (None, None, Some(exec_env)) => {
                    self.validate_execution_environment(exec_env, &server.name)?;
                }
                // Legacy format - command only
                (Some(cmd), None, None) => {
                    if cmd.run.is_empty() {
                        return Err(AgentError::ConfigError("MCP server command cannot be empty".to_string()));
                    }
                }
                // Modern format - execution_type only
                (None, Some(_), None) => {
                    // Modern format is valid, no additional validation needed here
                }
                // Invalid combinations
                (Some(_), Some(_), _) => {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' has both legacy 'command' and 'execution_type' fields. Use only one format.",
                        server.name
                    )));
                }
                (Some(_), _, Some(_)) => {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' has both legacy 'command' and new 'execution_environment' fields. Use only one format.",
                        server.name
                    )));
                }
                (_, Some(_), Some(_)) => {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' has both 'execution_type' and 'execution_environment' fields. Use only one format.",
                        server.name
                    )));
                }
                // No configuration provided
                (None, None, None) => {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' must have one of: 'command' (legacy), 'execution_type' (modern), or 'execution_environment' (new) field.",
                        server.name
                    )));
                }
            }
        }

// Validate schema configuration if enabled
        if self.agent.schema.enabled {
            if let Some(input_schema) = &self.agent.schema.input {
                // Validate that the input schema is a valid JSON schema
                if let Err(e) = jsonschema::JSONSchema::compile(&input_schema.schema) {
                    return Err(AgentError::ConfigError(format!("Invalid input JSON schema: {}", e)));
                }
            }
            
            if let Some(output_schema) = &self.agent.schema.output {
                // Validate that the output schema is a valid JSON schema
                if let Err(e) = jsonschema::JSONSchema::compile(&output_schema.schema) {
                    return Err(AgentError::ConfigError(format!("Invalid output JSON schema: {}", e)));
                }
            }
            
            if self.agent.schema.validation.max_validation_attempts == 0 {
                return Err(AgentError::ConfigError("Schema validation max_validation_attempts must be greater than 0".to_string()));
            }
        }

        Ok(())
    }

    fn validate_execution_environment(&self, exec_env: &McpExecutionEnvironment, server_name: &str) -> Result<(), AgentError> {
        match exec_env {
            McpExecutionEnvironment::Host { runtime, entry_point, .. } |
            McpExecutionEnvironment::Local { runtime, entry_point, .. } => {
                if runtime.is_empty() {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' runtime cannot be empty", server_name
                    )));
                }
                if entry_point.is_empty() {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' entry_point cannot be empty", server_name
                    )));
                }
            }
            McpExecutionEnvironment::Container { image, entry_point, .. } => {
                if image.is_empty() {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' container image cannot be empty", server_name
                    )));
                }
                if entry_point.is_empty() {
                    return Err(AgentError::ConfigError(format!(
                        "MCP server '{}' entry_point cannot be empty", server_name
                    )));
                }
            }
        }
        Ok(())
    }
}
