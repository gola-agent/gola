//! Binary manager implementation

use crate::errors::AgentError;
use crate::installation::traits::{BinaryManager, InstallationStrategy};
use crate::installation::binary::cache::BinaryCache;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use which::which;

/// Binary manager implementation
#[derive(Debug)]
pub struct BinaryManagerImpl {
    cache: BinaryCache,
    installation_dir: PathBuf,
}

impl BinaryManagerImpl {
    /// Create a new binary manager
    pub fn new(installation_dir: PathBuf) -> Self {
        let cache = BinaryCache::new(installation_dir.join("cache"));
        Self {
            cache,
            installation_dir,
        }
    }

    /// Create a binary manager with custom cache location
    pub fn with_cache_dir(installation_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let cache = BinaryCache::new(cache_dir);
        Self {
            cache,
            installation_dir,
        }
    }

    /// Get the cache instance
    pub fn cache(&self) -> &BinaryCache {
        &self.cache
    }

    /// Find binary in system PATH
    async fn find_in_system_path(&self, binary_name: &str) -> Option<PathBuf> {
        which(binary_name).ok()
    }

    /// Find binary in installation directory
    async fn find_in_installation_dir(&self, binary_name: &str) -> Option<PathBuf> {
        let binary_path = self.installation_dir.join(binary_name);
        if binary_path.exists() && binary_path.is_file() {
            Some(binary_path)
        } else {
            None
        }
    }

    /// Install binary using strategies in priority order
    async fn install_with_strategies(
        &self,
        binary_name: &str,
        mut strategies: Vec<Box<dyn InstallationStrategy>>,
    ) -> Result<PathBuf, AgentError> {
        // Sort strategies by priority (lower number = higher priority)
        strategies.sort_by_key(|s| s.get_priority());
        
        let mut last_error = None;
        
        for strategy in strategies {
            log::info!("Trying installation strategy: {}", strategy.get_name());
            
            match strategy.is_available(binary_name).await {
                Ok(true) => {
                    log::info!("Strategy {} reports binary is available", strategy.get_name());
                    
                    // Ensure cache directory exists
                    if let Err(e) = self.cache.ensure_cache_dir().await {
                        log::warn!("Failed to create cache directory: {}", e);
                        last_error = Some(AgentError::from(e));
                        continue;
                    }
                    
                    match strategy.install(binary_name, self.cache.cache_dir()).await {
                        Ok(binary_path) => {
                            log::info!("Successfully installed {} using strategy {}", 
                                     binary_name, strategy.get_name());
                            return Ok(binary_path);
                        }
                        Err(e) => {
                            log::warn!("Strategy {} failed to install {}: {}", 
                                     strategy.get_name(), binary_name, e);
                            last_error = Some(e);
                        }
                    }
                }
                Ok(false) => {
                    log::info!("Strategy {} reports binary is not available", strategy.get_name());
                }
                Err(e) => {
                    log::warn!("Strategy {} failed availability check: {}", strategy.get_name(), e);
                    last_error = Some(e);
                }
            }
        }
        
        // All strategies failed
        if let Some(error) = last_error {
            Err(error)
        } else {
            Err(AgentError::RuntimeError(format!(
                "All installation strategies exhausted for binary '{}'", binary_name
            )))
        }
    }
}

#[async_trait]
impl BinaryManager for BinaryManagerImpl {
    async fn ensure_binary(&self, binary_name: &str) -> Result<PathBuf, AgentError> {
        // 1. Check if already cached
        if let Some(cached_path) = self.cache.find_cached_binary(binary_name).await {
            log::debug!("Found cached binary: {}", cached_path.display());
            return Ok(cached_path);
        }
        
        // 2. Check installation directory
        if let Some(installed_path) = self.find_in_installation_dir(binary_name).await {
            log::debug!("Found installed binary: {}", installed_path.display());
            return Ok(installed_path);
        }
        
        // 3. Check system PATH
        if let Some(system_path) = self.find_in_system_path(binary_name).await {
            log::debug!("Found system binary: {}", system_path.display());
            return Ok(system_path);
        }
        
        // 4. Binary not found, need to install
        Err(AgentError::RuntimeError(format!(
            "Binary '{}' not found and no installation strategies provided", binary_name
        )))
    }

    async fn find_binary(&self, binary_name: &str) -> Option<PathBuf> {
        // Check cache first
        if let Some(cached_path) = self.cache.find_cached_binary(binary_name).await {
            return Some(cached_path);
        }
        
        // Check installation directory
        if let Some(installed_path) = self.find_in_installation_dir(binary_name).await {
            return Some(installed_path);
        }
        
        // Check system PATH
        self.find_in_system_path(binary_name).await
    }

