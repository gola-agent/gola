//! Client SDK for programmatic agent interaction across deployment boundaries
//!
//! This crate abstracts the complexity of agent communication protocols, providing
//! a unified interface whether agents run in-process or across networks. The design
//! prioritizes deployment flexibility by allowing seamless transitions between local
//! development (direct client) and production deployments (HTTP client) without code
//! changes. This abstraction layer is crucial for building agent-powered applications
//! that can scale from prototypes to distributed systems.

use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub mod direct_client;
pub mod http_client;
pub mod types;

pub use types::*;

/// AgentClient trait for communicating with Gola agents
#[async_trait]
pub trait AgentClient: Send + Sync {
    /// Stream a request to the agent and receive response events
    async fn stream_request(
        &self,
        request: StreamRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>>;

    /// Check if the agent is healthy and reachable
    async fn health_check(&self) -> Result<()>;

    /// Clear the agent's memory
    async fn clear_memory(&self) -> Result<()>;
}

/// Factory for creating AgentClient instances
pub struct AgentClientFactory;

impl AgentClientFactory {
    /// Create an HTTP client for remote servers
    pub fn create_http_client(base_url: String) -> Box<dyn AgentClient> {
        Box::new(http_client::HttpAgentClient::new(base_url))
    }

    /// Create a direct client for embedded servers
    pub fn create_direct_client<T: gola_ag_ui_server::AgentHandler>(
        server_handle: std::sync::Arc<T>,
    ) -> Box<dyn AgentClient> {
        Box::new(direct_client::DirectAgentClient::new(server_handle))
    }
}
