use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::errors::AgentError;
use crate::rag::embeddings::{cosine_similarity, euclidean_distance};

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub document_id: usize,
    pub score: f32,
    pub distance: f32,
}

impl SearchResult {
    pub fn new(document_id: usize, score: f32, distance: f32) -> Self {
        Self {
            document_id,
            score,
            distance,
        }
    }
}

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn add_document(
        &mut self,
        document_id: usize,
        embedding: Vec<f32>,
    ) -> Result<(), AgentError>;

    async fn add_documents(&mut self, documents: Vec<(usize, Vec<f32>)>) -> Result<(), AgentError> {
        for (doc_id, embedding) in documents {
            self.add_document(doc_id, embedding).await?;
        }
        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, AgentError>;

    async fn remove_document(&mut self, document_id: usize) -> Result<(), AgentError>;

    async fn clear(&mut self) -> Result<(), AgentError>;

    fn document_count(&self) -> usize;

    fn embedding_dimension(&self) -> Option<usize>;
}

#[derive(Debug, Clone)]
pub struct InMemoryVectorStore {
    embeddings: HashMap<usize, Vec<f32>>,
    embedding_dimension: Option<usize>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
            embedding_dimension: None,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            embeddings: HashMap::with_capacity(capacity),
            embedding_dimension: None,
        }
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn add_document(
        &mut self,
        document_id: usize,
        embedding: Vec<f32>,
    ) -> Result<(), AgentError> {
        // Validate embedding dimension
        if let Some(expected_dim) = self.embedding_dimension {
            if embedding.len() != expected_dim {
                return Err(AgentError::RagError(format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    expected_dim,
                    embedding.len()
                )));
            }
        } else {
            self.embedding_dimension = Some(embedding.len());
        }

        self.embeddings.insert(document_id, embedding);
        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, AgentError> {
        if self.embeddings.is_empty() {
            return Ok(Vec::new());
        }

        if let Some(expected_dim) = self.embedding_dimension {
            if query_embedding.len() != expected_dim {
                return Err(AgentError::RagError(format!(
                    "Query embedding dimension mismatch: expected {}, got {}",
                    expected_dim,
                    query_embedding.len()
                )));
            }
        }

        let mut results = Vec::new();

        for (doc_id, embedding) in &self.embeddings {
            let similarity = cosine_similarity(query_embedding, embedding);
            let distance = euclidean_distance(query_embedding, embedding);

            results.push(SearchResult::new(*doc_id, similarity, distance));
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(top_k);
        Ok(results)
    }

    async fn remove_document(&mut self, document_id: usize) -> Result<(), AgentError> {
        self.embeddings.remove(&document_id);
        Ok(())
    }

    async fn clear(&mut self) -> Result<(), AgentError> {
        self.embeddings.clear();
        self.embedding_dimension = None;
        Ok(())
    }

    fn document_count(&self) -> usize {
        self.embeddings.len()
    }

    fn embedding_dimension(&self) -> Option<usize> {
        self.embedding_dimension
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentVectorStore {
    embeddings: HashMap<usize, Vec<f32>>,
    embedding_dimension: Option<usize>,
    #[serde(skip)]
    file_path: Option<std::path::PathBuf>,
}

impl PersistentVectorStore {
    pub fn new() -> Self {
        Self {
            embeddings: HashMap::new(),
            embedding_dimension: None,
            file_path: None,
        }
    }

    pub fn with_file_path<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.file_path = Some(path.into());
        self
    }

    pub async fn save(&self) -> Result<(), AgentError> {
        if let Some(path) = &self.file_path {
            let data = serde_json::to_string_pretty(self).map_err(|e| {
                AgentError::RagError(format!("Failed to serialize vector store: {}", e))
            })?;

            tokio::fs::write(path, data).await.map_err(|e| {
                AgentError::RagError(format!("Failed to write vector store to file: {}", e))
            })?;
        }
        Ok(())
    }

    pub async fn load<P: Into<std::path::PathBuf>>(path: P) -> Result<Self, AgentError> {
        let path = path.into();
        let data = tokio::fs::read_to_string(&path).await.map_err(|e| {
            AgentError::RagError(format!("Failed to read vector store file: {}", e))
        })?;

        let mut store: PersistentVectorStore = serde_json::from_str(&data).map_err(|e| {
            AgentError::RagError(format!("Failed to deserialize vector store: {}", e))
        })?;

        store.file_path = Some(path);
        Ok(store)
    }
}

impl Default for PersistentVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorStore for PersistentVectorStore {
    async fn add_document(
        &mut self,
        document_id: usize,
        embedding: Vec<f32>,
    ) -> Result<(), AgentError> {
        if let Some(expected_dim) = self.embedding_dimension {
            if embedding.len() != expected_dim {
                return Err(AgentError::RagError(format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    expected_dim,
                    embedding.len()
                )));
            }
        } else {
            self.embedding_dimension = Some(embedding.len());
        }

        self.embeddings.insert(document_id, embedding);

        if self.file_path.is_some() {
            self.save().await?;
        }

        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>, AgentError> {
        if self.embeddings.is_empty() {
            return Ok(Vec::new());
        }

        if let Some(expected_dim) = self.embedding_dimension {
            if query_embedding.len() != expected_dim {
                return Err(AgentError::RagError(format!(
                    "Query embedding dimension mismatch: expected {}, got {}",
                    expected_dim,
                    query_embedding.len()
                )));
            }
        }

        let mut results = Vec::new();

        for (doc_id, embedding) in &self.embeddings {
            let similarity = cosine_similarity(query_embedding, embedding);
            let distance = euclidean_distance(query_embedding, embedding);

            results.push(SearchResult::new(*doc_id, similarity, distance));
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(top_k);
        Ok(results)
    }

    async fn remove_document(&mut self, document_id: usize) -> Result<(), AgentError> {
        self.embeddings.remove(&document_id);

        if self.file_path.is_some() {
            self.save().await?;
        }

        Ok(())
    }

    async fn clear(&mut self) -> Result<(), AgentError> {
        self.embeddings.clear();
        self.embedding_dimension = None;

        if self.file_path.is_some() {
            self.save().await?;
        }

        Ok(())
    }

    fn document_count(&self) -> usize {
        self.embeddings.len()
    }

    fn embedding_dimension(&self) -> Option<usize> {
        self.embedding_dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_vector_store() {
        let mut store = InMemoryVectorStore::new();

        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];
        let embedding3 = vec![0.0, 0.0, 1.0];

        store.add_document(1, embedding1.clone()).await.unwrap();
        store.add_document(2, embedding2.clone()).await.unwrap();
        store.add_document(3, embedding3.clone()).await.unwrap();

        assert_eq!(store.document_count(), 3);
        assert_eq!(store.embedding_dimension(), Some(3));

        let query = vec![1.0, 0.0, 0.0];
        let results = store.search(&query, 2).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].document_id, 1); // Should be most similar
        assert!(results[0].score > results[1].score);
    }

    #[tokio::test]
    async fn test_vector_store_dimension_validation() {
        let mut store = InMemoryVectorStore::new();

        let embedding1 = vec![1.0, 0.0, 0.0];
        store.add_document(1, embedding1).await.unwrap();

        let embedding2 = vec![1.0, 0.0]; // Different dimension
        let result = store.add_document(2, embedding2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_vector_store_remove_and_clear() {
        let mut store = InMemoryVectorStore::new();

        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];

        store.add_document(1, embedding1).await.unwrap();
        store.add_document(2, embedding2).await.unwrap();

        assert_eq!(store.document_count(), 2);

        store.remove_document(1).await.unwrap();
        assert_eq!(store.document_count(), 1);

        store.clear().await.unwrap();
        assert_eq!(store.document_count(), 0);
        assert_eq!(store.embedding_dimension(), None);
    }

    #[tokio::test]
    async fn test_search_empty_store() {
        let store = InMemoryVectorStore::new();
        let query = vec![1.0, 0.0, 0.0];
        let results = store.search(&query, 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_result_ordering() {
        let mut store = InMemoryVectorStore::new();

        store.add_document(1, vec![1.0, 0.0, 0.0]).await.unwrap(); // Perfect match
        store.add_document(2, vec![0.5, 0.5, 0.0]).await.unwrap(); // Partial match
        store.add_document(3, vec![0.0, 1.0, 0.0]).await.unwrap(); // No match

        let query = vec![1.0, 0.0, 0.0];
        let results = store.search(&query, 3).await.unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].document_id, 1); // Best match first
        assert!(results[0].score > results[1].score);
        assert!(results[1].score > results[2].score);
    }

    #[tokio::test]
    async fn test_batch_add_documents() {
        let mut store = InMemoryVectorStore::new();

        let documents = vec![
            (1, vec![1.0, 0.0, 0.0]),
            (2, vec![0.0, 1.0, 0.0]),
            (3, vec![0.0, 0.0, 1.0]),
        ];

        store.add_documents(documents).await.unwrap();
        assert_eq!(store.document_count(), 3);
    }
}
