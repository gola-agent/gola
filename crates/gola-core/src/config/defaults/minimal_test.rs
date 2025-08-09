//! Minimal working test for core default provider functionality
//! This tests the core traits without complex configuration structures

#[cfg(test)]
mod minimal_tests {
    use super::super::traits::{DefaultProvider, DefaultPriority, DefaultsContext, DefaultRegistry};
    use crate::errors::AgentError;
    use tempfile::TempDir;

    /// Simple test configuration struct
    #[derive(Debug, Clone, PartialEq)]
    struct SimpleConfig {
        name: String,
        value: i32,
        enabled: bool,
    }

    /// Mock provider for testing
    struct MockProvider {
        priority: DefaultPriority,
        config: SimpleConfig,
        can_provide: bool,
    }

    impl DefaultProvider<SimpleConfig> for MockProvider {
        fn priority(&self) -> DefaultPriority {
            self.priority
        }
        
        fn can_provide(&self, _context: &DefaultsContext) -> bool {
            self.can_provide
        }
        
        fn provide_defaults(&self, _context: &DefaultsContext) -> Result<SimpleConfig, AgentError> {
            Ok(self.config.clone())
        }
        
        fn description(&self) -> &'static str {
            "Mock provider for testing"
        }
    }

    /// Test 1: Priority ordering works correctly
    #[test]
    fn test_priority_ordering() {
        assert!(DefaultPriority::Hardcoded < DefaultPriority::Convention);
        assert!(DefaultPriority::Convention < DefaultPriority::Environment);
        assert!(DefaultPriority::Environment < DefaultPriority::Profile);
        assert!(DefaultPriority::Profile < DefaultPriority::Explicit);
    }

    /// Test 2: Context can be built successfully
    #[test]
    fn test_context_creation() {
        let temp_dir = TempDir::new().unwrap();
        
        let context = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            Some("test-env".to_string()),
            Some("test-profile".to_string()),
        ).unwrap();
        
        assert_eq!(context.get_environment(), "test-env");
        assert_eq!(context.active_profile, Some("test-profile".to_string()));
        assert!(!context.get_project_name().is_empty());
    }

    /// Test 3: Mock provider works as expected
    #[test]
    fn test_mock_provider() {
        let temp_dir = TempDir::new().unwrap();
        let context = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            None,
            None,
        ).unwrap();

        let provider = MockProvider {
            priority: DefaultPriority::Hardcoded,
            config: SimpleConfig {
                name: "test".to_string(),
                value: 42,
                enabled: true,
            },
            can_provide: true,
        };

        assert_eq!(provider.priority(), DefaultPriority::Hardcoded);
        assert!(provider.can_provide(&context));
        assert_eq!(provider.description(), "Mock provider for testing");

        let result = provider.provide_defaults(&context).unwrap();
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
        assert!(result.enabled);
    }

    /// Test 4: Registry manages multiple providers correctly
    #[test]
    fn test_registry_management() {
        let temp_dir = TempDir::new().unwrap();
        let context = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            None,
            None,
        ).unwrap();

        let mut registry = DefaultRegistry::new();

        // Register providers in mixed priority order
        registry.register(MockProvider {
            priority: DefaultPriority::Hardcoded,
            config: SimpleConfig {
                name: "hardcoded".to_string(),
                value: 1,
                enabled: true,
            },
            can_provide: true,
        });

        registry.register(MockProvider {
            priority: DefaultPriority::Convention,
            config: SimpleConfig {
                name: "convention".to_string(),
                value: 2,
                enabled: true,
            },
            can_provide: true,
        });

        // Should return providers sorted by priority (highest first)
        let providers = registry.list_providers();
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].1, DefaultPriority::Convention); // Higher priority first
        assert_eq!(providers[1].1, DefaultPriority::Hardcoded);

        // Should return highest priority provider
        let best = registry.get_best_defaults(&context).unwrap();
        assert!(best.is_some());
        let config = best.unwrap();
        assert_eq!(config.name, "convention"); // Convention has higher priority
        assert_eq!(config.value, 2);

        // Should return all available defaults
        let all = registry.resolve_defaults(&context).unwrap();
        assert_eq!(all.len(), 2);
    }

    /// Test 5: Registry handles provider availability
    #[test]
    fn test_registry_provider_availability() {
        let temp_dir = TempDir::new().unwrap();
        let context = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            None,
            None,
        ).unwrap();

        let mut registry = DefaultRegistry::new();

        // Register a provider that cannot provide
        registry.register(MockProvider {
            priority: DefaultPriority::Convention,
            config: SimpleConfig {
                name: "unavailable".to_string(),
                value: 99,
                enabled: false,
            },
            can_provide: false,
        });

        // Register a provider that can provide
        registry.register(MockProvider {
            priority: DefaultPriority::Hardcoded,
            config: SimpleConfig {
                name: "available".to_string(),
                value: 42,
                enabled: true,
            },
            can_provide: true,
        });

        // Should skip unavailable provider and return available one
        let best = registry.get_best_defaults(&context).unwrap();
        assert!(best.is_some());
        let config = best.unwrap();
        assert_eq!(config.name, "available"); // Only the available provider
        assert_eq!(config.value, 42);

        // Should return only available providers
        let all = registry.resolve_defaults(&context).unwrap();
        assert_eq!(all.len(), 1); // Only one provider can provide
        assert_eq!(all[0].name, "available");
    }
}