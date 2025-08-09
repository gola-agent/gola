// src/test_utils/mock_llm_server.rs
use axum::{routing::post, Json, Router};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

use crate::core_types::{LLMResponse, Message};
use crate::errors::AgentError;
use crate::llm::ToolMetadata; // Assuming this is pub
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct MockLLMRequestPayload {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<ToolMetadata>>,
}

#[derive(Clone)]
struct MockServerState {
    responses: Arc<Mutex<VecDeque<Result<LLMResponse, AgentError>>>>,
    requests: Arc<Mutex<Vec<MockLLMRequestPayload>>>,
}

impl MockServerState {
    fn new(responses: Vec<Result<LLMResponse, AgentError>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(VecDeque::from(responses))),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

async fn chat_completions_handler(
    axum::extract::State(state): axum::extract::State<MockServerState>,
    Json(payload): Json<MockLLMRequestPayload>,
) -> Result<Json<LLMResponse>, axum::http::StatusCode> {
    log::debug!("Mock LLM server received request: {:?}", payload.messages);
    if payload.tools.is_some() {
        log::debug!("Mock LLM server received tools: {:?}", payload.tools);
    }
    state.requests.lock().unwrap().push(payload);

    match state.responses.lock().unwrap().pop_front() {
        Some(Ok(resp)) => {
            log::debug!("Mock LLM server sending response: {:?}", resp.content);
            if resp.tool_calls.is_some() {
                log::debug!("Mock LLM server sending tool_calls: {:?}", resp.tool_calls);
            }
            Ok(Json(resp))
        }
        Some(Err(e)) => {
            // Simulate an LLM error. The actual error content might be lost here
            // unless we define a specific error structure for the API.
            // For now, just return a generic server error.
            log::error!("Mock LLM server simulating an error: {:?}", e);
            Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        }
        None => {
            log::error!("Mock LLM server ran out of responses!");
            // Respond with an error indicating no more responses are configured
            Err(axum::http::StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

pub struct MockLLMServer {
    addr: SocketAddr,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    pub recorded_requests: Arc<Mutex<Vec<MockLLMRequestPayload>>>,
}

impl MockLLMServer {
    pub async fn start(responses: Vec<Result<LLMResponse, AgentError>>) -> Self {
        let state = MockServerState::new(responses);
        let recorded_requests_clone = state.requests.clone();

        let app = Router::new()
            .route("/v1/chat/completions", post(chat_completions_handler))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap_or_else(|e| {
            panic!("Failed to bind mock server to 127.0.0.1:0. Error: {}", e);
        });
        let addr = listener.local_addr().unwrap();
        log::info!("Mock LLM server listening on {}", addr);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    log::info!("Mock LLM server shutting down gracefully.");
                })
                .await
                .unwrap_or_else(|e| {
                    // This might happen if the port is already in use during shutdown,
                    // or other server errors. For tests, often a simple log is fine.
                    log::error!("Mock LLM server error: {}", e);
                });
            log::info!("Mock LLM server task completed.");
        });

        MockLLMServer {
            addr,
            shutdown_tx,
            recorded_requests: recorded_requests_clone,
        }
    }

    pub fn address(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn shutdown(self) {
        if self.shutdown_tx.send(()).is_err() {
            log::warn!("Mock LLM server shutdown signal already sent or receiver dropped.");
        }
        // Give a small moment for the server to process shutdown.
        // A more robust way would be to await the join handle of the spawned tokio task,
        // but that requires passing it out or more complex state management.
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        log::info!("MockLLMServer shutdown method completed for address: {}", self.addr);
    }

    pub fn get_requests(&self) -> Vec<MockLLMRequestPayload> {
        self.recorded_requests.lock().unwrap().clone()
    }
}

