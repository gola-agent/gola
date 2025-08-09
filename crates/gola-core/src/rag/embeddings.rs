use crate::errors::AgentError;
use crate::llm::LLM;
use crate::rag::cache::{EmbeddingCache, NoOpEmbeddingCache};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[async_trait]
pub trait EmbeddingGenerator: Send + Sync {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, AgentError>;

    async fn generate_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AgentError> {
        let mut embeddings = Vec::new();
        for text in texts {
            let embedding = self.generate_embedding(text).await?;
            embeddings.push(embedding);
        }
        Ok(embeddings)
    }

    fn embedding_dimension(&self) -> usize;

    /// Support for downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any;
}

pub struct DummyEmbeddingGenerator {
    embedding_dimension: usize,
}

impl DummyEmbeddingGenerator {
    pub fn new() -> Self {
        Self {
            embedding_dimension: 384,
        }
    }

    pub fn with_dimension(dimension: usize) -> Self {
        Self {
            embedding_dimension: dimension,
        }
    }
}

impl Default for DummyEmbeddingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingGenerator for DummyEmbeddingGenerator {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, AgentError> {
        // Generate a simple hash-based embedding for testing
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // Create a deterministic embedding based on the hash
        let mut embedding = vec![0.0; self.embedding_dimension];
        for i in 0..self.embedding_dimension {
            let seed = hash.wrapping_add(i as u64);
            embedding[i] = ((seed % 1000) as f32 - 500.0) / 500.0; // Normalize to [-1, 1]
        }

        // Normalize the vector
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for val in &mut embedding {
                *val /= magnitude;
            }
        }

