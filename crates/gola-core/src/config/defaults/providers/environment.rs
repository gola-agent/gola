//! Environment-specific default providers
//!
//! These providers adjust defaults based on the detected environment
//! (development, staging, production, etc.) and detect available services
//! from environment variables.

use crate::config::defaults::traits::{DefaultProvider, DefaultPriority, DefaultsContext};
use crate::config::types::*;
use crate::errors::AgentError;

// Import the environment detection modules
#[path = "environment_detector.rs"]
pub mod detector;
#[path = "environment_selector.rs"]
pub mod selector;
#[path = "environment_types.rs"]
pub mod types;

#[cfg(test)]
#[path = "environment_tests.rs"]
mod tests;

pub use detector::*;
pub use selector::*;
pub use types::*;

/// Environment-specific defaults for the complete Gola configuration
pub struct EnvironmentGolaProvider;

impl EnvironmentGolaProvider {
    pub fn new() -> Self {
        Self
    }
}

impl DefaultProvider<GolaConfig> for EnvironmentGolaProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Environment
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        // For now, just check if we can detect any environment-based configuration
        // This could be expanded to check for specific environment patterns
        true
    }
    
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<GolaConfig, AgentError> {
        // TODO: Implement environment-specific configuration overrides
        Err(AgentError::ConfigError("Environment Gola provider not implemented".to_string()))
    }
    
    fn description(&self) -> &'static str {
        "Environment-specific configuration overrides"
    }
}

/// Environment-based LLM provider that detects providers from environment variables
pub struct EnvironmentLlmProvider {
    detector: LlmProviderDetector,
    selector: LlmProviderSelector,
}

impl EnvironmentLlmProvider {
    pub fn new() -> Self {
        Self {
            detector: LlmProviderDetector::new(),
            selector: LlmProviderSelector::new(),
        }
    }
}

impl DefaultProvider<LlmConfig> for EnvironmentLlmProvider {
    fn priority(&self) -> DefaultPriority {
        DefaultPriority::Environment
    }
    
    fn can_provide(&self, _context: &DefaultsContext) -> bool {
        // Check if any LLM provider environment variables are detected
        !self.detector.detect_providers().is_empty()
    }
    
    fn provide_defaults(&self, context: &DefaultsContext) -> Result<LlmConfig, AgentError> {
        // Detect available providers from environment
        let detected_providers = self.detector.detect_providers();
        
        if detected_providers.is_empty() {
            return Err(AgentError::ConfigError(
                "No LLM provider environment variables detected. Set OPENAI_API_KEY, ANTHROPIC_API_KEY, or GEMINI_API_KEY".to_string()
            ));
        }
        
        // Log detected providers
        let provider_names: Vec<String> = detected_providers.iter()
            .map(|p| format!("{:?}", p.provider_type))
            .collect();
        log::info!("Detected {} LLM providers: {}", detected_providers.len(), provider_names.join(", "));
        
        // Select the best provider based on context
        let selected = self.selector.select_provider(&detected_providers, context)?;
        
        // Log selection decision
        log::info!(
            "Selected {:?} ({}) based on {}",
            selected.provider_type(),
            selected.get_default_model(),
            selected.selection_reason
        );
        
        // Generate configuration for selected provider
        let config = selected.to_llm_config()?;
        
        // Log override instructions
        log::info!("Override with PREFERRED_LLM_PROVIDER={}", 
                  format!("{:?}", selected.provider_type()).to_lowercase());
        
        Ok(config)
    }
    
    fn description(&self) -> &'static str {
        "Environment-based LLM provider detection and selection"
    }
}

// Stub implementations for other environment providers
pub struct EnvironmentToolsProvider;
impl EnvironmentToolsProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<ToolsConfig> for EnvironmentToolsProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Environment }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { false } // Not implemented yet
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<ToolsConfig, AgentError> {
        Err(AgentError::ConfigError("Environment tools provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Environment-specific tools configuration" }
}

pub struct EnvironmentRagProvider;
impl EnvironmentRagProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<RagSystemConfig> for EnvironmentRagProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Environment }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { false } // Not implemented yet
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<RagSystemConfig, AgentError> {
        Err(AgentError::ConfigError("Environment RAG provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Environment-specific RAG configuration" }
}

pub struct EnvironmentPromptProvider;
impl EnvironmentPromptProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<PromptConfig> for EnvironmentPromptProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Environment }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { false } // Not implemented yet
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<PromptConfig, AgentError> {
        Err(AgentError::ConfigError("Environment prompt provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Environment-specific prompt configuration" }
}

pub struct EnvironmentEnvironmentProvider;
impl EnvironmentEnvironmentProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<EnvironmentConfig> for EnvironmentEnvironmentProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Environment }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { false } // Not implemented yet
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<EnvironmentConfig, AgentError> {
        Err(AgentError::ConfigError("Environment environment provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Environment-specific environment configuration" }
}

pub struct EnvironmentLoggingProvider;
impl EnvironmentLoggingProvider {
    pub fn new() -> Self { Self }
}
impl DefaultProvider<LoggingConfig> for EnvironmentLoggingProvider {
    fn priority(&self) -> DefaultPriority { DefaultPriority::Environment }
    fn can_provide(&self, _context: &DefaultsContext) -> bool { false } // Not implemented yet
    fn provide_defaults(&self, _context: &DefaultsContext) -> Result<LoggingConfig, AgentError> {
        Err(AgentError::ConfigError("Environment logging provider not implemented".to_string()))
    }
    fn description(&self) -> &'static str { "Environment-specific logging configuration" }
}