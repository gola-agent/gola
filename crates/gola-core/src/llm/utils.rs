//! Utility functions for composing LLM clients with cross-cutting concerns
//!
//! This module provides factory methods that compose base LLM implementations with
//! resilience patterns like auto-recovery and context truncation. The layered
//! approach enables separation of concerns where each wrapper handles a specific
//! failure mode. This design philosophy ensures that business logic remains clean
//! while infrastructure concerns are handled transparently.

use crate::llm::{AutoRecoveryLLM, LLM, ContextTruncatingLLM, HttpLLMClient};
use crate::config::LlmConfig;
use crate::errors::AgentError;
use std::sync::Arc;

/// Utility functions for creating LLM clients using the provider system
pub struct LLMFactory;

impl LLMFactory {
    /// Creates an HTTP LLM client wrapped with context truncation
    pub fn create_http_with_truncation(endpoint_url: String) -> Arc<dyn LLM> {
        let client = HttpLLMClient::new(endpoint_url);
        
        let truncating_client = ContextTruncatingLLM::new(Arc::new(client))
            .with_max_retries(5)
            .with_truncation_ratio(0.3)
            .with_min_messages(2);
            
        Arc::new(truncating_client)
    }
    
    /// Wraps any existing LLM client with context truncation
    pub fn wrap_with_truncation(
        llm: Arc<dyn LLM>,
        max_retries: Option<usize>,
        truncation_ratio: Option<f32>,
        min_messages: Option<usize>,
    ) -> Arc<dyn LLM> {
        let mut truncating_client = ContextTruncatingLLM::new(llm);
        
        if let Some(retries) = max_retries {
            truncating_client = truncating_client.with_max_retries(retries);
        }
        
        if let Some(ratio) = truncation_ratio {
            truncating_client = truncating_client.with_truncation_ratio(ratio);
        }
        
        if let Some(min_msgs) = min_messages {
            truncating_client = truncating_client.with_min_messages(min_msgs);
        }
        
        Arc::new(truncating_client)
    }

    /// Wrap any existing LLM with auto-recovery capabilities
    pub fn wrap_with_auto_recovery(
        llm: Arc<dyn LLM>,
        max_retries: Option<usize>,
    ) -> Arc<dyn LLM> {
        let mut auto_recovery = AutoRecoveryLLM::new(llm);
        if let Some(retries) = max_retries {
            auto_recovery = auto_recovery.with_max_retries(retries);
        }
        Arc::new(auto_recovery)
    }

    /// Create an LLM client from configuration with all wrappers applied
    /// This is the recommended way to create LLM clients
    pub fn create_llm_with_config(config: &LlmConfig) -> Result<Arc<dyn LLM>, AgentError> {
        // Create the base client using provider factory
        let base_client = crate::llm::providers::create_llm_client(config)?;
        
        // Apply auto-recovery wrapper
        let client = Arc::new(AutoRecoveryLLM::new(base_client));
        
        // Apply context truncation wrapper
        let client = Arc::new(
            ContextTruncatingLLM::new(client)
                .with_max_retries(5)
                .with_truncation_ratio(0.3)
                .with_min_messages(2)
        );
        
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{LlmProvider, LlmConfig, LlmAuth, ModelParameters};

    fn create_test_config() -> LlmConfig {
        LlmConfig {
            provider: LlmProvider::OpenAI,
            model: "gpt-4.1-mini".to_string(),
            auth: LlmAuth {
                api_key: Some("test-key".to_string()),
                api_key_env: None,
                headers: std::collections::HashMap::new(),
            },
            parameters: ModelParameters::default(),
        }
    }
    
    #[test]
    fn test_create_http_with_truncation() {
        let llm = LLMFactory::create_http_with_truncation(
            "http://localhost:8080".to_string(),
        );
        
        // Just verify it creates successfully
        assert!(Arc::strong_count(&llm) > 0);
    }

    #[test] 
    fn test_wrap_with_truncation() {
        let config = create_test_config();
        let base_client = crate::llm::providers::create_llm_client(&config).unwrap();
        
        let wrapped = LLMFactory::wrap_with_truncation(
            base_client,
            Some(3),
            Some(0.5),
            Some(1),
        );
        
        assert!(Arc::strong_count(&wrapped) > 0);
    }

    #[test]
    fn test_wrap_with_auto_recovery() {
        let config = create_test_config();
        let base_client = crate::llm::providers::create_llm_client(&config).unwrap();
        
        let wrapped = LLMFactory::wrap_with_auto_recovery(base_client, Some(5));
        assert!(Arc::strong_count(&wrapped) > 0);
    }

    #[test]
    fn test_create_llm_with_config() {
        let config = create_test_config();
        let result = LLMFactory::create_llm_with_config(&config);
        assert!(result.is_ok());
        
        let llm = result.unwrap();
        assert!(Arc::strong_count(&llm) > 0);
    }
}