//! Rust-native MCP client implementation for high-performance tool execution
//!
//! This module provides a Rust-native implementation of the MCP protocol, offering
//! better performance and type safety compared to generic protocol implementations.
//! The design choice to have a specialized Rust client enables zero-copy operations,
//! efficient process management, and tight integration with the Tokio runtime. This
//! approach is critical for latency-sensitive tool invocations and resource management.

use async_trait::async_trait;
use rmcp::{
    model::{CallToolRequestParam, RawContent, ResourceContents, Tool},
    service::{DynService, RunningService, ServiceExt},
    transport::TokioChildProcess,
    RoleClient,
};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tiktoken_rs::p50k_base;
use tokio::process::Command;
use tokio::sync::RwLock;

use super::mcp_client::{MCPClientTrait, MCPToolInfo};
use crate::config::McpCommand;
use crate::errors::AgentError;

pub struct RMCPClient {
    service: Option<RunningService<RoleClient, Box<dyn DynService<RoleClient>>>>,
    connected: Arc<RwLock<bool>>,
    server_info: Arc<RwLock<Option<String>>>,
    token_limit: u32,
}

impl RMCPClient {
    pub async fn new(command: &str, args: &[&str]) -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }

    pub async fn new_with_mcp_command(mcp_command: &McpCommand) -> Result<Self, AgentError> {
        log::info!(
            "🚀 Starting MCP server with command: {} {:?}",
            mcp_command.run,
            mcp_command.args
        );

        let mut cmd = Command::new(&mcp_command.run);
        cmd.args(&mcp_command.args);

        if let Some(working_dir) = &mcp_command.working_dir {
            log::info!("📁 Setting working directory: {}", working_dir.display());
            cmd.current_dir(working_dir);
        }

        // Start with a clean environment and inherit from parent
        cmd.env_clear();
        cmd.envs(std::env::vars());

        // Add/override with configured environment variables
        if !mcp_command.env.is_empty() {
            log::info!("🔧 Setting {} environment variables:", mcp_command.env.len());
            for (key, value) in &mcp_command.env {
                log::info!("   {}={}", key, value);
                cmd.env(key, value);
            }
        } else {
            log::info!("📝 No custom environment variables configured, using inherited environment");
        }

        // Log current environment for debugging
        log::debug!("🌍 Current working directory: {:?}", std::env::current_dir());
        log::debug!("🔍 Environment variables being passed to MCP server:");
        for (key, value) in std::env::vars() {
            if key.contains("API") || key.contains("KEY") || key.contains("TOKEN") || key.contains("SECRET") {
                log::debug!("   {}=***REDACTED***", key);
            } else {
                log::debug!("   {}={}", key, value);
            }
        }

        // In embedded mode, redirect MCP server stdout/stderr to log file
        if std::env::var("GOLA_EMBEDDED_MODE").is_ok() {
            use std::fs::OpenOptions;
            use std::process::Stdio;
            
            // Create or append to gola-mcp.log with timestamp
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
            let separator = format!("\n=== MCP Server '{}' started at {} ===\n", 
                mcp_command.run, timestamp);
                
            let mut mcp_log_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("gola-mcp.log")
                .map_err(|e| AgentError::MCPError(format!("Failed to create MCP log file: {}", e)))?;
                
            // Write separator to help distinguish different server sessions
            use std::io::Write;
            mcp_log_file.write_all(separator.as_bytes())
                .map_err(|e| AgentError::MCPError(format!("Failed to write to MCP log file: {}", e)))?;
            
            // Convert to Stdio::from
            let mcp_log_stdio = Stdio::from(mcp_log_file.try_clone()
                .map_err(|e| AgentError::MCPError(format!("Failed to clone MCP log file handle: {}", e)))?);
            
            cmd.stdout(mcp_log_stdio);
            cmd.stderr(Stdio::from(mcp_log_file));
            
            log::info!("📝 MCP server '{}' logs will be written to gola-mcp.log", mcp_command.run);
        }

        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| AgentError::MCPError(format!("Failed to create transport: {}", e)))?;

        let service_handler: Box<dyn DynService<RoleClient>> = Box::new(());
        let service = service_handler.serve(transport).await.map_err(|e| {
            log::error!(
                "💥 Service initialization failed (this means initialize request failed): {}",
                e
            );
            AgentError::MCPError(format!("Failed to create service: {}", e))
        })?;

        let server_info_str = Some(format!("{:?}", service.peer_info()));
        log::info!("✅ Connected to MCP server: {:?}", server_info_str);

        Ok(Self {
            service: Some(service),
            connected: Arc::new(RwLock::new(true)),
            server_info: Arc::new(RwLock::new(server_info_str)),
            token_limit: mcp_command.token_limit,
        })
    }

    pub async fn new_git_server() -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: "uvx".to_string(),
            args: vec!["mcp-server-git".to_string()],
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }

    pub async fn new_fetch_server() -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: "uvx".to_string(),
            args: vec!["mcp-server-fetch".to_string()],
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }
    pub async fn new_filesystem_server() -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: "uvx".to_string(),
            args: vec!["mcp-server-filesystem".to_string()],
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }

    pub async fn new_everything_server() -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: "npx".to_string(),
            args: vec!["-y".to_string(), "@modelcontextprotocol/server-everything".to_string()],
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }

    pub async fn new_python_server(module_name: &str) -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: "python".to_string(),
            args: vec!["-m".to_string(), module_name.to_string()],
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }

    pub async fn new_node_server(package_name: &str) -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: "npx".to_string(),
            args: vec!["-y".to_string(), package_name.to_string()],
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }

    pub async fn new_gmail_server() -> Result<Self, AgentError> {
        let mcp_command = McpCommand {
            run: "npx".to_string(),
            args: vec!["-y".to_string(), "@gongrzhe/server-gmail-autoauth-mcp".to_string()],
            ..Default::default()
        };
        Self::new_with_mcp_command(&mcp_command).await
    }
    pub async fn get_server_info(&self) -> Option<String> {
        self.server_info.read().await.clone()
    }

    pub async fn disconnect(&mut self) -> Result<(), AgentError> {
        if let Some(service) = self.service.take() {
            service
                .cancel()
                .await
                .map_err(|e| AgentError::MCPError(format!("Failed to cancel service: {}", e)))?;
        }
        *self.connected.write().await = false;
        log::info!("Disconnected from MCP server");
        Ok(())
    }

    fn truncate_response(&self, tool_name: &str, content: &[rmcp::model::Content]) -> String {
        const TRUNCATION_MESSAGE: &str = " [...TRUNCATED...]";
        if content.is_empty() {
            return "Tool executed successfully (no content returned)".to_string();
        }

        let mut full_text = String::new();
        for c in content {
            let text = match &c.raw {
                RawContent::Text(text_content) => text_content.text.clone(),
                RawContent::Image(image_content) => {
                    format!(
                        "Image ({}, {} bytes)",
                        image_content.mime_type,
                        image_content.data.len()
                    )
                }
                RawContent::Resource(resource_content) => {
                    match &resource_content.resource {
                        ResourceContents::TextResourceContents { uri, .. } => {
                            format!("Resource: {}", uri)
                        }
                        ResourceContents::BlobResourceContents { uri, .. } => {
                            format!("Resource: {}", uri)
                        }
                    }
                }
                RawContent::Audio(audio_content) => {
                    format!(
                        "Audio ({}, {} bytes)",
                        audio_content.mime_type,
                        audio_content.data.len()
                    )
                }
            };
            full_text.push_str(&text);
            full_text.push('\n');
        }

        let bpe = p50k_base().unwrap();
        let mut tokens = bpe.encode_with_special_tokens(&full_text);

        if tokens.len() > self.token_limit as usize {
            log::warn!(
                "Truncating MCP response for tool '{}' due to token limit ({}).",
                tool_name,
                self.token_limit
            );
            tokens.truncate(self.token_limit as usize);
            let mut truncated_text = bpe.decode(tokens).unwrap_or_default();
            truncated_text.push_str(TRUNCATION_MESSAGE);
            
            while bpe.encode_with_special_tokens(&truncated_text).len() > self.token_limit as usize {
                truncated_text.pop();
            }
            return truncated_text;
        }

        full_text
    }
}

