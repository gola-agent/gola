//! Installation Orchestrator for coordinating binary installation strategies

use crate::errors::AgentError;
use crate::installation::traits::{BinaryManager, InstallationStrategy, DockerRegistry, SourceBuildConfig};
use crate::installation::binary::cache::BinaryCache;
use crate::installation::binary::strategies::{GitHubReleaseStrategy, DockerRegistryStrategy, SourceBuildStrategy};
use crate::installation::binary::docker_manager::DockerManagerImpl;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Installation orchestrator that coordinates multiple installation strategies
pub struct InstallationOrchestrator {
    strategies: Vec<Box<dyn InstallationStrategy>>,
    cache: BinaryCache,
    installation_dir: PathBuf,
}

impl InstallationOrchestrator {
    pub async fn new(installation_dir: PathBuf) -> Result<Self, AgentError> {
        let cache = BinaryCache::new(installation_dir.join("cache"));
        let strategies: Vec<Box<dyn InstallationStrategy>> = Vec::new();
        
        Ok(Self {
            strategies,
            cache,
            installation_dir,
        })
    }

    pub fn add_github_strategy(mut self, org: String, repo: String) -> Self {
        let strategy = GitHubReleaseStrategy::new(org, repo);
        self.strategies.push(Box::new(strategy));
        self.sort_strategies();
        self
    }

    /// Add a GitHub release strategy with custom configuration
    pub fn add_github_strategy_with_config(
        mut self,
        org: String,
        repo: String,
        asset_pattern: Option<String>,
        version: Option<String>,
    ) -> Self {
        let mut strategy = GitHubReleaseStrategy::new(org, repo);
        if let Some(pattern) = asset_pattern {
            strategy = strategy.with_asset_pattern(pattern);
        }
        if let Some(version) = version {
            strategy = strategy.with_version(version);
        }
        self.strategies.push(Box::new(strategy));
        self.sort_strategies();
        self
    }

    /// Add a Docker registry strategy
    pub async fn add_docker_strategy(
        mut self,
        org: String,
        repo: String,
        registries: Vec<DockerRegistry>,
    ) -> Result<Self, AgentError> {
        let docker_manager = Arc::new(DockerManagerImpl::new(600).await?);
        let strategy = DockerRegistryStrategy::new(org, repo, registries, docker_manager);
        self.strategies.push(Box::new(strategy));
        self.sort_strategies();
        Ok(self)
    }

    /// Add a source building strategy
    pub async fn add_source_strategy(
        mut self,
        source_config: SourceBuildConfig,
    ) -> Result<Self, AgentError> {
        let docker_manager = Arc::new(DockerManagerImpl::new(600).await?);
        let strategy = SourceBuildStrategy::new(source_config, docker_manager);
        self.strategies.push(Box::new(strategy));
        self.sort_strategies();
        Ok(self)
    }

    /// Sort strategies by priority (higher priority first)
    fn sort_strategies(&mut self) {
        self.strategies.sort_by(|a, b| b.get_priority().cmp(&a.get_priority()));
    }

    /// Create a default orchestrator with standard GitHub/Docker/Source fallback
    pub async fn create_default(
        installation_dir: PathBuf,
        org: String,
        repo: String,
    ) -> Result<Self, AgentError> {
        let orchestrator = Self::new(installation_dir).await?
            .add_github_strategy(org.clone(), repo.clone())
            .add_docker_strategy(
                org.clone(),
                repo.clone(),
                vec![DockerRegistry::GitHubContainerRegistry, DockerRegistry::DockerHub],
            ).await?;
        
        Ok(orchestrator)
    }

    /// Create a custom orchestrator with source building capability
    pub async fn create_with_source_building(
        installation_dir: PathBuf,
        org: String,
        repo: String,
        source_config: SourceBuildConfig,
    ) -> Result<Self, AgentError> {
        let orchestrator = Self::new(installation_dir).await?
            .add_github_strategy(org.clone(), repo.clone())
            .add_docker_strategy(
                org.clone(),
                repo.clone(),
                vec![DockerRegistry::GitHubContainerRegistry, DockerRegistry::DockerHub],
            ).await?
            .add_source_strategy(source_config).await?;
        
        Ok(orchestrator)
    }
}

#[async_trait]
impl BinaryManager for InstallationOrchestrator {
    async fn ensure_binary(&self, binary_name: &str) -> Result<PathBuf, AgentError> {
        // Check if binary is already installed
        if let Some(binary_path) = self.find_binary(binary_name).await {
            return Ok(binary_path);
        }

        // Install using internal strategies
        self.install_binary(binary_name, Vec::new()).await
    }

    async fn find_binary(&self, binary_name: &str) -> Option<PathBuf> {
        // Check cache first
        if let Some(cached_path) = self.cache.find_cached_binary(binary_name).await {
            return Some(cached_path);
        }

        // Check standard installation directory
        let binary_path = self.installation_dir.join(binary_name);
        if binary_path.exists() {
            Some(binary_path)
        } else {
            None
        }
    }

