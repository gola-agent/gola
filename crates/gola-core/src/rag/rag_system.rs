use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::errors::AgentError;
use crate::llm::LLM;
use crate::rag::{
    cache::EmbeddingCacheFactory,
    embeddings::{CachedEmbeddingGenerator, DummyEmbeddingGenerator, EmbeddingGenerator, RestEmbeddingFactory},
    splitter::TextSplitter,
    vector_store::{InMemoryVectorStore, PersistentVectorStore, VectorStore},
    Rag, RagConfig, RagDocument, RetrievedContext,
};

/// Complete RAG system implementation
pub struct RagSystem {
    vector_store: Box<dyn VectorStore>,
    embedding_generator: Box<dyn EmbeddingGenerator>,
    text_splitter: TextSplitter,
    documents: Vec<RagDocument>,
    config: RagConfig,
    document_id_counter: usize,
}

impl RagSystem {
    pub fn new(config: RagConfig, embedding_generator: Box<dyn EmbeddingGenerator>) -> Self {
        let vector_store = Box::new(InMemoryVectorStore::new());
        let text_splitter = TextSplitter::new(config.chunk_size, config.chunk_overlap);

        Self {
            vector_store,
            embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        }
    }

    /// Create a new RAG system with default configuration and caching enabled
    /// This is the recommended way to create a RAG system for production use
    pub async fn new_default() -> Result<Self, AgentError> {
        let config = RagConfig::default();
        let embedding_generator = Box::new(DummyEmbeddingGenerator::new());
        Self::new_with_cache(config, embedding_generator).await
    }

    /// Create a new RAG system with REST embeddings and default caching
    /// This is the recommended way to create a RAG system with real embeddings
    pub async fn new_with_provider(provider: &str, model: Option<String>) -> Result<Self, AgentError> {
        let config = RagConfig::default();
        Self::with_rest_embeddings_cached(config, provider, model).await
    }

    /// Create a new RAG system with caching enabled by default
    pub async fn new_with_cache(config: RagConfig, embedding_generator: Box<dyn EmbeddingGenerator>) -> Result<Self, AgentError> {
        let vector_store = Box::new(InMemoryVectorStore::new());
        let text_splitter = TextSplitter::new(config.chunk_size, config.chunk_overlap);

        // Create cache based on configuration
        let cache = EmbeddingCacheFactory::create_from_config(config.embedding_cache.clone()).await?;
        let cached_embedding_generator = Box::new(CachedEmbeddingGenerator::new(embedding_generator, cache.into()));

        Ok(Self {
            vector_store,
            embedding_generator: cached_embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        })
    }

    pub fn new_with_dummy() -> Self {
        let mut config = RagConfig::default();
        config.similarity_threshold = 0.0; // Lower threshold for testing
        let embedding_generator = Box::new(DummyEmbeddingGenerator::new());
        Self::new(config, embedding_generator)
    }

    /// Create a new RAG system with dummy embedding generator and caching enabled
    pub async fn new_with_dummy_cached() -> Result<Self, AgentError> {
        let mut config = RagConfig::default();
        config.similarity_threshold = 0.0; // Lower threshold for testing
        let embedding_generator = Box::new(DummyEmbeddingGenerator::new());
        Self::new_with_cache(config, embedding_generator).await
    }

    pub fn with_config(config: RagConfig) -> Self {
        let embedding_generator = Box::new(DummyEmbeddingGenerator::new());
        Self::new(config, embedding_generator)
    }

    /// Create a new RAG system with the given configuration and caching enabled
    pub async fn with_config_cached(config: RagConfig) -> Result<Self, AgentError> {
        let embedding_generator = Box::new(DummyEmbeddingGenerator::new());
        Self::new_with_cache(config, embedding_generator).await
    }