fn convert_tool(tool: &Tool) -> MCPToolInfo {
    MCPToolInfo {
        name: tool.name.to_string(),
        description: tool
            .description
            .as_ref()
            .map(|d| d.as_ref())
            .unwrap_or("")
            .to_string(),
        input_schema: Value::Object(tool.input_schema.as_ref().clone()),
    }
}

#[async_trait]
impl MCPClientTrait for RMCPClient {
    async fn list_tools(&self) -> Result<Vec<MCPToolInfo>, AgentError> {
        if !*self.connected.read().await {
            return Err(AgentError::MCPError("Not connected".to_string()));
        }

        let service = self
            .service
            .as_ref()
            .ok_or_else(|| AgentError::MCPError("Service not available".to_string()))?;

        log::debug!("About to call list_tools on MCP service...");

        // Use longer timeout for potentially slow MCP servers
        let tools_response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            service.list_tools(Default::default()),
        )
        .await
        .map_err(|_| {
            log::error!("MCP list_tools operation timed out after 30 seconds");
            AgentError::MCPError("Timeout waiting for list_tools response".to_string())
        })?
        .map_err(|e| {
            log::error!("MCP list_tools operation failed: {}", e);
            AgentError::MCPError(format!("Failed to list tools: {}", e))
        })?;

