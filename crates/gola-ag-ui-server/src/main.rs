//! ag-ui-server binary
//!
//! A Server-Sent Events (SSE) server that implements the ag-ui protocol.

use async_trait::async_trait;
use clap::Parser;
use gola_ag_ui_server::{shutdown_signal, AgUiServer, AgentHandler, AgentStream, ServerConfig};
use gola_ag_ui_types::{Role, RunAgentInput};
use std::net::SocketAddr;
use std::time::Duration;

/// Command line arguments for the ag-ui server.
#[derive(Parser, Debug)]
#[command(name = "ag-ui-server")]
#[command(about = "A Server-Sent Events server implementing the ag-ui protocol")]
#[command(version)]
struct Args {
    /// Server bind address
    #[arg(short, long, default_value = "127.0.0.1:3000")]
    bind: String,

    /// Enable CORS
    #[arg(long, default_value = "true")]
    cors: bool,

    /// CORS allowed origins (comma-separated)
    #[arg(long)]
    cors_origins: Option<String>,

    /// Request timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,

    /// Maximum request body size in bytes
    #[arg(long, default_value = "1048576")] // 1MB
    max_body_size: usize,

    /// Enable request logging
    #[arg(long, default_value = "true")]
    logging: bool,

    /// SSE keep-alive interval in seconds
    #[arg(long, default_value = "30")]
    keepalive: u64,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
}

/// A simple echo agent that responds with the last user message.
#[derive(Clone)]
struct EchoAgent {
    name: String,
}

impl EchoAgent {
    fn new() -> Self {
        Self {
            name: "Echo Agent".to_string(),
        }
    }
}

#[async_trait]
impl AgentHandler for EchoAgent {
    async fn handle_input(&self, input: RunAgentInput) -> gola_ag_ui_server::Result<AgentStream> {
        use gola_ag_ui_server::agent::streams;

        // Get the last user message
        let last_message = input
            .messages
            .iter()
            .rev()
            .find(|msg| matches!(msg.role(), Role::User))
            .ok_or_else(|| {
                gola_ag_ui_server::ServerError::invalid_input("No user message found")
            })?;

        let response = format!("Echo: {}", last_message.content().unwrap_or(""));

        // Create a streaming response with a small delay to demonstrate streaming
        Ok(streams::streaming_text_response(
            response,
            Duration::from_millis(50),
        ))
    }

    async fn validate_input(&self, input: &RunAgentInput) -> gola_ag_ui_server::Result<()> {
        if input.messages.is_empty() {
            return Err(gola_ag_ui_server::ServerError::invalid_input(
                "No messages provided",
            ));
        }

        // Check if there's at least one user message
        let has_user_message = input
            .messages
            .iter()
            .any(|msg| matches!(msg.role(), Role::User));

        if !has_user_message {
            return Err(gola_ag_ui_server::ServerError::invalid_input(
                "No user message found",
            ));
        }

        Ok(())
    }

    async fn get_metadata(
        &self,
    ) -> gola_ag_ui_server::Result<gola_ag_ui_server::agent::AgentMetadata> {
        Ok(gola_ag_ui_server::agent::AgentMetadata::new(
            &self.name,
            "A simple echo agent that repeats user messages with streaming",
        )
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_capability("text_generation")
        .with_capability("streaming")
        .with_streaming(true)
        .with_tools(false))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&args.log_level))
        .init();

    // Parse bind address
    let bind_addr: SocketAddr = args
        .bind
        .parse()
        .map_err(|e| format!("Invalid bind address '{}': {}", args.bind, e))?;

    // Parse CORS origins
    let cors_origins = args
        .cors_origins
        .map(|origins| origins.split(',').map(|s| s.trim().to_string()).collect());

    // Create server configuration
    let config = ServerConfig::new()
        .with_bind_addr(bind_addr)
        .with_cors(args.cors)
        .with_cors_origins(cors_origins.unwrap_or_default())
        .with_request_timeout(Duration::from_secs(args.timeout))
        .with_max_body_size(args.max_body_size)
        .with_logging(args.logging)
        .with_sse_keepalive(Duration::from_secs(args.keepalive));

    // Create the echo agent
    let agent = EchoAgent::new();

    // Create and start the server
    let server = AgUiServer::with_config(agent, config);

    log::info!("Starting ag-ui server...");
    log::info!("Configuration:");
    log::info!("  Bind address: {}", bind_addr);
    log::info!("  CORS enabled: {}", args.cors);
    log::info!("  Request timeout: {}s", args.timeout);
    log::info!("  Max body size: {} bytes", args.max_body_size);
    log::info!("  Logging enabled: {}", args.logging);
    log::info!("  SSE keep-alive: {}s", args.keepalive);

    // Start server with graceful shutdown
    server.serve_with_shutdown(shutdown_signal()).await?;

    Ok(())
}