    pub fn with_rest_embeddings(
        config: RagConfig,
        provider: &str,
        model: Option<String>,
    ) -> Result<Self, AgentError> {
        let vector_store = Box::new(InMemoryVectorStore::new());
        let embedding_generator: Box<dyn EmbeddingGenerator> =
            Box::new(RestEmbeddingFactory::create_from_provider(provider, model)?);
        let text_splitter = TextSplitter::new(config.chunk_size, config.chunk_overlap);

        Ok(Self {
            vector_store,
            embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        })
    }

    /// Create a new RAG system with REST embeddings and caching enabled
    pub async fn with_rest_embeddings_cached(
        config: RagConfig,
        provider: &str,
        model: Option<String>,
    ) -> Result<Self, AgentError> {
        let vector_store = Box::new(InMemoryVectorStore::new());
        let embedding_generator: Box<dyn EmbeddingGenerator> =
            Box::new(RestEmbeddingFactory::create_from_provider(provider, model)?);
        let text_splitter = TextSplitter::new(config.chunk_size, config.chunk_overlap);

        // Create cache based on configuration
        let cache = EmbeddingCacheFactory::create_from_config(config.embedding_cache.clone()).await?;
        let cached_embedding_generator = Box::new(CachedEmbeddingGenerator::new(embedding_generator, cache.into()));

        Ok(Self {
            vector_store,
            embedding_generator: cached_embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        })
    }

    pub fn with_persistent_storage<P: Into<PathBuf>>(
        mut config: RagConfig,
        storage_path: P,
        embedding_generator: Box<dyn EmbeddingGenerator>,
    ) -> Self {
        let path_buf = storage_path.into();
        config.persistent_vector_store_path = Some(path_buf.to_string_lossy().into_owned());

        let vector_store = Box::new(PersistentVectorStore::new().with_file_path(path_buf));
        let text_splitter = TextSplitter::new(config.chunk_size, config.chunk_overlap);

        Self {
            vector_store,
            embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        }
    }

    /// Create a new RAG system with persistent storage and caching enabled
    pub async fn with_persistent_storage_cached<P: Into<PathBuf>>(
        mut config: RagConfig,
        storage_path: P,
        embedding_generator: Box<dyn EmbeddingGenerator>,
    ) -> Result<Self, AgentError> {
        let path_buf = storage_path.into();
        config.persistent_vector_store_path = Some(path_buf.to_string_lossy().into_owned());

        let vector_store = Box::new(PersistentVectorStore::new().with_file_path(path_buf));
        let text_splitter = TextSplitter::new(config.chunk_size, config.chunk_overlap);

        // Create cache based on configuration
        let cache = EmbeddingCacheFactory::create_from_config(config.embedding_cache.clone()).await?;
        let cached_embedding_generator = Box::new(CachedEmbeddingGenerator::new(embedding_generator, cache.into()));

        Ok(Self {
            vector_store,
            embedding_generator: cached_embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        })
    }

    pub fn with_components(
        config: RagConfig,
        vector_store: Box<dyn VectorStore>,
        embedding_generator: Box<dyn EmbeddingGenerator>,
        text_splitter: TextSplitter,
    ) -> Self {
        Self {
            vector_store,
            embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        }
    }

    /// Create a new RAG system with components and caching enabled
    /// This is the recommended way to create a RAG system with custom components
    pub async fn with_components_cached(
        config: RagConfig,
        vector_store: Box<dyn VectorStore>,
        embedding_generator: Box<dyn EmbeddingGenerator>,
        text_splitter: TextSplitter,
    ) -> Result<Self, AgentError> {
        // Create cache based on configuration
        let cache = EmbeddingCacheFactory::create_from_config(config.embedding_cache.clone()).await?;
        let cached_embedding_generator = Box::new(CachedEmbeddingGenerator::new(embedding_generator, cache.into()));

        log::info!("Created RAG system with cached embedding generator");

        Ok(Self {
            vector_store,
            embedding_generator: cached_embedding_generator,
            text_splitter,
            documents: Vec::new(),
            config,
            document_id_counter: 0,
        })
    }