        log::debug!("Successfully received list_tools response");

        let tools = tools_response
            .tools
            .iter()
            .map(convert_tool)
            .collect();

        log::debug!(
            "Listed {} tools from MCP server",
            tools_response.tools.len()
        );
        Ok(tools)
    }

    async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<String, AgentError> {
        if !*self.connected.read().await {
            return Err(AgentError::MCPError("Not connected".to_string()));
        }

        let service = self
            .service
            .as_ref()
            .ok_or_else(|| AgentError::MCPError("Service not available".to_string()))?;

        let arguments = if arguments.is_null() {
            None
        } else {
            arguments.as_object().cloned()
        };

        let request = CallToolRequestParam {
            name: tool_name.to_string().into(),
            arguments,
        };

        let result = service.call_tool(request).await.map_err(|e| {
            AgentError::MCPError(format!("Failed to call tool '{}': {}", tool_name, e))
        })?;

        let result_str = self.truncate_response(tool_name, &result.content);

        log::debug!("Tool '{}' executed successfully", tool_name);
        Ok(result_str)
    }

    async fn is_connected(&self) -> bool {
        *self.connected.read().await && self.service.is_some()
    }
}

impl Drop for RMCPClient {
    fn drop(&mut self) {
        if let Some(service) = self.service.take() {
            // Spawn a task to cancel the service since we can't await in Drop
            tokio::spawn(async move {
                if let Err(e) = service.cancel().await {
                    log::warn!("Failed to cancel service during drop: {}", e);
                }
            });
        }
    }
}

pub struct RMCPClientFactory;

impl RMCPClientFactory {
    pub async fn create_git_client() -> Result<RMCPClient, AgentError> {
        RMCPClient::new_git_server().await
    }

    pub async fn create_fetch_client() -> Result<RMCPClient, AgentError> {
        RMCPClient::new_fetch_server().await
    }

    pub async fn create_filesystem_client() -> Result<RMCPClient, AgentError> {
        RMCPClient::new_filesystem_server().await
    }

    pub async fn create_everything_client() -> Result<RMCPClient, AgentError> {
        RMCPClient::new_everything_server().await
    }

    pub async fn create_gmail_client() -> Result<RMCPClient, AgentError> {
        RMCPClient::new_gmail_server().await
    }

