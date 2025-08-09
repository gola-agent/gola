//! Source building strategy for binary installation

use crate::errors::AgentError;
use crate::installation::traits::{InstallationStrategy, DockerManager, SourceBuildConfig, priority};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Source building strategy implementation
pub struct SourceBuildStrategy {
    pub source_config: SourceBuildConfig,
    pub docker_manager: Arc<dyn DockerManager>,
}

impl SourceBuildStrategy {
    /// Create a new source building strategy
    pub fn new(source_config: SourceBuildConfig, docker_manager: Arc<dyn DockerManager>) -> Self {
        Self {
            source_config,
            docker_manager,
        }
    }
}

#[async_trait]
impl InstallationStrategy for SourceBuildStrategy {
    async fn is_available(&self, _binary_name: &str) -> Result<bool, AgentError> {
        // Check if Docker is available (required for source building)
        if !self.docker_manager.is_available().await {
            return Ok(false);
        }
        
        // For now, assume source building is always available if Docker is available
        // In the future, we might want to check if the repository exists
        Ok(true)
    }

    async fn install(&self, binary_name: &str, target_dir: &Path) -> Result<PathBuf, AgentError> {
        // Check if Docker is available
        if !self.docker_manager.is_available().await {
            return Err(AgentError::RuntimeError("Docker is not available for source building".to_string()));
        }
        
        // Use Docker manager to build from source
        let binary_path = self.docker_manager.build_from_source(&self.source_config, target_dir).await?;
        
        log::info!("Successfully built {} from source", binary_name);
        Ok(binary_path)
    }

    fn get_priority(&self) -> u8 {
        priority::SOURCE_BUILD
    }

    fn get_name(&self) -> &'static str {
        "source-build"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installation::traits::DockerManager;
    use std::collections::HashMap;
    use async_trait::async_trait;

    // Mock Docker manager for testing
    struct MockDockerManager {
        is_available: bool,
        build_succeeds: bool,
    }

    #[async_trait]
    impl DockerManager for MockDockerManager {
        async fn is_available(&self) -> bool {
            self.is_available
        }

        async fn extract_binary_from_image(
            &self,
            _image: &str,
            _binary_name: &str,
            _target_dir: &Path,
        ) -> Result<PathBuf, AgentError> {
            unimplemented!("Mock doesn't implement extract_binary_from_image")
        }

        async fn search_image(
            &self,
            _registry: &crate::installation::traits::DockerRegistry,
            _org: &str,
            _repo: &str,
        ) -> Result<Option<String>, AgentError> {
            unimplemented!("Mock doesn't implement search_image")
        }

        async fn build_from_source(
            &self,
            _source_config: &SourceBuildConfig,
            target_dir: &Path,
        ) -> Result<PathBuf, AgentError> {
            if self.build_succeeds {
                let binary_path = target_dir.join("test-binary");
                tokio::fs::write(&binary_path, b"built binary").await?;
                Ok(binary_path)
            } else {
                Err(AgentError::RuntimeError("Mock build failed".to_string()))
            }
        }
    }

    #[tokio::test]
    async fn test_source_build_strategy_availability() {
        let docker_manager = Arc::new(MockDockerManager {
            is_available: true,
            build_succeeds: true,
        });
        
        let source_config = SourceBuildConfig {
            repository: "https://github.com/test/repo".to_string(),
            git_ref: Some("main".to_string()),
            build_image: "gola/rust-builder".to_string(),
            build_command: "cargo build --release".to_string(),
            binary_path: "target/release/test-binary".to_string(),
            build_env: HashMap::new(),
        };
        
        let strategy = SourceBuildStrategy::new(source_config, docker_manager);
        
        let available = strategy.is_available("test-binary").await.unwrap();
        assert!(available);
        assert_eq!(strategy.get_name(), "source-build");
        assert_eq!(strategy.get_priority(), priority::SOURCE_BUILD);
    }

    #[tokio::test]
    async fn test_source_build_strategy_unavailable_when_no_docker() {
        let docker_manager = Arc::new(MockDockerManager {
            is_available: false,
            build_succeeds: true,
        });
        
        let source_config = SourceBuildConfig {
            repository: "https://github.com/test/repo".to_string(),
            git_ref: Some("main".to_string()),
            build_image: "gola/rust-builder".to_string(),
            build_command: "cargo build --release".to_string(),
            binary_path: "target/release/test-binary".to_string(),
            build_env: HashMap::new(),
        };
        
        let strategy = SourceBuildStrategy::new(source_config, docker_manager);
        
        let available = strategy.is_available("test-binary").await.unwrap();
        assert!(!available);
    }

    #[tokio::test]
    async fn test_source_build_strategy_install_success() {
        let docker_manager = Arc::new(MockDockerManager {
            is_available: true,
            build_succeeds: true,
        });
        
        let source_config = SourceBuildConfig {
            repository: "https://github.com/test/repo".to_string(),
            git_ref: Some("main".to_string()),
            build_image: "gola/rust-builder".to_string(),
            build_command: "cargo build --release".to_string(),
            binary_path: "target/release/test-binary".to_string(),
            build_env: HashMap::new(),
        };
        
        let strategy = SourceBuildStrategy::new(source_config, docker_manager);
        
        let temp_dir = tempfile::tempdir().unwrap();
        let result = strategy.install("test-binary", temp_dir.path()).await;
        
        assert!(result.is_ok());
        let binary_path = result.unwrap();
        assert!(binary_path.exists());
    }

    #[tokio::test]
    async fn test_source_build_strategy_install_failure() {
        let docker_manager = Arc::new(MockDockerManager {
            is_available: true,
            build_succeeds: false,
        });
        
        let source_config = SourceBuildConfig {
            repository: "https://github.com/test/repo".to_string(),
            git_ref: Some("main".to_string()),
            build_image: "gola/rust-builder".to_string(),
            build_command: "cargo build --release".to_string(),
            binary_path: "target/release/test-binary".to_string(),
            build_env: HashMap::new(),
        };
        
        let strategy = SourceBuildStrategy::new(source_config, docker_manager);
        
        let temp_dir = tempfile::tempdir().unwrap();
        let result = strategy.install("test-binary", temp_dir.path()).await;
        
        assert!(result.is_err());
    }
}