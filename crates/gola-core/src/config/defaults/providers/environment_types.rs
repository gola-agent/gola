//! Data structures for environment-based provider detection

use crate::config::types::{LlmProvider, LlmConfig, LlmAuth, ModelParameters};
use crate::errors::AgentError;
use std::collections::HashMap;

/// Confidence level for provider detection
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DetectionConfidence {
    Low = 1,
    Medium = 2,
    High = 3,
}

/// A detected LLM provider from environment variables
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    /// The type of LLM provider
    pub provider_type: LlmProvider,
    
    /// Environment variable name containing the API key
    pub api_key_env: String,
    
    /// Custom base URL if detected
    pub base_url: Option<String>,
    
    /// Confidence level of this detection
    pub confidence: DetectionConfidence,
    
    /// Additional configuration extracted from environment
    pub additional_config: HashMap<String, String>,
}

impl DetectedProvider {
    /// Create a new detected provider
    pub fn new(
        provider_type: LlmProvider,
        api_key_env: String,
        confidence: DetectionConfidence,
    ) -> Self {
        Self {
            provider_type,
            api_key_env,
            base_url: None,
            confidence,
            additional_config: HashMap::new(),
        }
    }
    
    /// Set the base URL for this provider
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = Some(base_url);
        self
    }
    
    /// Add additional configuration
    pub fn with_config(mut self, key: String, value: String) -> Self {
        self.additional_config.insert(key, value);
        self
    }
    
    /// Get the default model for this provider
    pub fn get_default_model(&self) -> &'static str {
        match self.provider_type {
            LlmProvider::OpenAI => "gpt-4.1-mini",
            LlmProvider::Anthropic => "claude-3-5-sonnet-latest", 
            LlmProvider::Gemini => "gemini-2.0-flash",
            LlmProvider::Custom { .. } => "unknown",
        }
    }
    
    /// Convert to LlmConfig
    pub fn to_llm_config(&self) -> Result<LlmConfig, AgentError> {
        let model = self.additional_config
            .get("model")
            .map(|s| s.clone())
            .unwrap_or_else(|| self.get_default_model().to_string());
            
        let auth = LlmAuth {
            api_key: None,
            api_key_env: Some(self.api_key_env.clone()),
            headers: HashMap::new(),
        };
        
        let parameters = match self.provider_type {
            LlmProvider::OpenAI => ModelParameters {
                temperature: 0.0,
                max_tokens: 8000,
                top_p: 0.9,
                frequency_penalty: 0.0,
                presence_penalty: 0.0,
                stop_sequences: Vec::new(),
                anthropic_version: None,
                system_message: None,
                safety_settings: None,
            },
            LlmProvider::Anthropic => ModelParameters {
                temperature: 0.7,
                max_tokens: 4096,
                top_p: 0.9,
                frequency_penalty: 0.0,
                presence_penalty: 0.0,
                stop_sequences: Vec::new(),
                anthropic_version: Some("2023-06-01".to_string()),
                system_message: None,
                safety_settings: None,
            },
            LlmProvider::Gemini => ModelParameters {
                temperature: 0.7,
                max_tokens: 2048,
                top_p: 0.9,
                frequency_penalty: 0.0,
                presence_penalty: 0.0,
                stop_sequences: Vec::new(),
                anthropic_version: None,
                system_message: None,
                safety_settings: Some(serde_json::json!({"category": "moderate"})),
            },
            LlmProvider::Custom { .. } => ModelParameters::default(),
        };
        
        Ok(LlmConfig {
            provider: self.provider_type.clone(),
            model,
            auth,
            parameters,
        })
    }
}

/// Selected provider with selection reasoning
#[derive(Debug, Clone)]
pub struct SelectedProvider {
    /// The detected provider information
    pub provider: DetectedProvider,
    
    /// Reason for selection (for logging/debugging)
    pub selection_reason: String,
}

impl SelectedProvider {
    /// Create a new selected provider
    pub fn new(provider: DetectedProvider, reason: String) -> Self {
        Self {
            provider,
            selection_reason: reason,
        }
    }
    
    /// Get the provider type
    pub fn provider_type(&self) -> &LlmProvider {
        &self.provider.provider_type
    }
    
    /// Get the default model
    pub fn get_default_model(&self) -> &'static str {
        self.provider.get_default_model()
    }
    
    /// Convert to LlmConfig
    pub fn to_llm_config(&self) -> Result<LlmConfig, AgentError> {
        self.provider.to_llm_config()
    }
}

/// Strategy for selecting between multiple detected providers
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionStrategy {
    /// Select based on project context (Rust -> Anthropic, etc.)
    ContextAware,
    
    /// Use explicit user preference from environment variable
    Explicit,
    
    /// Fall back to capability-based ordering (Anthropic > OpenAI > Gemini)
    CapabilityBased,
}