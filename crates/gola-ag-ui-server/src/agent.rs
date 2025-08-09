//! Agent handler trait and utilities for the ag-ui server.

use async_trait::async_trait;
use futures_util::Stream;
use gola_ag_ui_types::{
    AuthorizationConfig, Event, PendingAuthorization, RunAgentInput, ToolAuthorizationResponseEvent,
};
use gola_ag_ui_types::{RunErrorEvent, RunFinishedEvent, RunStartedEvent};
use gola_ag_ui_types::{
    TextMessageChunkEvent, TextMessageContentEvent, TextMessageEndEvent, TextMessageStartEvent,
};
use serde::{Deserialize, Serialize};
use std::pin::Pin;

use crate::error::{Result, ServerError};

/// Type alias for agent event streams.
pub type AgentStream = Pin<Box<dyn Stream<Item = Event> + Send>>;

/// Trait for handling agent inputs and producing event streams.
///
/// Implementors of this trait define how agent inputs are processed
/// and converted into streams of ag-ui events.
#[async_trait]
pub trait AgentHandler: Send + Sync + Clone + 'static {
    /// Handle an agent input and return a stream of events.
    ///
    /// This method should process the input and return a stream that emits
    /// ag-ui events representing the agent's response. The stream should
    /// emit events in real-time as the agent processes the input.
    ///
    /// # Arguments
    ///
    /// * `input` - The agent input to process
    ///
    /// # Returns
    ///
    /// A stream of ag-ui events representing the agent's response.
    ///
    /// # Errors
    ///
    /// Returns an error if the input cannot be processed or if there's
    /// an issue creating the event stream.
    async fn handle_input(&self, input: RunAgentInput) -> Result<AgentStream>;

    /// Optional method to validate input before processing.
    ///
    /// The default implementation always returns Ok(()).
    /// Override this method to add custom validation logic.
    async fn validate_input(&self, input: &RunAgentInput) -> Result<()> {
        // Default validation - check for required fields
        if input.messages.is_empty() {
            return Err(ServerError::invalid_input("Messages cannot be empty"));
        }

        Ok(())
    }

    /// Optional method called when a client connects.
    ///
    /// The default implementation does nothing.
    /// Override this method to add custom connection handling.
    async fn on_connect(&self) -> Result<()> {
        Ok(())
    }

    /// Optional method called when a client disconnects.
    ///
    /// The default implementation does nothing.
    /// Override this method to add custom disconnection handling.
    async fn on_disconnect(&self) -> Result<()> {
        Ok(())
    }

    /// Get the current authorization configuration.
    ///
    /// The default implementation returns a default configuration with Ask mode.
    /// Override this method to provide custom authorization settings.
    async fn get_authorization_config(&self) -> Result<AuthorizationConfig> {
        Ok(AuthorizationConfig::default())
    }

    /// Set the authorization configuration.
    ///
    /// The default implementation returns an error indicating the operation is not supported.
    /// Override this method to provide authorization configuration functionality.
    async fn set_authorization_config(&self, _config: AuthorizationConfig) -> Result<()> {
        Err(ServerError::invalid_input(
            "Authorization configuration not supported by this agent",
        ))
    }

    /// Handle an authorization response from the user.
    ///
    /// This method is called when the user responds to a tool authorization request.
    /// The default implementation returns an error indicating the operation is not supported.
    /// Override this method to handle authorization responses.
    async fn handle_authorization_response(
        &self,
        _response: ToolAuthorizationResponseEvent,
    ) -> Result<()> {
        Err(ServerError::invalid_input(
            "Authorization handling not supported by this agent",
        ))
    }

    /// Get pending authorization requests.
    ///
    /// The default implementation returns an empty list.
    /// Override this method to provide pending authorization information.
    async fn get_pending_authorizations(&self) -> Result<Vec<PendingAuthorization>> {
        Ok(vec![])
    }

    /// Cancel a pending authorization request.
    ///
    /// The default implementation returns an error indicating the operation is not supported.
    /// Override this method to provide authorization cancellation functionality.
    async fn cancel_authorization(&self, _tool_call_id: String) -> Result<()> {
        Err(ServerError::invalid_input(
            "Authorization cancellation not supported by this agent",
        ))
    }

    /// Optional method to get agent metadata.
    ///
    /// The default implementation returns basic metadata.
    /// Override this method to provide custom agent information.
    async fn get_metadata(&self) -> Result<AgentMetadata> {
        Ok(AgentMetadata::default())
    }

    /// Get agent memory statistics.
    ///
    /// The default implementation returns None, indicating memory stats are not available.
    /// Override this method to provide memory statistics.
    async fn get_memory_stats(&self) -> Result<Option<serde_json::Value>> {
        Ok(None)
    }

    /// Clear agent memory.
    ///
    /// The default implementation returns an error indicating the operation is not supported.
    /// Override this method to provide memory clearing functionality.
    async fn clear_memory(&self) -> Result<()> {
        Err(ServerError::invalid_input(
            "Memory clearing not supported by this agent",
        ))
    }

    /// Get available tools for this agent.
    ///
    /// The default implementation returns an empty list.
    /// Override this method to provide the tools available to this agent.
    async fn get_available_tools(&self) -> Result<Vec<gola_ag_ui_types::Tool>> {
        Ok(vec![])
    }
}

