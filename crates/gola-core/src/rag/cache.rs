use crate::errors::AgentError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for embedding cache implementations
#[async_trait]
pub trait EmbeddingCache: Send + Sync {
    /// Get an embedding from the cache
    async fn get(&self, text: &str) -> Option<Vec<f32>>;

    /// Store an embedding in the cache
    async fn put(&self, text: String, embedding: Vec<f32>) -> Result<(), AgentError>;

    /// Get multiple embeddings from the cache
    async fn get_batch(&self, texts: &[String]) -> Vec<Option<Vec<f32>>>;

    /// Store multiple embeddings in the cache
    async fn put_batch(
        &self,
        texts: Vec<String>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), AgentError>;

    /// Clear all cached embeddings
    async fn clear(&self) -> Result<(), AgentError>;

    /// Get the number of cached embeddings
    async fn size(&self) -> usize;

    /// Save cache to persistent storage (if supported)
    async fn save(&self, path: &Path) -> Result<(), AgentError>;

    /// Load cache from persistent storage (if supported)
    async fn load(&self, path: &Path) -> Result<(), AgentError>;

    /// Check if the cache supports persistence
    fn supports_persistence(&self) -> bool;
}

/// Configuration for embedding cache
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingCacheConfig {
    /// Maximum number of embeddings to cache in memory
    pub max_size: usize,
    /// Whether to enable persistent caching
    pub persistent: bool,
    /// Path for persistent cache storage
    pub cache_file_path: Option<String>,
    /// Whether to compress cached data
    pub compress: bool,
    /// Cache eviction strategy
    pub eviction_strategy: EvictionStrategy,
}

impl Default for EmbeddingCacheConfig {
    fn default() -> Self {
        Self {
            max_size: 10000,
            persistent: false,
            cache_file_path: None,
            compress: false,
            eviction_strategy: EvictionStrategy::LRU,
        }
    }
}

/// Cache eviction strategies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EvictionStrategy {
    /// Least Recently Used
    LRU,
    /// First In, First Out
    FIFO,
    /// Least Frequently Used
    LFU,
}

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    embedding: Vec<f32>,
    access_count: u64,
    last_accessed: u64,
    created_at: u64,
}

impl CacheEntry {
    fn new(embedding: Vec<f32>) -> Self {
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64; // Cast u128 to u64, safe for current/near-future timestamps

        Self {
            embedding,
            access_count: 1,
            last_accessed: now_nanos,
            created_at: now_nanos,
        }
    }

    fn access(&mut self) {
        self.access_count += 1;
        self.last_accessed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64; // Cast u128 to u64
    }
}

/// In-memory embedding cache with configurable eviction strategies
pub struct InMemoryEmbeddingCache {
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    config: EmbeddingCacheConfig,
}