    pub async fn create_custom_client(
        command: &str,
        args: &[&str],
    ) -> Result<RMCPClient, AgentError> {
        let mcp_command = McpCommand {
            run: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        RMCPClient::new_with_mcp_command(&mcp_command).await
    }

    pub async fn create_client_from_config(
        mcp_command: &McpCommand,
    ) -> Result<RMCPClient, AgentError> {
        RMCPClient::new_with_mcp_command(mcp_command).await
    }

    pub async fn create_client_collection() -> Result<HashMap<String, RMCPClient>, AgentError> {
        let mut clients = HashMap::new();

        match Self::create_git_client().await {
            Ok(client) => {
                clients.insert("git".to_string(), client);
            }
            Err(e) => {
                log::warn!("Failed to create git client: {}", e);
            }
        }

        match Self::create_fetch_client().await {
            Ok(client) => {
                clients.insert("fetch".to_string(), client);
            }
            Err(e) => {
                log::warn!("Failed to create fetch client: {}", e);
            }
        }

        match Self::create_everything_client().await {
            Ok(client) => {
                clients.insert("everything".to_string(), client);
            }
            Err(e) => {
                log::warn!("Failed to create everything client: {}", e);
            }
        }

        match Self::create_gmail_client().await {
            Ok(client) => {
                clients.insert("gmail".to_string(), client);
            }
            Err(e) => {
                log::warn!("Failed to create gmail client: {}", e);
            }
        }

        if clients.is_empty() {
            return Err(AgentError::MCPError(
                "Failed to create any MCP clients".to_string(),
            ));
        }

        log::info!(
            "Created {} MCP clients: {:?}",
            clients.len(),
            clients.keys().collect::<Vec<_>>()
        );
        Ok(clients)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::borrow::Cow;
    use rmcp::model::{Content, RawContent};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_rmcp_client_factory() {
        let result = RMCPClientFactory::create_client_collection().await;

        match result {
            Ok(clients) => {
                println!("Successfully created {} clients", clients.len());
            }
            Err(e) => {
                println!("Expected failure (MCP servers not available): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_rmcp_client_connection_state() {
        assert!(true); // Placeholder test
    }

    #[tokio::test]
    async fn test_tool_conversion() {
        use rmcp::model::Tool;
        
        let rmcp_tool = Tool {
            name: Cow::from("test_tool"),
            description: Some(Cow::from("Test tool description")),
            input_schema: Arc::new(
                json!({  // input_schema is Arc<JsonObject>
                    "type": "object",
                    "properties": {
                        "param1": {
                            "type": "string",
                            "description": "First parameter"
                        }
                    },
                    "required": ["param1"]
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
            annotations: None,
        };

        let mcp_tool_info = convert_tool(&rmcp_tool);

        assert_eq!(mcp_tool_info.name, "test_tool");
        assert_eq!(mcp_tool_info.description, "Test tool description");
        assert_eq!(mcp_tool_info.input_schema["type"], "object");
        assert!(mcp_tool_info.input_schema["properties"]["param1"].is_object());
    }

    #[tokio::test]
    async fn test_truncate_response() {
        let client = RMCPClient {
            service: None,
            connected: Arc::new(RwLock::new(true)),
            server_info: Arc::new(RwLock::new(None)),
            token_limit: 100,
        };

        let content_json = json!([
            {
                "type": "text",
                "text": "Here are the best flights for this route and time: "
            },
            {
                "type": "text",
                "text": "This flight departs at 1:00 PM on Thursday, September 25th from SFO, local time, and arrives at 8:00 AM on Friday, September 26th in EZE, local time. The flight is operated by United and has a duration of 15 hr with 1 stop in between. And it's price is $758 and is considered one of the best options by Google Flights!"
            },
            {
                "type": "text",
                "text": "This flight departs at 1:37 PM on Thursday, September 25th from SFO, local time, and arrives at 9:45 AM on Friday, September 26th in EZE, local time. The flight is operated by Delta and has a duration of 16 hours and 8 minutes with 1 stop in between. And it's price is $758 and is considered one of the best options by Google Flights!"
            },
            {
                "type": "text",
                "text": "This flight departs at 11:10 AM on Thursday, September 25th from SFO, local time, and arrives at 7:20 AM on Friday, September 26th in EZE, local time. The flight is operated by American and has a duration of 16 hours and 10 minutes with 1 stop in between. And it's price is $798 and is considered one of the best options by Google Flights!"
            }
        ]);

        let content: Vec<Content> = content_json
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let text = c.get("text").unwrap().as_str().unwrap().to_string();
                Content {
                    raw: RawContent::Text(rmcp::model::RawTextContent {
                        text,
                    }),
                    annotations: None,
                }
            })
            .collect();

        let truncated = client.truncate_response("test_tool", &content);
        let bpe = p50k_base().unwrap();
        let token_count = bpe.encode_with_special_tokens(&truncated).len();

        assert!(token_count <= 100);
        assert!(truncated.contains("Here are the best flights for this route and time:"));
        assert!(truncated.contains("This flight departs at 1:00 PM on Thursday, September 25th from SFO, local time, and arrives at 8:00 AM on Friday, September 26th in EZE, local time. The flight is operated by United and has a duration of 15 hr with 1 stop in between. And it's price is $758 and is considered one of the best options by Google Flights!"));
        assert!(!truncated.contains("This flight departs at 1:37 PM on Thursday, September 25th from SFO, local time, and arrives at 9:45 AM on Friday, September 26th in EZE, local time. The flight is operated by Delta and has a duration of 16 hours and 8 minutes with 1 stop in between. And it's price is $758 and is considered one of the best options by Google Flights!"));
    }
}
