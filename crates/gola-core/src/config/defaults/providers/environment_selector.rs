//! Provider selection logic for choosing between multiple detected LLM providers

use super::types::{DetectedProvider, SelectedProvider, SelectionStrategy};
use crate::config::defaults::traits::DefaultsContext;
use crate::config::types::LlmProvider;
use crate::errors::AgentError;
use std::env;

/// Selector for choosing the best LLM provider from detected options
pub struct LlmProviderSelector {
    _strategy: SelectionStrategy,
}

impl LlmProviderSelector {
    /// Create a new LLM provider selector
    pub fn new() -> Self {
        Self {
            _strategy: SelectionStrategy::ContextAware,
        }
    }
    
    /// Select the best provider from detected options
    pub fn select_provider(
        &self,
        detected_providers: &[DetectedProvider],
        context: &DefaultsContext,
    ) -> Result<SelectedProvider, AgentError> {
        if detected_providers.is_empty() {
            return Err(AgentError::ConfigError(
                "No providers available for selection".to_string()
            ));
        }
        
        // Check for explicit override first
        if let Some(selected) = self.try_explicit_selection(detected_providers)? {
            return Ok(selected);
        }
        
        // Apply context-aware selection
        if let Some(selected) = self.try_context_aware_selection(detected_providers, context)? {
            return Ok(selected);
        }
        
        // Fall back to capability-based selection
        self.capability_based_selection(detected_providers)
    }
    
    /// Try to select provider based on explicit user preference
    fn try_explicit_selection(
        &self,
        detected_providers: &[DetectedProvider],
    ) -> Result<Option<SelectedProvider>, AgentError> {
        if let Ok(preferred) = env::var("PREFERRED_LLM_PROVIDER") {
            let preferred_lower = preferred.to_lowercase();
            
            let target_provider = match preferred_lower.as_str() {
                "openai" => LlmProvider::OpenAI,
                "anthropic" => LlmProvider::Anthropic,
                "gemini" => LlmProvider::Gemini,
                _ => {
                    log::warn!("Unknown PREFERRED_LLM_PROVIDER value: '{}'. Valid options: openai, anthropic, gemini", preferred);
                    return Ok(None);
                }
            };
            
            // Find the preferred provider in detected list
            for provider in detected_providers {
                if std::mem::discriminant(&provider.provider_type) == std::mem::discriminant(&target_provider) {
                    let reason = format!("explicit user preference (PREFERRED_LLM_PROVIDER={})", preferred_lower);
                    return Ok(Some(SelectedProvider::new(provider.clone(), reason)));
                }
            }
            
            log::warn!(
                "Preferred provider '{}' not available. Available providers: {:?}",
                preferred_lower,
                detected_providers.iter().map(|p| format!("{:?}", p.provider_type)).collect::<Vec<_>>()
            );
        }
        
        Ok(None)
    }
    
    /// Try to select provider based on project context
    fn try_context_aware_selection(
        &self,
        detected_providers: &[DetectedProvider],
        context: &DefaultsContext,
    ) -> Result<Option<SelectedProvider>, AgentError> {
        let preferred_provider = if context.project_info.is_rust_project {
            // Rust projects: Prefer Anthropic for better systems programming support
            LlmProvider::Anthropic
        } else if context.project_info.is_node_project {
            // Node.js projects: Prefer OpenAI for broad web development training
            LlmProvider::OpenAI
        } else if context.project_info.is_python_project {
            // Python projects: Check if it's data science focused
            if self.is_data_science_project(context) {
                LlmProvider::Gemini // Google's strength in ML/data science
            } else {
                LlmProvider::OpenAI // General Python development
            }
        } else {
            // Unknown/general projects: Prefer OpenAI for versatility
            LlmProvider::OpenAI
        };
        
        // Find the preferred provider in detected list
        for provider in detected_providers {
            if std::mem::discriminant(&provider.provider_type) == std::mem::discriminant(&preferred_provider) {
                let project_type = self.get_project_type_description(context);
                let reason = format!("context-aware selection for {} project", project_type);
                return Ok(Some(SelectedProvider::new(provider.clone(), reason)));
            }
        }
        
        // Preferred provider not available, continue to fallback
        Ok(None)
    }
    
    /// Apply capability-based selection (fallback)
    fn capability_based_selection(
        &self,
        detected_providers: &[DetectedProvider],
    ) -> Result<SelectedProvider, AgentError> {
        // Priority order: Anthropic > OpenAI > Gemini (capability-based)
        let priority_order = [
            LlmProvider::Anthropic,
            LlmProvider::OpenAI,
            LlmProvider::Gemini,
        ];
        
        for preferred_type in &priority_order {
            for provider in detected_providers {
                if std::mem::discriminant(&provider.provider_type) == std::mem::discriminant(preferred_type) {
                    let reason = "capability-based fallback selection".to_string();
                    return Ok(SelectedProvider::new(provider.clone(), reason));
                }
            }
        }
        
        // If none of the prioritized providers are available, just take the first one
        let provider = detected_providers.first().unwrap(); // Safe because we checked empty earlier
        let reason = "first available provider".to_string();
        Ok(SelectedProvider::new(provider.clone(), reason))
    }
    
    /// Check if this appears to be a data science project
    fn is_data_science_project(&self, context: &DefaultsContext) -> bool {
        // Look for data science indicators in project structure
        // This is a simplified check - could be enhanced with more sophisticated detection
        
        // Check for common data science Python packages in requirements
        // (This would require parsing requirements.txt or similar files)
        
        // For now, use simple heuristics
        if let Some(ref project_name) = context.project_info.project_name {
            let name_lower = project_name.to_lowercase();
            return name_lower.contains("data") || 
                   name_lower.contains("ml") || 
                   name_lower.contains("ai") ||
                   name_lower.contains("analytics") ||
                   name_lower.contains("science");
        }
        
        false
    }
    
    /// Get a human-readable description of the project type
    fn get_project_type_description(&self, context: &DefaultsContext) -> &'static str {
        if context.project_info.is_rust_project {
            "Rust"
        } else if context.project_info.is_node_project {
            "Node.js"
        } else if context.project_info.is_python_project {
            if self.is_data_science_project(context) {
                "Python data science"
            } else {
                "Python"
            }
        } else {
            "general"
        }
    }
}

impl Default for LlmProviderSelector {
    fn default() -> Self {
        Self::new()
    }
}