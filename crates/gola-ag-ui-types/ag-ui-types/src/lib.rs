//! # ag-ui-types
//!
//! Rust types for the ag-ui specification, providing strongly-typed structures
//! for agent-user interaction protocols.
//!
//! This crate implements the core types from the ag-ui specification without
//! any transport or messaging stack specifics, making it suitable for use
//! across different implementations.
//!
//! ## Features
//!
//! - **Strongly typed**: All types are defined with proper Rust type safety
//! - **Serde support**: Full serialization/deserialization support
//! - **Event system**: Complete event type definitions for streaming interactions
//! - **Message types**: Support for all message roles (user, assistant, system, developer, tool)
//! - **Tool calls**: Full support for function calling and tool interactions
//! - **State management**: Types for state snapshots and deltas
//!
//! ## Example
//!
//! ```rust
//! use ag_ui_types::{Message, Role};
//!
//! let message = Message::new_user(
//!     "msg_123".to_string(),
//!     "Hello, how can you help me?".to_string()
//! );
//!
//! assert_eq!(message.role(), Role::User);
//! ```

pub mod types;
pub mod events;
pub mod error;

pub use types::*;
pub use events::*;
pub use error::*;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_user_message_serialization() {
        let msg = UserMessage::new("test_id".to_string(), "Hello world".to_string());
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: UserMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_message_enum_serialization() {
        let message = Message::new_user("test_id".to_string(), "Hello".to_string());
        
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        
        match deserialized {
            Message::User { content, id, .. } => {
                assert_eq!(content, "Hello");
                assert_eq!(id, "test_id");
            }
            _ => panic!("Expected User message"),
        }
    }

    #[test]
    fn test_tool_call_creation() {
        let function_call = FunctionCall::new(
            "get_weather".to_string(),
            r#"{"location": "San Francisco"}"#.to_string(),
        );
        let tool_call = ToolCall::new("call_123".to_string(), function_call);
        
        assert_eq!(tool_call.id, "call_123");
        assert_eq!(tool_call.function.name, "get_weather");
        assert_eq!(tool_call.call_type, ToolCallType::Function);
    }

    #[test]
    fn test_event_serialization() {
        let event = TextMessageStartEvent::new("msg_123".to_string());
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: TextMessageStartEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, deserialized);
    }

    #[test]
    fn test_run_agent_input() {
        let input = RunAgentInput::new(
            "thread_123".to_string(),
            "run_456".to_string(),
            serde_json::json!({"key": "value"}),
            vec![],
            vec![],
            vec![],
            serde_json::json!({}),
        );
        
        assert_eq!(input.thread_id, "thread_123");
        assert_eq!(input.run_id, "run_456");
    }

    #[test]
    fn test_message_role_extraction() {
        let user_msg = Message::new_user("1".to_string(), "Hello".to_string());
        let system_msg = Message::new_system("2".to_string(), "System prompt".to_string());
        let assistant_msg = Message::new_assistant("3".to_string(), "Hi there!".to_string());
        
        assert_eq!(user_msg.role(), Role::User);
        assert_eq!(system_msg.role(), Role::System);
        assert_eq!(assistant_msg.role(), Role::Assistant);
    }

    #[test]
    fn test_assistant_with_tool_calls() {
        let function_call = FunctionCall::new(
            "get_weather".to_string(),
            r#"{"location": "NYC"}"#.to_string(),
        );
        let tool_call = ToolCall::new("call_1".to_string(), function_call);
        let message = Message::new_assistant_with_tool_calls("msg_1".to_string(), vec![tool_call]);
        
        match message {
            Message::Assistant { tool_calls, content, .. } => {
                assert!(tool_calls.is_some());
                assert!(content.is_none());
                assert_eq!(tool_calls.unwrap().len(), 1);
            }
            _ => panic!("Expected Assistant message"),
        }
    }
}
