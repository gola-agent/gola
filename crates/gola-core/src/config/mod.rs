//! Configuration module for the agent framework
//! 
//! This module provides a comprehensive configuration system inspired by the distri project
//! but adapted for single-agent use cases. It supports YAML configuration files and
//! programmatic configuration building.

pub mod types;
pub mod loader;
pub mod builder;
pub mod validation;
pub mod github;
pub mod defaults;

pub use types::*;
pub use loader::*;
pub use builder::ConfigBuilder as LegacyConfigBuilder;
pub use validation::*;
pub use github::*;
pub use defaults::*;

#[cfg(test)]
mod tests;

use crate::errors::AgentError;
use std::path::Path;

/// Load a configuration from a YAML file
pub async fn load_config<P: AsRef<Path>>(path: P) -> Result<GolaConfig, AgentError> {
    ConfigLoader::from_file(path).await
}

/// Create a new configuration builder
pub fn config() -> LegacyConfigBuilder {
    LegacyConfigBuilder::new()
}

/// Validate a configuration
pub fn validate_config(config: &GolaConfig) -> Result<(), AgentError> {
    config.validate()
}
