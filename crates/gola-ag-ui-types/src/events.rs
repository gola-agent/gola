//! Event types for the ag-ui specification.

use crate::authorization::{
    AuthorizationStatusEvent, ToolAuthorizationRequestEvent, ToolAuthorizationResponseEvent,
};
use crate::types::{Message, State};
use serde::{Deserialize, Serialize};

/// The type of event in the ag-ui protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    /// Start of a text message.
    TextMessageStart,
    /// Content of a text message.
    TextMessageContent,
    /// End of a text message.
    TextMessageEnd,
    /// Chunk of a text message.
    TextMessageChunk,
    /// Start of a tool call.
    ToolCallStart,
    /// Arguments for a tool call.
    ToolCallArgs,
    /// End of a tool call.
    ToolCallEnd,
    /// Chunk of a tool call.
    ToolCallChunk,
    /// Request for tool authorization.
    ToolAuthorizationRequest,
    /// Response to tool authorization request.
    ToolAuthorizationResponse,
    /// Status update for tool authorization.
    AuthorizationStatus,
    /// Snapshot of the current state.
    StateSnapshot,
    /// Delta change to the state.
    StateDelta,
    /// Snapshot of all messages.
    MessagesSnapshot,
    /// Raw event from the underlying system.
    Raw,
    /// Custom event defined by the implementation.
    Custom,
    /// A run has started.
    RunStarted,
    /// A run has finished.
    RunFinished,
    /// A run encountered an error.
    RunError,
    /// A step has started.
    StepStarted,
    /// A step has finished.
    StepFinished,
}

/// Base event structure for all events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseEvent {
    /// The type of event.
    #[serde(rename = "type")]
    pub event_type: EventType,
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
}

/// Event indicating the start of a text message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMessageStartEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the message.
    #[serde(rename = "messageId")]
    pub message_id: String,
    /// The role of the message sender (always "assistant" for this event).
    pub role: String,
}

/// Event containing a piece of text message content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMessageContentEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the message.
    #[serde(rename = "messageId")]
    pub message_id: String,
    /// The content delta (must not be empty).
    pub delta: String,
}

/// Event indicating the end of a text message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMessageEndEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the message.
    #[serde(rename = "messageId")]
    pub message_id: String,
}

/// Event containing a chunk of text message content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMessageChunkEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the message (optional).
    #[serde(rename = "messageId", skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// The role of the message sender (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The content delta (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
}

/// Event indicating the start of a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallStartEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the tool call.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// The name of the tool being called.
    #[serde(rename = "toolCallName")]
    pub tool_call_name: String,
    /// The ID of the parent message (optional).
    #[serde(rename = "parentMessageId", skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
}

/// Event containing tool call arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallArgsEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the tool call.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    /// The arguments delta.
    pub delta: String,
}

/// Event indicating the end of a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallEndEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the tool call.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
}

/// Event containing a chunk of tool call content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallChunkEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The ID of the tool call (optional).
    #[serde(rename = "toolCallId", skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// The name of the tool being called (optional).
    #[serde(rename = "toolCallName", skip_serializing_if = "Option::is_none")]
    pub tool_call_name: Option<String>,
    /// The ID of the parent message (optional).
    #[serde(rename = "parentMessageId", skip_serializing_if = "Option::is_none")]
    pub parent_message_id: Option<String>,
    /// The content delta (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
}

/// Event containing a snapshot of the current state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshotEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The state snapshot.
    pub snapshot: State,
}

/// Event containing a delta change to the state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateDeltaEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The state delta as a JSON Patch (RFC 6902).
    pub delta: Vec<serde_json::Value>,
}

/// Event containing a snapshot of all messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessagesSnapshotEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The messages snapshot.
    pub messages: Vec<Message>,
}

/// Event containing a raw event from the underlying system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The raw event data.
    pub event: serde_json::Value,
    /// The source of the raw event (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Event containing a custom event defined by the implementation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The name of the custom event.
    pub name: String,
    /// The value of the custom event.
    pub value: serde_json::Value,
}

/// Event indicating that a run has started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStartedEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The thread ID.
    #[serde(rename = "threadId")]
    pub thread_id: String,
    /// The run ID.
    #[serde(rename = "runId")]
    pub run_id: String,
}

/// Event indicating that a run has finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFinishedEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The thread ID.
    #[serde(rename = "threadId")]
    pub thread_id: String,
    /// The run ID.
    #[serde(rename = "runId")]
    pub run_id: String,
}

