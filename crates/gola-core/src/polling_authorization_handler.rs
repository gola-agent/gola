//! Polling-based authorization handler for connecting gola-core agent to gola-term
//! 
//! This handler implements a polling approach to tool authorization where the client
//! periodically checks for pending authorizations and sends responses.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use tokio::sync::{oneshot, Mutex};

use crate::errors::AgentError;
use crate::guardrails::{AuthorizationHandler, AuthorizationRequest, AuthorizationResponse, AuthorizationContext};
use gola_ag_ui_types::{
    PendingAuthorization, AuthorizationConfig, AuthorizationStatus,
    ToolAuthorizationMode, AuthorizationResponse as UiAuthorizationResponse
};

/// Polling-based authorization handler that stores pending authorization requests
/// and allows clients to poll for them and respond asynchronously
pub struct PollingAuthorizationHandler {
    pending_requests: Arc<Mutex<HashMap<String, PendingAuthorizationRequest>>>,
    config: Arc<Mutex<AuthorizationConfig>>,
}

/// Internal structure for tracking pending authorization requests
struct PendingAuthorizationRequest {
    response_sender: oneshot::Sender<AuthorizationResponse>,
    created_at: Instant,
    context: AuthorizationContext,
    _step_number: usize,
}

impl PollingAuthorizationHandler {
    /// Create a new polling authorization handler
    pub fn new() -> Self {
        Self {
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(AuthorizationConfig::default())),
        }
    }

    /// Handle an authorization response from the client
    pub async fn handle_response(
        &self, 
        tool_call_id: String, 
        response: UiAuthorizationResponse
    ) -> Result<(), AgentError> {
        let mut pending = self.pending_requests.lock().await;
        
        if let Some(pending_request) = pending.remove(&tool_call_id) {
            let auth_response = match response {
                UiAuthorizationResponse::Approve => AuthorizationResponse::Yes,
                UiAuthorizationResponse::Deny => AuthorizationResponse::No,
                UiAuthorizationResponse::ApproveAndAllow => AuthorizationResponse::All,
            };

            // Send the response back to the waiting authorization check
            if pending_request.response_sender.send(auth_response).is_err() {
                log::warn!("Failed to send authorization response for tool call: {}", tool_call_id);
            } else {
                log::debug!("Sent authorization response for tool call: {}", tool_call_id);
            }
            Ok(())
        } else {
            Err(AgentError::AuthorizationFailed(format!(
                "No pending authorization found for tool call: {}", tool_call_id
            )))
        }
    }

    /// Get current authorization configuration
    pub async fn get_config(&self) -> AuthorizationConfig {
        self.config.lock().await.clone()
    }

    pub async fn set_config(&self, config: AuthorizationConfig) {
        *self.config.lock().await = config;
    }

    /// Get pending authorization requests
    pub async fn get_pending_authorizations(&self) -> Vec<PendingAuthorization> {
        let pending = self.pending_requests.lock().await;
        let now = Instant::now();
        let timeout_duration = self.get_timeout_duration().await;
        
        pending.values().map(|req| {
            let status = if let Some(timeout) = timeout_duration {
                if now.duration_since(req.created_at) > timeout {
                    AuthorizationStatus::TimedOut
                } else {
                    AuthorizationStatus::Pending
                }
            } else {
                AuthorizationStatus::Pending
            };

            PendingAuthorization {
                tool_call_id: req.context.tool_call_id.clone().unwrap_or_default(),
                tool_call_name: req.context.tool_name.clone(),
                tool_call_args: req.context.tool_arguments.to_string(),
                description: Some(req.context.tool_description.clone()),
                status,
                created_at: req.created_at.elapsed().as_secs() as i64,
                expires_at: timeout_duration.map(|timeout| {
                    (req.created_at + timeout).duration_since(now).as_secs() as i64
                }),
            }
        }).collect()
    }

    pub async fn cancel_authorization(&self, tool_call_id: String) -> Result<(), AgentError> {
        let mut pending = self.pending_requests.lock().await;
        
        if let Some(pending_request) = pending.remove(&tool_call_id) {
            // Send a "No" response to unblock the waiting authorization check
            if pending_request.response_sender.send(AuthorizationResponse::No).is_err() {
                log::warn!("Failed to send cancellation response for tool call: {}", tool_call_id);
            }
            log::debug!("Cancelled authorization request for tool call: {}", tool_call_id);
            Ok(())
        } else {
            Err(AgentError::AuthorizationFailed(format!(
                "No pending authorization found for tool call: {}", tool_call_id
            )))
        }
    }

    /// Clean up expired authorization requests
    pub async fn cleanup_expired_requests(&self) {
        if let Some(timeout) = self.get_timeout_duration().await {
            let mut pending = self.pending_requests.lock().await;
            let now = Instant::now();
            
            let expired_keys: Vec<String> = pending.iter()
                .filter(|(_, req)| now.duration_since(req.created_at) > timeout)
                .map(|(key, _)| key.clone())
                .collect();

            for key in expired_keys {
                if let Some(expired_request) = pending.remove(&key) {
                    // Send a "No" response to unblock the waiting authorization check
                    if expired_request.response_sender.send(AuthorizationResponse::No).is_err() {
                        log::warn!("Failed to send timeout response for tool call: {}", key);
                    }
                    log::debug!("Authorization request timed out for tool call: {}", key);
                }
            }
        }
    }

    /// Get the timeout duration from the current configuration
    async fn get_timeout_duration(&self) -> Option<Duration> {
        let config = self.config.lock().await;
        config.timeout_seconds.map(|secs| Duration::from_secs(secs))
    }
}

