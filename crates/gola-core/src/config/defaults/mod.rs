//! Type-safe default configuration system
//!
//! This module implements a convention-over-configuration approach inspired by Maven,
//! where sensible defaults are provided based on project structure, environment,
//! and user-defined profiles while allowing full customization through YAML configuration.

pub mod traits;
pub mod providers;
pub mod context;
pub mod profiles;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod minimal_test;

pub use traits::*;
pub use providers::*;
pub use profiles::*;

use crate::config::types::GolaConfig;
use crate::errors::AgentError;

/// Main entry point for resolving a complete Gola configuration with defaults
pub fn resolve_config_with_defaults(
    explicit_config: Option<GolaConfig>,
    working_dir: Option<std::path::PathBuf>,
    environment: Option<String>,
    profile_name: Option<String>,
) -> Result<GolaConfig, AgentError> {
    let context = DefaultsContext::build(working_dir, environment, profile_name)?;
    
    let builder = ConfigBuilder::new()
        .register_provider(HardcodedGolaProvider::new())
        .register_provider(ConventionGolaProvider::new())
        .register_provider(EnvironmentGolaProvider::new())
        .register_provider(ProfileGolaProvider::new());
    
    let builder = if let Some(config) = explicit_config {
        builder.with_explicit_config(config)
    } else {
        builder
    };
    
    builder.build(&context)
}

/// Initialize default registries for all configuration sections
pub fn initialize_default_system() -> DefaultSystemState {
    DefaultSystemState::new()
}

/// Global state holder for the default system
pub struct DefaultSystemState {
    pub gola_registry: DefaultRegistry<GolaConfig>,
    pub llm_registry: DefaultRegistry<crate::config::types::LlmConfig>,
    pub tools_registry: DefaultRegistry<crate::config::types::ToolsConfig>,
    pub rag_registry: DefaultRegistry<crate::config::types::RagSystemConfig>,
}

impl DefaultSystemState {
    pub fn new() -> Self {
        let mut gola_registry = DefaultRegistry::new();
        let mut llm_registry = DefaultRegistry::new();
        let mut tools_registry = DefaultRegistry::new();
        let mut rag_registry = DefaultRegistry::new();
        
        // Register hardcoded providers
        gola_registry.register(HardcodedGolaProvider::new());
        llm_registry.register(HardcodedLlmProvider::new());
        tools_registry.register(HardcodedToolsProvider::new());
        rag_registry.register(HardcodedRagProvider::new());
        
        // Register convention-based providers
        gola_registry.register(ConventionGolaProvider::new());
        llm_registry.register(ConventionLlmProvider::new());
        tools_registry.register(ConventionToolsProvider::new());
        rag_registry.register(ConventionRagProvider::new());
        
        // Register environment providers
        gola_registry.register(EnvironmentGolaProvider::new());
        llm_registry.register(EnvironmentLlmProvider::new());
        tools_registry.register(EnvironmentToolsProvider::new());
        rag_registry.register(EnvironmentRagProvider::new());
        
        Self {
            gola_registry,
            llm_registry,
            tools_registry,
            rag_registry,
        }
    }
}