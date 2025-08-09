//! Model Context Protocol (MCP) client for external tool integration
//!
//! This module implements the MCP standard for tool interoperability, enabling
//! agents to discover and invoke tools from external MCP servers. The protocol
//! abstraction ensures compatibility with a growing ecosystem of tools while
//! maintaining type safety and error handling. This design choice future-proofs
//! the agent framework against tool evolution and enables community contributions.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::errors::AgentError;

#[derive(Debug, Clone)]
pub struct MCPToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[async_trait]
pub trait MCPClientTrait: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<MCPToolInfo>, AgentError>;
    async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<String, AgentError>;
    async fn is_connected(&self) -> bool;
}

// Mock implementation for testing
pub struct MockMCPClient {
    connected: bool,
}

impl MockMCPClient {
    pub fn new() -> Self {
        Self { connected: true }
    }

    pub fn with_connection_status(connected: bool) -> Self {
        Self { connected }
    }
}

impl Default for MockMCPClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MCPClientTrait for MockMCPClient {
    async fn list_tools(&self) -> Result<Vec<MCPToolInfo>, AgentError> {
        if !self.connected {
            return Err(AgentError::MCPError("Not connected".to_string()));
        }

        Ok(vec![
            MCPToolInfo {
                name: "mock_tool_1".to_string(),
                description: "First mock tool".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "input": {
                            "type": "string",
                            "description": "Input parameter"
                        }
                    },
                    "required": ["input"]
                }),
            },
            MCPToolInfo {
                name: "mock_tool_2".to_string(),
                description: "Second mock tool".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "value": {
                            "type": "number",
                            "description": "Numeric value"
                        }
                    },
                    "required": ["value"]
                }),
            },
        ])
    }

    async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<String, AgentError> {
        if !self.connected {
            return Err(AgentError::MCPError("Not connected".to_string()));
        }

        Ok(format!(
            "Mock result from {} with arguments: {}",
            tool_name,
            arguments
        ))
    }

    async fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_mock_mcp_client_list_tools() {
        let client = MockMCPClient::new();
        let tools = client.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "mock_tool_1");
        assert_eq!(tools[1].name, "mock_tool_2");
    }

    #[tokio::test]
    async fn test_mock_mcp_client_call_tool() {
        let client = MockMCPClient::new();
        let result = client
            .call_tool("test_tool", json!({"input": "test"}))
            .await
            .unwrap();
        assert!(result.contains("Mock result"));
        assert!(result.contains("test_tool"));
    }

    #[tokio::test]
    async fn test_mock_mcp_client_disconnected() {
        let client = MockMCPClient::with_connection_status(false);
        assert!(!client.is_connected().await);
        
        let tools_result = client.list_tools().await;
        assert!(tools_result.is_err());
        
        let call_result = client.call_tool("test", json!({})).await;
        assert!(call_result.is_err());
    }

    #[tokio::test]
    async fn test_mock_mcp_client_is_connected() {
        let client = MockMCPClient::new();
        assert!(client.is_connected().await);
        
        let disconnected_client = MockMCPClient::with_connection_status(false);
        assert!(!disconnected_client.is_connected().await);
    }
}