/// Event indicating that a run encountered an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunErrorEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The error message.
    pub message: String,
    /// The error code (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Event indicating that a step has started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepStartedEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The name of the step.
    #[serde(rename = "stepName")]
    pub step_name: String,
}

/// Event indicating that a step has finished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepFinishedEvent {
    /// Timestamp when the event occurred (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    /// Raw event data from the underlying system (optional).
    #[serde(rename = "rawEvent", skip_serializing_if = "Option::is_none")]
    pub raw_event: Option<serde_json::Value>,
    /// The name of the step.
    #[serde(rename = "stepName")]
    pub step_name: String,
}

/// An event in the ag-ui protocol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Event {
    /// Start of a text message.
    TextMessageStart(TextMessageStartEvent),
    /// Content of a text message.
    TextMessageContent(TextMessageContentEvent),
    /// End of a text message.
    TextMessageEnd(TextMessageEndEvent),
    /// Chunk of a text message.
    TextMessageChunk(TextMessageChunkEvent),
    /// Start of a tool call.
    ToolCallStart(ToolCallStartEvent),
    /// Arguments for a tool call.
    ToolCallArgs(ToolCallArgsEvent),
    /// End of a tool call.
    ToolCallEnd(ToolCallEndEvent),
    /// Chunk of a tool call.
    ToolCallChunk(ToolCallChunkEvent),
    /// Request for tool authorization.
    ToolAuthorizationRequest(ToolAuthorizationRequestEvent),
    /// Response to tool authorization request.
    ToolAuthorizationResponse(ToolAuthorizationResponseEvent),
    /// Status update for tool authorization.
    AuthorizationStatus(AuthorizationStatusEvent),
    /// Snapshot of the current state.
    StateSnapshot(StateSnapshotEvent),
    /// Delta change to the state.
    StateDelta(StateDeltaEvent),
    /// Snapshot of all messages.
    MessagesSnapshot(MessagesSnapshotEvent),
    /// Raw event from the underlying system.
    Raw(RawEvent),
    /// Custom event defined by the implementation.
    Custom(CustomEvent),
    /// A run has started.
    RunStarted(RunStartedEvent),
    /// A run has finished.
    RunFinished(RunFinishedEvent),
    /// A run encountered an error.
    RunError(RunErrorEvent),
    /// A step has started.
    StepStarted(StepStartedEvent),
    /// A step has finished.
    StepFinished(StepFinishedEvent),
}

impl Event {
    /// Get the event type.
    pub fn event_type(&self) -> EventType {
        match self {
            Event::TextMessageStart(_) => EventType::TextMessageStart,
            Event::TextMessageContent(_) => EventType::TextMessageContent,
            Event::TextMessageEnd(_) => EventType::TextMessageEnd,
            Event::TextMessageChunk(_) => EventType::TextMessageChunk,
            Event::ToolCallStart(_) => EventType::ToolCallStart,
            Event::ToolCallArgs(_) => EventType::ToolCallArgs,
            Event::ToolCallEnd(_) => EventType::ToolCallEnd,
            Event::ToolCallChunk(_) => EventType::ToolCallChunk,
            Event::ToolAuthorizationRequest(_) => EventType::ToolAuthorizationRequest,
            Event::ToolAuthorizationResponse(_) => EventType::ToolAuthorizationResponse,
            Event::AuthorizationStatus(_) => EventType::AuthorizationStatus,
            Event::StateSnapshot(_) => EventType::StateSnapshot,
            Event::StateDelta(_) => EventType::StateDelta,
            Event::MessagesSnapshot(_) => EventType::MessagesSnapshot,
            Event::Raw(_) => EventType::Raw,
            Event::Custom(_) => EventType::Custom,
            Event::RunStarted(_) => EventType::RunStarted,
            Event::RunFinished(_) => EventType::RunFinished,
            Event::RunError(_) => EventType::RunError,
            Event::StepStarted(_) => EventType::StepStarted,
            Event::StepFinished(_) => EventType::StepFinished,
        }
    }

