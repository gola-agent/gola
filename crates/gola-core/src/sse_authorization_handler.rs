//! SSE-based authorization handler for connecting gola-core agent to gola-ag-ui-server

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::errors::AgentError;
use crate::guardrails::{AuthorizationHandler, AuthorizationRequest, AuthorizationResponse, AuthorizationContext};
use gola_ag_ui_types::{
    ToolAuthorizationRequestEvent, ToolAuthorizationResponseEvent, 
    AuthorizationConfig, PendingAuthorization, AuthorizationStatus,
    ToolAuthorizationMode, AuthorizationResponse as UiAuthorizationResponse
};

/// SSE-based authorization handler that sends authorization requests via events
/// and waits for responses from the UI client
pub struct SseAuthorizationHandler {
    event_sender: mpsc::UnboundedSender<ToolAuthorizationRequestEvent>,
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

impl SseAuthorizationHandler {
    pub fn new(event_sender: mpsc::UnboundedSender<ToolAuthorizationRequestEvent>) -> Self {
        Self {
            event_sender,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            config: Arc::new(Mutex::new(AuthorizationConfig::default())),
        }
    }

    pub async fn handle_response(&self, response: ToolAuthorizationResponseEvent) -> Result<(), AgentError> {
        let mut pending = self.pending_requests.lock().await;
        
        if let Some(pending_request) = pending.remove(&response.tool_call_id) {
            let auth_response = match response.response {
                UiAuthorizationResponse::Approve => AuthorizationResponse::Yes,
                UiAuthorizationResponse::Deny => AuthorizationResponse::No,
                UiAuthorizationResponse::ApproveAndAllow => AuthorizationResponse::All,
            };

            // Send the response back to the waiting authorization check
            if pending_request.response_sender.send(auth_response).is_err() {
                log::warn!("Failed to send authorization response for tool call: {}", response.tool_call_id);
            }
        } else {
            log::warn!("Received authorization response for unknown tool call: {}", response.tool_call_id);
        }

        Ok(())
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
                    (req.created_at + timeout).elapsed().as_secs() as i64
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
            log::info!("Cancelled authorization request for tool call: {}", tool_call_id);
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
                    log::info!("Authorization request timed out for tool call: {}", key);
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
impl AuthorizationHandler for SseAuthorizationHandler {
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

        // For Ask mode, send SSE event and wait for response
        let tool_call_id = request.context.tool_call_id.clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Create the authorization request event
        let auth_request_event = ToolAuthorizationRequestEvent::with_description(
            tool_call_id.clone(),
            request.context.tool_name.clone(),
            request.context.tool_arguments.to_string(),
            request.context.tool_description.clone(),
        );

        // Send the event via SSE
        if let Err(e) = self.event_sender.send(auth_request_event) {
            return Err(AgentError::AuthorizationFailed(format!(
                "Failed to send authorization request event: {}", e
            )));
        }

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
        }

        // Wait for the response with timeout
        let timeout_duration = self.get_timeout_duration().await
            .unwrap_or(Duration::from_secs(30)); // Default 30 second timeout

        match tokio::time::timeout(timeout_duration, response_receiver).await {
            Ok(Ok(response)) => {
                log::info!("Received authorization response for tool call {}: {:?}", tool_call_id, response);
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

impl SseAuthorizationHandler {
    async fn cleanup_pending_request(&self, tool_call_id: &str) {
        let mut pending = self.pending_requests.lock().await;
        pending.remove(tool_call_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_sse_authorization_handler_creation() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = SseAuthorizationHandler::new(sender);
        
        let config = handler.get_config().await;
        assert_eq!(config.mode, ToolAuthorizationMode::Ask);
        assert!(config.enabled);
    }

    #[tokio::test]
    async fn test_authorization_config_update() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = SseAuthorizationHandler::new(sender);
        
        let new_config = AuthorizationConfig::new(ToolAuthorizationMode::AlwaysAllow)
            .with_enabled(false);
        
        handler.set_config(new_config.clone()).await;
        let retrieved_config = handler.get_config().await;
        
        assert_eq!(retrieved_config.mode, ToolAuthorizationMode::AlwaysAllow);
        assert!(!retrieved_config.enabled);
    }

    #[tokio::test]
    async fn test_always_allow_mode() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = SseAuthorizationHandler::new(sender);
        
        // Set to always allow mode
        let config = AuthorizationConfig::new(ToolAuthorizationMode::AlwaysAllow);
        handler.set_config(config).await;
        
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: serde_json::json!({}),
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
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = SseAuthorizationHandler::new(sender);
        
        // Set to always deny mode
        let config = AuthorizationConfig::new(ToolAuthorizationMode::AlwaysDeny);
        handler.set_config(config).await;
        
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: serde_json::json!({}),
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
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let handler = SseAuthorizationHandler::new(sender);
        
        let context = AuthorizationContext {
            tool_name: "test_tool".to_string(),
            tool_description: "A test tool".to_string(),
            tool_arguments: serde_json::json!({}),
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
        
        // Wait for the SSE event to be sent
        let sse_event = receiver.recv().await.unwrap();
        assert_eq!(sse_event.tool_call_id, "call_123");
        assert_eq!(sse_event.tool_call_name, "test_tool");
        
        // Send a response
        let response_event = ToolAuthorizationResponseEvent::new(
            "call_123".to_string(),
            UiAuthorizationResponse::Approve,
        );
        
        handler.handle_response(response_event).await.unwrap();
        
        // Check that the authorization request completed successfully
        let auth_result = auth_task.await.unwrap().unwrap();
        assert_eq!(auth_result, AuthorizationResponse::Yes);
    }

    #[tokio::test]
    async fn test_pending_authorizations() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = SseAuthorizationHandler::new(sender);
        
        // No pending requests initially
        let pending = handler.get_pending_authorizations().await;
        assert!(pending.is_empty());
        
        // TODO: Add test for actual pending requests
        // This would require setting up a more complex test scenario
    }

    #[tokio::test]
    async fn test_cancel_authorization() {
        let (sender, _receiver) = mpsc::unbounded_channel();
        let handler = SseAuthorizationHandler::new(sender);
        
        // Try to cancel a non-existent authorization
        let result = handler.cancel_authorization("non_existent".to_string()).await;
        assert!(result.is_err());
    }
}

// Implement Clone for SseAuthorizationHandler
impl Clone for SseAuthorizationHandler {
    fn clone(&self) -> Self {
        Self {
            event_sender: self.event_sender.clone(),
            pending_requests: Arc::clone(&self.pending_requests),
            config: Arc::clone(&self.config),
        }
    }
}