        Ok(embedding)
    }

    fn embedding_dimension(&self) -> usize {
        self.embedding_dimension
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct LLMEmbeddingGenerator {
    #[allow(dead_code)] // Will be used in future implementation
    llm: Box<dyn LLM>,
    embedding_dimension: usize,
}

impl LLMEmbeddingGenerator {
    pub fn new(llm: Box<dyn LLM>) -> Self {
        Self {
            llm,
            embedding_dimension: 1536,
        }
    }

    pub fn with_dimension(mut self, dimension: usize) -> Self {
        self.embedding_dimension = dimension;
        self
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    dot_product / (magnitude_a * magnitude_b)
}

pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Cached embedding generator that wraps another generator with caching
pub struct CachedEmbeddingGenerator {
    inner: Box<dyn EmbeddingGenerator>,
    cache: Arc<dyn EmbeddingCache>,
}

impl CachedEmbeddingGenerator {
    /// Create a new cached embedding generator
    pub fn new(inner: Box<dyn EmbeddingGenerator>, cache: Arc<dyn EmbeddingCache>) -> Self {
        Self { inner, cache }
    }

    /// Create a cached generator with no-op cache (effectively no caching)
    pub fn with_no_cache(inner: Box<dyn EmbeddingGenerator>) -> Self {
        Self {
            inner,
            cache: Arc::new(NoOpEmbeddingCache::new()),
        }
    }

    /// Get cache statistics if available
    pub async fn cache_size(&self) -> usize {
        self.cache.size().await
    }

    /// Clear the cache
    pub async fn clear_cache(&self) -> Result<(), AgentError> {
        self.cache.clear().await
    }

    /// Save cache to disk if supported
    pub async fn save_cache(&self, path: &std::path::Path) -> Result<(), AgentError> {
        if self.cache.supports_persistence() {
            self.cache.save(path).await
        } else {
            Err(AgentError::RagError(
                "Cache does not support persistence".to_string(),
            ))
        }
    }

    /// Load cache from disk if supported
    pub async fn load_cache(&self, path: &std::path::Path) -> Result<(), AgentError> {
        if self.cache.supports_persistence() {
            self.cache.load(path).await
        } else {
            Err(AgentError::RagError(
                "Cache does not support persistence".to_string(),
            ))
        }
    }
}

#[async_trait]
impl EmbeddingGenerator for CachedEmbeddingGenerator {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, AgentError> {
        // Check cache first
        if let Some(cached_embedding) = self.cache.get(text).await {
            log::debug!("Cache hit for text: {}", text);
            return Ok(cached_embedding);
        }

        log::debug!("Cache miss for text: {}", text);

        // Generate embedding using inner generator
        let embedding = self.inner.generate_embedding(text).await?;

        // Store in cache
        if let Err(e) = self.cache.put(text.to_string(), embedding.clone()).await {
            log::warn!("Failed to cache embedding: {}", e);
        }

        Ok(embedding)
    }

    async fn generate_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AgentError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Check cache for all texts
        let cached_results = self.cache.get_batch(texts).await;

        // Separate cached and uncached texts
        let mut uncached_texts = Vec::new();
        let mut uncached_indices = Vec::new();
        let mut results = vec![None; texts.len()];

        for (i, (text, cached_result)) in texts.iter().zip(cached_results.iter()).enumerate() {
            if let Some(embedding) = cached_result {
                results[i] = Some(embedding.clone());
            } else {
                uncached_texts.push(text.clone());
                uncached_indices.push(i);
            }
        }

        log::debug!(
            "Cache stats: {}/{} hits, {} misses",
            results.iter().filter(|r| r.is_some()).count(),
            texts.len(),
            uncached_texts.len()
        );

        // Generate embeddings for uncached texts
        if !uncached_texts.is_empty() {
            let new_embeddings = self.inner.generate_embeddings(&uncached_texts).await?;

            // Store new embeddings in cache
            if let Err(e) = self
                .cache
                .put_batch(uncached_texts.clone(), new_embeddings.clone())
                .await
            {
                log::warn!("Failed to cache batch embeddings: {}", e);
            }

            // Fill in the results
            for (idx, embedding) in uncached_indices.into_iter().zip(new_embeddings.into_iter()) {
                results[idx] = Some(embedding);
            }
        }

        // Convert Option<Vec<f32>> to Vec<f32>
        let final_results: Result<Vec<Vec<f32>>, _> = results
            .into_iter()
            .enumerate()
            .map(|(i, opt)| {
                opt.ok_or_else(|| {
                    AgentError::RagError(format!("Missing embedding for text at index {}", i))
                })
            })
            .collect();

        final_results
    }

    fn embedding_dimension(&self) -> usize {
        self.inner.embedding_dimension()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl EmbeddingData {
    pub fn new(text: String, embedding: Vec<f32>) -> Self {
        Self {
            text,
            embedding,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, metadata: std::collections::HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }
}

#[derive(Debug, Clone)]
pub struct RestEmbeddingConfig {
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub model_name: String,
    pub embedding_dimension: usize,
    pub timeout_seconds: u64,
    pub max_batch_size: usize,
    pub provider: EmbeddingProvider,
}

impl Default for RestEmbeddingConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            model_name: "text-embedding-3-small".to_string(),
            embedding_dimension: 1536,
            timeout_seconds: 30,
            max_batch_size: 100,
            provider: EmbeddingProvider::OpenAI,
        }
    }
}

/// Supported embedding providers
#[derive(Debug, Clone, PartialEq)]
pub enum EmbeddingProvider {
    OpenAI,
    Cohere,
    HuggingFace,
    Custom,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((euclidean_distance(&a, &b) - 1.0).abs() < 0.001);
        assert!((euclidean_distance(&a, &c) - 1.0).abs() < 0.001);
        assert!((euclidean_distance(&b, &c) - 2.0_f32.sqrt()).abs() < 0.001);
    }

    #[test]
    fn test_embedding_data() {
        let embedding = vec![0.1, 0.2, 0.3];
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("source".to_string(), "test.txt".to_string());

        let data = EmbeddingData::new("test text".to_string(), embedding.clone())
            .with_metadata(metadata.clone());

        assert_eq!(data.text, "test text");
        assert_eq!(data.embedding, embedding);
        assert_eq!(data.metadata, metadata);
    }

    #[tokio::test]
    async fn test_dummy_embedding_generator() {
        let generator = DummyEmbeddingGenerator::new();

        let text = "Test text for embedding";
        let embedding = generator.generate_embedding(text).await.unwrap();

        assert_eq!(embedding.len(), 384);

        // Test that the same text produces the same embedding
        let embedding2 = generator.generate_embedding(text).await.unwrap();
        assert_eq!(embedding, embedding2);

        // Test that different texts produce different embeddings
        let embedding3 = generator
            .generate_embedding("Different text")
            .await
            .unwrap();
        assert_ne!(embedding, embedding3);
    }

    #[tokio::test]
    async fn test_rest_embedding_client_creation() {
        let client = RestEmbeddingClient::new_openai("test-key".to_string(), None);
        assert!(client.is_ok());

        let client = client.unwrap();
        assert_eq!(client.config().provider, EmbeddingProvider::OpenAI);
        assert_eq!(client.config().embedding_dimension, 1536);
    }

    #[tokio::test]
    async fn test_rest_embedding_generation() {
        let client = RestEmbeddingClient::new_openai("test-key".to_string(), None).unwrap();
        let text = "Test embedding generation";

        // This should return an error since it's not implemented
        let embedding = client.generate_embedding(text).await;
        assert!(embedding.is_err());

        let texts = vec!["Text 1".to_string(), "Text 2".to_string()];
        let embeddings = client.generate_embeddings(&texts).await;
        assert!(embeddings.is_err());
    }

    #[tokio::test]
    async fn test_cached_embedding_generator() {
        use crate::rag::cache::{EmbeddingCacheConfig, InMemoryEmbeddingCache};

        let dummy_generator = Box::new(DummyEmbeddingGenerator::new());
        let cache = Arc::new(InMemoryEmbeddingCache::new(EmbeddingCacheConfig::default()));
        let cached_generator = CachedEmbeddingGenerator::new(dummy_generator, cache);

        let text = "Test caching";

        // First call should generate and cache
        let embedding1 = cached_generator.generate_embedding(text).await.unwrap();
        assert_eq!(cached_generator.cache_size().await, 1);

        // Second call should use cache
        let embedding2 = cached_generator.generate_embedding(text).await.unwrap();
        assert_eq!(embedding1, embedding2);
        assert_eq!(cached_generator.cache_size().await, 1);

        // Clear cache
        cached_generator.clear_cache().await.unwrap();
        assert_eq!(cached_generator.cache_size().await, 0);
    }

    #[tokio::test]
    async fn test_cached_embedding_generator_batch() {
        use crate::rag::cache::{EmbeddingCacheConfig, InMemoryEmbeddingCache};

        let dummy_generator = Box::new(DummyEmbeddingGenerator::new());
        let cache = Arc::new(InMemoryEmbeddingCache::new(EmbeddingCacheConfig::default()));
        let cached_generator = CachedEmbeddingGenerator::new(dummy_generator, cache);

        let texts = vec![
            "Text 1".to_string(),
            "Text 2".to_string(),
            "Text 3".to_string(),
        ];

        // First batch call
        let embeddings1 = cached_generator.generate_embeddings(&texts).await.unwrap();
        assert_eq!(embeddings1.len(), 3);
        assert_eq!(cached_generator.cache_size().await, 3);

        // Second batch call with some overlap
        let texts2 = vec![
            "Text 2".to_string(),
            "Text 3".to_string(),
            "Text 4".to_string(),
        ];
        let embeddings2 = cached_generator.generate_embeddings(&texts2).await.unwrap();
        assert_eq!(embeddings2.len(), 3);
        assert_eq!(cached_generator.cache_size().await, 4); // Only Text 4 is new

        // Verify cached embeddings are the same
        assert_eq!(embeddings1[1], embeddings2[0]); // Text 2
        assert_eq!(embeddings1[2], embeddings2[1]); // Text 3
    }
}

