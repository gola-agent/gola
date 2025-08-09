//! Binary caching and storage management

use crate::installation::errors::InstallationError;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Manages binary caching and storage
#[derive(Debug, Clone)]
pub struct BinaryCache {
    cache_dir: PathBuf,
}

impl BinaryCache {
    /// Create a new binary cache
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Get the path where a binary should be cached
    pub fn get_binary_path(&self, binary_name: &str) -> PathBuf {
        self.cache_dir.join(binary_name)
    }

    /// Check if a binary is already cached
    pub async fn is_cached(&self, binary_name: &str) -> bool {
        let binary_path = self.get_binary_path(binary_name);
        binary_path.exists() && binary_path.is_file()
    }

    /// Find a cached binary
    pub async fn find_cached_binary(&self, binary_name: &str) -> Option<PathBuf> {
        let binary_path = self.get_binary_path(binary_name);
        if self.is_cached(binary_name).await {
            Some(binary_path)
        } else {
            None
        }
    }

    /// Ensure the cache directory exists
    pub async fn ensure_cache_dir(&self) -> Result<(), InstallationError> {
        fs::create_dir_all(&self.cache_dir).await?;
        Ok(())
    }

    /// Get the cache directory path
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Clear the entire cache
    pub async fn clear_cache(&self) -> Result<(), InstallationError> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir).await?;
        }
        Ok(())
    }

    /// Remove a specific binary from cache
    pub async fn remove_binary(&self, binary_name: &str) -> Result<(), InstallationError> {
        let binary_path = self.get_binary_path(binary_name);
        if binary_path.exists() {
            fs::remove_file(binary_path).await?;
        }
        Ok(())
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> Result<CacheStats, InstallationError> {
        let mut stats = CacheStats::default();
        
        if !self.cache_dir.exists() {
            return Ok(stats);
        }
        
        let mut entries = fs::read_dir(&self.cache_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                stats.binary_count += 1;
                stats.total_size += metadata.len();
            }
        }
        
        Ok(stats)
    }
}

/// Cache statistics
#[derive(Debug, Default)]
pub struct CacheStats {
    /// Number of cached binaries
    pub binary_count: usize,
    /// Total size of cached binaries in bytes
    pub total_size: u64,
}

impl CacheStats {
    /// Get total size in human-readable format
    pub fn total_size_human(&self) -> String {
        let sizes = ["B", "KB", "MB", "GB"];
        let mut size = self.total_size as f64;
        let mut unit_index = 0;
        
        while size >= 1024.0 && unit_index < sizes.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }
        
        format!("{:.2} {}", size, sizes[unit_index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_binary_cache_creation() {
        let temp_dir = tempdir().unwrap();
        let cache = BinaryCache::new(temp_dir.path().to_path_buf());
        
        assert_eq!(cache.cache_dir(), temp_dir.path());
    }

    #[tokio::test]
    async fn test_binary_path_generation() {
        let temp_dir = tempdir().unwrap();
        let cache = BinaryCache::new(temp_dir.path().to_path_buf());
        
        let binary_path = cache.get_binary_path("test-binary");
        assert_eq!(binary_path, temp_dir.path().join("test-binary"));
    }

    #[tokio::test]
    async fn test_cache_directory_creation() {
        let temp_dir = tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let cache = BinaryCache::new(cache_dir.clone());
        
        assert!(!cache_dir.exists());
        cache.ensure_cache_dir().await.unwrap();
        assert!(cache_dir.exists());
    }

    #[tokio::test]
    async fn test_binary_caching() {
        let temp_dir = tempdir().unwrap();
        let cache = BinaryCache::new(temp_dir.path().to_path_buf());
        
        // Initially not cached
        assert!(!cache.is_cached("test-binary").await);
        assert!(cache.find_cached_binary("test-binary").await.is_none());
        
        // Create a fake binary
        let binary_path = cache.get_binary_path("test-binary");
        fs::write(&binary_path, b"fake binary content").await.unwrap();
        
        // Now it should be cached
        assert!(cache.is_cached("test-binary").await);
        assert!(cache.find_cached_binary("test-binary").await.is_some());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let temp_dir = tempdir().unwrap();
        let cache = BinaryCache::new(temp_dir.path().to_path_buf());
        
        // Empty cache
        let stats = cache.get_cache_stats().await.unwrap();
        assert_eq!(stats.binary_count, 0);
        assert_eq!(stats.total_size, 0);
        
        // Add some binaries
        cache.ensure_cache_dir().await.unwrap();
        fs::write(cache.get_binary_path("binary1"), b"content1").await.unwrap();
        fs::write(cache.get_binary_path("binary2"), b"content2").await.unwrap();
        
        let stats = cache.get_cache_stats().await.unwrap();
        assert_eq!(stats.binary_count, 2);
        assert_eq!(stats.total_size, 16); // 8 bytes each
    }

    #[test]
    fn test_cache_stats_human_readable() {
        let stats = CacheStats {
            binary_count: 5,
            total_size: 1024,
        };
        assert_eq!(stats.total_size_human(), "1.00 KB");
        
        let stats = CacheStats {
            binary_count: 3,
            total_size: 1024 * 1024,
        };
        assert_eq!(stats.total_size_human(), "1.00 MB");
    }
}