/// Metadata about an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
    /// Agent version
    pub version: String,
    /// Supported capabilities
    pub capabilities: Vec<String>,
    /// Maximum context length
    pub max_context_length: Option<usize>,
    /// Whether the agent supports streaming
    pub supports_streaming: bool,
    /// Whether the agent supports tool calls
    pub supports_tools: bool,
    /// Whether the agent supports authorization guardrails
    pub supports_authorization: bool,
    /// List of available tools
    #[serde(default)]
    pub available_tools: Vec<gola_ag_ui_types::Tool>,
}

impl Default for AgentMetadata {
    fn default() -> Self {
        Self {
            name: "Generic Agent".to_string(),
            description: "A generic ag-ui compatible agent".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec!["text_generation".to_string()],
            max_context_length: None,
            supports_streaming: true,
            supports_tools: false,
            supports_authorization: false,
            available_tools: vec![],
        }
    }
}

impl AgentMetadata {
    /// Create new agent metadata.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            ..Default::default()
        }
    }

    /// Set the agent version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Add a capability.
    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    /// Set multiple capabilities.
    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Set the maximum context length.
    pub fn with_max_context_length(mut self, max_length: usize) -> Self {
        self.max_context_length = Some(max_length);
        self
    }

    /// Enable or disable streaming support.
    pub fn with_streaming(mut self, supports_streaming: bool) -> Self {
        self.supports_streaming = supports_streaming;
        self
    }

    /// Enable or disable tool support.
    pub fn with_tools(mut self, supports_tools: bool) -> Self {
        self.supports_tools = supports_tools;
        self
    }

    /// Enable or disable authorization support.
    pub fn with_authorization(mut self, supports_authorization: bool) -> Self {
        self.supports_authorization = supports_authorization;
        self
    }

    /// Set available tools and update supports_tools flag accordingly.
    pub fn with_available_tools(mut self, tools: Vec<gola_ag_ui_types::Tool>) -> Self {
        self.supports_tools = !tools.is_empty();
        self.available_tools = tools;
        self
    }
}

/// Utility functions for creating common event streams.
pub mod streams {
    use super::*;
    use async_stream::stream;
    use std::time::Duration;
    use tokio::time::sleep;

    /// Create a simple text response stream.
    pub fn text_response(content: impl Into<String>) -> AgentStream {
        let content = content.into();
        let message_id = uuid::Uuid::new_v4().to_string();
        let thread_id = uuid::Uuid::new_v4().to_string();
        let run_id = uuid::Uuid::new_v4().to_string();

        Box::pin(stream! {
            yield Event::RunStarted(RunStartedEvent::new(thread_id.clone(), run_id.clone()));
            yield Event::TextMessageStart(TextMessageStartEvent::new(message_id.clone()));
            yield Event::TextMessageContent(TextMessageContentEvent::new(message_id.clone(), content.clone()));
            yield Event::TextMessageEnd(TextMessageEndEvent::new(message_id));
            yield Event::RunFinished(RunFinishedEvent::new(thread_id, run_id));
        })
    }