pub struct RestEmbeddingClient {
    client: Client,
    config: RestEmbeddingConfig,
}

impl RestEmbeddingClient {
    /// Create a new REST embedding client with the given configuration
    pub fn new(config: RestEmbeddingConfig) -> Result<Self, AgentError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| AgentError::RagError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { client, config })
    }

    /// Create a client for OpenAI embeddings
    pub fn new_openai(api_key: String, model: Option<String>) -> Result<Self, AgentError> {
        let config = RestEmbeddingConfig {
            api_base_url: "https://api.openai.com/v1".to_string(),
            api_key: Some(api_key),
            model_name: model.unwrap_or_else(|| "text-embedding-3-small".to_string()),
            embedding_dimension: 1536,
            timeout_seconds: 30,
            max_batch_size: 100,
            provider: EmbeddingProvider::OpenAI,
        };
        Self::new(config)
    }

    /// Get the current configuration
    pub fn config(&self) -> &RestEmbeddingConfig {
        &self.config
    }

    pub fn create_batches(&self, texts: &[String]) -> Vec<Vec<String>> {
        let batch_size = self.config.max_batch_size;
        texts
            .chunks(batch_size)
            .map(|chunk| chunk.to_vec())
            .collect()
    }

    async fn call_openai_api(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AgentError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| AgentError::RagError("OpenAI API key not configured".to_string()))?;

        let url = format!("{}/embeddings", self.config.api_base_url);

        let valid_texts: Vec<&String> = texts
            .iter()
            .filter(|text| !text.trim().is_empty())
            .collect();

        if valid_texts.is_empty() {
            return Err(AgentError::RagError(
                "No valid texts provided for embedding".to_string(),
            ));
        }

        let input_value = if valid_texts.len() == 1 {
            json!(valid_texts[0])
        } else {
            json!(valid_texts)
        };

        let payload = json!({
            "model": self.config.model_name,
            "input": input_value,
            "encoding_format": "float"
        });

        log::debug!(
            "OpenAI API request: {}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::RagError(format!("OpenAI API request failed: {}", e)))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .map_err(|e| AgentError::RagError(format!("Failed to read response: {}", e)))?;

        if !status.is_success() {
            log::error!("OpenAI API error response: {}", response_text);
            return Err(AgentError::RagError(format!(
                "OpenAI API error ({}): {}",
                status, response_text
            )));
        }

        let response_data: OpenAIEmbeddingResponse =
            serde_json::from_str(&response_text).map_err(|e| {
                AgentError::RagError(format!(
                    "Failed to parse OpenAI response: {}. Response: {}",
                    e, response_text
                ))
            })?;

        let mut embeddings = Vec::new();
        for item in response_data.data {
            embeddings.push(item.embedding);
        }

        if embeddings.len() != valid_texts.len() {
            return Err(AgentError::RagError(format!(
                "Mismatch between input texts ({}) and returned embeddings ({})",
                valid_texts.len(),
                embeddings.len()
            )));
        }

        Ok(embeddings)
    }

    async fn call_cohere_api(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AgentError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| AgentError::RagError("Cohere API key not configured".to_string()))?;

        let url = format!("{}/embed", self.config.api_base_url);

        let payload = json!({
            "model": self.config.model_name,
            "texts": texts,
            "input_type": "search_document"
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::RagError(format!("Cohere API request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::RagError(format!(
                "Cohere API error: {}",
                error_text
            )));
        }

        let response_data: CohereEmbeddingResponse = response
            .json()
            .await
            .map_err(|e| AgentError::RagError(format!("Failed to parse Cohere response: {}", e)))?;

        Ok(response_data.embeddings)
    }

    async fn call_huggingface_api(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AgentError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            AgentError::RagError("HuggingFace API key not configured".to_string())
        })?;

        let url = format!(
            "{}/pipeline/feature-extraction/{}",
            self.config.api_base_url, self.config.model_name
        );

        let payload = json!({
            "inputs": texts,
            "options": {
                "wait_for_model": true
            }
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::RagError(format!("HuggingFace API request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::RagError(format!(
                "HuggingFace API error: {}",
                error_text
            )));
        }

        let embeddings: Vec<Vec<f32>> = response.json().await.map_err(|e| {
            AgentError::RagError(format!("Failed to parse HuggingFace response: {}", e))
        })?;

        Ok(embeddings)
    }

    async fn call_custom_api(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AgentError> {
        let url = format!("{}/embeddings", self.config.api_base_url);

        let mut request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let payload = json!({
            "model": self.config.model_name,
            "input": texts
        });

        let response = request
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::RagError(format!("Custom API request failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::RagError(format!(
                "Custom API error: {}",
                error_text
            )));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| AgentError::RagError(format!("Failed to read response: {}", e)))?;

        // Try OpenAI format first
        if let Ok(openai_response) = serde_json::from_str::<OpenAIEmbeddingResponse>(&response_text)
        {
            let embeddings = openai_response
                .data
                .into_iter()
                .map(|item| item.embedding)
                .collect();
            return Ok(embeddings);
        }

        // Fallback to simple array format
        let embeddings: Vec<Vec<f32>> = serde_json::from_str(&response_text).map_err(|e| {
            AgentError::RagError(format!("Failed to parse custom API response: {}", e))
        })?;

        Ok(embeddings)
    }
}

