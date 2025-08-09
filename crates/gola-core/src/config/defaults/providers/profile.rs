//! Profile-based default providers
//!
//! These providers load defaults from YAML profile files that users can create
//! and share to define common configuration patterns.

use crate::config::defaults::traits::{DefaultProvider, DefaultPriority, DefaultsContext};
use crate::config::types::*;
use crate::errors::AgentError;

/// Profile-based defaults for the complete Gola configuration
pub struct ProfileGolaProvider;

impl ProfileGolaProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<GolaConfig> for ProfileGolaProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Profile
    }
    
    fn can_provide(&self, context: &DefaultsContext) -> bool {
        context.active_profile.is_some()
    }
    
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<GolaConfig, AgentError> {
        // TODO: Implement YAML profile loading and inheritance
        Err(AgentError::ConfigError("Profile Gola provider not implemented".to_string()))
    }
    
    fn description(&self) -> &'static str {
        "YAML profile-based configuration defaults"
    }
}

// Stub implementations for other profile providers
pub struct ProfileLlmProvider;
impl ProfileLlmProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<LlmConfig> for ProfileLlmProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Profile }
    fn can_provide(&self, context: &DefaultsContext) -> bool { context.active_profile.is_some() }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<LlmConfig, AgentError> {
        Err(AgentError::ConfigError("Profile LLM provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Profile-based LLM configuration" }
}

pub struct ProfileToolsProvider;
impl ProfileToolsProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<ToolsConfig> for ProfileToolsProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Profile }
    fn can_provide(&self, context: &DefaultsContext) -> bool { context.active_profile.is_some() }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<ToolsConfig, AgentError> {
        Err(AgentError::ConfigError("Profile tools provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Profile-based tools configuration" }
}

pub struct ProfileRagProvider;
impl ProfileRagProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<RagSystemConfig> for ProfileRagProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Profile }
    fn can_provide(&self, context: &DefaultsContext) -> bool { context.active_profile.is_some() }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<RagSystemConfig, AgentError> {
        Err(AgentError::ConfigError("Profile RAG provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Profile-based RAG configuration" }
}

pub struct ProfilePromptProvider;
impl ProfilePromptProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<PromptConfig> for ProfilePromptProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Profile }
    fn can_provide(&self, context: &DefaultsContext) -> bool { context.active_profile.is_some() }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<PromptConfig, AgentError> {
        Err(AgentError::ConfigError("Profile prompt provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Profile-based prompt configuration" }
}

pub struct ProfileEnvironmentProvider;
impl ProfileEnvironmentProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<EnvironmentConfig> for ProfileEnvironmentProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Profile }
    fn can_provide(&self, context: &DefaultsContext) -> bool { context.active_profile.is_some() }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<EnvironmentConfig, AgentError> {
        Err(AgentError::ConfigError("Profile environment provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Profile-based environment configuration" }
}

pub struct ProfileLoggingProvider;
impl ProfileLoggingProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<LoggingConfig> for ProfileLoggingProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Profile }
    fn can_provide(&self, context: &DefaultsContext) -> bool { context.active_profile.is_some() }
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<LoggingConfig, AgentError> {
        Err(AgentError::ConfigError("Profile logging provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Profile-based logging configuration" }
}