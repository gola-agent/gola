//! Server-Sent Events (SSE) infrastructure for real-time agent-UI communication
//!
//! This crate implements the ag-ui protocol for streaming agent execution events to
//! web clients. The design choice of SSE over WebSockets prioritizes simplicity and
//! HTTP/2 compatibility while maintaining real-time responsiveness. This architecture
//! enables progressive disclosure of agent reasoning, making AI decision-making
//! transparent and interruptible. The streaming approach is essential for building
//! trust through observability in human-AI collaborative workflows.

pub mod agent;
pub mod error;
pub mod sse;

pub use agent::{AgentHandler, AgentStream};
pub use error::{Result, ServerError};
pub use sse::{SseEvent, SseStream};

// Re-export commonly used types from ag-ui-types
pub use gola_ag_ui_types::{
    AuthorizationConfig, AuthorizationResponse, AuthorizationStatus, AuthorizationStatusEvent,
    Event, Message, PendingAuthorization, RunAgentInput, Tool, ToolAuthorizationMode,
    ToolAuthorizationRequestEvent, ToolAuthorizationResponseEvent, ToolCall,
};

use axum::extract::{Json as AxumJson, State};
use axum::http::StatusCode;
use axum::response::{Json, Response};
use axum::routing::{delete, get, options, post};
use axum::{middleware, Router};
use serde::Serialize;
use serde_json::json;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Health check response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub version: String,
}

/// Configuration for the ag-ui server.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Server bind address
    pub bind_addr: SocketAddr,
    /// Enable CORS
    pub enable_cors: bool,
    /// CORS allowed origins (if None, allows any origin)
    pub cors_origins: Option<Vec<String>>,
    /// Request timeout duration
    pub request_timeout: Duration,
    /// Maximum request body size in bytes
    pub max_body_size: usize,
    /// Enable request logging
    pub enable_logging: bool,
    /// Keep-alive interval for SSE connections
    pub sse_keepalive_interval: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:3000".parse().unwrap(),
            enable_cors: true,
            cors_origins: None, // Allow any origin
            request_timeout: Duration::from_secs(30),
            max_body_size: 1024 * 1024, // 1MB
            enable_logging: true,
            sse_keepalive_interval: Duration::from_secs(30),
        }
    }
}

impl ServerConfig {
    /// Create a new server configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the bind address.
    pub fn with_bind_addr(mut self, addr: SocketAddr) -> Self {
        self.bind_addr = addr;
        self
    }

    /// Parse and set the bind address from a string.
    pub fn with_bind_addr_str(mut self, addr: &str) -> Result<Self> {
        self.bind_addr = addr
            .parse()
            .map_err(|e| ServerError::config_error(format!("Invalid bind address: {}", e)))?;
        Ok(self)
    }

    /// Enable or disable CORS.
    pub fn with_cors(mut self, enable: bool) -> Self {
        self.enable_cors = enable;
        self
    }

    /// Set allowed CORS origins.
    pub fn with_cors_origins(mut self, origins: Vec<String>) -> Self {
        self.cors_origins = Some(origins);
        self
    }

    /// Set request timeout.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Set maximum request body size.
    pub fn with_max_body_size(mut self, size: usize) -> Self {
        self.max_body_size = size;
        self
    }

    /// Enable or disable request logging.
    pub fn with_logging(mut self, enable: bool) -> Self {
        self.enable_logging = enable;
        self
    }

    /// Set SSE keep-alive interval.
    pub fn with_sse_keepalive(mut self, interval: Duration) -> Self {
        self.sse_keepalive_interval = interval;
        self
    }
}

/// Shared application state containing the agent and configuration.
#[derive(Clone)]
pub struct AppState<T: AgentHandler + Clone> {
    pub agent: T,
    pub config: ServerConfig,
}

