//! Retrieval-augmented generation search integration for semantic document retrieval.
//!
//! This module provides agents with the ability to perform semantic similarity searches
//! across indexed knowledge bases, enabling context-aware information retrieval and
//! integration with large language model workflows.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::errors::AgentError;
use crate::llm::ToolMetadata;
use crate::rag::Rag;
use crate::tools::Tool;
pub struct RagSearchTool {
    rag_system: Arc<dyn Rag>,
    metadata: ToolMetadata,
}

impl RagSearchTool {
    pub fn new(rag_system: Arc<dyn Rag>) -> Self {
        let metadata = ToolMetadata {
            name: "rag_search".to_string(),
            description: "Search the knowledge base for relevant information using semantic similarity. Use this when you need to find specific information from the available documents.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant information"
                    },
                    "top_k": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "minimum": 1,
                        "maximum": 20
                    }
                },
                "required": ["query"]
            }),
        };

        Self {
            rag_system,
            metadata,
        }
    }

    pub fn with_metadata(rag_system: Arc<dyn Rag>, metadata: ToolMetadata) -> Self {
        Self {
            rag_system,
            metadata,
        }
    }
}

#[async_trait]
impl Tool for RagSearchTool {
    fn metadata(&self) -> ToolMetadata {
        self.metadata.clone()
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::ToolError {
                tool_name: "rag_search".to_string(),
                message: "Missing required parameter: query".to_string(),
            })?;

        if query.trim().is_empty() {
            return Err(AgentError::ToolError {
                tool_name: "rag_search".to_string(),
                message: "Query cannot be empty".to_string(),
            });
        }

        let top_k = arguments
            .get("top_k")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        log::info!("RAG search query: '{}', top_k: {:?}", query, top_k);

        let results = self.rag_system.retrieve(query, top_k).await
            .map_err(|e| AgentError::ToolError {
                tool_name: "rag_search".to_string(),
                message: format!("RAG search failed: {}", e),
            })?;

        if results.is_empty() {
            Ok("No relevant documents found for the given query.".to_string())
        } else {
            let mut response = format!("Found {} relevant documents:\n\n", results.len());
            
            for (i, ((doc, source), score)) in results.documents.iter()
                .zip(results.sources.iter())
                .zip(results.scores.iter())
                .enumerate() {
                
                response.push_str(&format!(
                    "Document {} (Score: {:.3}):\n",
                    i + 1,
                    score
                ));
                response.push_str(&format!("Source: {}\n", source));
                response.push_str(&format!("Content: {}\n\n", doc.content));
            }

            Ok(response)
        }
    }
}

pub struct RagSearchToolBuilder {
    rag_system: Option<Arc<dyn Rag>>,
    name: String,
    description: String,
    max_results: usize,
}

impl RagSearchToolBuilder {
    pub fn new() -> Self {
        Self {
            rag_system: None,
            name: "rag_search".to_string(),
            description: "Search the knowledge base for relevant information using semantic similarity.".to_string(),
            max_results: 20,
        }
    }

    pub fn with_rag_system(mut self, rag_system: Arc<dyn Rag>) -> Self {
        self.rag_system = Some(rag_system);
        self
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    pub fn with_max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results;
        self
    }

    pub fn build(self) -> Result<RagSearchTool, AgentError> {
        let rag_system = self.rag_system.ok_or_else(|| AgentError::ToolError {
            tool_name: self.name.clone(),
            message: "RAG system is required".to_string(),
        })?;

        let metadata = ToolMetadata {
            name: self.name,
            description: self.description,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant information"
                    },
                    "top_k": {
                        "type": "integer",
                        "description": format!("Maximum number of results to return (default: 5, max: {})", self.max_results),
                        "minimum": 1,
                        "maximum": self.max_results
                    }
                },
                "required": ["query"]
            }),
        };

        Ok(RagSearchTool {
            rag_system,
            metadata,
        })
    }
}

impl Default for RagSearchToolBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::{DummyRag, RagDocument};
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_rag_search_tool_creation() {
        let rag_system = Arc::new(DummyRag::new());
        let tool = RagSearchTool::new(rag_system);
        
        let metadata = tool.metadata();
        assert_eq!(metadata.name, "rag_search");
        assert!(metadata.description.contains("Search"));
    }

    #[tokio::test]
    async fn test_rag_search_tool_execute() {
        let mut rag_system = DummyRag::new();
        let documents = vec![
            RagDocument::new("Machine learning is powerful".to_string(), "ml.txt".to_string()),
        ];
        rag_system.add_documents(documents).await.unwrap();

        let tool = RagSearchTool::new(Arc::new(rag_system));
        
        let args = json!({
            "query": "machine learning",
            "top_k": 3
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("relevant documents") || result.contains("Dummy context"));
    }

    #[tokio::test]
    async fn test_rag_search_tool_missing_query() {
        let rag_system = Arc::new(DummyRag::new());
        let tool = RagSearchTool::new(rag_system);
        
        let args = json!({
            "top_k": 3
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        
        if let Err(AgentError::ToolError { message, .. }) = result {
            assert!(message.contains("Missing required parameter: query"));
        }
    }

    #[tokio::test]
    async fn test_rag_search_tool_empty_query() {
        let rag_system = Arc::new(DummyRag::new());
        let tool = RagSearchTool::new(rag_system);
        
        let args = json!({
            "query": "",
            "top_k": 3
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        
        if let Err(AgentError::ToolError { message, .. }) = result {
            assert!(message.contains("Query cannot be empty"));
        }
    }

    #[tokio::test]
    async fn test_rag_search_tool_builder() {
        let rag_system = Arc::new(DummyRag::new());
        
        let tool = RagSearchToolBuilder::new()
            .with_rag_system(rag_system)
            .with_name("custom_search".to_string())
            .with_description("Custom search tool".to_string())
            .with_max_results(10)
            .build()
            .unwrap();

        let metadata = tool.metadata();
        assert_eq!(metadata.name, "custom_search");
        assert_eq!(metadata.description, "Custom search tool");
    }

    #[tokio::test]
    async fn test_rag_search_tool_builder_missing_rag() {
        let result = RagSearchToolBuilder::new()
            .with_name("test".to_string())
            .build();

        assert!(result.is_err());
        if let Err(AgentError::ToolError { message, .. }) = result {
            assert!(message.contains("RAG system is required"));
        }
    }

    #[tokio::test]
    async fn test_rag_search_tool_no_results() {
        let rag_system = Arc::new(DummyRag::new());
        let tool = RagSearchTool::new(rag_system);
        
        let args = json!({
            "query": "nonexistent topic"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.contains("No relevant documents found") || result.contains("Dummy context"));
    }
}