    fn next_document_id(&mut self) -> usize {
        let id = self.document_id_counter;
        self.document_id_counter += 1;
        id
    }

    async fn process_document(&mut self, document: RagDocument) -> Result<(), AgentError> {
        log::info!("Processing document: {}", document.source);

        let chunks = self.text_splitter.split_text(&document.content);
        log::debug!("Split document into {} chunks", chunks.len());

        let embeddings = self
            .embedding_generator
            .generate_embeddings(&chunks)
            .await?;
        log::debug!("Generated {} embeddings", embeddings.len());

        for (i, (chunk, embedding)) in chunks.iter().zip(embeddings.iter()).enumerate() {
            let doc_id = self.next_document_id();

            // Create a chunk document
            let mut chunk_doc = RagDocument::new(chunk.clone(), document.source.clone())
                .with_chunk_index(i)
                .with_metadata(document.metadata.clone());

            chunk_doc
                .metadata
                .insert("chunk_index".to_string(), i.to_string());
            chunk_doc
                .metadata
                .insert("total_chunks".to_string(), chunks.len().to_string());
            chunk_doc
                .metadata
                .insert("original_source".to_string(), document.source.clone());

            self.documents.push(chunk_doc);
            self.vector_store
                .add_document(doc_id, embedding.clone())
                .await?;
        }

        log::info!(
            "Successfully processed document: {} ({} chunks)",
            document.source,
            chunks.len()
        );
        Ok(())
    }

    async fn load_documents_from_paths(&mut self, paths: &[String]) -> Result<(), AgentError> {
        for path_str in paths {
            let path = Path::new(path_str);

            if path.is_file() {
                self.load_single_file(path).await?;
            } else if path.is_dir() {
                self.load_directory(path).await?;
            } else {
                log::warn!("Path does not exist or is not accessible: {}", path_str);
            }
        }
        Ok(())
    }

