//! Core types for the ag-ui specification.

use serde::{Deserialize, Serialize};

/// A function call with name and arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The name of the function to call.
    pub name: String,
    /// The arguments to pass to the function, as a JSON string.
    pub arguments: String,
}

/// A tool call, modeled after OpenAI tool calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for the tool call.
    pub id: String,
    /// The type of tool call (currently only "function" is supported).
    #[serde(rename = "type")]
    pub call_type: ToolCallType,
    /// The function call details.
    pub function: FunctionCall,
}

/// The type of tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolCallType {
    /// A function call.
    Function,
}

/// Message roles in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// A developer message.
    Developer,
    /// A system message.
    System,
    /// An assistant message.
    Assistant,
    /// A user message.
    User,
    /// A tool result message.
    Tool,
}

/// A developer message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeveloperMessage {
    /// Unique identifier for the message.
    pub id: String,
    /// The content of the message.
    pub content: String,
    /// The name associated with the message (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A system message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemMessage {
    /// Unique identifier for the message.
    pub id: String,
    /// The content of the message.
    pub content: String,
    /// The name associated with the message (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// An assistant message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistantMessage {
    /// Unique identifier for the message.
    pub id: String,
    /// The content of the message (optional if tool calls are present).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// The name associated with the message (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls made by the assistant (optional).
    #[serde(rename = "toolCalls", skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// A user message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMessage {
    /// Unique identifier for the message.
    pub id: String,
    /// The content of the message.
    pub content: String,
    /// The name associated with the message (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A tool result message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMessage {
    /// Unique identifier for the message.
    pub id: String,
    /// The content of the tool result.
    pub content: String,
    /// The ID of the tool call this message responds to.
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
}

/// A message in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    /// A developer message.
    Developer {
        /// Unique identifier for the message.
        id: String,
        /// The content of the message.
        content: String,
        /// The name associated with the message (optional).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// A system message.
    System {
        /// Unique identifier for the message.
        id: String,
        /// The content of the message.
        content: String,
        /// The name associated with the message (optional).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// An assistant message.
    Assistant {
        /// Unique identifier for the message.
        id: String,
        /// The content of the message (optional if tool calls are present).
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        /// The name associated with the message (optional).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Tool calls made by the assistant (optional).
        #[serde(rename = "toolCalls", skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    /// A user message.
    User {
        /// Unique identifier for the message.
        id: String,
        /// The content of the message.
        content: String,
        /// The name associated with the message (optional).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// A tool result message.
    Tool {
        /// Unique identifier for the message.
        id: String,
        /// The content of the tool result.
        content: String,
        /// The ID of the tool call this message responds to.
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
    },
}

impl Message {
    /// Get the ID of the message.
    pub fn id(&self) -> &str {
        match self {
            Message::Developer { id, .. } => id,
            Message::System { id, .. } => id,
            Message::Assistant { id, .. } => id,
            Message::User { id, .. } => id,
            Message::Tool { id, .. } => id,
        }
    }

    /// Get the role of the message.
    pub fn role(&self) -> Role {
        match self {
            Message::Developer { .. } => Role::Developer,
            Message::System { .. } => Role::System,
            Message::Assistant { .. } => Role::Assistant,
            Message::User { .. } => Role::User,
            Message::Tool { .. } => Role::Tool,
        }
    }

    /// Get the content of the message, if any.
    pub fn content(&self) -> Option<&str> {
        match self {
            Message::Developer { content, .. } => Some(content),
            Message::System { content, .. } => Some(content),
            Message::Assistant { content, .. } => content.as_deref(),
            Message::User { content, .. } => Some(content),
            Message::Tool { content, .. } => Some(content),
        }
    }

    /// Create a new developer message.
    pub fn new_developer(id: String, content: String) -> Self {
        Message::Developer {
            id,
            content,
            name: None,
        }
    }

    /// Create a new developer message with a name.
    pub fn new_developer_with_name(id: String, content: String, name: String) -> Self {
        Message::Developer {
            id,
            content,
            name: Some(name),
        }
    }

    /// Create a new system message.
    pub fn new_system(id: String, content: String) -> Self {
        Message::System {
            id,
            content,
            name: None,
        }
    }

    /// Create a new system message with a name.
    pub fn new_system_with_name(id: String, content: String, name: String) -> Self {
        Message::System {
            id,
            content,
            name: Some(name),
        }
    }

    /// Create a new assistant message with content.
    pub fn new_assistant(id: String, content: String) -> Self {
        Message::Assistant {
            id,
            content: Some(content),
            name: None,
            tool_calls: None,
        }
    }

    /// Create a new assistant message with tool calls.
    pub fn new_assistant_with_tool_calls(id: String, tool_calls: Vec<ToolCall>) -> Self {
        Message::Assistant {
            id,
            content: None,
            name: None,
            tool_calls: Some(tool_calls),
        }
    }

    /// Create a new assistant message with both content and tool calls.
    pub fn new_assistant_with_content_and_tool_calls(
        id: String,
        content: String,
        tool_calls: Vec<ToolCall>,
    ) -> Self {
        Message::Assistant {
            id,
            content: Some(content),
            name: None,
            tool_calls: Some(tool_calls),
        }
    }

    /// Create a new user message.
    pub fn new_user(id: String, content: String) -> Self {
        Message::User {
            id,
            content,
            name: None,
        }
    }

    /// Create a new user message with a name.
    pub fn new_user_with_name(id: String, content: String, name: String) -> Self {
        Message::User {
            id,
            content,
            name: Some(name),
        }
    }

    /// Create a new tool message.
    pub fn new_tool(id: String, content: String, tool_call_id: String) -> Self {
        Message::Tool {
            id,
            content,
            tool_call_id,
        }
    }
}

/// Additional context for the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// Description of the context.
    pub description: String,
    /// The context value.
    pub value: String,
}

/// A tool definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    /// The name of the tool.
    pub name: String,
    /// Description of what the tool does.
    pub description: String,
    /// JSON Schema for the tool parameters.
    pub parameters: serde_json::Value,
}

/// Input for running an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunAgentInput {
    /// The thread ID for this conversation.
    #[serde(rename = "threadId")]
    pub thread_id: String,
    /// The run ID for this execution.
    #[serde(rename = "runId")]
    pub run_id: String,
    /// The current state of the agent.
    pub state: serde_json::Value,
    /// The messages in the conversation.
    pub messages: Vec<Message>,
    /// Available tools for the agent.
    pub tools: Vec<Tool>,
    /// Additional context for the agent.
    pub context: Vec<Context>,
    /// Forwarded properties from the client.
    #[serde(rename = "forwardedProps")]
    pub forwarded_props: serde_json::Value,
}

