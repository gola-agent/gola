//! Environment variable detection for LLM providers

use super::types::{DetectedProvider, DetectionConfidence};
use crate::config::types::LlmProvider;
use std::env;

/// Detector for LLM providers in environment variables
pub struct LlmProviderDetector {
    // No caching for now to ensure tests work correctly
}

impl LlmProviderDetector {
    /// Create a new LLM provider detector
    pub fn new() -> Self {
        Self {}
    }
    
    /// Detect all available LLM providers from environment variables
    pub fn detect_providers(&self) -> Vec<DetectedProvider> {
        let mut providers = Vec::new();
        let env_vars: std::collections::HashMap<String, String> = env::vars().collect();
        
        // Detect OpenAI providers
        providers.extend(self.detect_openai_providers(&env_vars));
        
        // Detect Anthropic providers
        providers.extend(self.detect_anthropic_providers(&env_vars));
        
        // Detect Gemini providers
        providers.extend(self.detect_gemini_providers(&env_vars));
        
        // Sort by confidence (highest first)
        providers.sort_by(|a, b| b.confidence.cmp(&a.confidence));
        
        // Deduplicate - if same provider detected multiple times, keep highest confidence
        providers = self.deduplicate_providers(providers);
        
        providers
    }
    
    /// Detect OpenAI providers from environment variables
    fn detect_openai_providers(&self, env_vars: &std::collections::HashMap<String, String>) -> Vec<DetectedProvider> {
        let mut providers = Vec::new();
        
        // Primary detection patterns
        if let Some(_) = env_vars.get("OPENAI_API_KEY") {
            let mut provider = DetectedProvider::new(
                LlmProvider::OpenAI,
                "OPENAI_API_KEY".to_string(),
                DetectionConfidence::High,
            );
            
            // Check for additional configuration
            if let Some(base_url) = env_vars.get("OPENAI_BASE_URL") {
                provider = provider.with_base_url(base_url.clone());
            }
            
            if let Some(org_id) = env_vars.get("OPENAI_ORG_ID") {
                provider = provider.with_config("org_id".to_string(), org_id.clone());
            }
            
            if let Some(model) = env_vars.get("OPENAI_MODEL") {
                provider = provider.with_config("model".to_string(), model.clone());
            }
            
            providers.push(provider);
        }
        
        // Alternative detection patterns
        if let Some(_) = env_vars.get("OPENAI_TOKEN") {
            let provider = DetectedProvider::new(
                LlmProvider::OpenAI,
                "OPENAI_TOKEN".to_string(),
                DetectionConfidence::Medium,
            );
            providers.push(provider);
        }
        
        providers
    }
    
    /// Detect Anthropic providers from environment variables
    fn detect_anthropic_providers(&self, env_vars: &std::collections::HashMap<String, String>) -> Vec<DetectedProvider> {
        let mut providers = Vec::new();
        
        // Primary detection patterns
        if let Some(_) = env_vars.get("ANTHROPIC_API_KEY") {
            let mut provider = DetectedProvider::new(
                LlmProvider::Anthropic,
                "ANTHROPIC_API_KEY".to_string(),
                DetectionConfidence::High,
            );
            
            // Check for additional configuration
            if let Some(base_url) = env_vars.get("ANTHROPIC_BASE_URL") {
                provider = provider.with_base_url(base_url.clone());
            }
            
            if let Some(version) = env_vars.get("ANTHROPIC_VERSION") {
                provider = provider.with_config("version".to_string(), version.clone());
            }
            
            if let Some(model) = env_vars.get("ANTHROPIC_MODEL") {
                provider = provider.with_config("model".to_string(), model.clone());
            }
            
            providers.push(provider);
        }
        
        // Alternative detection patterns
        if let Some(_) = env_vars.get("CLAUDE_API_KEY") {
            let provider = DetectedProvider::new(
                LlmProvider::Anthropic,
                "CLAUDE_API_KEY".to_string(),
                DetectionConfidence::Medium,
            );
            providers.push(provider);
        }
        
        if let Some(_) = env_vars.get("ANTHROPIC_TOKEN") {
            let provider = DetectedProvider::new(
                LlmProvider::Anthropic,
                "ANTHROPIC_TOKEN".to_string(),
                DetectionConfidence::Medium,
            );
            providers.push(provider);
        }
        
        providers
    }
    
    /// Detect Gemini providers from environment variables
    fn detect_gemini_providers(&self, env_vars: &std::collections::HashMap<String, String>) -> Vec<DetectedProvider> {
        let mut providers = Vec::new();
        
        // Primary detection patterns
        if let Some(_) = env_vars.get("GEMINI_API_KEY") {
            let mut provider = DetectedProvider::new(
                LlmProvider::Gemini,
                "GEMINI_API_KEY".to_string(),
                DetectionConfidence::High,
            );
            
            // Check for additional configuration
            if let Some(base_url) = env_vars.get("GEMINI_BASE_URL") {
                provider = provider.with_base_url(base_url.clone());
            }
            
            if let Some(project_id) = env_vars.get("GOOGLE_PROJECT_ID") {
                provider = provider.with_config("project_id".to_string(), project_id.clone());
            }
            
            if let Some(model) = env_vars.get("GEMINI_MODEL") {
                provider = provider.with_config("model".to_string(), model.clone());
            }
            
            providers.push(provider);
        }
        
        // Google service account detection
        if let Some(_) = env_vars.get("GOOGLE_APPLICATION_CREDENTIALS") {
            let provider = DetectedProvider::new(
                LlmProvider::Gemini,
                "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                DetectionConfidence::Medium,
            );
            providers.push(provider);
        }
        
        // Generic Google API key (lower confidence)
        if let Some(_) = env_vars.get("GOOGLE_API_KEY") {
            // Only include if no other Gemini detection found
            let has_specific_gemini = env_vars.contains_key("GEMINI_API_KEY") || 
                                    env_vars.contains_key("GOOGLE_APPLICATION_CREDENTIALS");
            
            if !has_specific_gemini {
                let provider = DetectedProvider::new(
                    LlmProvider::Gemini,
                    "GOOGLE_API_KEY".to_string(),
                    DetectionConfidence::Low,
                );
                providers.push(provider);
            }
        }
        
        providers
    }
    
    /// Remove duplicate providers, keeping the highest confidence detection
    fn deduplicate_providers(&self, mut providers: Vec<DetectedProvider>) -> Vec<DetectedProvider> {
        // Sort by provider type and confidence
        providers.sort_by(|a, b| {
            match (&a.provider_type, &b.provider_type) {
                (LlmProvider::OpenAI, LlmProvider::OpenAI) |
                (LlmProvider::Anthropic, LlmProvider::Anthropic) |
                (LlmProvider::Gemini, LlmProvider::Gemini) => {
                    // Same provider type, sort by confidence (highest first)
                    b.confidence.cmp(&a.confidence)
                },
                _ => std::cmp::Ordering::Equal,
            }
        });
        
        // Keep only the highest confidence detection for each provider type
        let mut seen_providers = std::collections::HashSet::new();
        let mut deduplicated = Vec::new();
        
        for provider in providers {
            let provider_key = match provider.provider_type {
                LlmProvider::OpenAI => "openai",
                LlmProvider::Anthropic => "anthropic", 
                LlmProvider::Gemini => "gemini",
                LlmProvider::Custom { .. } => "custom",
            };
            
            if !seen_providers.contains(provider_key) {
                seen_providers.insert(provider_key);
                deduplicated.push(provider);
            }
        }
        
        deduplicated
    }
}

impl Default for LlmProviderDetector {
    fn default() -> Self {
        Self::new()
    }
}