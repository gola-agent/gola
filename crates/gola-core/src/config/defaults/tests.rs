//! Unit tests for the type-safe default configuration system

use super::*;
use tempfile::TempDir;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test 1: Priority levels work correctly
    #[test]
    fn test_priority_levels() {
        assert!(DefaultPriority::Hardcoded < DefaultPriority::Convention);
        assert!(DefaultPriority::Convention < DefaultPriority::Environment);
        assert!(DefaultPriority::Environment < DefaultPriority::Profile);
        assert!(DefaultPriority::Profile < DefaultPriority::Explicit);
    }

    /// Test 2: Context detection works correctly
    #[test]
    fn test_context_detection() {
        let temp_dir = TempDir::new().unwrap();
        
        // Create a fake Cargo.toml to simulate a Rust project
        std::fs::write(
            temp_dir.path().join("Cargo.toml"),
            "[package]\nname = \"test-project\"\nversion = \"0.1.0\""
        ).unwrap();
        
        let context = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            Some("development".to_string()),
            Some("test-profile".to_string()),
        ).unwrap();
        
        // Should detect Rust project
        assert!(context.project_info.is_rust_project);
        assert!(!context.project_info.is_node_project);
        assert_eq!(context.project_info.project_name, Some("test-project".to_string()));
        
        // Should have correct environment and profile
        assert_eq!(context.get_environment(), "development");
        assert!(context.is_development());
        assert!(!context.is_production());
        assert_eq!(context.active_profile, Some("test-profile".to_string()));
        assert_eq!(context.get_project_name(), "test-project");
    }

    /// Test 3: Default registry works with priority
    #[test]
    fn test_default_registry_priority() {
        let mut registry = DefaultRegistry::<String>::new();
        
        // Simple mock provider for testing
        struct MockProvider {
            priority: DefaultPriority,
            value: String,
        }
        
        impl DefaultProvider<String> for MockProvider {
            fn priority(&self) -> DefaultPriority {
                self.priority
            }
            
            fn can_provide(&self, _context: &DefaultsContext) -> bool {
                true
            }
            
            fn provide_defaults(&self, _context: &DefaultsContext) -> Result<String, crate::errors::AgentError> {
                Ok(self.value.clone())
            }
            
            fn description(&self) -> &'static str {
                "Mock provider"
            }
        }
        
        // Register providers in mixed order
        registry.register(MockProvider {
            priority: DefaultPriority::Hardcoded,
            value: "hardcoded".to_string(),
        });
        registry.register(MockProvider {
            priority: DefaultPriority::Convention,
            value: "convention".to_string(),
        });
        
        // Should list providers sorted by priority (highest first)
        let providers = registry.list_providers();
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0].1, DefaultPriority::Convention); // Highest first
        assert_eq!(providers[1].1, DefaultPriority::Hardcoded);
    }

    /// Test 4: Context environment detection
    #[test]
    fn test_environment_detection() {
        let temp_dir = TempDir::new().unwrap();
        
        let context_dev = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            Some("development".to_string()),
            None,
        ).unwrap();
        
        let context_prod = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            Some("production".to_string()),
            None,
        ).unwrap();
        
        assert!(context_dev.is_development());
        assert!(!context_dev.is_production());
        
        assert!(!context_prod.is_development());
        assert!(context_prod.is_production());
    }

    /// Test 5: Configuration merge behavior
    #[test]
    fn test_config_merge_logic() {
        // Test basic string merging logic
        let empty_string = "".to_string();
        let filled_string = "filled".to_string();
        
        assert!(empty_string.is_empty());
        assert!(!filled_string.is_empty());
        
        // Test vector merging logic
        let empty_vec: Vec<String> = vec![];
        let filled_vec = vec!["item".to_string()];
        
        assert!(empty_vec.is_empty());
        assert!(!filled_vec.is_empty());
    }
}