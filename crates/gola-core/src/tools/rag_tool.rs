
//! Retrieval-augmented generation tools for knowledge-grounded responses
//!
//! This module bridges the agent's reasoning capabilities with external knowledge
//! bases through semantic search and document retrieval. The RAG integration enables
//! agents to ground their responses in specific documentation, reducing hallucination
//! and improving factual accuracy. This design is essential for enterprise deployments
//! where agents must reference authoritative sources and maintain audit trails.

use crate::errors::AgentError;
use crate::llm::ToolMetadata;
use crate::rag::{Rag, RagDocument};
use crate::tools::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// RAG Search Tool
pub struct RagSearchTool {
    rag: Box<dyn Rag>,
}

impl RagSearchTool {
    pub fn new(rag: Box<dyn Rag>) -> Self {
        Self { rag }
    }
}

#[async_trait]
impl Tool for RagSearchTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "rag_search".to_string(),
            description: "Search the RAG knowledge base for relevant information".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant documents"
                    },
                    "top_k": {
                        "type": "integer",
                        "description": "Maximum number of documents to retrieve (default: 5)",
                        "minimum": 1,
                        "maximum": 20
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::ToolError {
                tool_name: "rag_search".to_string(),
                message: "Missing or invalid 'query' parameter".to_string(),
            })?;

        let top_k = arguments
            .get("top_k")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        log::info!("RAG search: '{}' (top_k: {:?})", query, top_k);

        let context = self.rag.retrieve(query, top_k).await?;
        Ok(context.format_for_llm())
    }
}

// RAG Add Document Tool
pub struct RagAddDocumentTool {
    rag: Arc<RwLock<Box<dyn Rag>>>,
}

impl RagAddDocumentTool {
    pub fn new(rag: Arc<RwLock<Box<dyn Rag>>>) -> Self {
        Self { rag }
    }
}

#[async_trait]
impl Tool for RagAddDocumentTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "rag_add_document".to_string(),
            description: "Add a document to the RAG knowledge base".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The content of the document to add"
                    },
                    "source": {
                        "type": "string",
                        "description": "The source identifier for the document"
                    },
                    "metadata": {
                        "type": "object",
                        "description": "Optional metadata for the document",
                        "additionalProperties": {
                            "type": "string"
                        }
                    }
                },
                "required": ["content", "source"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        let content = arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::ToolError {
                tool_name: "rag_add_document".to_string(),
                message: "Missing or invalid 'content' parameter".to_string(),
            })?;

        let source = arguments
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::ToolError {
                tool_name: "rag_add_document".to_string(),
                message: "Missing or invalid 'source' parameter".to_string(),
            })?;

        let metadata = if let Some(meta_value) = arguments.get("metadata") {
            if let Some(meta_obj) = meta_value.as_object() {
                let mut metadata = HashMap::new();
                for (key, value) in meta_obj {
                    if let Some(str_value) = value.as_str() {
                        metadata.insert(key.clone(), str_value.to_string());
                    }
                }
                metadata
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        let document = RagDocument::new(content.to_string(), source.to_string())
            .with_metadata(metadata);

        let mut rag = self.rag.write().await;
        rag.add_documents(vec![document]).await?;

        log::info!("Added document to RAG: source={}", source);
        Ok(format!("Successfully added document from source '{}' to the knowledge base", source))
    }
}

// RAG Statistics Tool
pub struct RagStatsTool {
    rag: Arc<RwLock<Box<dyn Rag>>>,
}

impl RagStatsTool {
    pub fn new(rag: Arc<RwLock<Box<dyn Rag>>>) -> Self {
        Self { rag }
    }
}

#[async_trait]
impl Tool for RagStatsTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "rag_stats".to_string(),
            description: "Get statistics about the RAG knowledge base".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, _arguments: Value) -> Result<String, AgentError> {
        let rag = self.rag.read().await;
        let document_count = rag.document_count();

        log::info!("RAG stats requested: {} documents", document_count);

        Ok(format!(
            "RAG Knowledge Base Statistics:\n\nTotal documents: {}\nStatus: Active",
            document_count
        ))
    }
}

// RAG Clear Tool
pub struct RagClearTool {
    rag: Arc<RwLock<Box<dyn Rag>>>,
}

impl RagClearTool {
    pub fn new(rag: Arc<RwLock<Box<dyn Rag>>>) -> Self {
        Self { rag }
    }
}

#[async_trait]
impl Tool for RagClearTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "rag_clear".to_string(),
            description: "Clear all documents from the RAG knowledge base".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "confirm": {
                        "type": "boolean",
                        "description": "Confirmation flag to proceed with clearing (must be true)"
                    }
                },
                "required": ["confirm"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        let confirm = arguments
            .get("confirm")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !confirm {
            return Ok("Clear operation cancelled. Set 'confirm' to true to proceed.".to_string());
        }

        let mut rag = self.rag.write().await;
        let document_count = rag.document_count();
        rag.clear().await?;

        log::info!("RAG knowledge base cleared: {} documents removed", document_count);

        Ok(format!(
            "Successfully cleared the RAG knowledge base. Removed {} documents.",
            document_count
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::DummyRag;
    use serde_json::json;

    #[tokio::test]
    async fn test_rag_search_tool() {
        let rag = Box::new(DummyRag::new());
        let tool = RagSearchTool::new(rag);

        let args = json!({
            "query": "test query"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("Retrieved Context:"));
    }

    #[tokio::test]
    async fn test_rag_search_tool_missing_query() {
        let rag = Box::new(DummyRag::new());
        let tool = RagSearchTool::new(rag);

        let args = json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_rag_add_document_tool() {
        let rag = Arc::new(RwLock::new(Box::new(DummyRag::new()) as Box<dyn Rag>));
        let tool = RagAddDocumentTool::new(rag.clone());

        let args = json!({
            "content": "Test document content",
            "source": "test.txt"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("Successfully added document"));

        let rag_read = rag.read().await;
        assert_eq!(rag_read.document_count(), 1);
    }

    #[tokio::test]
    async fn test_rag_stats_tool() {
        let rag = Arc::new(RwLock::new(Box::new(DummyRag::new()) as Box<dyn Rag>));
        let tool = RagStatsTool::new(rag.clone());

        let args = json!({});

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("RAG Knowledge Base Statistics"));
        assert!(result.contains("Total documents: 0"));
    }

    #[tokio::test]
    async fn test_rag_clear_tool() {
        let rag = Arc::new(RwLock::new(Box::new(DummyRag::new()) as Box<dyn Rag>));
        let tool = RagClearTool::new(rag.clone());

        // Test without confirmation
        let args = json!({
            "confirm": false
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("Clear operation cancelled"));

        // Test with confirmation
        let args = json!({
            "confirm": true
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("Successfully cleared"));
    }
}
