//! Docker registry strategy for binary installation

use crate::errors::AgentError;
use crate::installation::traits::{InstallationStrategy, DockerManager, DockerRegistry, priority};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Docker registry strategy implementation
pub struct DockerRegistryStrategy {
    pub org: String,
    pub repo: String,
    pub registries: Vec<DockerRegistry>,
    pub docker_manager: Arc<dyn DockerManager>,
}

impl DockerRegistryStrategy {
    /// Create a new Docker registry strategy
    pub fn new(
        org: String,
        repo: String,
        registries: Vec<DockerRegistry>,
        docker_manager: Arc<dyn DockerManager>,
    ) -> Self {
        Self {
            org,
            repo,
            registries,
            docker_manager,
        }
    }
}

#[async_trait]
impl InstallationStrategy for DockerRegistryStrategy {
    async fn is_available(&self, _binary_name: &str) -> Result<bool, AgentError> {
        // Check if Docker is available
        if !self.docker_manager.is_available().await {
            return Ok(false);
        }
        
        // Search registries for the image
        for registry in &self.registries {
            if let Ok(Some(_image)) = self.docker_manager.search_image(registry, &self.org, &self.repo).await {
                return Ok(true);
            }
        }
        
        Ok(false)
    }

    async fn install(&self, binary_name: &str, target_dir: &Path) -> Result<PathBuf, AgentError> {
        // Check if Docker is available
        if !self.docker_manager.is_available().await {
            return Err(AgentError::RuntimeError("Docker is not available".to_string()));
        }
        
        // Try each registry until one succeeds
        for registry in &self.registries {
            if let Ok(Some(image)) = self.docker_manager.search_image(registry, &self.org, &self.repo).await {
                match self.docker_manager.extract_binary_from_image(&image, binary_name, target_dir).await {
                    Ok(binary_path) => {
                        log::info!("Successfully extracted {} from Docker image {}", binary_name, image);
                        return Ok(binary_path);
                    }
                    Err(e) => {
                        log::warn!("Failed to extract {} from Docker image {}: {}", binary_name, image, e);
                        continue;
                    }
                }
            }
        }
        
        Err(AgentError::RuntimeError(format!(
            "Failed to install {} from any Docker registry", binary_name
        )))
    }

    fn get_priority(&self) -> u8 {
        priority::DOCKER_REGISTRY
    }

    fn get_name(&self) -> &'static str {
        "docker-registry"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installation::traits::DockerManager;
    use async_trait::async_trait;

    // Mock Docker manager for testing
    struct MockDockerManager {
        is_available: bool,
        has_image: bool,
        extract_succeeds: bool,
    }

    #[async_trait]
    impl DockerManager for MockDockerManager {
        async fn is_available(&self) -> bool {
            self.is_available
        }

        async fn extract_binary_from_image(
            &self,
            _image: &str,
            binary_name: &str,
            target_dir: &Path,
        ) -> Result<PathBuf, AgentError> {
            if self.extract_succeeds {
                let binary_path = target_dir.join(binary_name);
                tokio::fs::write(&binary_path, b"mock binary").await?;
                Ok(binary_path)
            } else {
                Err(AgentError::RuntimeError("Mock extraction failed".to_string()))
            }
        }

        async fn search_image(
            &self,
            _registry: &DockerRegistry,
            _org: &str,
            _repo: &str,
        ) -> Result<Option<String>, AgentError> {
            if self.has_image {
                Ok(Some("mock/image:latest".to_string()))
            } else {
                Ok(None)
            }
        }

        async fn build_from_source(
            &self,
            _source_config: &crate::installation::traits::SourceBuildConfig,
            _target_dir: &Path,
        ) -> Result<PathBuf, AgentError> {
            unimplemented!("Mock doesn't implement build_from_source")
        }
    }

    #[tokio::test]
    async fn test_docker_strategy_availability() {
        let docker_manager = Arc::new(MockDockerManager {
            is_available: true,
            has_image: true,
            extract_succeeds: true,
        });
        
        let strategy = DockerRegistryStrategy::new(
            "test-org".to_string(),
            "test-repo".to_string(),
            vec![DockerRegistry::DockerHub],
            docker_manager,
        );
        
        let available = strategy.is_available("test-binary").await.unwrap();
        assert!(available);
        assert_eq!(strategy.get_name(), "docker-registry");
        assert_eq!(strategy.get_priority(), priority::DOCKER_REGISTRY);
    }

    #[tokio::test]
    async fn test_docker_strategy_unavailable_when_no_docker() {
        let docker_manager = Arc::new(MockDockerManager {
            is_available: false,
            has_image: true,
            extract_succeeds: true,
        });
        
        let strategy = DockerRegistryStrategy::new(
            "test-org".to_string(),
            "test-repo".to_string(),
            vec![DockerRegistry::DockerHub],
            docker_manager,
        );
        
        let available = strategy.is_available("test-binary").await.unwrap();
        assert!(!available);
    }

    #[tokio::test]
    async fn test_docker_strategy_unavailable_when_no_image() {
        let docker_manager = Arc::new(MockDockerManager {
            is_available: true,
            has_image: false,
            extract_succeeds: true,
        });
        
        let strategy = DockerRegistryStrategy::new(
            "test-org".to_string(),
            "test-repo".to_string(),
            vec![DockerRegistry::DockerHub],
            docker_manager,
        );
        
        let available = strategy.is_available("test-binary").await.unwrap();
        assert!(!available);
    }
}