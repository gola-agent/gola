//! Server-Sent Events (SSE) implementation for ag-ui protocol.

use axum::response::sse::{Event as AxumEvent, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use futures_util::Stream;
use gola_ag_ui_types::Event;
use pin_project_lite::pin_project;
use serde_json;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::error::{Result, ServerError};

/// An SSE event that can be sent to clients.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// Event type (optional)
    pub event_type: Option<String>,
    /// Event data
    pub data: String,
    /// Event ID (optional)
    pub id: Option<String>,
    /// Retry interval in milliseconds (optional)
    pub retry: Option<u64>,
}

impl SseEvent {
    /// Create a new SSE event with just data.
    pub fn new(data: impl Into<String>) -> Self {
        Self {
            event_type: None,
            data: data.into(),
            id: None,
            retry: None,
        }
    }

    /// Create a new SSE event with event type and data.
    pub fn with_type(event_type: impl Into<String>, data: impl Into<String>) -> Self {
        Self {
            event_type: Some(event_type.into()),
            data: data.into(),
            id: None,
            retry: None,
        }
    }

    /// Set the event ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the retry interval.
    pub fn with_retry(mut self, retry: u64) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Create an SSE event from an ag-ui Event.
    pub fn from_ag_ui_event(event: &Event) -> Result<Self> {
        let data = serde_json::to_string(event).map_err(ServerError::Json)?;

        let event_type = match event {
            Event::TextMessageStart(_) => "TEXT_MESSAGE_START",
            Event::TextMessageContent(_) => "TEXT_MESSAGE_CONTENT",
            Event::TextMessageEnd(_) => "TEXT_MESSAGE_END",
            Event::TextMessageChunk(_) => "TEXT_MESSAGE_CHUNK",
            Event::ToolCallStart(_) => "TOOL_CALL_START",
            Event::ToolCallArgs(_) => "TOOL_CALL_ARGS",
            Event::ToolCallEnd(_) => "TOOL_CALL_END",
            Event::ToolCallChunk(_) => "TOOL_CALL_CHUNK",
            Event::ToolAuthorizationRequest(_) => "TOOL_AUTHORIZATION_REQUEST",
            Event::ToolAuthorizationResponse(_) => "TOOL_AUTHORIZATION_RESPONSE",
            Event::AuthorizationStatus(_) => "AUTHORIZATION_STATUS",
            Event::StateSnapshot(_) => "STATE_SNAPSHOT",
            Event::StateDelta(_) => "STATE_DELTA",
            Event::MessagesSnapshot(_) => "MESSAGES_SNAPSHOT",
            Event::RunStarted(_) => "RUN_STARTED",
            Event::RunFinished(_) => "RUN_FINISHED",
            Event::RunError(_) => "RUN_ERROR",
            Event::StepStarted(_) => "STEP_STARTED",
            Event::StepFinished(_) => "STEP_FINISHED",
            Event::Custom(_) => "CUSTOM",
            Event::Raw(_) => "RAW",
        };

        Ok(Self::with_type(event_type, data))
    }
}

impl From<SseEvent> for AxumEvent {
    fn from(event: SseEvent) -> Self {
        let mut axum_event = AxumEvent::default().data(event.data);

        if let Some(event_type) = event.event_type {
            axum_event = axum_event.event(event_type);
        }

        if let Some(id) = event.id {
            axum_event = axum_event.id(id);
        }

        if let Some(retry) = event.retry {
            axum_event = axum_event.retry(Duration::from_millis(retry));
        }

        axum_event
    }
}

pin_project! {
    /// A stream wrapper that converts ag-ui Events to SSE events.
    pub struct SseStream<S> {
        #[pin]
        inner: S,
    }
}

impl<S> SseStream<S> {
    /// Create a new SSE stream wrapper.
    pub fn new(stream: S) -> Self {
        Self { inner: stream }
    }
}

impl<S> Stream for SseStream<S>
where
    S: Stream<Item = Event>,
{
    type Item = std::result::Result<AxumEvent, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match this.inner.poll_next(cx) {
            Poll::Ready(Some(event)) => {
                match SseEvent::from_ag_ui_event(&event) {
                    Ok(sse_event) => Poll::Ready(Some(Ok(sse_event.into()))),
                    Err(e) => {
                        log::error!("Failed to convert ag-ui event to SSE: {}", e);
                        // Create an error event
                        let error_event = SseEvent::with_type(
                            "error",
                            format!(r#"{{"error": "Failed to serialize event: {}"}}"#, e),
                        );
                        Poll::Ready(Some(Ok(error_event.into())))
                    }
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Create an SSE response from a stream of ag-ui Events.
pub fn create_sse_response<S>(stream: S) -> Response
where
    S: Stream<Item = Event> + Send + 'static,
{
    let sse_stream = SseStream::new(stream);

    Sse::new(sse_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(30))
                .text("keep-alive"),
        )
        .into_response()
}

/// Create an SSE response with custom keep-alive settings.
pub fn create_sse_response_with_keepalive<S>(
    stream: S,
    keepalive_interval: Duration,
    keepalive_text: impl Into<String>,
) -> Response
where
    S: Stream<Item = Event> + Send + 'static,
{
    let sse_stream = SseStream::new(stream);

    Sse::new(sse_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(keepalive_interval)
                .text(keepalive_text.into()),
        )
        .into_response()
}

/// Utility function to create a simple text SSE event.
pub fn text_event(data: impl Into<String>) -> AxumEvent {
    SseEvent::new(data).into()
}

/// Utility function to create a typed SSE event.
pub fn typed_event(event_type: impl Into<String>, data: impl Into<String>) -> AxumEvent {
    SseEvent::with_type(event_type, data).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{stream, StreamExt as _};
    use gola_ag_ui_types::{Event, TextMessageStartEvent}; // Import StreamExt for .next()

    #[test]
    fn test_sse_event_creation() {
        let event = SseEvent::new("test data");
        assert_eq!(event.data, "test data");
        assert!(event.event_type.is_none());
        assert!(event.id.is_none());
        assert!(event.retry.is_none());
    }

    #[test]
    fn test_sse_event_with_type() {
        let event = SseEvent::with_type("test_type", "test data");
        assert_eq!(event.data, "test data");
        assert_eq!(event.event_type, Some("test_type".to_string()));
    }

    #[test]
    fn test_sse_event_from_ag_ui_event() {
        let ag_ui_event = Event::TextMessageStart(TextMessageStartEvent::new("msg-1".to_string()));

        let sse_event = SseEvent::from_ag_ui_event(&ag_ui_event).unwrap();
        assert_eq!(sse_event.event_type, Some("TEXT_MESSAGE_START".to_string()));
        assert!(sse_event.data.contains("msg-1"));
    }

    #[tokio::test]
    async fn test_sse_stream() {
        let events = vec![
            Event::TextMessageStart(TextMessageStartEvent::new("msg-1".to_string())),
            Event::TextMessageContent(gola_ag_ui_types::TextMessageContentEvent::new(
                "msg-1".to_string(),
                "World".to_string(),
            )),
        ];

        let stream = stream::iter(events);
        let mut sse_stream = SseStream::new(stream);

        let first = sse_stream.next().await.unwrap().unwrap();
        let second = sse_stream.next().await.unwrap().unwrap();

        // Verify the events were converted properly
        // Format the AxumEvent to its string representation for checking content
        assert!(format!("{:?}", first).contains("msg-1"));
        assert!(format!("{:?}", second).contains("World"));
    }
}