impl InMemoryEmbeddingCache {
    /// Create a new in-memory embedding cache
    pub fn new(config: EmbeddingCacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Create a new cache with default configuration
    pub fn with_max_size(max_size: usize) -> Self {
        let config = EmbeddingCacheConfig {
            max_size,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Evict entries based on the configured strategy
    async fn evict_if_needed(&self) {
        let mut cache = self.cache.write().await;

        if cache.len() <= self.config.max_size {
            return;
        }

        let entries_to_remove = cache.len() - self.config.max_size;
        let mut entries: Vec<(String, CacheEntry)> = cache.drain().collect();

        // Sort based on eviction strategy
        match self.config.eviction_strategy {
            EvictionStrategy::LRU => {
                entries.sort_by_key(|(_, entry)| (entry.last_accessed, entry.created_at));
            }
            EvictionStrategy::FIFO => {
                entries.sort_by_key(|(_, entry)| entry.created_at);
            }
            EvictionStrategy::LFU => {
                entries.sort_by_key(|(_, entry)| entry.access_count);
            }
        }

        // Remove the oldest/least used entries
        entries.drain(0..entries_to_remove);

        // Put the remaining entries back
        for (key, entry) in entries {
            cache.insert(key, entry);
        }

        log::debug!(
            "Evicted {} entries from embedding cache, {} entries remaining",
            entries_to_remove,
            cache.len()
        );
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        let total_size = cache.len();
        let total_access_count: u64 = cache.values().map(|entry| entry.access_count).sum();

        CacheStats {
            total_entries: total_size,
            max_size: self.config.max_size,
            total_accesses: total_access_count,
            hit_rate: 0.0,
        }
    }
}

#[async_trait]
impl EmbeddingCache for InMemoryEmbeddingCache {
    async fn get(&self, text: &str) -> Option<Vec<f32>> {
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(text) {
            entry.access();
            Some(entry.embedding.clone())
        } else {
            None
        }
    }

    async fn put(&self, text: String, embedding: Vec<f32>) -> Result<(), AgentError> {
        let mut cache = self.cache.write().await;
        cache.insert(text, CacheEntry::new(embedding));
        drop(cache);

        self.evict_if_needed().await;

        Ok(())
    }

    async fn get_batch(&self, texts: &[String]) -> Vec<Option<Vec<f32>>> {
        let mut cache = self.cache.write().await;
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            if let Some(entry) = cache.get_mut(text) {
                entry.access();
                results.push(Some(entry.embedding.clone()));
            } else {
                results.push(None);
            }
        }

        results
    }

    async fn put_batch(
        &self,
        texts: Vec<String>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), AgentError> {
        if texts.len() != embeddings.len() {
            return Err(AgentError::RagError(
                "Texts and embeddings length mismatch".to_string(),
            ));
        }

        let mut cache = self.cache.write().await;
        for (text, embedding) in texts.into_iter().zip(embeddings.into_iter()) {
            cache.insert(text, CacheEntry::new(embedding));
        }
        drop(cache);

        self.evict_if_needed().await;

        Ok(())
    }

    async fn clear(&self) -> Result<(), AgentError> {
        let mut cache = self.cache.write().await;
        cache.clear();
        Ok(())
    }

    async fn size(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }

    async fn save(&self, path: &Path) -> Result<(), AgentError> {
        let cache = self.cache.read().await;
        let data = cache.clone();
        drop(cache);

        let serialized = 
            serde_json::to_vec_pretty(&data)
                .map_err(|e| AgentError::RagError(format!("Failed to serialize cache: {}", e)))?;

        tokio::fs::write(path, serialized)
            .await
            .map_err(|e| AgentError::RagError(format!("Failed to write cache to file: {}", e)))?;

        log::info!(
            "Saved embedding cache with {} entries to {:?}",
            data.len(),
            path
        );
        Ok(())
    }

    async fn load(&self, path: &Path) -> Result<(), AgentError> {
        if !path.exists() {
            log::warn!(
                "Cache file {:?} does not exist, starting with empty cache",
                path
            );
            return Ok(());
        }

        let data = tokio::fs::read(path)
            .await
            .map_err(|e| AgentError::RagError(format!("Failed to read cache file: {}", e)))?;

        let cache_data: HashMap<String, CacheEntry> = 
            serde_json::from_slice(&data)
                .map_err(|e| AgentError::RagError(format!("Failed to deserialize cache: {}", e)))?;

        let mut cache = self.cache.write().await;
        *cache = cache_data;

        log::info!(
            "Loaded embedding cache with {} entries from {:?}",
            cache.len(),
            path
        );
        Ok(())
    }

    fn supports_persistence(&self) -> bool {
        true
    }
}

/// Persistent embedding cache that automatically saves to disk
pub struct PersistentEmbeddingCache {
    inner: InMemoryEmbeddingCache,
    cache_path: std::path::PathBuf,
    auto_save_interval: Option<std::time::Duration>,
    last_save: Arc<RwLock<std::time::Instant>>,
}

impl PersistentEmbeddingCache {
    /// Create a new persistent embedding cache
    pub async fn new(
        config: EmbeddingCacheConfig,
        cache_path: impl AsRef<Path>,
    ) -> Result<Self, AgentError> {
        let cache_path = cache_path.as_ref().to_path_buf();
        let inner = InMemoryEmbeddingCache::new(config);

        // Load existing cache if it exists
        if cache_path.exists() {
            inner.load(&cache_path).await?;
        }

        Ok(Self {
            inner,
            cache_path,
            auto_save_interval: Some(std::time::Duration::from_secs(300)),
            last_save: Arc::new(RwLock::new(std::time::Instant::now())),
        })
    }

    /// Set auto-save interval (None to disable auto-save)
    pub fn with_auto_save_interval(mut self, interval: Option<std::time::Duration>) -> Self {
        self.auto_save_interval = interval;
        self
    }

    /// Check if auto-save is needed and perform it
    async fn auto_save_if_needed(&self) -> Result<(), AgentError> {
        if let Some(interval) = self.auto_save_interval {
            let last_save = *self.last_save.read().await;
            if last_save.elapsed() >= interval {
                self.save(&self.cache_path).await?;
                let mut last_save_write = self.last_save.write().await;
                *last_save_write = std::time::Instant::now();
            }
        }
        Ok(())
    }
}

#[async_trait]
impl EmbeddingCache for PersistentEmbeddingCache {
    async fn get(&self, text: &str) -> Option<Vec<f32>> {
        self.inner.get(text).await
    }

    async fn put(&self, text: String, embedding: Vec<f32>) -> Result<(), AgentError> {
        self.inner.put(text, embedding).await?;
        self.auto_save_if_needed().await?;
        Ok(())
    }

    async fn get_batch(&self, texts: &[String]) -> Vec<Option<Vec<f32>>> {
        self.inner.get_batch(texts).await
    }

    async fn put_batch(
        &self,
        texts: Vec<String>,
        embeddings: Vec<Vec<f32>>,
    ) -> Result<(), AgentError> {
        self.inner.put_batch(texts, embeddings).await?;
        self.auto_save_if_needed().await?;
        Ok(())
    }

    async fn clear(&self) -> Result<(), AgentError> {
        self.inner.clear().await?;
        self.save(&self.cache_path).await?;
        Ok(())
    }

    async fn size(&self) -> usize {
        self.inner.size().await
    }

    async fn save(&self, path: &Path) -> Result<(), AgentError> {
        self.inner.save(path).await
    }

    async fn load(&self, path: &Path) -> Result<(), AgentError> {
        self.inner.load(path).await
    }

    fn supports_persistence(&self) -> bool {
        true
    }
}

/// No-op cache that doesn't store anything
pub struct NoOpEmbeddingCache;

impl NoOpEmbeddingCache {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoOpEmbeddingCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingCache for NoOpEmbeddingCache {
    async fn get(&self, _text: &str) -> Option<Vec<f32>> {
        None
    }

    async fn put(&self, _text: String, _embedding: Vec<f32>) -> Result<(), AgentError> {
        Ok(())
    }

    async fn get_batch(&self, texts: &[String]) -> Vec<Option<Vec<f32>>> {
        vec![None; texts.len()]
    }

    async fn put_batch(
        &self,
        _texts: Vec<String>,
        _embeddings: Vec<Vec<f32>>,
    ) -> Result<(), AgentError> {
        Ok(())
    }

    async fn clear(&self) -> Result<(), AgentError> {
        Ok(())
    }

    async fn size(&self) -> usize {
        0
    }

    async fn save(&self, _path: &Path) -> Result<(), AgentError> {
        Ok(())
    }

    async fn load(&self, _path: &Path) -> Result<(), AgentError> {
        Ok(())
    }

    fn supports_persistence(&self) -> bool {
        false
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_entries: usize,
    pub max_size: usize,
    pub total_accesses: u64,
    pub hit_rate: f64,
}

/// Factory for creating embedding caches
pub struct EmbeddingCacheFactory;

impl EmbeddingCacheFactory {
    /// Create an in-memory cache
    pub fn create_memory_cache(config: EmbeddingCacheConfig) -> Box<dyn EmbeddingCache> {
        Box::new(InMemoryEmbeddingCache::new(config))
    }

    /// Create a persistent cache
    pub async fn create_persistent_cache(
        config: EmbeddingCacheConfig,
        cache_path: impl AsRef<Path>,
    ) -> Result<Box<dyn EmbeddingCache>, AgentError> {
        let cache = PersistentEmbeddingCache::new(config, cache_path).await?;
        Ok(Box::new(cache))
    }

    /// Create a no-op cache
    pub fn create_noop_cache() -> Box<dyn EmbeddingCache> {
        Box::new(NoOpEmbeddingCache::new())
    }

    /// Create a cache based on configuration
    pub async fn create_from_config(
        config: EmbeddingCacheConfig,
    ) -> Result<Box<dyn EmbeddingCache>, AgentError> {
        if config.persistent {
            if let Some(cache_path) = config.cache_file_path.clone() {
                Self::create_persistent_cache(config, cache_path).await
            } else {
                Err(AgentError::RagError(
                    "Persistent cache enabled but no cache file path provided".to_string(),
                ))
            }
        } else {
            Ok(Self::create_memory_cache(config))
        }
    }
}

/// Utility function to create a text hash for cache keys
pub fn text_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_in_memory_cache_basic_operations() {
        let cache = InMemoryEmbeddingCache::with_max_size(100);

        // Test put and get
        let text = "test text".to_string();
        let embedding = vec![0.1, 0.2, 0.3];

        cache.put(text.clone(), embedding.clone()).await.unwrap();
        let retrieved = cache.get(&text).await;

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap(), embedding);

        // Test size
        assert_eq!(cache.size().await, 1);

        // Test clear
        cache.clear().await.unwrap();
        assert_eq!(cache.size().await, 0);
        assert!(cache.get(&text).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_batch_operations() {
        let cache = InMemoryEmbeddingCache::with_max_size(100);

        let texts = vec!["text1".to_string(), "text2".to_string()];
        let embeddings = vec![vec![0.1, 0.2], vec![0.3, 0.4]];

        cache
            .put_batch(texts.clone(), embeddings.clone())
            .await
            .unwrap();

        let retrieved = cache.get_batch(&texts).await;
        assert_eq!(retrieved.len(), 2);
        assert!(retrieved[0].is_some());
        assert!(retrieved[1].is_some());
        assert_eq!(retrieved[0].as_ref().unwrap(), &embeddings[0]);
        assert_eq!(retrieved[1].as_ref().unwrap(), &embeddings[1]);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let config = EmbeddingCacheConfig {
            max_size: 2,
            eviction_strategy: EvictionStrategy::LRU,
            ..Default::default()
        };
        let cache = InMemoryEmbeddingCache::new(config);

        // Add 2 items first (within limit)
        cache.put("text1".to_string(), vec![0.1]).await.unwrap();
        cache.put("text2".to_string(), vec![0.2]).await.unwrap();
        assert_eq!(cache.size().await, 2);

        // Access text1 to make it more recently used than text2
        cache.get("text1").await;
        
        // Add a third item, which should evict text2 (least recently used)
        cache.put("text3".to_string(), vec![0.3]).await.unwrap();

        // Should still have 2 items
        assert_eq!(cache.size().await, 2);
        
        // text2 should have been evicted, text1 and text3 should remain
        assert!(cache.get("text1").await.is_some(), "text1 should still exist (was accessed recently)");
        assert!(cache.get("text2").await.is_none(), "text2 should have been evicted (least recently used)");
        assert!(cache.get("text3").await.is_some(), "text3 should still exist (just added)");
    }

    #[tokio::test]
    async fn test_persistent_cache() {
        let temp_dir = tempdir().unwrap();
        let cache_path = temp_dir.path().join("test_cache.json");

        let config = EmbeddingCacheConfig::default();

        // Create cache and add some data
        {
            let cache = PersistentEmbeddingCache::new(config.clone(), &cache_path)
                .await
                .unwrap();

            cache
                .put("test".to_string(), vec![0.1, 0.2, 0.3])
                .await
                .unwrap();
            cache.save(&cache_path).await.unwrap();
        }

        // Create new cache and load data
        {
            let cache = PersistentEmbeddingCache::new(config, &cache_path)
                .await
                .unwrap();

            let retrieved = cache.get("test").await;
            assert!(retrieved.is_some());
            assert_eq!(retrieved.unwrap(), vec![0.1, 0.2, 0.3]);
        }
    }

    #[tokio::test]
    async fn test_noop_cache() {
        let cache = NoOpEmbeddingCache::new();

        cache.put("test".to_string(), vec![0.1, 0.2]).await.unwrap();
        assert!(cache.get("test").await.is_none());
        assert_eq!(cache.size().await, 0);
        assert!(!cache.supports_persistence());
    }

    #[tokio::test]
    async fn test_cache_factory() {
        // Test memory cache creation
        let config = EmbeddingCacheConfig {
            persistent: false,
            ..Default::default()
        };
        let cache = EmbeddingCacheFactory::create_from_config(config)
            .await
            .unwrap();
        assert!(cache.supports_persistence());

        // Test no-op cache creation
        let noop_cache = EmbeddingCacheFactory::create_noop_cache();
        assert!(!noop_cache.supports_persistence());
    }

    #[test]
    fn test_text_hash() {
        let text1 = "hello world";
        let text2 = "hello world";
        let text3 = "different text";

        assert_eq!(text_hash(text1), text_hash(text2));
        assert_ne!(text_hash(text1), text_hash(text3));
    }
}
