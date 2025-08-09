//! Error types for ag-ui operations.

use thiserror::Error;

/// Errors that can occur in ag-ui operations.
#[derive(Error, Debug, Clone, PartialEq)]
pub enum AgUiError {
    /// A validation error occurred.
    #[error("Validation error: {message}")]
    Validation { message: String },
    
    /// A serialization error occurred.
    #[error("Serialization error: {message}")]
    Serialization { message: String },
    
    /// An invalid event type was encountered.
    #[error("Invalid event type: {event_type}")]
    InvalidEventType { event_type: String },
    
    /// An invalid message role was encountered.
    #[error("Invalid message role: {role}")]
    InvalidRole { role: String },
    
    /// A required field was missing.
    #[error("Missing required field: {field}")]
    MissingField { field: String },
    
    /// A generic error occurred.
    #[error("{message}")]
    Generic { message: String },
}

impl AgUiError {
    /// Create a new validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }
    
    /// Create a new serialization error.
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization {
            message: message.into(),
        }
    }
    
    /// Create a new generic error.
    pub fn generic(message: impl Into<String>) -> Self {
        Self::Generic {
            message: message.into(),
        }
    }
}

/// Result type for ag-ui operations.
pub type AgUiResult<T> = Result<T, AgUiError>;