    /// Create a streaming text response that emits content in chunks.
    pub fn streaming_text_response(
        content: impl Into<String>,
        chunk_delay: Duration,
    ) -> AgentStream {
        let content = content.into();
        let message_id = uuid::Uuid::new_v4().to_string();
        let thread_id = uuid::Uuid::new_v4().to_string();
        let run_id = uuid::Uuid::new_v4().to_string();

        Box::pin(stream! {
            yield Event::RunStarted(RunStartedEvent::new(thread_id.clone(), run_id.clone()));
            yield Event::TextMessageStart(TextMessageStartEvent::new(message_id.clone()));

            // Split content into words and stream them
            let words: Vec<String> = content.split_whitespace().map(|s| s.to_string()).collect();
            for word in words {
                yield Event::TextMessageChunk(TextMessageChunkEvent {
                    timestamp: None,
                    raw_event: None,
                    message_id: Some(message_id.clone()),
                    role: Some("assistant".to_string()),
                    delta: Some(format!("{} ", word)),
                });
                sleep(chunk_delay).await;
            }

            yield Event::TextMessageEnd(TextMessageEndEvent::new(message_id));
            yield Event::RunFinished(RunFinishedEvent::new(thread_id, run_id));
        })
    }

    /// Create an authorization request event stream.
    pub fn authorization_request(
        tool_call_id: String,
        tool_call_name: String,
        tool_call_args: String,
        description: Option<String>,
    ) -> AgentStream {
        use gola_ag_ui_types::{Event, ToolAuthorizationRequestEvent};

        Box::pin(stream! {
            let request = if let Some(desc) = description {
                ToolAuthorizationRequestEvent::with_description(
                    tool_call_id,
                    tool_call_name,
                    tool_call_args,
                    desc,
                )
            } else {
                ToolAuthorizationRequestEvent::new(
                    tool_call_id,
                    tool_call_name,
                    tool_call_args,
                )
            };
            yield Event::ToolAuthorizationRequest(request);
        })
    }

    /// Create an authorization status event stream.
    pub fn authorization_status(
        tool_call_id: String,
        status: gola_ag_ui_types::AuthorizationStatus,
        message: Option<String>,
    ) -> AgentStream {
        use gola_ag_ui_types::{AuthorizationStatusEvent, Event};

        Box::pin(stream! {
            let status_event = if let Some(msg) = message {
                AuthorizationStatusEvent::with_message(tool_call_id, status, msg)
            } else {
                AuthorizationStatusEvent::new(tool_call_id, status)
            };
            yield Event::AuthorizationStatus(status_event);
        })
    }

    /// Create an error response stream.
    pub fn error_response(error: impl Into<String>) -> AgentStream {
        let error_msg = error.into();

        Box::pin(stream! {
            yield Event::RunError(RunErrorEvent::new(error_msg));
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use gola_ag_ui_types::Message;
    use std::time::Duration; // Import Duration

    #[derive(Clone)]
    struct TestAgent;

    #[async_trait]
    impl AgentHandler for TestAgent {
        async fn handle_input(&self, input: RunAgentInput) -> Result<AgentStream> {
            let last_message = input.messages.last().unwrap();
            let response = format!("Echo: {}", last_message.content().unwrap_or(""));
            Ok(streams::text_response(response))
        }
    }

    #[tokio::test]
    async fn test_agent_handler() {
        let agent = TestAgent;
        let input = RunAgentInput::new(
            "thread-1".to_string(),
            "run-1".to_string(),
            serde_json::json!({}),
            vec![Message::new_user(
                "msg-1".to_string(),
                "Hello, world!".to_string(),
            )],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        let stream = agent.handle_input(input).await.unwrap(); // removed mut
        let events: Vec<_> = stream.collect().await;

        assert!(!events.is_empty());
        // Should have at least run started, message start, content, end, and run finished
        assert!(events.len() >= 5);
    }

    #[test]
    fn test_agent_metadata() {
        let metadata = AgentMetadata::new("Test Agent", "A test agent")
            .with_version("2.0.0")
            .with_capability("test_capability")
            .with_max_context_length(1000)
            .with_streaming(true)
            .with_tools(true);

        assert_eq!(metadata.name, "Test Agent");
        assert_eq!(metadata.version, "2.0.0");
        assert!(metadata
            .capabilities
            .contains(&"test_capability".to_string()));
        assert_eq!(metadata.max_context_length, Some(1000));
        assert!(metadata.supports_streaming);
        assert!(metadata.supports_tools);
    }

    #[tokio::test]
    async fn test_streaming_text_response() {
        let stream = streams::streaming_text_response("Hello world", Duration::from_millis(1)); // Removed mut
        let events: Vec<_> = stream.collect().await;

        // Should have run started, message start, chunks, message end, run finished
        assert!(events.len() >= 5);

        // Check for chunk events
        let chunk_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::TextMessageChunk(_)))
            .collect();
        assert!(!chunk_events.is_empty());
    }
}