#[async_trait]
impl EmbeddingGenerator for RestEmbeddingClient {
    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, AgentError> {
        let embeddings = self.generate_embeddings(&[text.to_string()]).await?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| AgentError::RagError("No embedding returned from API".to_string()))
    }

    async fn generate_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AgentError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        log::info!(
            "Generating embeddings for {} texts using {:?} provider",
            texts.len(),
            self.config.provider
        );

        // Process in batches
        let batches = self.create_batches(texts);
        let mut all_embeddings = Vec::new();

        for (i, batch) in batches.iter().enumerate() {
            log::debug!(
                "Processing batch {}/{} ({} texts)",
                i + 1,
                batches.len(),
                batch.len()
            );

            let batch_embeddings = match self.config.provider {
                EmbeddingProvider::OpenAI => self.call_openai_api(batch).await?,
                EmbeddingProvider::Cohere => self.call_cohere_api(batch).await?,
                EmbeddingProvider::HuggingFace => self.call_huggingface_api(batch).await?,
                EmbeddingProvider::Custom => self.call_custom_api(batch).await?,
            };

            all_embeddings.extend(batch_embeddings);

            // Add a small delay between batches to respect rate limits
            if i < batches.len() - 1 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        log::info!("Successfully generated {} embeddings", all_embeddings.len());
        Ok(all_embeddings)
    }

    fn embedding_dimension(&self) -> usize {
        self.config.embedding_dimension
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, serde::Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<OpenAIEmbeddingItem>,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct OpenAIEmbeddingItem {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, serde::Deserialize)]
struct CohereEmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Enhanced factory for creating REST embedding clients
pub struct RestEmbeddingFactory;