    /// Get the timestamp of the event, if any.
    pub fn timestamp(&self) -> Option<i64> {
        match self {
            Event::TextMessageStart(e) => e.timestamp,
            Event::TextMessageContent(e) => e.timestamp,
            Event::TextMessageEnd(e) => e.timestamp,
            Event::TextMessageChunk(e) => e.timestamp,
            Event::ToolCallStart(e) => e.timestamp,
            Event::ToolCallArgs(e) => e.timestamp,
            Event::ToolCallEnd(e) => e.timestamp,
            Event::ToolCallChunk(e) => e.timestamp,
            Event::ToolAuthorizationRequest(e) => e.timestamp,
            Event::ToolAuthorizationResponse(e) => e.timestamp,
            Event::AuthorizationStatus(e) => e.timestamp,
            Event::StateSnapshot(e) => e.timestamp,
            Event::StateDelta(e) => e.timestamp,
            Event::MessagesSnapshot(e) => e.timestamp,
            Event::Raw(e) => e.timestamp,
            Event::Custom(e) => e.timestamp,
            Event::RunStarted(e) => e.timestamp,
            Event::RunFinished(e) => e.timestamp,
            Event::RunError(e) => e.timestamp,
            Event::StepStarted(e) => e.timestamp,
            Event::StepFinished(e) => e.timestamp,
        }
    }

    /// Creates an `Event` from an SSE event name and data string.
    ///
    /// # Arguments
    ///
    /// * `event_name`: The name of the SSE event (e.g., "TEXT_MESSAGE_START").
    /// * `data`: The JSON string data associated with the event.
    ///
    /// # Returns
    ///
    /// A `Result` containing the parsed `Event` or an `AgUiError` if parsing fails
    /// or the event type is unknown/unsupported for direct SSE mapping.
    pub fn from_sse(event_name: &str, data: &str) -> crate::error::AgUiResult<Self> {
        match event_name {
            "TEXT_MESSAGE_START" => serde_json::from_str::<TextMessageStartEvent>(data)
                .map(Event::TextMessageStart)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse TextMessageStartEvent: {}", e))),
            "TEXT_MESSAGE_CONTENT" => serde_json::from_str::<TextMessageContentEvent>(data)
                .map(Event::TextMessageContent)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse TextMessageContentEvent: {}", e))),
            "TEXT_MESSAGE_END" => serde_json::from_str::<TextMessageEndEvent>(data)
                .map(Event::TextMessageEnd)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse TextMessageEndEvent: {}", e))),
            "TEXT_MESSAGE_CHUNK" => serde_json::from_str::<TextMessageChunkEvent>(data)
                .map(Event::TextMessageChunk)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse TextMessageChunkEvent: {}", e))),
            "TOOL_CALL_START" => serde_json::from_str::<ToolCallStartEvent>(data)
                .map(Event::ToolCallStart)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse ToolCallStartEvent: {}", e))),
            "TOOL_CALL_ARGS" => serde_json::from_str::<ToolCallArgsEvent>(data)
                .map(Event::ToolCallArgs)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse ToolCallArgsEvent: {}", e))),
            "TOOL_CALL_END" => serde_json::from_str::<ToolCallEndEvent>(data)
                .map(Event::ToolCallEnd)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse ToolCallEndEvent: {}", e))),
            "TOOL_CALL_CHUNK" => serde_json::from_str::<ToolCallChunkEvent>(data)
                .map(Event::ToolCallChunk)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse ToolCallChunkEvent: {}", e))),
            "TOOL_AUTHORIZATION_REQUEST" => serde_json::from_str::<ToolAuthorizationRequestEvent>(data)
                .map(Event::ToolAuthorizationRequest)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse ToolAuthorizationRequestEvent: {}", e))),
            "TOOL_AUTHORIZATION_RESPONSE" => serde_json::from_str::<ToolAuthorizationResponseEvent>(data)
                .map(Event::ToolAuthorizationResponse)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse ToolAuthorizationResponseEvent: {}", e))),
            "AUTHORIZATION_STATUS" => serde_json::from_str::<AuthorizationStatusEvent>(data)
                .map(Event::AuthorizationStatus)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse AuthorizationStatusEvent: {}", e))),
            "STATE_SNAPSHOT" => serde_json::from_str::<StateSnapshotEvent>(data)
                .map(Event::StateSnapshot)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse StateSnapshotEvent: {}", e))),
            "STATE_DELTA" => serde_json::from_str::<StateDeltaEvent>(data)
                .map(Event::StateDelta)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse StateDeltaEvent: {}", e))),
            "MESSAGES_SNAPSHOT" => serde_json::from_str::<MessagesSnapshotEvent>(data)
                .map(Event::MessagesSnapshot)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse MessagesSnapshotEvent: {}", e))),
            "RAW" => serde_json::from_str::<RawEvent>(data)
                .map(Event::Raw)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse RawEvent: {}", e))),
            "CUSTOM" => serde_json::from_str::<CustomEvent>(data)
                .map(Event::Custom)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse CustomEvent: {}", e))),
            "RUN_STARTED" => serde_json::from_str::<RunStartedEvent>(data)
                .map(Event::RunStarted)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse RunStartedEvent: {}", e))),
            "RUN_FINISHED" => serde_json::from_str::<RunFinishedEvent>(data)
                .map(Event::RunFinished)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse RunFinishedEvent: {}", e))),
            "RUN_ERROR" => serde_json::from_str::<RunErrorEvent>(data)
                .map(Event::RunError)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse RunErrorEvent: {}", e))),
            "STEP_STARTED" => serde_json::from_str::<StepStartedEvent>(data)
                .map(Event::StepStarted)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse StepStartedEvent: {}", e))),
            "STEP_FINISHED" => serde_json::from_str::<StepFinishedEvent>(data)
                .map(Event::StepFinished)
                .map_err(|e| crate::error::AgUiError::serialization(format!("Failed to parse StepFinishedEvent: {}", e))),
            _ => {
                serde_json::from_str::<Event>(data)
                    .map_err(|e_tagged| {
                        crate::error::AgUiError::InvalidEventType {
                            event_type: format!("SSE event name '{}' with data '{}' could not be deserialized into a known Event variant: Direct: {}", event_name, data, e_tagged),
                        }
                    })
            }
        }
    }
}

