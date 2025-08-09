//! Core traits and types for the type-safe default provider system
//!
//! This module defines the foundational traits that enable convention-over-configuration
//! for Gola's YAML-based configuration system.

use crate::config::types::{GolaConfig, LlmProvider};
use crate::errors::AgentError;
use std::collections::HashMap;

/// Priority levels for default providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DefaultPriority {
    /// Hardcoded defaults (lowest priority)
    Hardcoded = 1,
    /// Convention-based defaults based on project structure/patterns
    Convention = 2,
    /// Environment-specific defaults (development, staging, production)
    Environment = 3,
    /// User-defined YAML profile defaults
    Profile = 4,
    /// Explicit configuration (highest priority)
    Explicit = 5,
}

/// Context information available to default providers for making decisions
#[derive(Debug, Clone)]
pub struct DefaultsContext {
    /// Current working directory
    pub working_dir: std::path::PathBuf,
    /// Environment type (dev, staging, prod, etc.)
    pub environment: Option<String>,
    /// Project metadata detected from files
    pub project_info: ProjectInfo,
    /// Environment variables available
    pub env_vars: HashMap<String, String>,
    /// Active YAML profile name
    pub active_profile: Option<String>,
}

/// Project metadata detected from the filesystem
#[derive(Debug, Clone, Default)]
pub struct ProjectInfo {
    /// Whether this is a Rust project (Cargo.toml present)
    pub is_rust_project: bool,
    /// Whether this is a Node.js project (package.json present)
    pub is_node_project: bool,
    /// Whether this is a Python project (pyproject.toml, setup.py present)
    pub is_python_project: bool,
    /// Detected project name
    pub project_name: Option<String>,
    /// Git repository information
    pub git_info: Option<GitInfo>,
}

/// Git repository information
#[derive(Debug, Clone)]
pub struct GitInfo {
    /// Current branch name
    pub branch: String,
    /// Remote origin URL
    pub origin_url: Option<String>,
    /// Whether the repo has uncommitted changes
    pub has_changes: bool,
}

/// Core trait for providing type-safe defaults
pub trait DefaultProvider<T> {
    /// Get the priority level of this provider
    fn priority(&self) -> DefaultPriority;
    
    /// Check if this provider can provide defaults for the given context
    fn can_provide(&self, context: &DefaultsContext) -> bool;
    
    /// Provide the default configuration
    fn provide_defaults(&self, context: &DefaultsContext) -> Result<T, AgentError>;
    
    /// Get a human-readable description of this provider
    fn description(&self) -> &'static str;
}

/// Registry for managing default providers
pub struct DefaultRegistry<T> {
    providers: Vec<Box<dyn DefaultProvider<T>>>,
}

impl<T> Default for DefaultRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> DefaultRegistry<T> {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }
    
    /// Register a new default provider
    pub fn register<P: DefaultProvider<T> + 'static>(&mut self, provider: P) {
        self.providers.push(Box::new(provider));
        // Keep providers sorted by priority (highest first)
        self.providers.sort_by_key(|p| std::cmp::Reverse(p.priority()));
    }
    
    /// Resolve defaults using all registered providers
    pub fn resolve_defaults(&self, context: &DefaultsContext) -> Result<Vec<T>, AgentError> {
        let mut results = Vec::new();
        
        for provider in &self.providers {
            if provider.can_provide(context) {
                match provider.provide_defaults(context) {
                    Ok(defaults) => {
                        results.push(defaults);
                    }
                    Err(e) => {
                        // Log the error but continue with other providers
                        eprintln!("Warning: Default provider '{}' failed: {}", 
                                provider.description(), e);
                    }
                }
            }
        }
        
        Ok(results)
    }
    
    /// Get the highest priority defaults that can be provided
    pub fn get_best_defaults(&self, context: &DefaultsContext) -> Result<Option<T>, AgentError> {
        for provider in &self.providers {
            if provider.can_provide(context) {
                return provider.provide_defaults(context).map(Some);
            }
        }
        Ok(None)
    }
    
    /// List all available providers and their priorities
    pub fn list_providers(&self) -> Vec<(&'static str, DefaultPriority)> {
        self.providers
            .iter()
            .map(|p| (p.description(), p.priority()))
            .collect()
    }
}