impl RestEmbeddingFactory {
    /// Create an OpenAI embedding client from environment variable
    pub fn create_openai_from_env(
        model: Option<String>,
    ) -> Result<RestEmbeddingClient, AgentError> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
            AgentError::RagError("OPENAI_API_KEY environment variable not set".to_string())
        })?;
        RestEmbeddingClient::new_openai(api_key, model)
    }

    /// Create a Cohere embedding client from environment variable
    pub fn create_cohere_from_env(
        model: Option<String>,
    ) -> Result<RestEmbeddingClient, AgentError> {
        let api_key = std::env::var("COHERE_API_KEY").map_err(|_| {
            AgentError::RagError("COHERE_API_KEY environment variable not set".to_string())
        })?;

        let config = RestEmbeddingConfig {
            api_base_url: "https://api.cohere.ai/v1".to_string(),
            api_key: Some(api_key),
            model_name: model.unwrap_or_else(|| "embed-english-v3.0".to_string()),
            embedding_dimension: 1024,
            timeout_seconds: 30,
            max_batch_size: 96,
            provider: EmbeddingProvider::Cohere,
        };

        RestEmbeddingClient::new(config)
    }

    /// Create a HuggingFace embedding client from environment variable
    pub fn create_huggingface_from_env(
        model: Option<String>,
    ) -> Result<RestEmbeddingClient, AgentError> {
        let api_key = std::env::var("HUGGINGFACE_API_KEY").map_err(|_| {
            AgentError::RagError("HUGGINGFACE_API_KEY environment variable not set".to_string())
        })?;

        let config = RestEmbeddingConfig {
            api_base_url: "https://api-inference.huggingface.co".to_string(),
            api_key: Some(api_key),
            model_name: model
                .unwrap_or_else(|| "sentence-transformers/all-MiniLM-L6-v2".to_string()),
            embedding_dimension: 384,
            timeout_seconds: 60,
            max_batch_size: 32,
            provider: EmbeddingProvider::HuggingFace,
        };

        RestEmbeddingClient::new(config)
    }

    /// Create a client based on provider name and environment variables
    pub fn create_from_provider(
        provider: &str,
        model: Option<String>,
    ) -> Result<RestEmbeddingClient, AgentError> {
        match provider.to_lowercase().as_str() {
            "openai" => Self::create_openai_from_env(model),
            "cohere" => Self::create_cohere_from_env(model),
            "huggingface" => Self::create_huggingface_from_env(model),
            _ => Err(AgentError::RagError(format!(
                "Unsupported provider: {}",
                provider
            ))),
        }
    }
}
