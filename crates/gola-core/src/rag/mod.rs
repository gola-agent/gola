//! Retrieval-Augmented Generation (RAG) system for knowledge-grounded AI responses
//!
//! This module implements a complete RAG pipeline that enables agents to access and
//! reason over large document collections. The design addresses the fundamental
//! limitation of static LLM knowledge by providing dynamic access to external
//! information sources. Through semantic search and intelligent chunking, agents
//! can ground their responses in authoritative documentation, reducing hallucination
//! and improving factual accuracy in specialized domains.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::errors::AgentError;
use crate::llm::LLM;

pub mod cache;
pub mod embeddings;
pub mod splitter;
pub mod vector_store;

pub use cache::*;
pub use embeddings::*;
pub use splitter::*;
pub use vector_store::*;

/// Configuration for RAG system
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    pub chunk_size: usize,
    pub chunk_overlap: usize,
    pub top_k: usize,
    pub similarity_threshold: f32,
    pub embedding_model_name: String,
    pub reranker_model_name: Option<String>,
    pub persistent_vector_store_path: Option<String>,
    pub embedding_cache: EmbeddingCacheConfig,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            chunk_size: 1000,
            chunk_overlap: 200,
            top_k: 5,
            similarity_threshold: 0.7,
            embedding_model_name: "text-embedding-ada-002".to_string(),
            reranker_model_name: None,
            persistent_vector_store_path: None,
            embedding_cache: EmbeddingCacheConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedContext {
    pub documents: Vec<RagDocument>,
    pub sources: Vec<String>,
    pub scores: Vec<f32>,
}

impl RetrievedContext {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            sources: Vec::new(),
            scores: Vec::new(),
        }
    }

    pub fn add_document(&mut self, document: RagDocument, source: String, score: f32) {
        self.documents.push(document);
        self.sources.push(source);
        self.scores.push(score);
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn format_for_llm(&self) -> String {
        if self.is_empty() {
            return "No relevant context found.".to_string();
        }

        let mut formatted = String::new();
        formatted.push_str("Retrieved Context:\n\n");

        for (i, (doc, source)) in self.documents.iter().zip(self.sources.iter()).enumerate() {
            formatted.push_str(&format!("Source {}: {}\n", i + 1, source));
            formatted.push_str(&format!("Content: {}\n\n", doc.content));
        }

        formatted
    }
}

#[async_trait]
pub trait Rag: Send + Sync {
    async fn retrieve(
        &self,
        query: &str,
        top_k: Option<usize>,
    ) -> Result<RetrievedContext, AgentError>;

    async fn add_documents(&mut self, documents: Vec<RagDocument>) -> Result<(), AgentError>;

    async fn add_documents_from_paths(&mut self, paths: &[String]) -> Result<(), AgentError>;

    async fn clear(&mut self) -> Result<(), AgentError>;

    fn document_count(&self) -> usize;

    async fn save(&self, path: &Path) -> Result<(), AgentError>;

    async fn load(path: &Path, embedding_llm: Box<dyn LLM>) -> Result<Box<dyn Rag>, AgentError>
    where
        Self: Sized;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagDocument {
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub source: String,
    pub chunk_index: Option<usize>,
}

impl RagDocument {
    pub fn new(content: String, source: String) -> Self {
        Self {
            content,
            metadata: HashMap::new(),
            source,
            chunk_index: None,
        }
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_chunk_index(mut self, chunk_index: usize) -> Self {
        self.chunk_index = Some(chunk_index);
        self
    }
}

pub struct DummyRag {
    documents: Vec<RagDocument>,
}

impl DummyRag {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
        }
    }
}

#[async_trait]
impl Rag for DummyRag {
    async fn retrieve(
        &self,
        query: &str,
        top_k: Option<usize>,
    ) -> Result<RetrievedContext, AgentError> {
        log::info!("[DummyRag] Retrieving context for query: {}", query);

        let k = top_k.unwrap_or(3);
        let mut context = RetrievedContext::new();

        // Return dummy context
        for i in 0..k.min(2) {
            let dummy_doc = RagDocument::new(
                format!("Dummy context {} related to: {}", i + 1, query),
                "dummy_source".to_string(),
            );
            context.add_document(dummy_doc, "dummy_source".to_string(), 0.9);
        }

        Ok(context)
    }

    async fn add_documents(&mut self, documents: Vec<RagDocument>) -> Result<(), AgentError> {
        self.documents.extend(documents);
        Ok(())
    }

    async fn add_documents_from_paths(&mut self, _paths: &[String]) -> Result<(), AgentError> {
        // Dummy implementation - just add some fake documents
        let dummy_doc = RagDocument::new(
            "This is dummy content from a file".to_string(),
            "dummy_file.txt".to_string(),
        );
        self.documents.push(dummy_doc);
        Ok(())
    }

    async fn clear(&mut self) -> Result<(), AgentError> {
        self.documents.clear();
        Ok(())
    }

    fn document_count(&self) -> usize {
        self.documents.len()
    }

    async fn save(&self, _path: &Path) -> Result<(), AgentError> {
        // Dummy implementation - do nothing
        Ok(())
    }

    async fn load(_path: &Path, _embedding_llm: Box<dyn LLM>) -> Result<Box<dyn Rag>, AgentError> {
        Ok(Box::new(DummyRag::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dummy_rag_retrieve() {
        let rag = DummyRag::new();
        let query = "test query";
        let result = rag.retrieve(query, Some(3)).await;
        assert!(result.is_ok());
        let context = result.unwrap();
        assert_eq!(context.documents.len(), 2);
        assert!(context.documents[0].content.contains(query));
    }

    #[tokio::test]
    async fn test_retrieved_context_format() {
        let mut context = RetrievedContext::new();

        let doc1 = RagDocument::new("First document content".to_string(), "doc1.txt".to_string());
        let doc2 = RagDocument::new(
            "Second document content".to_string(),
            "doc2.txt".to_string(),
        );

        context.add_document(doc1, "doc1.txt".to_string(), 0.9);
        context.add_document(doc2, "doc2.txt".to_string(), 0.8);

        let formatted = context.format_for_llm();
        assert!(formatted.contains("Retrieved Context:"));
        assert!(formatted.contains("Source 1: doc1.txt"));
        assert!(formatted.contains("Source 2: doc2.txt"));
        assert!(formatted.contains("First document content"));
        assert!(formatted.contains("Second document content"));
    }

    #[tokio::test]
    async fn test_rag_document_creation() {
        let doc = RagDocument::new("Test content".to_string(), "test.txt".to_string())
            .with_chunk_index(5);

        assert_eq!(doc.content, "Test content");
        assert_eq!(doc.source, "test.txt");
        assert_eq!(doc.chunk_index, Some(5));
    }
}

pub mod rag_system;
pub use rag_system::*;