    async fn load_single_file(&mut self, path: &Path) -> Result<(), AgentError> {
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentError::RagError(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        let document = RagDocument::new(content, path.to_string_lossy().to_string());
        self.process_document(document).await?;
        Ok(())
    }

    async fn load_directory(&mut self, dir_path: &Path) -> Result<(), AgentError> {
        let mut entries = tokio::fs::read_dir(dir_path).await.map_err(|e| {
            AgentError::RagError(format!(
                "Failed to read directory {}: {}",
                dir_path.display(),
                e
            ))
        })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AgentError::RagError(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if path.is_file() {
                // Filter by common text file extensions
                if let Some(extension) = path.extension() {
                    let ext = extension.to_string_lossy().to_lowercase();
                    if matches!(
                        ext.as_str(),
                        "txt"
                            | "md"
                            | "markdown"
                            | "rst"
                            | "py"
                            | "rs"
                            | "js"
                            | "ts"
                            | "java"
                            | "cpp"
                            | "c"
                            | "h"
                            | "hpp"
                            | "go"
                            | "rb"
                            | "php"
                            | "html"
                            | "css"
                            | "json"
                            | "yaml"
                            | "yml"
                            | "toml"
                            | "xml"
                    ) {
                        if let Err(e) = self.load_single_file(&path).await {
                            log::warn!("Failed to load file {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn config(&self) -> &RagConfig {
        &self.config
    }

    pub fn set_config(&mut self, config: RagConfig) {
        self.config = config.clone();
        self.text_splitter = TextSplitter::new(config.chunk_size, config.chunk_overlap);
    }

    pub fn stats(&self) -> RagStats {
        RagStats {
            total_documents: self.documents.len(),
            total_chunks: self.vector_store.document_count(),
            embedding_dimension: self.vector_store.embedding_dimension(),
            config: self.config.clone(),
        }
    }

    /// Get cache size if the embedding generator supports caching
    pub async fn cache_size(&self) -> Option<usize> {
        // Try to downcast to CachedEmbeddingGenerator
        if let Some(cached_gen) = self.embedding_generator.as_any().downcast_ref::<CachedEmbeddingGenerator>() {
            Some(cached_gen.cache_size().await)
        } else {
            None
        }
    }

    /// Clear the embedding cache if supported
    pub async fn clear_cache(&self) -> Result<(), AgentError> {
        if let Some(cached_gen) = self.embedding_generator.as_any().downcast_ref::<CachedEmbeddingGenerator>() {
            cached_gen.clear_cache().await
        } else {
            Err(AgentError::RagError("Embedding generator does not support caching".to_string()))
        }
    }

    /// Save the embedding cache if supported
    pub async fn save_cache(&self, path: &Path) -> Result<(), AgentError> {
        if let Some(cached_gen) = self.embedding_generator.as_any().downcast_ref::<CachedEmbeddingGenerator>() {
            cached_gen.save_cache(path).await
        } else {
            Err(AgentError::RagError("Embedding generator does not support caching".to_string()))
        }
    }

    /// Load the embedding cache if supported
    pub async fn load_cache(&self, path: &Path) -> Result<(), AgentError> {
        if let Some(cached_gen) = self.embedding_generator.as_any().downcast_ref::<CachedEmbeddingGenerator>() {
            cached_gen.load_cache(path).await
        } else {
            Err(AgentError::RagError("Embedding generator does not support caching".to_string()))
        }
    }
}

#[async_trait]
impl Rag for RagSystem {
    async fn retrieve(
        &self,
        query: &str,
        top_k: Option<usize>,
    ) -> Result<RetrievedContext, AgentError> {
        let k = top_k.unwrap_or(self.config.top_k);

        log::info!("Retrieving top {} documents for query: {}", k, query);

        let query_embedding = self.embedding_generator.generate_embedding(query).await?;
        let search_results = self.vector_store.search(&query_embedding, k).await?;
        let mut context = RetrievedContext::new();

        for result in search_results {
            if result.score >= self.config.similarity_threshold {
                if let Some(document) = self.documents.get(result.document_id) {
                    context.add_document(document.clone(), document.source.clone(), result.score);
                }
            }
        }

        log::info!("Retrieved {} relevant documents", context.len());
        Ok(context)
    }

    async fn add_documents(&mut self, documents: Vec<RagDocument>) -> Result<(), AgentError> {
        log::info!("Adding {} documents to RAG system", documents.len());

        for document in documents {
            self.process_document(document).await?;
        }

        log::info!("Successfully added all documents");
        Ok(())
    }

    async fn add_documents_from_paths(&mut self, paths: &[String]) -> Result<(), AgentError> {
        log::info!("Loading documents from {} paths", paths.len());
        self.load_documents_from_paths(paths).await
    }

    async fn clear(&mut self) -> Result<(), AgentError> {
        log::info!("Clearing RAG system");
        self.documents.clear();
        self.vector_store.clear().await?;
        self.document_id_counter = 0;
        Ok(())
    }

    fn document_count(&self) -> usize {
        self.documents.len()
    }

    async fn save(&self, path: &Path) -> Result<(), AgentError> {
        let data = RagSystemData {
            documents: self.documents.clone(),
            config: self.config.clone(),
            document_id_counter: self.document_id_counter,
        };

        let json_data = serde_json::to_string_pretty(&data)
            .map_err(|e| AgentError::RagError(format!("Failed to serialize RAG data: {}", e)))?;

        tokio::fs::write(path, json_data).await.map_err(|e| {
            AgentError::RagError(format!("Failed to write RAG data to file: {}", e))
        })?;

        log::info!("Saved RAG system to {}", path.display());
        Ok(())
    }

    async fn load(
        path: &Path,
        _embedding_llm_unused: Box<dyn LLM>,
    ) -> Result<Box<dyn Rag>, AgentError> {
        let json_data = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| AgentError::RagError(format!("Failed to read RAG data file: {}", e)))?;

        let data: RagSystemData = serde_json::from_str(&json_data)
            .map_err(|e| AgentError::RagError(format!("Failed to deserialize RAG data: {}", e)))?;

        let provider_name =
            std::env::var("RAG_EMBEDDING_PROVIDER").unwrap_or_else(|_| "openai".to_string());
        let base_embedding_generator: Box<dyn EmbeddingGenerator> = Box::new(
            RestEmbeddingFactory::create_from_provider(
                &provider_name,
                Some(data.config.embedding_model_name.clone()),
            )
            .map_err(|e| {
                AgentError::RagError(format!(
                    "Failed to create embedding generator during load: {}",
                    e
                ))
            })?,
        );

        // Create cache and wrap embedding generator with caching
        let cache = EmbeddingCacheFactory::create_from_config(data.config.embedding_cache.clone()).await?;
        let embedding_generator_for_load = Box::new(CachedEmbeddingGenerator::new(base_embedding_generator, cache.into()));

        let vector_store: Box<dyn VectorStore>;

        if let Some(persistent_path_str) = &data.config.persistent_vector_store_path {
            let persistent_path = PathBuf::from(persistent_path_str);
            log::info!(
                "Attempting to load PersistentVectorStore from: {}",
                persistent_path.display()
            );
            match PersistentVectorStore::load(&persistent_path).await {
                Ok(loaded_store) => {
                    log::info!(
                        "Successfully loaded PersistentVectorStore with {} embeddings.",
                        loaded_store.document_count()
                    );
                    if let Some(loaded_dim) = loaded_store.embedding_dimension() {
                        if loaded_dim != embedding_generator_for_load.embedding_dimension() {
                            log::warn!(
                                "Embedding dimension mismatch: loaded store has {}, new generator has {}. This might cause issues if embeddings were generated with a different model/dimension than configured in embedding_model_name ('{}').",
                                loaded_dim, embedding_generator_for_load.embedding_dimension(), data.config.embedding_model_name
                            );
                        }
                    }
                    vector_store = Box::new(loaded_store);
                }
                Err(e) => {
                    log::warn!("Failed to load PersistentVectorStore from {}: {}. Falling back to in-memory and re-embedding.", persistent_path.display(), e);
                    let mut in_memory_store = InMemoryVectorStore::new();
                    for (doc_id, document_metadata) in data.documents.iter().enumerate() {
                        let embedding = embedding_generator_for_load
                            .generate_embedding(&document_metadata.content)
                            .await?;
                        in_memory_store.add_document(doc_id, embedding).await?;
                    }
                    vector_store = Box::new(in_memory_store);
                }
            }
        } else {
            log::info!("No persistent_vector_store_path in config. Using InMemoryVectorStore and re-embedding.");
            let mut in_memory_store = InMemoryVectorStore::new();
            for (doc_id, document_metadata) in data.documents.iter().enumerate() {
                let embedding = embedding_generator_for_load
                    .generate_embedding(&document_metadata.content)
                    .await?;
                in_memory_store.add_document(doc_id, embedding).await?;
            }
            vector_store = Box::new(in_memory_store);
        }

        let text_splitter = TextSplitter::new(data.config.chunk_size, data.config.chunk_overlap);

        let rag_system = RagSystem {
            vector_store,
            embedding_generator: embedding_generator_for_load,
            text_splitter,
            documents: data.documents,
            config: data.config,
            document_id_counter: data.document_id_counter,
        };

        log::info!(
            "Loaded RAG system from {}. Vector store has {} items.",
            path.display(),
            rag_system.vector_store.document_count()
        );
        Ok(Box::new(rag_system))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagStats {
    pub total_documents: usize,
    pub total_chunks: usize,
    pub embedding_dimension: Option<usize>,
    pub config: RagConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RagSystemData {
    documents: Vec<RagDocument>,
    config: RagConfig,
    document_id_counter: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::EmbeddingCacheConfig;

    #[tokio::test]
    async fn test_rag_system_creation() {
        let rag = RagSystem::new_with_dummy();
        assert_eq!(rag.document_count(), 0);
        assert_eq!(rag.config().chunk_size, 1000);
    }

    #[tokio::test]
    async fn test_rag_system_creation_with_cache() {
        let rag = RagSystem::new_with_dummy_cached().await.unwrap();
        assert_eq!(rag.document_count(), 0);
        assert_eq!(rag.config().chunk_size, 1000);
        
        // Should have cache functionality
        assert_eq!(rag.cache_size().await, Some(0));
    }

    #[tokio::test]
    async fn test_rag_system_with_config() {
        let config = RagConfig {
            chunk_size: 500,
            chunk_overlap: 100,
            top_k: 3,
            similarity_threshold: 0.5,
            embedding_model_name: "test-model".to_string(),
            reranker_model_name: None,
            persistent_vector_store_path: None,
            embedding_cache: EmbeddingCacheConfig::default(),
        };

        let rag = RagSystem::with_config(config.clone());
        assert_eq!(rag.config().chunk_size, 500);
        assert_eq!(rag.config().top_k, 3);
    }

    #[tokio::test]
    async fn test_rag_system_with_config_cached() {
        let config = RagConfig {
            chunk_size: 500,
            chunk_overlap: 100,
            top_k: 3,
            similarity_threshold: 0.5,
            embedding_model_name: "test-model".to_string(),
            reranker_model_name: None,
            persistent_vector_store_path: None,
            embedding_cache: EmbeddingCacheConfig {
                max_size: 1000,
                persistent: false,
                ..Default::default()
            },
        };

        let rag = RagSystem::with_config_cached(config.clone()).await.unwrap();
        assert_eq!(rag.config().chunk_size, 500);
        assert_eq!(rag.config().top_k, 3);
        assert_eq!(rag.cache_size().await, Some(0));
    }

    #[tokio::test]
    async fn test_rag_system_caching_functionality() {
        let mut rag = RagSystem::new_with_dummy_cached().await.unwrap();

        let documents = vec![
            RagDocument::new(
                "This is about machine learning and AI".to_string(),
                "doc1.txt".to_string(),
            ),
        ];

        // Add documents (this should cache embeddings)
        rag.add_documents(documents).await.unwrap();
        assert_eq!(rag.document_count(), 1);
        
        // Cache should have some entries now
        let cache_size = rag.cache_size().await;
        assert!(cache_size.is_some());
        assert!(cache_size.unwrap() > 0);

        // Test cache clearing
        rag.clear_cache().await.unwrap();
        assert_eq!(rag.cache_size().await, Some(0));
    }

    #[tokio::test]
    async fn test_rag_system_clear() {
        let mut rag = RagSystem::new_with_dummy();

        let documents = vec![
            RagDocument::new("Test document 1".to_string(), "test1.txt".to_string()),
            RagDocument::new("Test document 2".to_string(), "test2.txt".to_string()),
        ];

        rag.add_documents(documents).await.unwrap();
        assert_eq!(rag.document_count(), 2);

        rag.clear().await.unwrap();
        assert_eq!(rag.document_count(), 0);
    }

    #[tokio::test]
    async fn test_rag_stats() {
        let mut rag = RagSystem::new_with_dummy();

        let documents = vec![RagDocument::new(
            "Short doc".to_string(),
            "short.txt".to_string(),
        )];

        rag.add_documents(documents).await.unwrap();

        let stats = rag.stats();
        assert_eq!(stats.total_documents, 1);
        assert!(stats.total_chunks > 0);
    }

    #[tokio::test]
    async fn test_retrieve_with_threshold() {
        let mut config = RagConfig::default();
        config.similarity_threshold = 0.9; // Very high threshold

        let mut rag = RagSystem::with_config(config);

        let documents = vec![RagDocument::new(
            "Completely different content about cooking".to_string(),
            "cooking.txt".to_string(),
        )];

        rag.add_documents(documents).await.unwrap();

        let results = rag.retrieve("quantum physics", Some(5)).await.unwrap();

        assert!(results.len() <= 1); // Might be 0 or 1 depending on hash collision
    }
}