#[async_trait]
impl AuthorizationHandler for PollingAuthorizationHandler {
    async fn request_authorization(
        &self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, AgentError> {
        let config = self.get_config().await;
        
        // Check if authorization is disabled or in always-allow mode
        if !config.enabled || config.mode == ToolAuthorizationMode::AlwaysAllow {
            return Ok(AuthorizationResponse::Yes);
        }

        // Check if in always-deny mode
        if config.mode == ToolAuthorizationMode::AlwaysDeny {
            return Ok(AuthorizationResponse::No);
        }

        // For Ask mode, create a pending authorization request
        let tool_call_id = request.context.tool_call_id.clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Create a channel to receive the response
        let (response_sender, response_receiver) = oneshot::channel();

        // Store the pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(tool_call_id.clone(), PendingAuthorizationRequest {
                response_sender,
                created_at: Instant::now(),
                context: request.context,
                _step_number: request.step_number,
            });
            
            log::debug!("Created pending authorization request for tool call: {}", tool_call_id);
        }

        // Wait for the response with timeout
        let timeout_duration = self.get_timeout_duration().await
            .unwrap_or(Duration::from_secs(30)); // Default 30 second timeout

        match tokio::time::timeout(timeout_duration, response_receiver).await {
            Ok(Ok(response)) => {
                log::debug!("Received authorization response for tool call {}: {:?}", tool_call_id, response);
                
                // If response is "All", update the configuration to always allow
                if response == AuthorizationResponse::All {
                    let mut new_config = self.get_config().await;
                    new_config.mode = ToolAuthorizationMode::AlwaysAllow;
                    self.set_config(new_config).await;
                    log::debug!("Updated authorization mode to AlwaysAllow based on 'All' response");
                }
                
                Ok(response)
            }
            Ok(Err(_)) => {
                // Channel was closed without sending a response
                self.cleanup_pending_request(&tool_call_id).await;
                Err(AgentError::AuthorizationFailed(
                    "Authorization response channel was closed".to_string()
                ))
            }
            Err(_) => {
                // Timeout occurred
                self.cleanup_pending_request(&tool_call_id).await;
                log::warn!("Authorization request timed out for tool call: {}", tool_call_id);
                Ok(AuthorizationResponse::No) // Default to deny on timeout
            }
        }
    }
}

impl PollingAuthorizationHandler {
    async fn cleanup_pending_request(&self, tool_call_id: &str) {
        let mut pending = self.pending_requests.lock().await;
        pending.remove(tool_call_id);
    }
}