/// The state of an agent (can be any JSON value).
pub type State = serde_json::Value;

impl DeveloperMessage {
    /// Create a new developer message.
    pub fn new(id: String, content: String) -> Self {
        Self {
            id,
            content,
            name: None,
        }
    }

    /// Create a new developer message with a name.
    pub fn with_name(id: String, content: String, name: String) -> Self {
        Self {
            id,
            content,
            name: Some(name),
        }
    }
}

impl SystemMessage {
    /// Create a new system message.
    pub fn new(id: String, content: String) -> Self {
        Self {
            id,
            content,
            name: None,
        }
    }

    /// Create a new system message with a name.
    pub fn with_name(id: String, content: String, name: String) -> Self {
        Self {
            id,
            content,
            name: Some(name),
        }
    }
}

impl AssistantMessage {
    /// Create a new assistant message with content.
    pub fn new(id: String, content: String) -> Self {
        Self {
            id,
            content: Some(content),
            name: None,
            tool_calls: None,
        }
    }

    /// Create a new assistant message with tool calls.
    pub fn with_tool_calls(id: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            id,
            content: None,
            name: None,
            tool_calls: Some(tool_calls),
        }
    }

    /// Create a new assistant message with both content and tool calls.
    pub fn with_content_and_tool_calls(
        id: String,
        content: String,
        tool_calls: Vec<ToolCall>,
    ) -> Self {
        Self {
            id,
            content: Some(content),
            name: None,
            tool_calls: Some(tool_calls),
        }
    }
}

impl UserMessage {
    /// Create a new user message.
    pub fn new(id: String, content: String) -> Self {
        Self {
            id,
            content,
            name: None,
        }
    }

    /// Create a new user message with a name.
    pub fn with_name(id: String, content: String, name: String) -> Self {
        Self {
            id,
            content,
            name: Some(name),
        }
    }
}

impl ToolMessage {
    /// Create a new tool message.
    pub fn new(id: String, content: String, tool_call_id: String) -> Self {
        Self {
            id,
            content,
            tool_call_id,
        }
    }
}

impl ToolCall {
    /// Create a new tool call.
    pub fn new(id: String, function: FunctionCall) -> Self {
        Self {
            id,
            call_type: ToolCallType::Function,
            function,
        }
    }
}

impl FunctionCall {
    /// Create a new function call.
    pub fn new(name: String, arguments: String) -> Self {
        Self { name, arguments }
    }
}

impl Context {
    /// Create a new context.
    pub fn new(description: String, value: String) -> Self {
        Self { description, value }
    }
}

impl Tool {
    /// Create a new tool.
    pub fn new(name: String, description: String, parameters: serde_json::Value) -> Self {
        Self {
            name,
            description,
            parameters,
        }
    }
}

impl RunAgentInput {
    /// Create a new run agent input.
    pub fn new(
        thread_id: String,
        run_id: String,
        state: serde_json::Value,
        messages: Vec<Message>,
        tools: Vec<Tool>,
        context: Vec<Context>,
        forwarded_props: serde_json::Value,
    ) -> Self {
        Self {
            thread_id,
            run_id,
            state,
            messages,
            tools,
            context,
            forwarded_props,
        }
    }
}
