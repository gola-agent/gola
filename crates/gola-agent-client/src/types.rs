use gola_ag_ui_types::{Context, Event as GolaEvent, Message, RunAgentInput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Request for streaming agent interaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRequest {
    pub thread_id: String,
    pub run_id: String,
    pub messages: Vec<Message>,
    pub tools: Vec<gola_ag_ui_types::Tool>,
    pub context: Vec<Context>,
    pub state: Value,
    pub forwarded_props: Value,
}

impl StreamRequest {
    /// Create a new stream request with default values
    pub fn new(messages: Vec<Message>) -> Self {
        let thread_id = format!(
            "th_{}",
            Uuid::new_v4().to_string().replace('-', "")[..8].to_string()
        );
        let run_id = Uuid::new_v4().to_string();

        Self {
            thread_id,
            run_id,
            messages,
            tools: Vec::new(),
            context: Vec::new(),
            state: Value::Object(serde_json::Map::new()),
            forwarded_props: Value::Object(serde_json::Map::new()),
        }
    }

    /// Convert to RunAgentInput for the server
    pub fn to_run_agent_input(&self) -> RunAgentInput {
        RunAgentInput {
            thread_id: self.thread_id.clone(),
            run_id: self.run_id.clone(),
            messages: self.messages.clone(),
            tools: self.tools.clone(),
            context: self.context.clone(),
            state: self.state.clone(),
            forwarded_props: self.forwarded_props.clone(),
        }
    }
}

/// Event received from agent stream
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text content delta from the agent
    TextDelta(String),
    /// Tool call in progress
    ToolCall(String),
    /// Agent run completed
    RunFinished,
    /// Agent run error
    RunError(String),
    /// Other event not directly handled
    Other(GolaEvent),
}

impl StreamEvent {
    /// Convert from GolaEvent to StreamEvent
    pub fn from_gola_event(event: GolaEvent) -> Self {
        match event {
            GolaEvent::TextMessageContent(msg_event) => StreamEvent::TextDelta(msg_event.delta),
            GolaEvent::TextMessageChunk(chunk_event) => {
                StreamEvent::TextDelta(chunk_event.delta.unwrap_or_default())
            }
            GolaEvent::RunFinished(_) => StreamEvent::RunFinished,
            GolaEvent::RunError(error_event) => StreamEvent::RunError(error_event.message),
            GolaEvent::ToolCallStart(tool_event) => {
                StreamEvent::ToolCall(tool_event.tool_call_name)
            }
            other => StreamEvent::Other(other),
        }
    }
}