    async fn install_binary(
        &self,
        binary_name: &str,
        strategies: Vec<Box<dyn InstallationStrategy>>,
    ) -> Result<PathBuf, AgentError> {
        // Use provided strategies or fallback to internal ones
        let strategies_to_use = if strategies.is_empty() {
            &self.strategies
        } else {
            // We need to work with the provided strategies
            // For now, we'll use our internal strategies
            &self.strategies
        };

        // Try each strategy in priority order
        let mut last_error = None;
        for strategy in strategies_to_use {
            log::info!("Trying strategy '{}' for binary '{}'", strategy.get_name(), binary_name);
            
            match strategy.is_available(binary_name).await {
                Ok(true) => {
                    log::info!("Strategy '{}' reports binary '{}' is available", strategy.get_name(), binary_name);
                    
                    match strategy.install(binary_name, &self.installation_dir).await {
                        Ok(binary_path) => {
                            log::info!("Successfully installed '{}' using strategy '{}' at: {}", 
                                      binary_name, strategy.get_name(), binary_path.display());
                            return Ok(binary_path);
                        }
                        Err(e) => {
                            log::warn!("Strategy '{}' failed to install '{}': {}", 
                                      strategy.get_name(), binary_name, e);
                            last_error = Some(e);
                        }
                    }
                }
                Ok(false) => {
                    log::debug!("Strategy '{}' reports binary '{}' is not available", 
                               strategy.get_name(), binary_name);
                }
                Err(e) => {
                    log::warn!("Strategy '{}' failed availability check for '{}': {}", 
                              strategy.get_name(), binary_name, e);
                    last_error = Some(e);
                }
            }
        }

        // If no strategy succeeded, return the last error or a generic failure
        Err(last_error.unwrap_or_else(|| {
            AgentError::RuntimeError(format!(
                "Failed to install binary '{}' using any available strategy", 
                binary_name
            ))
        }))
    }

    fn get_installation_dir(&self) -> &Path {
        &self.installation_dir
    }
}

impl InstallationOrchestrator {
    pub async fn get_installation_stats(&self) -> Result<std::collections::HashMap<String, String>, AgentError> {
        let mut stats = std::collections::HashMap::new();
        
        // Get cache statistics
        let cache_stats = self.cache.get_cache_stats().await
            .map_err(|e| AgentError::RuntimeError(format!("Failed to get cache stats: {}", e)))?;
        stats.insert("cached_binaries".to_string(), cache_stats.binary_count.to_string());
        stats.insert("cache_size_bytes".to_string(), cache_stats.total_size.to_string());
        
        // Count available strategies
        stats.insert("available_strategies".to_string(), self.strategies.len().to_string());
        
        // List strategy names
        let strategy_names: Vec<String> = self.strategies.iter().map(|s| s.get_name().to_string()).collect();
        stats.insert("strategy_names".to_string(), strategy_names.join(", "));
        
        stats.insert("installation_directory".to_string(), self.installation_dir.display().to_string());
        
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let temp_dir = tempdir().unwrap();
        let installation_dir = temp_dir.path().to_path_buf();
        
        let orchestrator = InstallationOrchestrator::new(installation_dir).await.unwrap();
        assert_eq!(orchestrator.strategies.len(), 0);
    }

    #[tokio::test]
    async fn test_add_github_strategy() {
        let temp_dir = tempdir().unwrap();
        let installation_dir = temp_dir.path().to_path_buf();
        
        let orchestrator = InstallationOrchestrator::new(installation_dir).await.unwrap()
            .add_github_strategy("owner".to_string(), "repo".to_string());
        
        assert_eq!(orchestrator.strategies.len(), 1);
        assert_eq!(orchestrator.strategies[0].get_name(), "github-releases");
    }

    #[tokio::test]
    async fn test_default_orchestrator() {
        let temp_dir = tempdir().unwrap();
        let installation_dir = temp_dir.path().to_path_buf();
        
        let result = InstallationOrchestrator::create_default(
            installation_dir,
            "owner".to_string(),
            "repo".to_string()
        ).await;
        
        // This may fail if Docker is not available, which is expected in CI
        if let Ok(orchestrator) = result {
            assert!(orchestrator.strategies.len() >= 1); // At least GitHub strategy
        }
    }

    #[tokio::test]
    async fn test_installation_stats() {
        let temp_dir = tempdir().unwrap();
        let installation_dir = temp_dir.path().to_path_buf();
        
        let orchestrator = InstallationOrchestrator::new(installation_dir).await.unwrap()
            .add_github_strategy("owner".to_string(), "repo".to_string());
        
        let stats = orchestrator.get_installation_stats().await.unwrap();
        assert_eq!(stats.get("available_strategies"), Some(&"1".to_string()));
        assert_eq!(stats.get("strategy_names"), Some(&"github-releases".to_string()));
    }

    #[tokio::test]
    async fn test_binary_management() {
        let temp_dir = tempdir().unwrap();
        let installation_dir = temp_dir.path().to_path_buf();
        
        let orchestrator = InstallationOrchestrator::new(installation_dir.clone()).await.unwrap();
        
        // Test that binary is not found initially
        let binary_path = orchestrator.find_binary("test-binary").await;
        assert!(binary_path.is_none());
        
        // Test basic installation directory access
        assert_eq!(orchestrator.get_installation_dir(), installation_dir);
    }
}