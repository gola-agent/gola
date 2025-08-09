//! Mock Authorization implementations for testing

use async_trait::async_trait;
use gola_core::guardrails::{AuthorizationHandler, AuthorizationRequest, AuthorizationResponse, AuthorizationMode};
use gola_core::errors::AgentError;
use std::sync::Arc;
use std::sync::Mutex;

/// Mock authorization handler for testing
pub struct MockAuthorizationHandler {
    mode: AuthorizationMode,
    responses: Arc<Mutex<Vec<bool>>>,
    call_count: Arc<Mutex<usize>>,
}

impl MockAuthorizationHandler {
    pub fn new(mode: AuthorizationMode) -> Self {
        Self {
            mode,
            responses: Arc::new(Mutex::new(vec![true])), // Default to approving
            call_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn with_responses(mode: AuthorizationMode, responses: Vec<bool>) -> Self {
        Self {
            mode,
            responses: Arc::new(Mutex::new(responses)),
            call_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn always_approve() -> Self {
        Self::new(AuthorizationMode::None)
    }

    pub fn always_deny() -> Self {
        Self::with_responses(AuthorizationMode::SSE, vec![false])
    }

    pub fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl AuthorizationHandler for MockAuthorizationHandler {
    fn mode(&self) -> AuthorizationMode {
        self.mode.clone()
    }

    async fn request_authorization(
        &mut self,
        request: AuthorizationRequest,
    ) -> Result<AuthorizationResponse, AgentError> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;

        let responses = self.responses.lock().unwrap();
        let index = (*count - 1) % responses.len();
        let approved = responses[index];

        Ok(AuthorizationResponse {
            approved,
            modified_request: if approved { Some(request) } else { None },
        })
    }

    async fn close(&mut self) -> Result<(), AgentError> {
        Ok(())
    }
}