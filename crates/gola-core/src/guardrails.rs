
//! Tool execution guardrails for safe and controlled agent operations
//!
//! This module implements authorization mechanisms that prevent agents from
//! executing potentially dangerous operations without explicit approval. The
//! guardrails system is essential for maintaining security boundaries and ensuring
//! human oversight in high-stakes deployments. By intercepting tool calls before
//! execution, this design enables fine-grained control over agent capabilities
//! while maintaining operational flexibility.

use async_trait::async_trait;
use serde_json::Value;

use crate::errors::AgentError;

// ============================================================================
// GUARDRAILS TYPES AND TRAITS
// ============================================================================

/// Response from user for tool execution authorization
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationResponse {
    /// Execute this tool
    Yes,
    /// Do not execute this tool
    No,
    /// Execute this tool and all future tools without asking
    All,
}

/// Authorization mode for tool execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationMode {
    /// Always ask for authorization before executing tools
    Ask,
    /// Always allow tool execution without asking
    Allow,
    /// Always deny tool execution
    Deny,
}

impl Default for AuthorizationMode {
    fn default() -> Self {
        AuthorizationMode::Allow
    }
}

/// Context information for authorization request
#[derive(Debug, Clone)]
pub struct AuthorizationContext {
    /// Name of the tool to be executed
    pub tool_name: String,
    /// Description of the tool
    pub tool_description: String,
    /// Arguments that will be passed to the tool
    pub tool_arguments: Value,
    /// Optional tool call ID for tracking
    pub tool_call_id: Option<String>,
}

/// Request for tool execution authorization
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// Context information about the tool
    pub context: AuthorizationContext,
    /// Step number in the agent execution
    pub step_number: usize,
    /// Total number of steps planned
    pub max_steps: usize,
}

/// Trait for handling tool execution authorization requests
#[async_trait]
pub trait AuthorizationHandler: Send + Sync {
    /// Request authorization for tool execution
    /// Returns the user's response or an error if authorization fails
    async fn request_authorization(
        &self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, AgentError>;
}

/// Default authorization handler that always allows execution (for testing)
#[derive(Debug, Clone)]
pub struct DefaultAuthorizationHandler {
    default_response: AuthorizationResponse,
}

impl DefaultAuthorizationHandler {
    pub fn new() -> Self {
        Self {
            default_response: AuthorizationResponse::Yes,
        }
    }

    pub fn with_response(response: AuthorizationResponse) -> Self {
        Self {
            default_response: response,
        }
    }

    pub fn always_allow() -> Self {
        Self::with_response(AuthorizationResponse::Yes)
    }

    pub fn always_deny() -> Self {
        Self::with_response(AuthorizationResponse::No)
    }

    pub fn always_all() -> Self {
        Self::with_response(AuthorizationResponse::All)
    }
}

impl Default for DefaultAuthorizationHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AuthorizationHandler for DefaultAuthorizationHandler {
    async fn request_authorization(
        &self,
        _request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, AgentError> {
        Ok(self.default_response.clone())
    }
}

/// Mock authorization handler for testing with predefined responses
#[cfg(test)]
pub struct MockAuthorizationHandler {
    responses: std::sync::Mutex<Vec<Result<AuthorizationResponse, AgentError>>>,
}

#[cfg(test)]
impl MockAuthorizationHandler {
    pub fn new(mut responses: Vec<Result<AuthorizationResponse, AgentError>>) -> Self {
        // Reverse to use pop() for FIFO behavior
        responses.reverse();
        Self {
            responses: std::sync::Mutex::new(responses),
        }
    }

    pub fn always_yes() -> Self {
        Self::new(vec![Ok(AuthorizationResponse::Yes); 10]) // Enough for most tests
    }

    pub fn always_no() -> Self {
        Self::new(vec![Ok(AuthorizationResponse::No); 10])
    }

    pub fn always_all() -> Self {
        Self::new(vec![Ok(AuthorizationResponse::All); 10])
    }

    pub fn sequence(responses: Vec<AuthorizationResponse>) -> Self {
        let results = responses.into_iter().map(Ok).collect();
        Self::new(results)
    }

    pub fn with_error(error: AgentError) -> Self {
        Self::new(vec![Err(error)])
    }
}

#[cfg(test)]
#[async_trait]
impl AuthorizationHandler for MockAuthorizationHandler {
    async fn request_authorization(
        &self,
        _request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, AgentError> {
        self.responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or(Ok(AuthorizationResponse::Yes)) // Default fallback
    }
}
