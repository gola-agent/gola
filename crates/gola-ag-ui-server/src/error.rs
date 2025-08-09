//! Error types for the ag-ui server.

use thiserror::Error;

/// Result type alias for server operations.
pub type Result<T> = std::result::Result<T, ServerError>;

/// Errors that can occur in the ag-ui server.
#[derive(Error, Debug)]
pub enum ServerError {
    /// Agent execution error
    #[error("Agent execution failed: {0}")]
    AgentError(#[from] Box<dyn std::error::Error + Send + Sync>),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// HTTP server error
    #[error("HTTP server error: {0}")]
    Http(#[from] hyper::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid request format
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid agent input
    #[error("Invalid agent input: {0}")]
    InvalidInput(String),

    /// Stream error
    #[error("Stream error: {0}")]
    Stream(String),

    /// Server configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Internal server error
    #[error("Internal server error: {0}")]
    Internal(String),
}

impl ServerError {
    /// Create a new invalid request error.
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self::InvalidRequest(msg.into())
    }

    /// Create a new missing field error.
    pub fn missing_field(field: impl Into<String>) -> Self {
        Self::MissingField(field.into())
    }

    /// Create a new invalid input error.
    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    /// Create a new stream error.
    pub fn stream_error(msg: impl Into<String>) -> Self {
        Self::Stream(msg.into())
    }

    /// Create a new configuration error.
    pub fn config_error(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a new internal error.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

/// Convert ServerError to HTTP status code
impl ServerError {
    pub fn status_code(&self) -> u16 {
        match self {
            ServerError::InvalidRequest(_)
            | ServerError::MissingField(_)
            | ServerError::InvalidInput(_) => 400,
            ServerError::AgentError(_) | ServerError::Stream(_) => 422,
            ServerError::Json(_) => 400,
            ServerError::Http(_)
            | ServerError::Io(_)
            | ServerError::Config(_)
            | ServerError::Internal(_) => 500,
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            ServerError::AgentError(_) => "agent_error",
            ServerError::Json(_) => "json_error",
            ServerError::Http(_) => "http_error",
            ServerError::Io(_) => "io_error",
            ServerError::InvalidRequest(_) => "invalid_request",
            ServerError::MissingField(_) => "missing_field",
            ServerError::InvalidInput(_) => "invalid_input",
            ServerError::Stream(_) => "stream_error",
            ServerError::Config(_) => "config_error",
            ServerError::Internal(_) => "internal_error",
        }
    }
}
