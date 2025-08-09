use std::collections::HashMap;
use std::sync::Arc;
use crate::tools::{Tool, ToolRegistry};
use crate::errors::AgentError;
use super::ControlPlaneFactory;

/// Internal MCP server for control plane tools
/// This server runs within the Gola process and provides system control tools
pub struct ControlPlaneServer {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ControlPlaneServer {
    /// Create a new control plane server with all control tools
    pub fn new() -> Self {
        let mut tools = HashMap::new();
        
        // Register all control plane tools
        let assistant_done = ControlPlaneFactory::create_assistant_done();
        tools.insert(assistant_done.metadata().name.clone(), assistant_done);
        
        let report_progress = ControlPlaneFactory::create_report_progress();
        tools.insert(report_progress.metadata().name.clone(), report_progress);
        
        Self { tools }
    }
    
    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }
    
    /// List all available control plane tools
    pub fn list_tools(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
    
    /// Check if a tool name is a control plane tool
    pub fn is_control_tool(tool_name: &str) -> bool {
        matches!(tool_name, "assistant_done" | "report_progress")
    }
    
    /// Execute a control plane tool
    pub async fn execute_tool(&self, tool_name: &str, arguments: serde_json::Value) -> Result<String, AgentError> {
        match self.tools.get(tool_name) {
            Some(tool) => tool.execute(arguments).await,
            None => Err(AgentError::ToolError {
                tool_name: tool_name.to_string(),
                message: format!("Control plane tool '{}' not found", tool_name),
            }),
        }
    }
    
    /// Add control plane tools to an existing tool registry
    pub fn register_with_registry(&self, registry: &mut ToolRegistry) {
        for tool in self.tools.values() {
            registry.register_tool(tool.clone());
        }
    }
    
    /// Create a tool registry with control plane tools included
    pub fn create_enhanced_registry(&self, base_registry: ToolRegistry) -> ToolRegistry {
        let mut enhanced = base_registry;
        self.register_with_registry(&mut enhanced);
        enhanced
    }
    
    /// Get count of available control tools
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
}

impl Default for ControlPlaneServer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_control_plane_server_creation() {
        let server = ControlPlaneServer::new();
        assert!(server.tool_count() > 0);
        assert!(server.get_tool("assistant_done").is_some());
    }

    #[test]
    fn test_is_control_tool() {
        assert!(ControlPlaneServer::is_control_tool("assistant_done"));
        assert!(ControlPlaneServer::is_control_tool("report_progress"));
        assert!(!ControlPlaneServer::is_control_tool("regular_tool"));
        assert!(!ControlPlaneServer::is_control_tool("get_cheapest_flights"));
    }

    #[test]
    fn test_list_tools() {
        let server = ControlPlaneServer::new();
        let tools = server.list_tools();
        assert!(tools.contains(&"assistant_done".to_string()));
    }

    #[tokio::test]
    async fn test_execute_assistant_done() {
        let server = ControlPlaneServer::new();
        let args = json!({
            "summary": "Test completion message",
            "status": "success"
        });
        
        let result = server.execute_tool("assistant_done", args).await.unwrap();
        assert!(result.contains("Test completion message"));
        assert!(result.contains("is_completion"));
    }

    #[tokio::test]
    async fn test_execute_nonexistent_tool() {
        let server = ControlPlaneServer::new();
        let args = json!({});
        
        let result = server.execute_tool("nonexistent_tool", args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_register_with_registry() {
        let server = ControlPlaneServer::new();
        let mut registry = ToolRegistry::new();
        
        assert_eq!(registry.tool_count(), 0);
        server.register_with_registry(&mut registry);
        assert!(registry.tool_count() > 0);
        assert!(registry.get_tool("assistant_done").is_some());
    }

    #[test]
    fn test_create_enhanced_registry() {
        let server = ControlPlaneServer::new();
        let base_registry = ToolRegistry::new();
        
        let enhanced = server.create_enhanced_registry(base_registry);
        assert!(enhanced.tool_count() > 0);
        assert!(enhanced.get_tool("assistant_done").is_some());
    }
}