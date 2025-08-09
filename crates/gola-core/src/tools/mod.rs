//! Tool system for extending agent capabilities with external functionality
//!
//! This module provides the foundation for tool integration, enabling agents to
//! interact with external systems, APIs, and computational resources. The design
//! prioritizes extensibility through a plugin-like architecture where tools are
//! registered dynamically and invoked based on agent decisions. This approach
//! decouples core agent logic from specific tool implementations, allowing for
//! runtime tool discovery and hot-swapping of capabilities.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::errors::AgentError;
use crate::llm::ToolMetadata;
use crate::rag::Rag;
use tiktoken_rs::p50k_base;

// Core Tool trait that all tools must implement
#[async_trait]
pub trait Tool: Send + Sync {
    fn metadata(&self) -> ToolMetadata;
    async fn execute(&self, arguments: Value) -> Result<String, AgentError>;
}

// Tool registry for managing multiple tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register_tool(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.metadata().name.clone();
        self.tools.insert(name, tool);
    }

    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list_tools(&self) -> Vec<ToolMetadata> {
        self.tools.values().map(|tool| tool.metadata()).collect()
    }

    pub fn get_all_tools(&self) -> HashMap<String, Arc<dyn Tool>> {
        self.tools.clone()
    }

    pub fn remove_tool(&mut self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.remove(name)
    }

    pub fn clear(&mut self) {
        self.tools.clear();
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// Tool factory for creating common tools
pub struct ToolFactory;

impl ToolFactory {
    pub fn create_calculator() -> Arc<dyn Tool> {
        Arc::new(calculator::CalculatorTool::new())
    }

    pub fn create_web_search() -> Arc<dyn Tool> {
        Arc::new(web_search::WebSearchTool::new())
    }

    pub fn create_web_search_with_tavily(api_key: String) -> Arc<dyn Tool> {
        Arc::new(web_search::WebSearchTool::with_tavily_api_key(api_key))
    }

    pub fn create_web_search_with_serper(api_key: String) -> Arc<dyn Tool> {
        Arc::new(web_search::WebSearchTool::with_serper_api_key(api_key))
    }

    pub fn create_rag_search(rag: Box<dyn Rag>) -> Arc<dyn Tool> {
        Arc::new(rag_tool::RagSearchTool::new(rag))
    }

    pub fn create_rag_add_document(rag: Arc<RwLock<Box<dyn Rag>>>) -> Arc<dyn Tool> {
        Arc::new(rag_tool::RagAddDocumentTool::new(rag))
    }

    pub fn create_rag_stats(rag: Arc<RwLock<Box<dyn Rag>>>) -> Arc<dyn Tool> {
        Arc::new(rag_tool::RagStatsTool::new(rag))
    }

    pub fn create_rag_clear(rag: Arc<RwLock<Box<dyn Rag>>>) -> Arc<dyn Tool> {
        Arc::new(rag_tool::RagClearTool::new(rag))
    }

    pub fn create_default_registry() -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Self::create_calculator());
        registry.register_tool(Self::create_web_search());
        registry
    }

    pub fn create_registry_with_rag(rag: Arc<RwLock<Box<dyn Rag>>>) -> ToolRegistry {
        let mut registry = Self::create_default_registry();
        registry.register_tool(Self::create_rag_stats(rag.clone()));
        registry.register_tool(Self::create_rag_add_document(rag.clone()));
        registry.register_tool(Self::create_rag_clear(rag));
        registry
    }
}

// MCP (Model Context Protocol) client trait and implementations
pub mod mcp_client;
pub mod rmcp_client;

// Individual tool implementations
pub mod calculator;
pub mod control_plane;
pub mod rag_tool;
pub mod web_search;

// Re-export commonly used items
pub use calculator::CalculatorTool;
pub use control_plane::{AssistantDoneTool, ControlPlaneFactory, ControlPlaneServer};
pub use mcp_client::{MCPClientTrait, MCPToolInfo, MockMCPClient};
pub use rmcp_client::{RMCPClient, RMCPClientFactory};
pub use rag_tool::{RagAddDocumentTool, RagClearTool, RagSearchTool, RagStatsTool};
pub use web_search::WebSearchTool;


pub struct MCPTool<C: MCPClientTrait> {
    client: Arc<C>,
    tool_info: mcp_client::MCPToolInfo,
    description_token_limit: u32,
}

impl<C: MCPClientTrait> MCPTool<C> {
    pub fn new(client: Arc<C>, tool_info: mcp_client::MCPToolInfo, description_token_limit: u32) -> Self {
        Self { client, tool_info, description_token_limit }
    }
}

#[async_trait]
impl<C: MCPClientTrait> Tool for MCPTool<C> {
    fn metadata(&self) -> ToolMetadata {
        let bpe = p50k_base().unwrap();
        let mut description = self.tool_info.description.clone();
        let mut tokens = bpe.encode_with_special_tokens(&description);

        if tokens.len() > self.description_token_limit as usize {
            tokens.truncate(self.description_token_limit as usize);
            description = bpe.decode(tokens).unwrap();
        }

        ToolMetadata {
            name: self.tool_info.name.clone(),
            description,
            input_schema: self.tool_info.input_schema.clone(),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        self.client
            .call_tool(&self.tool_info.name, arguments)
            .await
            .map_err(|e| AgentError::ToolError {
                tool_name: self.tool_info.name.clone(),
                message: format!("MCP tool execution failed: {}", e),
            })
    }
}

pub struct MCPToolFactory<C: MCPClientTrait> {
    client: Arc<C>,
    description_token_limit: u32,
}

impl<C: MCPClientTrait + 'static> MCPToolFactory<C> {
    pub fn new(client: Arc<C>, description_token_limit: u32) -> Self {
        Self { client, description_token_limit }
    }

