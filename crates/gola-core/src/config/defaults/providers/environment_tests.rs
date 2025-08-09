//! Tests for environment-based LLM provider detection and selection

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::config::defaults::traits::{DefaultsContext, ProjectInfo, DefaultProvider};
    use crate::config::defaults::providers::EnvironmentLlmProvider;
    use crate::config::types::LlmProvider;
    use std::env;
    use std::collections::HashMap;
    use tempfile::TempDir;
    use serial_test::serial;

    fn create_test_context(is_rust: bool, is_node: bool, is_python: bool, project_name: Option<String>) -> DefaultsContext {
        let temp_dir = TempDir::new().unwrap();
        DefaultsContext {
            working_dir: temp_dir.path().to_path_buf(),
            environment: Some("test".to_string()),
            active_profile: None,
            env_vars: HashMap::new(),
            project_info: ProjectInfo {
                is_rust_project: is_rust,
                is_node_project: is_node,
                is_python_project: is_python,
                project_name,
                git_info: None,
            },
        }
    }

    fn clear_llm_env_vars() {
        let vars_to_clear = [
            "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GEMINI_API_KEY",
            "OPENAI_TOKEN", "CLAUDE_API_KEY", "GOOGLE_API_KEY", "OPENROUTER_API_KEY",
            "ANTHROPIC_TOKEN", "GOOGLE_APPLICATION_CREDENTIALS", "GEMINI_BASE_URL",
            "OPENAI_BASE_URL", "ANTHROPIC_BASE_URL", "OPENAI_ORG_ID", "OPENAI_MODEL",
            "ANTHROPIC_VERSION", "ANTHROPIC_MODEL", "GOOGLE_PROJECT_ID", "GEMINI_MODEL",
            "PREFERRED_LLM_PROVIDER"
        ];
        
        for var in &vars_to_clear {
            env::remove_var(var);
        }
    }

    fn create_test_provider(provider_type: LlmProvider, env_var: &str) -> DetectedProvider {
        DetectedProvider::new(
            provider_type,
            env_var.to_string(),
            DetectionConfidence::High,
        )
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_no_env_vars() {
        clear_llm_env_vars();
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(false, false, false, None);
        
        // Should not be able to provide when no environment variables are set
        assert!(!provider.can_provide(&context));
        
        // Should return error when trying to provide defaults
        let result = provider.provide_defaults(&context);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No LLM provider environment variables detected"));
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_openai_detection() {
        clear_llm_env_vars();
        env::set_var("OPENAI_API_KEY", "sk-test123");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(false, false, false, None);
        
        // Should be able to provide
        assert!(provider.can_provide(&context));
        
        // Should return OpenAI configuration
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::OpenAI));
        assert_eq!(result.model, "gpt-4.1-mini");
        assert_eq!(result.auth.api_key_env, Some("OPENAI_API_KEY".to_string()));
        
        env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_anthropic_detection() {
        clear_llm_env_vars();
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-test123");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(false, false, false, None);
        
        // Should be able to provide
        assert!(provider.can_provide(&context));
        
        // Should return Anthropic configuration
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::Anthropic));
        assert_eq!(result.model, "claude-3-5-sonnet-latest");
        assert_eq!(result.auth.api_key_env, Some("ANTHROPIC_API_KEY".to_string()));
        assert_eq!(result.parameters.anthropic_version, Some("2023-06-01".to_string()));
        
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_gemini_detection() {
        clear_llm_env_vars();
        env::set_var("GEMINI_API_KEY", "AIza-test123");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(false, false, false, None);
        
        // Should be able to provide
        assert!(provider.can_provide(&context));
        
        // Should return Gemini configuration
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::Gemini));
        assert_eq!(result.model, "gemini-2.0-flash");
        assert_eq!(result.auth.api_key_env, Some("GEMINI_API_KEY".to_string()));
        assert!(result.parameters.safety_settings.is_some());
        
        env::remove_var("GEMINI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_context_aware_rust() {
        clear_llm_env_vars();
        env::set_var("OPENAI_API_KEY", "sk-test123");
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-test123");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(true, false, false, None); // Rust project
        
        // Should prefer Anthropic for Rust projects
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::Anthropic));
        
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_context_aware_node() {
        clear_llm_env_vars();
        env::set_var("OPENAI_API_KEY", "sk-test123");
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-test123");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(false, true, false, None); // Node.js project
        
        // Should prefer OpenAI for Node.js projects
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::OpenAI));
        
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_context_aware_python_data_science() {
        clear_llm_env_vars();
        env::set_var("OPENAI_API_KEY", "sk-test123");
        env::set_var("GEMINI_API_KEY", "AIza-test123");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(
            false, false, true, 
            Some("data-analysis-ml-project".to_string())
        ); // Python data science project
        
        // Should prefer Gemini for data science projects
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::Gemini));
        
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("GEMINI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_explicit_override_openai() {
        clear_llm_env_vars();
        env::set_var("OPENAI_API_KEY", "sk-test123");
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-test123");
        env::set_var("PREFERRED_LLM_PROVIDER", "openai");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(true, false, false, None); // Rust project (would normally prefer Anthropic)
        
        // Should override context preference and use OpenAI
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::OpenAI));
        
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("ANTHROPIC_API_KEY");
        env::remove_var("PREFERRED_LLM_PROVIDER");
    }

    #[test]
    #[serial]
    fn test_environment_llm_provider_explicit_override_anthropic() {
        clear_llm_env_vars();
        env::set_var("OPENAI_API_KEY", "sk-test123");
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-test123");
        env::set_var("PREFERRED_LLM_PROVIDER", "anthropic");
        
        let provider = EnvironmentLlmProvider::new();
        let context = create_test_context(false, true, false, None); // Node project (would normally prefer OpenAI)
        
        // Should override context preference and use Anthropic
        let result = provider.provide_defaults(&context).unwrap();
        assert!(matches!(result.provider, LlmProvider::Anthropic));
        
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("ANTHROPIC_API_KEY");
        env::remove_var("PREFERRED_LLM_PROVIDER");
    }

    #[test]
    #[serial]
    fn test_detector_openai_primary() {
        clear_llm_env_vars();
        env::set_var("OPENAI_API_KEY", "sk-test123");
        
        let detector = LlmProviderDetector::new();
        let providers = detector.detect_providers();
        
        assert_eq!(providers.len(), 1);
        assert!(matches!(providers[0].provider_type, LlmProvider::OpenAI));
        assert_eq!(providers[0].api_key_env, "OPENAI_API_KEY");
        assert_eq!(providers[0].confidence, DetectionConfidence::High);
        
        env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_detector_anthropic_primary() {
        clear_llm_env_vars();
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-test123");
        
        let detector = LlmProviderDetector::new();
        let providers = detector.detect_providers();
        
        assert_eq!(providers.len(), 1);
        assert!(matches!(providers[0].provider_type, LlmProvider::Anthropic));
        assert_eq!(providers[0].api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(providers[0].confidence, DetectionConfidence::High);
        
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    #[serial]
    fn test_detector_gemini_primary() {
        clear_llm_env_vars();
        env::set_var("GEMINI_API_KEY", "AIza-test123");
        
        let detector = LlmProviderDetector::new();
        let providers = detector.detect_providers();
        
        assert_eq!(providers.len(), 1);
        assert!(matches!(providers[0].provider_type, LlmProvider::Gemini));
        assert_eq!(providers[0].api_key_env, "GEMINI_API_KEY");
        assert_eq!(providers[0].confidence, DetectionConfidence::High);
        
        env::remove_var("GEMINI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_selector_explicit_selection_openai() {
        env::set_var("PREFERRED_LLM_PROVIDER", "openai");
        
        let selector = LlmProviderSelector::new();
        let providers = vec![
            create_test_provider(LlmProvider::OpenAI, "OPENAI_API_KEY"),
            create_test_provider(LlmProvider::Anthropic, "ANTHROPIC_API_KEY"),
        ];
        let context = create_test_context(false, false, false, None);
        
        let result = selector.select_provider(&providers, &context).unwrap();
        assert!(matches!(result.provider.provider_type, LlmProvider::OpenAI));
        assert!(result.selection_reason.contains("explicit user preference"));
        
        env::remove_var("PREFERRED_LLM_PROVIDER");
    }

    #[test]
    #[serial]
    fn test_selector_context_aware_rust_project() {
        // Ensure no explicit preference
        env::remove_var("PREFERRED_LLM_PROVIDER");
        
        let selector = LlmProviderSelector::new();
        let providers = vec![
            create_test_provider(LlmProvider::OpenAI, "OPENAI_API_KEY"),
            create_test_provider(LlmProvider::Anthropic, "ANTHROPIC_API_KEY"),
        ];
        let context = create_test_context(true, false, false, None); // Rust project
        
        let result = selector.select_provider(&providers, &context).unwrap();
        assert!(matches!(result.provider.provider_type, LlmProvider::Anthropic));
        assert!(result.selection_reason.contains("context-aware selection for Rust"));
    }
}