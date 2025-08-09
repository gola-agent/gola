//! Type definitions for the agent-UI communication protocol specification
//!
//! This crate provides the shared contract between agent servers and UI clients,
//! ensuring type-safe communication across system boundaries. The design philosophy
//! emphasizes protocol stability and backward compatibility, enabling independent
//! evolution of server and client implementations. By centralizing type definitions,
//! this approach prevents drift between components and enables compile-time validation
//! of protocol compliance across the entire system.
//!
//! ## Features
//!
//! - **Strongly typed**: All types are defined with proper Rust type safety
//! - **Serde support**: Full serialization/deserialization support
//! - **Event system**: Complete event type definitions for streaming interactions
//! - **Message types**: Support for all message roles (user, assistant, system, developer, tool)
//! - **Tool calls**: Full support for function calling and tool interactions
//! - **State management**: Types for state snapshots and deltas
//! - **Authorization**: Tool execution guardrails and authorization types
//!
//! ## Example
//!
//! ```rust
//! use gola_ag_ui_types::{Message, Role};
//!
//! let message = Message::new_user(
//!     "msg_123".to_string(),
//!     "Hello, how can you help me?".to_string()
//! );
//!
//! assert_eq!(message.role(), Role::User);
//! ```

pub mod authorization;
pub mod error;
pub mod events;
pub mod types;

pub use authorization::*;
pub use error::*;
pub use events::*;
pub use types::*;

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
            Message::Assistant {
                tool_calls,
                content,
                ..
            } => {
                assert!(tool_calls.is_some());
                assert!(content.is_none());
                assert_eq!(tool_calls.unwrap().len(), 1);
            }
            _ => panic!("Expected Assistant message"),
        }
    }

    #[test]
    fn test_authorization_config() {
        let config = AuthorizationConfig::new(ToolAuthorizationMode::Ask)
            .with_prompt_message("Please authorize this tool execution".to_string())
            .with_timeout(60)
            .with_enabled(true);

        assert_eq!(config.mode, ToolAuthorizationMode::Ask);
        assert!(config.enabled);
        assert_eq!(
            config.prompt_message,
            Some("Please authorize this tool execution".to_string())
        );
        assert_eq!(config.timeout_seconds, Some(60));

        let disabled_config = AuthorizationConfig::disabled();
        assert_eq!(disabled_config.mode, ToolAuthorizationMode::AlwaysAllow);
        assert!(!disabled_config.enabled);
    }

    #[test]
    fn test_pending_authorization() {
        let auth = PendingAuthorization::new(
            "call_123".to_string(),
            "get_weather".to_string(),
            r#"{"location": "NYC"}"#.to_string(),
        );

        assert_eq!(auth.tool_call_id, "call_123");
        assert_eq!(auth.tool_call_name, "get_weather");
        assert_eq!(auth.status, AuthorizationStatus::Pending);
        assert!(!auth.is_expired()); // Should not be expired without expiration time

        let auth_with_desc = PendingAuthorization::with_description(
            "call_456".to_string(),
            "send_email".to_string(),
            r#"{"to": "user@example.com"}"#.to_string(),
            "Send an email to the user".to_string(),
        );

        assert_eq!(
            auth_with_desc.description,
            Some("Send an email to the user".to_string())
        );
    }

    #[test]
    fn test_authorization_status_event() {
        let status_event =
            AuthorizationStatusEvent::new("call_123".to_string(), AuthorizationStatus::Approved);

        assert_eq!(status_event.tool_call_id, "call_123");
        assert_eq!(status_event.status, AuthorizationStatus::Approved);
        assert!(status_event.message.is_none());

        let status_with_msg = AuthorizationStatusEvent::with_message(
            "call_456".to_string(),
            AuthorizationStatus::Denied,
            "User denied the request".to_string(),
        );

        assert_eq!(
            status_with_msg.message,
            Some("User denied the request".to_string())
        );
    }

    #[test]
    fn test_authorization_types() {
        let auth_request = ToolAuthorizationRequestEvent::new(
            "call_123".to_string(),
            "get_weather".to_string(),
            r#"{"location": "NYC"}"#.to_string(),
        );

        assert_eq!(auth_request.tool_call_id, "call_123");
        assert_eq!(auth_request.tool_call_name, "get_weather");

        let auth_response = ToolAuthorizationResponseEvent::new(
            "call_123".to_string(),
            AuthorizationResponse::Approve,
        );

        assert_eq!(auth_response.tool_call_id, "call_123");
        assert_eq!(auth_response.response, AuthorizationResponse::Approve);
    }
}
