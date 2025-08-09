//! Error types for comprehensive failure handling across the agent framework
//!
//! This module provides a unified error hierarchy that captures all failure modes
//! in agent execution. The design philosophy emphasizes actionable error messages
//! that guide recovery strategies rather than just reporting failures. By categorizing
//! errors by their source (LLM, tools, configuration), the system enables targeted
//! retry logic and graceful degradation when specific subsystems fail.

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum AgentError {
    #[error("LLM interaction failed: {0}")]
    LLMError(String),
    #[error("Tool execution failed for '{tool_name}': {message}")]
    ToolError { tool_name: String, message: String },
    #[error("Code execution failed: {0}")]
    CodeExecutionError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Parsing error: {0}")]
    ParsingError(String),
    #[error("Maximum steps reached")]
    MaxStepsReached,
    #[error("Loop detection: {0}")]
    LoopDetection(String),
    #[error("MCP client error: {0}")]
    MCPError(String),
    #[error("Docker operation failed: {0}")]
    DockerError(String),
    #[error("RAG operation failed: {0}")]
    RagError(String),
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),
    #[error("Authorization handler error {0}")]
    AuthorizationHandlerError(String),
    #[error("Authorization denied error {0}")]
    AuthorizationDenied(String),
    #[error("Installer error: {0}")]
    InstallerError(String),
    #[error("Runtime error: {0}")]
    RuntimeError(String),
}

impl From<std::io::Error> for AgentError {
    fn from(err: std::io::Error) -> Self {
        AgentError::IoError(err.to_string())
    }
}

impl From<reqwest::Error> for AgentError {
    fn from(err: reqwest::Error) -> Self {
        AgentError::LLMError(err.to_string())
    }
}

// Specific error for Docker executor
#[derive(Error, Debug)]
pub enum DockerExecutorError {
    #[error("Bollard (Docker client) error: {0}")]
    BollardError(#[from] bollard::errors::Error),
    #[error("Container execution failed with exit code {exit_code:?}:\nStdout: {stdout}\nStderr: {stderr}")]
    ContainerFailed {
        exit_code: Option<i64>,
        stdout: String,
        stderr: String,
    },
    #[error("I/O error during Docker operation: {0}")]
    IoError(#[from] std::io::Error),
    #[error("UTF-8 decoding error from string: {0}")]
    StringFromUtf8Error(#[from] std::string::FromUtf8Error),
    #[error("UTF-8 decoding error from slice: {0}")]
    StrUtf8Error(#[from] std::str::Utf8Error),
    #[error("Could not create temporary file/directory: {0}")]
    TempFileError(String),
    #[error("Script execution timed out")]
    Timeout,
}