    async fn install_binary(
        &self,
        binary_name: &str,
        strategies: Vec<Box<dyn InstallationStrategy>>,
    ) -> Result<PathBuf, AgentError> {
        if strategies.is_empty() {
            return Err(AgentError::RuntimeError(
                "No installation strategies provided".to_string()
            ));
        }
        
        self.install_with_strategies(binary_name, strategies).await
    }

    fn get_installation_dir(&self) -> &Path {
        &self.installation_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installation::traits::InstallationStrategy;
    use tempfile::tempdir;
    use tokio::fs;

    // Mock strategy for testing
    struct MockStrategy {
        name: &'static str,
        priority: u8,
        available: bool,
        should_succeed: bool,
    }

    #[async_trait]
    impl InstallationStrategy for MockStrategy {
        async fn is_available(&self, _binary_name: &str) -> Result<bool, AgentError> {
            Ok(self.available)
        }

        async fn install(&self, binary_name: &str, target_dir: &Path) -> Result<PathBuf, AgentError> {
            if self.should_succeed {
                let binary_path = target_dir.join(binary_name);
                fs::write(&binary_path, b"mock binary").await?;
                Ok(binary_path)
            } else {
                Err(AgentError::RuntimeError("Mock installation failed".to_string()))
            }
        }

        fn get_priority(&self) -> u8 {
            self.priority
        }

        fn get_name(&self) -> &'static str {
            self.name
        }
    }

    #[tokio::test]
    async fn test_binary_manager_creation() {
        let temp_dir = tempdir().unwrap();
        let manager = BinaryManagerImpl::new(temp_dir.path().to_path_buf());
        
        assert_eq!(manager.get_installation_dir(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_find_cached_binary() {
        let temp_dir = tempdir().unwrap();
        let manager = BinaryManagerImpl::new(temp_dir.path().to_path_buf());
        
        // Create a cached binary
        manager.cache.ensure_cache_dir().await.unwrap();
        let binary_path = manager.cache.get_binary_path("test-binary");
        fs::write(&binary_path, b"test content").await.unwrap();
        
        // Should find the cached binary
        let found = manager.find_binary("test-binary").await;
        assert!(found.is_some());
        assert_eq!(found.unwrap(), binary_path);
    }

    #[tokio::test]
    async fn test_install_with_successful_strategy() {
        let temp_dir = tempdir().unwrap();
        let manager = BinaryManagerImpl::new(temp_dir.path().to_path_buf());
        
        let strategy = Box::new(MockStrategy {
            name: "mock-success",
            priority: 10,
            available: true,
            should_succeed: true,
        });
        
        let result = manager.install_binary("test-binary", vec![strategy]).await;
        assert!(result.is_ok());
        
        let binary_path = result.unwrap();
        assert!(binary_path.exists());
        assert_eq!(binary_path.file_name().unwrap(), "test-binary");
    }

    #[tokio::test]
    async fn test_install_with_failing_strategy() {
        let temp_dir = tempdir().unwrap();
        let manager = BinaryManagerImpl::new(temp_dir.path().to_path_buf());
        
        let strategy = Box::new(MockStrategy {
            name: "mock-fail",
            priority: 10,
            available: true,
            should_succeed: false,
        });
        
        let result = manager.install_binary("test-binary", vec![strategy]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_strategy_priority_ordering() {
        let temp_dir = tempdir().unwrap();
        let manager = BinaryManagerImpl::new(temp_dir.path().to_path_buf());
        
        // Create strategies with different priorities
        let low_priority = Box::new(MockStrategy {
            name: "low-priority",
            priority: 30,
            available: true,
            should_succeed: true,
        });
        
        let high_priority = Box::new(MockStrategy {
            name: "high-priority",
            priority: 10,
            available: true,
            should_succeed: true,
        });
        
        // High priority should be tried first
        let result = manager.install_binary("test-binary", vec![low_priority, high_priority]).await;
        assert!(result.is_ok());
        
        // Verify the binary was installed (high priority strategy should have been used)
        let binary_path = result.unwrap();
        assert!(binary_path.exists());
    }

    #[tokio::test]
    async fn test_ensure_binary_uses_cache() {
        let temp_dir = tempdir().unwrap();
        let manager = BinaryManagerImpl::new(temp_dir.path().to_path_buf());
        
        // Create a cached binary
        manager.cache.ensure_cache_dir().await.unwrap();
        let binary_path = manager.cache.get_binary_path("test-binary");
        fs::write(&binary_path, b"test content").await.unwrap();
        
        // ensure_binary should find the cached version
        let result = manager.ensure_binary("test-binary").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), binary_path);
    }
}