    pub async fn list_tools(&self) -> Result<Vec<mcp_client::MCPToolInfo>, AgentError> {
        self.client.list_tools().await.map_err(|e| {
            AgentError::ToolError {
                tool_name: "mcp_discovery".to_string(),
                message: format!("Failed to list MCP tools: {}", e),
            }
        })
    }

    pub async fn create_tool(&self, tool_name: &str) -> Result<Option<Arc<dyn Tool>>, AgentError> {
        let tool_infos = self.client.list_tools().await.map_err(|e| {
            AgentError::ToolError {
                tool_name: "mcp_discovery".to_string(),
                message: format!("Failed to discover MCP tools: {}", e),
            }
        })?;

        for tool_info in tool_infos {
            if tool_info.name == tool_name {
                let tool = Arc::new(MCPTool::new(self.client.clone(), tool_info, self.description_token_limit));
                return Ok(Some(tool));
            }
        }

        Ok(None)
    }

    pub async fn create_all_tools(&self) -> Result<Vec<Arc<dyn Tool>>, AgentError> {
        let tool_infos = self.client.list_tools().await.map_err(|e| {
            AgentError::ToolError {
                tool_name: "mcp_discovery".to_string(),
                message: format!("Failed to discover MCP tools: {}", e),
            }
        })?;

        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
        for tool_info in tool_infos {
            let tool = Arc::new(MCPTool::new(self.client.clone(), tool_info, self.description_token_limit));
            tools.push(tool);
        }

        Ok(tools)
    }

    pub async fn discover_tools(&self) -> Result<Vec<Arc<dyn Tool>>, AgentError> {
        let tool_infos = self.client.list_tools().await.map_err(|e| {
            AgentError::ToolError {
                tool_name: "mcp_discovery".to_string(),
                message: format!("Failed to discover MCP tools: {}", e),
            }
        })?;

        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();
        for tool_info in tool_infos {
            let tool = Arc::new(MCPTool::new(self.client.clone(), tool_info, self.description_token_limit));
            tools.push(tool);
        }

        Ok(tools)
    }

    pub async fn create_registry(&self) -> Result<ToolRegistry, AgentError> {
        let mut registry = ToolRegistry::new();
        let tools = self.discover_tools().await?;

        for tool in tools {
            registry.register_tool(tool);
        }

        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::DummyRag;
    use serde_json::json;

    #[test]
    fn test_tool_registry_creation() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    fn test_tool_registry_register_and_get() {
        let mut registry = ToolRegistry::new();
        let calculator = ToolFactory::create_calculator();
        
        registry.register_tool(calculator.clone());
        assert_eq!(registry.tool_count(), 1);
        
        let retrieved = registry.get_tool("calculator");
        assert!(retrieved.is_some());
        
        let nonexistent = registry.get_tool("nonexistent");
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_tool_registry_list_tools() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(ToolFactory::create_calculator());
        registry.register_tool(ToolFactory::create_web_search());
        
        let tools = registry.list_tools();
        assert_eq!(tools.len(), 2);
        
        let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
        assert!(tool_names.contains(&"calculator".to_string()));
        assert!(tool_names.contains(&"web_search".to_string()));
    }

    #[test]
    fn test_tool_registry_remove_and_clear() {
        let mut registry = ToolRegistry::new();
        registry.register_tool(ToolFactory::create_calculator());
        registry.register_tool(ToolFactory::create_web_search());
        
        assert_eq!(registry.tool_count(), 2);
        
        let removed = registry.remove_tool("calculator");
        assert!(removed.is_some());
        assert_eq!(registry.tool_count(), 1);
        
        registry.clear();
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    fn test_tool_factory_default_registry() {
        let registry = ToolFactory::create_default_registry();
        assert!(registry.tool_count() >= 2); // At least calculator and web_search
        
        assert!(registry.get_tool("calculator").is_some());
        assert!(registry.get_tool("web_search").is_some());
    }

    #[tokio::test]
    async fn test_tool_factory_rag_registry() {
        let rag = Arc::new(RwLock::new(Box::new(DummyRag::new()) as Box<dyn Rag>));
        let registry = ToolFactory::create_registry_with_rag(rag);
        
        assert!(registry.tool_count() >= 5); // Default tools + RAG tools
        assert!(registry.get_tool("rag_stats").is_some());
        assert!(registry.get_tool("rag_add_document").is_some());
        assert!(registry.get_tool("rag_clear").is_some());
    }

    #[tokio::test]
    async fn test_mcp_tool_factory_with_mock() {
        let mock_client = Arc::new(MockMCPClient::new());
        let factory = MCPToolFactory::new(mock_client, 10);
        
        let tools = factory.discover_tools().await.unwrap();
        assert_eq!(tools.len(), 2); // MockMCPClient returns 2 tools
        
        let registry = factory.create_registry().await.unwrap();
        assert_eq!(registry.tool_count(), 2);

        let tool1 = registry.get_tool("mock_tool_1").unwrap();
        let metadata1 = tool1.metadata();
        assert!(metadata1.description.len() < 50);
    }

    #[tokio::test]
    async fn test_mcp_tool_execution() {
        let mock_client = Arc::new(MockMCPClient::new());
        let tool_info = mcp_client::MCPToolInfo {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: json!({"type": "object"}),
        };
        
        let mcp_tool = MCPTool::new(mock_client, tool_info, 10);
        let metadata = mcp_tool.metadata();
        assert_eq!(metadata.name, "test_tool");
        
        let result = mcp_tool.execute(json!({"test": "value"})).await.unwrap();
        assert!(result.contains("Mock result"));
    }
}
pub mod rag_search_tool;