impl Default for PollingAuthorizationHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PollingAuthorizationHandler {
    fn clone(&self) -> Self {
        Self {
            pending_requests: Arc::clone(&self.pending_requests),
            config: Arc::clone(&self.config),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_polling_authorization_handler_creation() {
        let handler = PollingAuthorizationHandler::new();
        
        let config = handler.get_config().await;
        assert_eq!(config.mode, ToolAuthorizationMode::Ask);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_authorization_config_update() {
        let handler = PollingAuthorizationHandler::new();
        
        let new_config = AuthorizationConfig::new(ToolAuthorizationMode::AlwaysAllow)
            .with_enabled(false);
        
        handler.set_config(new_config.clone()).await;
        let retrieved_config = handler.get_config().await;
        
        assert_eq!(retrieved_config.mode, ToolAuthorizationMode::AlwaysAllow);
        assert!(!retrieved_config.enabled);
    }

    #[tokio::test]
    async fn test_always_allow_mode() {
        let handler = PollingAuthorizationHandler::new();
        
        // Set to always allow mode
        let config = AuthorizationConfig::new(ToolAuthorizationMode::AlwaysAllow);
        handler.set_config(config).await;
        
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: json!({}),
            tool_call_id: Some("call_123".to_string()),
        };
        
        let request = AuthorizationRequest {
            context,
            step_number: 1,
            max_steps: 10,
        };
        
        let response = handler.request_authorization(request).await.unwrap();
        assert_eq!(response, AuthorizationResponse::Yes);
    }

    #[tokio::test]
    async fn test_always_deny_mode() {
        let handler = PollingAuthorizationHandler::new();
        
        // Set to always deny mode
        let config = AuthorizationConfig::new(ToolAuthorizationMode::AlwaysDeny);
        handler.set_config(config).await;
        
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: json!({}),
            tool_call_id: Some("call_123".to_string()),
        };
        
        let request = AuthorizationRequest {
            context,
            step_number: 1,
            max_steps: 10,
        };
        
        let response = handler.request_authorization(request).await.unwrap();
        assert_eq!(response, AuthorizationResponse::No);
    }

    #[tokio::test]
    async fn test_handle_authorization_response() {
        let handler = PollingAuthorizationHandler::new();
        
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: json!({}),
            tool_call_id: Some("call_123".to_string()),
        };
        
        let request = AuthorizationRequest {
            context,
            step_number: 1,
            max_steps: 10,
        };
        
        // Start the authorization request in a background task
        let handler_clone = handler.clone();
        let auth_task = tokio::spawn(async move {
            handler_clone.request_authorization(request).await
        });
        
        // Wait a bit for the request to be processed
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Send a response
        handler.handle_response(
            "call_123".to_string(),
            UiAuthorizationResponse::Approve,
        ).await.unwrap();
        
        // Check that the authorization request completed successfully
        let auth_result = auth_task.await.unwrap().unwrap();
        assert_eq!(auth_result, AuthorizationResponse::Yes);
    }

    #[tokio::test]
    async fn test_pending_authorizations() {
        let handler = PollingAuthorizationHandler::new();
        
        // No pending requests initially
        let pending = handler.get_pending_authorizations().await;
        assert!(pending.is_empty());
        
        // Create a request that will wait for authorization
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: json!({}),
            tool_call_id: Some("call_123".to_string()),
        };
        
        let request = AuthorizationRequest {
            context,
            step_number: 1,
            max_steps: 10,
        };
        
        // Start the authorization request in a background task
        let handler_clone = handler.clone();
        let _auth_task = tokio::spawn(async move {
            handler_clone.request_authorization(request).await
        });
        
        // Wait a bit for the request to be processed
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Check that we have a pending request
        let pending = handler.get_pending_authorizations().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].tool_call_id, "call_123");
        assert_eq!(pending[0].tool_call_name, "test_tool");
        assert_eq!(pending[0].status, AuthorizationStatus::Pending);
    }

    #[tokio::test]
    async fn test_cancel_authorization() {
        let handler = PollingAuthorizationHandler::new();
        
        // Try to cancel a non-existent authorization
        let result = handler.cancel_authorization("non_existent".to_string()).await;
        assert!(result.is_err());
        
        // Create a request that will wait for authorization
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: json!({}),
            tool_call_id: Some("call_123".to_string()),
        };
        
        let request = AuthorizationRequest {
            context,
            step_number: 1,
            max_steps: 10,
        };
        
        // Start the authorization request in a background task
        let handler_clone = handler.clone();
        let auth_task = tokio::spawn(async move {
            handler_clone.request_authorization(request).await
        });
        
        // Wait a bit for the request to be processed
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Cancel the authorization
        let result = handler.cancel_authorization("call_123".to_string()).await;
        assert!(result.is_ok());
        
        // Check that the authorization request was denied
        let auth_result = auth_task.await.unwrap().unwrap();
        assert_eq!(auth_result, AuthorizationResponse::No);
        
        // Check that there are no more pending requests
        let pending = handler.get_pending_authorizations().await;
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_approve_and_allow_response() {
        let handler = PollingAuthorizationHandler::new();
        
        // Verify initial config is Ask mode
        let initial_config = handler.get_config().await;
        assert_eq!(initial_config.mode, ToolAuthorizationMode::Ask);
        
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: json!({}),
            tool_call_id: Some("call_123".to_string()),
        };
        
        let request = AuthorizationRequest {
            context,
            step_number: 1,
            max_steps: 10,
        };
        
        // Start the authorization request in a background task
        let handler_clone = handler.clone();
        let auth_task = tokio::spawn(async move {
            handler_clone.request_authorization(request).await
        });
        
        // Wait a bit for the request to be processed
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Send an ApproveAndAllow response
        handler.handle_response(
            "call_123".to_string(),
            UiAuthorizationResponse::ApproveAndAllow,
        ).await.unwrap();
        
        // Check that the authorization request was approved
        let auth_result = auth_task.await.unwrap().unwrap();
        assert_eq!(auth_result, AuthorizationResponse::All);
        
        // Check that the config was updated to AlwaysAllow
        let updated_config = handler.get_config().await;
        assert_eq!(updated_config.mode, ToolAuthorizationMode::AlwaysAllow);
    }
}
