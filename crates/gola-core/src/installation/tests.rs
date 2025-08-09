//! Tests for the installation system

#[cfg(test)]
mod tests {
    use crate::installation::traits::*;
    use crate::installation::errors::*;

    #[test]
    fn test_platform_detection() {
        let platform = Platform::current();
        assert!(!platform.os.is_empty());
        assert!(!platform.arch.is_empty());
        
        let asset_format = platform.to_asset_format();
        assert!(asset_format.contains("-"));
    }

    #[test]
    fn test_docker_registry_urls() {
        assert_eq!(DockerRegistry::GitHubContainerRegistry.url(), "ghcr.io");
        assert_eq!(DockerRegistry::DockerHub.url(), "docker.io");
        assert_eq!(DockerRegistry::Custom("custom.registry.com".to_string()).url(), "custom.registry.com");
    }

    #[test]
    fn test_source_build_config() {
        let config = SourceBuildConfig {
            repository: "https://github.com/test/repo".to_string(),
            git_ref: Some("main".to_string()),
            build_image: "gola/rust-builder".to_string(),
            build_command: "cargo build --release".to_string(),
            binary_path: "target/release/binary".to_string(),
            build_env: std::collections::HashMap::new(),
        };
        
        assert_eq!(config.repository, "https://github.com/test/repo");
        assert_eq!(config.git_ref.as_ref().unwrap(), "main");
    }

    #[test]
    fn test_priority_constants() {
        assert!(priority::GITHUB_RELEASES < priority::DOCKER_REGISTRY);
        assert!(priority::DOCKER_REGISTRY < priority::SOURCE_BUILD);
    }

    #[test]
    fn test_installation_error_conversion() {
        let installation_error = InstallationError::BinaryNotFound {
            name: "test-binary".to_string(),
        };
        
        let agent_error: crate::errors::AgentError = installation_error.into();
        assert!(matches!(agent_error, crate::errors::AgentError::RuntimeError(_)));
    }
}