/// Trait for merging configuration values with defaults
pub trait ConfigMerge<T> {
    /// Merge this configuration with defaults, preserving explicit values
    fn merge_with_defaults(&mut self, defaults: Vec<T>) -> Result<(), AgentError>;
}

/// Helper trait for detecting whether a field has been explicitly set
pub trait ExplicitlySet {
    /// Returns true if this value was explicitly set (not default)
    fn is_explicitly_set(&self) -> bool;
}

// Implement ExplicitlySet for Option types (None = not set, Some = explicitly set)
impl<T> ExplicitlySet for Option<T> {
    fn is_explicitly_set(&self) -> bool {
        self.is_some()
    }
}

/// Configuration builder that applies defaults in the correct order
pub struct ConfigBuilder<T> {
    registry: DefaultRegistry<T>,
    explicit_config: Option<T>,
}

impl<T> ConfigBuilder<T> {
    /// Create a new configuration builder
    pub fn new() -> Self {
        Self {
            registry: DefaultRegistry::new(),
            explicit_config: None,
        }
    }
    
    /// Set the explicit configuration
    pub fn with_explicit_config(mut self, config: T) -> Self {
        self.explicit_config = Some(config);
        self
    }
    
    /// Register a default provider
    pub fn register_provider<P: DefaultProvider<T> + 'static>(mut self, provider: P) -> Self {
        self.registry.register(provider);
        self
    }
    
    /// Build the final configuration by applying defaults
    pub fn build(self, context: &DefaultsContext) -> Result<T, AgentError> 
    where 
        T: ConfigMerge<T> + Clone,
    {
        let defaults = self.registry.resolve_defaults(context)?;
        
        match self.explicit_config {
            Some(mut config) => {
                config.merge_with_defaults(defaults)?;
                Ok(config)
            }
            None => {
                // Use the best available defaults
                self.registry
                    .get_best_defaults(context)?
                    .ok_or_else(|| AgentError::ConfigError(
                        "No default configuration available and no explicit config provided".to_string()
                    ))
            }
        }
    }
}

impl<T> Default for ConfigBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Implementation of ConfigMerge for GolaConfig
impl ConfigMerge<GolaConfig> for GolaConfig {
    fn merge_with_defaults(&mut self, defaults: Vec<GolaConfig>) -> Result<(), AgentError> {
        // Apply defaults in reverse order (lowest priority first)
        for default_config in defaults.into_iter().rev() {
            // Merge agent definition
            if self.agent.name.is_empty() {
                self.agent.name = default_config.agent.name;
            }
            if self.agent.description.is_empty() {
                self.agent.description = default_config.agent.description;
            }
            // Merge max_steps if it's the default value
            if self.agent.max_steps == 10 { // default_max_steps value
                self.agent.max_steps = default_config.agent.max_steps;
            }
            
            // For complex structures, use the default if current is empty/default
            // This is a simplified merge - a full implementation would do deep merging
            
            // Merge LLM config - if current is None or has default values, use the default config
            if self.llm.is_none() {
                self.llm = default_config.llm;
            } else if let (Some(current_llm), Some(default_llm)) = (&self.llm, &default_config.llm) {
                if matches!(current_llm.provider, LlmProvider::OpenAI) && current_llm.model == "gpt-4.1-mini" {
                    self.llm = Some(default_llm.clone());
                }
            }
            
            // Merge optional configs
            if self.rag.is_none() {
                self.rag = default_config.rag;
            }
            if self.prompts.is_none() {
                self.prompts = default_config.prompts;
            }
            if self.mcp_servers.is_empty() {
                self.mcp_servers = default_config.mcp_servers;
            }
            // Environment and logging are always present, but we can merge if they're default/empty
            if self.environment.variables.is_empty() && self.environment.env_files.is_empty() {
                self.environment = default_config.environment;
            }
            if self.logging.level == "info" && self.logging.format == "pretty" {
                self.logging = default_config.logging;
            }
            
            // Merge tracing config if not enabled
            if !self.tracing.enabled {
                self.tracing = default_config.tracing;
            }
        }
        
        Ok(())
    }
}