//! LLM provider implementations
//! 
//! This module contains provider-specific implementations for different LLM services.
//! Each provider implements the common LLM trait while handling provider-specific
//! protocols, authentication, and features.

use std::sync::Arc;
use crate::config::{LlmConfig, LlmProvider};
use crate::llm::LLM;
use crate::errors::AgentError;

pub mod openai;
pub mod anthropic;
pub mod gemini;

/// Create an LLM client based on the provider configuration
pub fn create_llm_client(config: &LlmConfig) -> Result<Arc<dyn LLM>, AgentError> {
    match &config.provider {
        LlmProvider::OpenAI => openai::create_client(config),
        LlmProvider::Anthropic => anthropic::create_client(config),
        LlmProvider::Gemini => gemini::create_client(config),
        LlmProvider::Custom { base_url } => {
            // For custom providers, use OpenAI-compatible client with custom base URL
            openai::create_custom_client(config, base_url)
        }
    }
}

/// Get the default model for a provider if none is specified
pub fn get_default_model(provider: &LlmProvider) -> &'static str {
    match provider {
        LlmProvider::OpenAI => "gpt-4.1-mini",
        LlmProvider::Anthropic => "claude-3-5-sonnet-latest",
        LlmProvider::Gemini => "gemini-2.0-flash",
        LlmProvider::Custom { .. } => "gpt-4.1-mini",
    }
}

/// Validate provider-specific configuration
pub fn validate_provider_config(config: &LlmConfig) -> Result<(), AgentError> {
    match &config.provider {
        LlmProvider::Anthropic => {
            // Anthropic requires an API version
            if config.parameters.anthropic_version.is_none() {
                return Err(AgentError::ConfigError(
                    "Anthropic provider requires 'anthropic_version' parameter".to_string()
                ));
            }
            
            // Validate API key configuration
            if config.auth.api_key.is_none() && config.auth.api_key_env.is_none() {
                return Err(AgentError::ConfigError(
                    "Anthropic provider requires either 'api_key' or 'api_key_env'".to_string()
                ));
            }
        }
        LlmProvider::Gemini => {
            // Validate API key configuration for Gemini
            if config.auth.api_key.is_none() && config.auth.api_key_env.is_none() {
                return Err(AgentError::ConfigError(
                    "Gemini provider requires either 'api_key' or 'api_key_env'".to_string()
                ));
            }
        }
        LlmProvider::OpenAI => {
            // Validate API key configuration for OpenAI
            if config.auth.api_key.is_none() && config.auth.api_key_env.is_none() {
                return Err(AgentError::ConfigError(
                    "OpenAI provider requires either 'api_key' or 'api_key_env'".to_string()
                ));
            }
        }
        LlmProvider::Custom { base_url } => {
            if base_url.is_empty() {
                return Err(AgentError::ConfigError(
                    "Custom provider requires a valid 'base_url'".to_string()
                ));
            }
        }
    }
    
    Ok(())
}