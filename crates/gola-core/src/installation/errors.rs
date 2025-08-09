//! Error types for the installation system

use crate::errors::AgentError;
use thiserror::Error;

/// Errors that can occur during installation
#[derive(Error, Debug)]
pub enum InstallationError {
    #[error("Installation strategy '{strategy}' failed: {reason}")]
    StrategyFailed { strategy: String, reason: String },
    
    #[error("Binary '{name}' not found in any available source")]
    BinaryNotFound { name: String },
    
    #[error("Docker is not available or not properly configured: {reason}")]
    DockerUnavailable { reason: String },
    
    #[error("Source build failed: {reason}")]
    SourceBuildFailed { reason: String },
    
    #[error("All installation strategies exhausted for binary '{name}'")]
    AllStrategiesExhausted { name: String },
    
    #[error("GitHub API error: {message}")]
    GitHubApiError { message: String },
    
    #[error("Docker registry error: {registry} - {message}")]
    DockerRegistryError { registry: String, message: String },
    
    #[error("Binary verification failed: {reason}")]
    BinaryVerificationFailed { reason: String },
    
    #[error("Platform not supported: {platform}")]
    PlatformNotSupported { platform: String },
    
    #[error("Invalid configuration: {message}")]
    InvalidConfiguration { message: String },
    
    #[error("I/O error during installation: {message}")]
    IoError { message: String },
    
    #[error("Network error: {message}")]
    NetworkError { message: String },
}

impl From<std::io::Error> for InstallationError {
    fn from(err: std::io::Error) -> Self {
        InstallationError::IoError {
            message: err.to_string(),
        }
    }
}

impl From<reqwest::Error> for InstallationError {
    fn from(err: reqwest::Error) -> Self {
        InstallationError::NetworkError {
            message: err.to_string(),
        }
    }
}

/// Convert installation errors to agent errors for backward compatibility
impl From<InstallationError> for AgentError {
    fn from(err: InstallationError) -> Self {
        match err {
            InstallationError::BinaryNotFound { name } => {
                AgentError::RuntimeError(format!("Binary '{}' not found", name))
            }
            InstallationError::DockerUnavailable { reason } => {
                AgentError::RuntimeError(format!("Docker unavailable: {}", reason))
            }
            InstallationError::SourceBuildFailed { reason } => {
                AgentError::RuntimeError(format!("Source build failed: {}", reason))
            }
            InstallationError::IoError { message } => {
                AgentError::IoError(message)
            }
            other => AgentError::RuntimeError(other.to_string()),
        }
    }
}

/// Result type for installation operations
pub type InstallationResult<T> = Result<T, InstallationError>;