// Convenience constructors for events
impl TextMessageStartEvent {
    /// Create a new text message start event.
    pub fn new(message_id: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            message_id,
            role: "assistant".to_string(),
        }
    }
}

impl TextMessageContentEvent {
    /// Create a new text message content event.
    pub fn new(message_id: String, delta: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            message_id,
            delta,
        }
    }
}

impl TextMessageEndEvent {
    /// Create a new text message end event.
    pub fn new(message_id: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            message_id,
        }
    }
}

impl ToolCallStartEvent {
    /// Create a new tool call start event.
    pub fn new(tool_call_id: String, tool_call_name: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
            tool_call_name,
            parent_message_id: None,
        }
    }
}

impl ToolCallArgsEvent {
    /// Create a new tool call args event.
    pub fn new(tool_call_id: String, delta: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
            delta,
        }
    }
}

impl ToolCallEndEvent {
    /// Create a new tool call end event.
    pub fn new(tool_call_id: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            tool_call_id,
        }
    }
}

impl StateSnapshotEvent {
    /// Create a new state snapshot event.
    pub fn new(snapshot: State) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            snapshot,
        }
    }
}

impl StateDeltaEvent {
    /// Create a new state delta event.
    pub fn new(delta: Vec<serde_json::Value>) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            delta,
        }
    }
}

impl MessagesSnapshotEvent {
    /// Create a new messages snapshot event.
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            messages,
        }
    }
}

impl RunStartedEvent {
    /// Create a new run started event.
    pub fn new(thread_id: String, run_id: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            thread_id,
            run_id,
        }
    }
}

impl RunFinishedEvent {
    /// Create a new run finished event.
    pub fn new(thread_id: String, run_id: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            thread_id,
            run_id,
        }
    }
}

impl RunErrorEvent {
    /// Create a new run error event.
    pub fn new(message: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            message,
            code: None,
        }
    }

    /// Create a new run error event with a code.
    pub fn with_code(message: String, code: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            message,
            code: Some(code),
        }
    }
}

impl StepStartedEvent {
    /// Create a new step started event.
    pub fn new(step_name: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            step_name,
        }
    }
}

impl StepFinishedEvent {
    /// Create a new step finished event.
    pub fn new(step_name: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            step_name,
        }
    }
}

impl CustomEvent {
    /// Create a new custom event.
    pub fn new(name: String, value: serde_json::Value) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            name,
            value,
        }
    }
}

impl RawEvent {
    /// Create a new raw event.
    pub fn new(event: serde_json::Value) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            event,
            source: None,
        }
    }

    /// Create a new raw event with a source.
    pub fn with_source(event: serde_json::Value, source: String) -> Self {
        Self {
            timestamp: None,
            raw_event: None,
            event,
            source: Some(source),
        }
    }
}