/// Handler for the /authorization POST endpoint.
async fn authorization_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
    AxumJson(auth_response): AxumJson<ToolAuthorizationResponseEvent>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::info!(
        "Received authorization response for tool call: {}",
        auth_response.tool_call_id
    );
    log::debug!("Authorization response: {:?}", auth_response.response);

    // Forward the authorization response to the agent
    match app_state
        .agent
        .handle_authorization_response(auth_response.clone())
        .await
    {
        Ok(()) => {
            log::info!(
                "Authorization response processed successfully for tool call: {}",
                auth_response.tool_call_id
            );
            Ok(Json(json!({
                "status": "success",
                "message": "Authorization response processed",
                "tool_call_id": auth_response.tool_call_id,
                "response": auth_response.response,
                "timestamp": chrono::Utc::now()
            })))
        }
        Err(e) => {
            log::error!(
                "Failed to process authorization response for tool call {}: {}",
                auth_response.tool_call_id,
                e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to process authorization response",
                    "details": e.to_string(),
                    "tool_call_id": auth_response.tool_call_id,
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /authorization/config GET endpoint.
async fn authorization_config_get_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::info!("Received authorization config get request");

    match app_state.agent.get_authorization_config().await {
        Ok(config) => Ok(Json(json!({
            "status": "success",
            "config": config,
            "timestamp": chrono::Utc::now()
        }))),
        Err(e) => {
            log::error!("Failed to get authorization config: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to get authorization config",
                    "details": e.to_string(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /authorization/config POST endpoint.
async fn authorization_config_set_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
    AxumJson(config): AxumJson<AuthorizationConfig>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::info!("Received authorization config set request");
    log::debug!("New authorization config: {:?}", config);

    match app_state
        .agent
        .set_authorization_config(config.clone())
        .await
    {
        Ok(()) => {
            log::info!("Authorization config updated successfully");
            Ok(Json(json!({
                "status": "success",
                "message": "Authorization config updated",
                "config": config,
                "timestamp": chrono::Utc::now()
            })))
        }
        Err(e) => {
            log::error!("Failed to set authorization config: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to set authorization config",
                    "details": e.to_string(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /authorization/pending GET endpoint.
async fn authorization_pending_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::debug!("Received pending authorizations request");

    match app_state.agent.get_pending_authorizations().await {
        Ok(pending) => Ok(Json(json!({
            "status": "success",
            "pending_authorizations": pending,
            "count": pending.len(),
            "timestamp": chrono::Utc::now()
        }))),
        Err(e) => {
            log::error!("Failed to get pending authorizations: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to get pending authorizations",
                    "details": e.to_string(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /authorization/cancel POST endpoint.
async fn authorization_cancel_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
    AxumJson(request): AxumJson<serde_json::Value>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::info!("Received authorization cancel request");

    let tool_call_id = match request.get("tool_call_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "Missing tool_call_id in request",
                    "timestamp": chrono::Utc::now()
                })),
            ));
        }
    };

    log::debug!("Cancelling authorization for tool call: {}", tool_call_id);

    match app_state
        .agent
        .cancel_authorization(tool_call_id.clone())
        .await
    {
        Ok(()) => {
            log::info!(
                "Authorization cancelled successfully for tool call: {}",
                tool_call_id
            );
            Ok(Json(json!({
                "status": "success",
                "message": "Authorization cancelled",
                "tool_call_id": tool_call_id,
                "timestamp": chrono::Utc::now()
            })))
        }
        Err(e) => {
            log::error!(
                "Failed to cancel authorization for tool call {}: {}",
                tool_call_id,
                e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to cancel authorization",
                    "details": e.to_string(),
                    "tool_call_id": tool_call_id,
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /tools GET endpoint.
async fn tools_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::info!("Received tools request");

    match app_state.agent.get_available_tools().await {
        Ok(tools) => {
            log::info!("Successfully retrieved {} tools", tools.len());
            Ok(Json(
                serde_json::to_value(tools).unwrap_or_else(|_| json!([])),
            ))
        }
        Err(e) => {
            log::error!("Failed to get available tools: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to retrieve available tools",
                    "details": e.to_string(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /memory/stats GET endpoint.
async fn memory_stats_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::info!("Received memory stats request");

    match app_state.agent.get_memory_stats().await {
        Ok(Some(stats)) => Ok(Json(json!({
            "status": "success",
            "memory_stats": stats,
            "timestamp": chrono::Utc::now()
        }))),
        Ok(None) => Ok(Json(json!({
            "status": "not_available",
            "message": "Memory stats not available for this agent",
            "timestamp": chrono::Utc::now()
        }))),
        Err(e) => {
            log::error!("Failed to get memory stats: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to get memory stats",
                    "details": e.to_string(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /memory/clear POST endpoint.
async fn memory_clear_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
) -> std::result::Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    log::info!("Received memory clear request");

    match app_state.agent.clear_memory().await {
        Ok(()) => Ok(Json(json!({
            "status": "success",
            "message": "Memory cleared successfully",
            "timestamp": chrono::Utc::now()
        }))),
        Err(e) => {
            log::error!("Failed to clear memory: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Failed to clear memory",
                    "details": e.to_string(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
    }
}

/// Handler for the /stream POST endpoint.
async fn stream_handler<T: AgentHandler + Clone>(
    State(app_state): State<AppState<T>>,
    AxumJson(input): AxumJson<RunAgentInput>,
) -> std::result::Result<Response, (StatusCode, Json<serde_json::Value>)> {
    log::info!("Received stream request for thread: {:?}", input.thread_id);

    // Validate the input using the agent
    if let Err(e) = app_state.agent.validate_input(&input).await {
        log::warn!("Input validation failed: {}", e);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "Invalid input",
                "details": e.to_string(),
                "timestamp": chrono::Utc::now()
            })),
        ));
    }

    // Handle the input and get the event stream
    let event_stream = match app_state.agent.handle_input(input).await {
        Ok(stream) => stream,
        Err(e) => {
            log::error!("Agent failed to handle input: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": "Agent processing failed",
                    "details": e.to_string(),
                    "timestamp": chrono::Utc::now()
                })),
            ));
        }
    };

    // Convert the event stream to SSE response
    let response = crate::sse::create_sse_response_with_keepalive(
        event_stream,
        app_state.config.sse_keepalive_interval,
        "keep-alive",
    );

    Ok(response)
}

/// The main ag-ui SSE server.
pub struct AgUiServer<T: AgentHandler + Clone> {
    agent: T,
    config: ServerConfig,
}

impl<T: AgentHandler + Clone + Send + Sync + 'static> AgUiServer<T> {
    /// Create a new server with the given agent and default configuration.
    pub fn new(agent: T) -> Self {
        Self {
            agent,
            config: ServerConfig::default(),
        }
    }

    /// Create a new server with custom configuration.
    pub fn with_config(agent: T, config: ServerConfig) -> Self {
        Self { agent, config }
    }

    /// Get the server configuration.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Update the server configuration.
    pub fn set_config(&mut self, config: ServerConfig) {
        self.config = config;
    }

    /// Build the Axum router with all routes and middleware.
    pub fn build_router(&self) -> Router {
        // Create shared state
        let state = AppState {
            agent: self.agent.clone(),
            config: self.config.clone(),
        };

        // Create router with all endpoints
        let mut router = Router::new()
            // Health and info endpoints
            .route("/health", get(|| async { 
                Json(HealthResponse {
                    status: "healthy".to_string(),
                    timestamp: chrono::Utc::now(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                })
            }))
            .route("/tools", get(tools_handler::<T>))
            // Memory management endpoints
            .route("/memory/stats", get(memory_stats_handler::<T>))
            .route("/memory/clear", delete(memory_clear_handler::<T>))
            // Legacy endpoint for remote terminal client
            .route("/agents/clear-memory", post(memory_clear_handler::<T>))
            // Authorization endpoints
            .route("/authorization", post(authorization_handler::<T>))
            .route("/authorization/config", get(authorization_config_get_handler::<T>))
            .route("/authorization/config", post(authorization_config_set_handler::<T>))
            .route("/authorization/pending", get(authorization_pending_handler::<T>))
            .route("/authorization/cancel", post(authorization_cancel_handler::<T>))
            // Main streaming endpoint
            .route("/stream", post(stream_handler::<T>))
            // Remote terminal client endpoint (for backward compatibility)
            .route("/agents/stream", post(stream_handler::<T>))
            // WebSocket endpoint (placeholder)
            .route("/ws", get(|| async {
                (
                    StatusCode::NOT_IMPLEMENTED,
                    Json(json!({
                        "error": "WebSocket support not yet implemented. Use the /stream endpoint for SSE.",
                        "timestamp": chrono::Utc::now()
                    }))
                )
            }))
            // CORS preflight
            .route("/stream", options(|| async { StatusCode::OK }))
            .route("/agents/stream", options(|| async { StatusCode::OK }))
            .route("/batch", options(|| async { StatusCode::OK }))
            .route("/tools", options(|| async { StatusCode::OK }))
            .route("/memory/stats", options(|| async { StatusCode::OK }))
            .route("/memory/clear", options(|| async { StatusCode::OK }))
            .route("/agents/clear-memory", options(|| async { StatusCode::OK }))
            .route("/authorization", options(|| async { StatusCode::OK }))
            .route("/authorization/config", options(|| async { StatusCode::OK }))
            .route("/authorization/pending", options(|| async { StatusCode::OK }))
            .route("/authorization/cancel", options(|| async { StatusCode::OK }))
            // Add the shared state
            .with_state(state);

        // Add middleware layers
        if self.config.enable_logging {
            router =
                router.layer(middleware::from_fn(
                    |request: axum::http::Request<axum::body::Body>,
                     next: axum::middleware::Next| async {
                        let request_id = uuid::Uuid::new_v4().to_string();
                        let method = request.method().clone();
                        let uri = request.uri().clone();

                        // Use debug level for polling-related authorization requests
                        if uri.path() == "/authorization/pending"
                            && method == axum::http::Method::GET
                        {
                            log::debug!("Request {} {} {}", request_id, method, uri);
                        } else {
                            log::info!("Request {} {} {}", request_id, method, uri);
                        }

                        let start = std::time::Instant::now();
                        let response = next.run(request).await;
                        let duration = start.elapsed();

                        // Use debug level for polling-related authorization responses
                        if uri.path() == "/authorization/pending"
                            && method == axum::http::Method::GET
                        {
                            log::debug!("Response {} completed in {:?}", request_id, duration);
                        } else {
                            log::info!("Response {} completed in {:?}", request_id, duration);
                        }

                        response
                    },
                ));
        }

        router = router.layer(TraceLayer::new_for_http());

        // Add CORS layer if enabled
        if self.config.enable_cors {
            let cors_layer = if let Some(ref origins) = self.config.cors_origins {
                let origins: std::result::Result<Vec<_>, _> =
                    origins.iter().map(|s| s.parse()).collect();
                match origins {
                    Ok(origins) => CorsLayer::new()
                        .allow_origin(origins)
                        .allow_methods(Any)
                        .allow_headers(Any),
                    Err(_) => CorsLayer::permissive(),
                }
            } else {
                CorsLayer::permissive()
            };
            router = router.layer(cors_layer);
        }

        router
    }

    /// Start the server and listen for connections.
    ///
    /// This method will block until the server is shut down.
    pub async fn serve(self) -> Result<()> {
        let router = self.build_router();
        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| {
                ServerError::config_error(format!(
                    "Failed to bind to {}: {}",
                    self.config.bind_addr, e
                ))
            })?;

        log::info!("ag-ui server starting on {}", self.config.bind_addr);
        log::info!("Health check: http://{}/health", self.config.bind_addr);
        log::info!("Tools endpoint: http://{}/tools", self.config.bind_addr);
        log::info!("Stream endpoint: http://{}/stream", self.config.bind_addr);
        log::info!(
            "Memory stats: http://{}/memory/stats",
            self.config.bind_addr
        );
        log::info!(
            "Memory clear: http://{}/memory/clear",
            self.config.bind_addr
        );
        log::info!(
            "Authorization: http://{}/authorization",
            self.config.bind_addr
        );
        log::info!(
            "Authorization config: http://{}/authorization/config",
            self.config.bind_addr
        );
        log::info!(
            "Authorization pending: http://{}/authorization/pending",
            self.config.bind_addr
        );
        log::info!(
            "Authorization cancel: http://{}/authorization/cancel",
            self.config.bind_addr
        );
        log::info!("WebSocket endpoint: http://{}/ws", self.config.bind_addr);

        // Call agent's on_connect method
        if let Err(e) = self.agent.on_connect().await {
            log::warn!("Agent on_connect failed: {}", e);
        }

        axum::serve(listener, router)
            .await
            .map_err(|e| ServerError::internal(format!("Server error: {}", e)))?;

        // Call agent's on_disconnect method
        if let Err(e) = self.agent.on_disconnect().await {
            log::warn!("Agent on_disconnect failed: {}", e);
        }

        Ok(())
    }

    /// Start the server with a custom bind address.
    pub async fn serve_on(self, addr: impl Into<SocketAddr>) -> Result<()> {
        let mut server = self;
        server.config.bind_addr = addr.into();
        server.serve().await
    }

    /// Start the server with graceful shutdown support.
    ///
    /// The server will shut down when the provided shutdown signal is received.
    pub async fn serve_with_shutdown<F>(self, shutdown_signal: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let router = self.build_router();
        let listener = TcpListener::bind(self.config.bind_addr)
            .await
            .map_err(|e| {
                ServerError::config_error(format!(
                    "Failed to bind to {}: {}",
                    self.config.bind_addr, e
                ))
            })?;

        log::info!(
            "ag-ui server starting on {} with graceful shutdown",
            self.config.bind_addr
        );

        // Call agent's on_connect method
        if let Err(e) = self.agent.on_connect().await {
            log::warn!("Agent on_connect failed: {}", e);
        }

        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(|e| ServerError::internal(format!("Server error: {}", e)))?;

        // Call agent's on_disconnect method
        if let Err(e) = self.agent.on_disconnect().await {
            log::warn!("Agent on_disconnect failed: {}", e);
        }

        log::info!("ag-ui server shut down gracefully");
        Ok(())
    }
}

/// Utility function to create a shutdown signal from Ctrl+C.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            log::info!("Received Ctrl+C, shutting down...");
        },
        _ = terminate => {
            log::info!("Received SIGTERM, shutting down...");
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt; // for `oneshot`

    #[derive(Clone)]
    struct MockAgent {
        clear_memory_called: Arc<Mutex<bool>>,
    }

    impl MockAgent {
        fn new() -> Self {
            Self {
                clear_memory_called: Arc::new(Mutex::new(false)),
            }
        }
    }

    #[async_trait]
    impl AgentHandler for MockAgent {
        async fn handle_input(&self, _input: RunAgentInput) -> Result<AgentStream> {
            unimplemented!()
        }

        async fn clear_memory(&self) -> Result<()> {
            let mut called = self.clear_memory_called.lock().unwrap();
            *called = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_memory_clear_endpoint() {
        // Arrange
        let mock_agent = MockAgent::new();
        let clear_memory_called = mock_agent.clear_memory_called.clone();
        let server = AgUiServer::new(mock_agent);
        let app = server.build_router();

        // Act
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/memory/clear")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Assert
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["status"], "success");
        assert_eq!(body["message"], "Memory cleared successfully");

        assert!(
            *clear_memory_called.lock().unwrap(),
            "clear_memory should have been called"
        );
    }
}
