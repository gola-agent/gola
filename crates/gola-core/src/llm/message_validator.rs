//! Message validation and automatic recovery for LLM interactions
//! 
//! This module provides functionality to detect and automatically fix common
//! message sequence issues that can cause LLM API errors, particularly the
//! "assistant message with tool_calls must be followed by tool messages" error.

use crate::core_types::{Message, Role};
use crate::errors::AgentError;
use std::collections::HashSet;

/// Validates and fixes message sequences to prevent common API errors
#[derive(Debug, Clone)]
pub struct MessageValidator {
    /// Whether to automatically fix validation errors
    pub auto_fix: bool,
    /// Whether to log validation issues
    pub log_issues: bool,
}

impl Default for MessageValidator {
    fn default() -> Self {
        Self {
            auto_fix: true,
            log_issues: true,
        }
    }
}

impl MessageValidator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_auto_fix(mut self, auto_fix: bool) -> Self {
        self.auto_fix = auto_fix;
        self
    }

    pub fn with_logging(mut self, log_issues: bool) -> Self {
        self.log_issues = log_issues;
        self
    }

    /// Validates and optionally fixes a sequence of messages
    pub fn validate_and_fix(&self, messages: Vec<Message>) -> Result<Vec<Message>, AgentError> {
        let issues = self.detect_issues(&messages);
        
        if issues.is_empty() {
            return Ok(messages);
        }

        if self.log_issues {
            for issue in &issues {
                log::warn!("Message validation issue detected: {}", issue.description);
            }
        }

        if self.auto_fix {
            let fixed_messages = self.fix_issues(messages, &issues)?;
            if self.log_issues {
                log::info!("Automatically fixed {} message validation issues", issues.len());
            }
            Ok(fixed_messages)
        } else {
            Err(AgentError::ValidationError(format!(
                "Message validation failed: {}",
                issues.iter()
                    .map(|i| i.description.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }

    /// Detects validation issues in message sequences
    fn detect_issues(&self, messages: &[Message]) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Check for orphaned tool calls (assistant messages with tool_calls not followed by tool responses)
        let orphaned_tool_calls = self.find_orphaned_tool_calls(messages);
        for (msg_index, tool_call_ids) in orphaned_tool_calls {
            issues.push(ValidationIssue {
                issue_type: ValidationIssueType::OrphanedToolCalls,
                message_index: msg_index,
                description: format!(
                    "Assistant message at index {} has tool_calls {:?} without corresponding tool responses",
                    msg_index, tool_call_ids
                ),
                tool_call_ids: Some(tool_call_ids),
            });
        }

        // Check for tool messages without corresponding assistant tool calls
        let orphaned_tool_responses = self.find_orphaned_tool_responses(messages);
        for (msg_index, tool_call_id) in orphaned_tool_responses {
            issues.push(ValidationIssue {
                issue_type: ValidationIssueType::OrphanedToolResponse,
                message_index: msg_index,
                description: format!(
                    "Tool message at index {} references tool_call_id '{}' without corresponding assistant tool call",
                    msg_index, tool_call_id
                ),
                tool_call_ids: Some(vec![tool_call_id]),
            });
        }

        issues
    }

    /// Finds assistant messages with tool_calls that don't have corresponding tool responses
    fn find_orphaned_tool_calls(&self, messages: &[Message]) -> Vec<(usize, Vec<String>)> {
        let mut orphaned = Vec::new();
        let mut pending_tool_calls: Vec<(usize, Vec<String>)> = Vec::new();

        for (i, message) in messages.iter().enumerate() {
            match &message.role {
                Role::Assistant => {
                    if let Some(tool_calls) = &message.tool_calls {
                        if !tool_calls.is_empty() {
                            let tool_call_ids: Vec<String> = tool_calls
                                .iter()
                                .filter_map(|tc| tc.id.clone())
                                .collect();
                            
                            if !tool_call_ids.is_empty() {
                                pending_tool_calls.push((i, tool_call_ids));
                            }
                        }
                    }
                }
                Role::Tool => {
                    if let Some(tool_call_id) = &message.tool_call_id {
                        // Remove this tool_call_id from pending calls
                        for (_, pending_ids) in &mut pending_tool_calls {
                            pending_ids.retain(|id| id != tool_call_id);
                        }
                        // Remove entries with no remaining pending IDs
                        pending_tool_calls.retain(|(_, ids)| !ids.is_empty());
                    }
                }
                Role::User => {
                    // User messages can interrupt tool call sequences
                    // Mark any remaining pending tool calls as orphaned
                    orphaned.extend(pending_tool_calls.drain(..));
                }
                _ => {}
            }
        }

        // Any remaining pending tool calls at the end are orphaned
        orphaned.extend(pending_tool_calls);
        orphaned
    }

    /// Finds tool messages that reference non-existent tool_call_ids
    fn find_orphaned_tool_responses(&self, messages: &[Message]) -> Vec<(usize, String)> {
        let mut orphaned = Vec::new();
        let mut available_tool_call_ids = HashSet::new();

        // First pass: collect all tool_call_ids from assistant messages
        for message in messages {
            if let Role::Assistant = message.role {
                if let Some(tool_calls) = &message.tool_calls {
                    for tool_call in tool_calls {
                        if let Some(id) = &tool_call.id {
                            available_tool_call_ids.insert(id.clone());
                        }
                    }
                }
            }
        }

        // Second pass: find tool messages with invalid tool_call_ids
        for (i, message) in messages.iter().enumerate() {
            if let Role::Tool = message.role {
                if let Some(tool_call_id) = &message.tool_call_id {
                    if !available_tool_call_ids.contains(tool_call_id) {
                        orphaned.push((i, tool_call_id.clone()));
                    }
                }
            }
        }

        orphaned
    }

    /// Fixes detected validation issues
    fn fix_issues(&self, mut messages: Vec<Message>, issues: &[ValidationIssue]) -> Result<Vec<Message>, AgentError> {
        // Sort issues by message index in reverse order to avoid index shifting
        let mut sorted_issues = issues.to_vec();
        sorted_issues.sort_by(|a, b| b.message_index.cmp(&a.message_index));

        for issue in sorted_issues {
            match issue.issue_type {
                ValidationIssueType::OrphanedToolCalls => {
                    messages = self.fix_orphaned_tool_calls(messages, &issue)?;
                }
                ValidationIssueType::OrphanedToolResponse => {
                    messages = self.fix_orphaned_tool_response(messages, &issue)?;
                }
            }
        }

        Ok(messages)
    }

    /// Fixes orphaned tool calls by adding synthetic tool responses or removing the tool calls
    fn fix_orphaned_tool_calls(&self, mut messages: Vec<Message>, issue: &ValidationIssue) -> Result<Vec<Message>, AgentError> {
        let msg_index = issue.message_index;
        
        if msg_index >= messages.len() {
            return Err(AgentError::ValidationError(
                "Invalid message index in validation issue".to_string()
            ));
        }

        let tool_call_ids = issue.tool_call_ids.as_ref()
            .ok_or_else(|| AgentError::ValidationError("Missing tool_call_ids in orphaned tool calls issue".to_string()))?;

        // Strategy 1: Add synthetic tool responses for missing tool calls
        // Insert them right after the assistant message with tool calls
        let insert_index = msg_index + 1;
        
        for (i, tool_call_id) in tool_call_ids.iter().enumerate() {
            let synthetic_response = Message {
                role: Role::Tool,
                content: "[Tool execution was interrupted or failed - continuing conversation]".to_string(),
                tool_call_id: Some(tool_call_id.clone()),
                tool_calls: None,
            };
            
            messages.insert(insert_index + i, synthetic_response);
        }

        if self.log_issues {
            log::info!("Added {} synthetic tool responses for orphaned tool calls", tool_call_ids.len());
        }

        Ok(messages)
    }

    /// Fixes orphaned tool responses by removing them or converting them to system messages
    fn fix_orphaned_tool_response(&self, mut messages: Vec<Message>, issue: &ValidationIssue) -> Result<Vec<Message>, AgentError> {
        let msg_index = issue.message_index;
        
        if msg_index >= messages.len() {
            return Err(AgentError::ValidationError(
                "Invalid message index in validation issue".to_string()
            ));
        }

        // Strategy: Convert orphaned tool response to a system message
        if let Some(message) = messages.get_mut(msg_index) {
            let original_content = message.content.clone();
            let truncated_content = truncate_with_ellipsis(&original_content, 2000);
            
            *message = Message {
                role: Role::System,
                content: format!("Previous tool result: {}", truncated_content),
                tool_call_id: None,
                tool_calls: None,
            };

            if self.log_issues {
                log::info!(
                    "Converted orphaned tool response to system message at index {} (content truncated: {})",
                    msg_index,
                    original_content.len() > truncated_content.len()
                );
            }
        }

        Ok(messages)
    }
}

/// Truncates a string to a maximum length, adding an ellipsis if truncated.
fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len - 5; // " ... "
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{} ...", &s[..end])
    }
}

#[derive(Debug, Clone)]
struct ValidationIssue {
    issue_type: ValidationIssueType,
    message_index: usize,
    description: String,
    tool_call_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
enum ValidationIssueType {
    OrphanedToolCalls,
    OrphanedToolResponse,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_types::ToolCall;
    use serde_json::json;

    fn create_test_tool_call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: Some(id.to_string()),
            name: name.to_string(),
            arguments: json!({}),
        }
    }

    #[test]
    fn test_valid_message_sequence() {
        let validator = MessageValidator::new();
        
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::Assistant,
                content: "I'll help you".to_string(),
                tool_call_id: None,
                tool_calls: Some(vec![create_test_tool_call("call_1", "test_tool")]),
            },
            Message {
                role: Role::Tool,
                content: "Tool result".to_string(),
                tool_call_id: Some("call_1".to_string()),
                tool_calls: None,
            },
        ];

        let result = validator.validate_and_fix(messages.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_orphaned_tool_calls_detection() {
        let validator = MessageValidator::new();
        
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::Assistant,
                content: "I'll use a tool".to_string(),
                tool_call_id: None,
                tool_calls: Some(vec![create_test_tool_call("call_1", "test_tool")]),
            },
            // Missing tool response - this should be detected and fixed
            Message {
                role: Role::User,
                content: "What happened?".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let issues = validator.detect_issues(&messages);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, ValidationIssueType::OrphanedToolCalls);
        assert_eq!(issues[0].message_index, 1);
    }

    #[test]
    fn test_orphaned_tool_calls_fix() {
        let validator = MessageValidator::new();
        
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::Assistant,
                content: "I'll use a tool".to_string(),
                tool_call_id: None,
                tool_calls: Some(vec![create_test_tool_call("call_1", "test_tool")]),
            },
            Message {
                role: Role::User,
                content: "What happened?".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = validator.validate_and_fix(messages);
        assert!(result.is_ok());
        
        let fixed_messages = result.unwrap();
        assert_eq!(fixed_messages.len(), 4); // Original 3 + 1 synthetic tool response
        
        // Check that synthetic tool response was inserted
        assert_eq!(fixed_messages[2].role, Role::Tool);
        assert_eq!(fixed_messages[2].tool_call_id, Some("call_1".to_string()));
        assert!(fixed_messages[2].content.contains("interrupted"));
    }

    #[test]
    fn test_orphaned_tool_response_detection() {
        let validator = MessageValidator::new();
        
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::Tool,
                content: "Tool result".to_string(),
                tool_call_id: Some("nonexistent_call".to_string()),
                tool_calls: None,
            },
        ];

        let issues = validator.detect_issues(&messages);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, ValidationIssueType::OrphanedToolResponse);
        assert_eq!(issues[0].message_index, 1);
    }

    #[test]
    fn test_orphaned_tool_response_fix() {
        let validator = MessageValidator::new();
        
        let messages = vec![
            Message {
                role: Role::User,
                content: "Hello".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
            Message {
                role: Role::Tool,
                content: "Tool result".to_string(),
                tool_call_id: Some("nonexistent_call".to_string()),
                tool_calls: None,
            },
        ];

        let result = validator.validate_and_fix(messages);
        assert!(result.is_ok());
        
        let fixed_messages = result.unwrap();
        assert_eq!(fixed_messages.len(), 2);
        
        // Check that tool message was converted to system message
        assert_eq!(fixed_messages[1].role, Role::System);
        assert!(fixed_messages[1].content.contains("Previous tool result: Tool result"));
        assert_eq!(fixed_messages[1].tool_call_id, None);
    }

    #[test]
    fn test_orphaned_tool_response_truncation() {
        let validator = MessageValidator::new();
        let long_content = "a".repeat(3000);
        
        let messages = vec![
            Message {
                role: Role::Tool,
                content: long_content.clone(),
                tool_call_id: Some("nonexistent_call".to_string()),
                tool_calls: None,
            },
        ];

        let result = validator.validate_and_fix(messages);
        assert!(result.is_ok());
        
        let fixed_messages = result.unwrap();
        assert_eq!(fixed_messages.len(), 1);
        
        let expected_content = format!("Previous tool result: {} ...", &long_content[..1995]);
        assert_eq!(fixed_messages[0].content, expected_content);
    }

    #[test]
    fn test_multiple_orphaned_tool_calls() {
        let validator = MessageValidator::new();
        
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: "I'll use multiple tools".to_string(),
                tool_call_id: None,
                tool_calls: Some(vec![
                    create_test_tool_call("call_1", "tool_1"),
                    create_test_tool_call("call_2", "tool_2"),
                ]),
            },
            Message {
                role: Role::User,
                content: "What happened?".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = validator.validate_and_fix(messages);
        assert!(result.is_ok());
        
        let fixed_messages = result.unwrap();
        assert_eq!(fixed_messages.len(), 4); // Original 2 + 2 synthetic tool responses
        
        // Check that both synthetic tool responses were inserted
        assert_eq!(fixed_messages[1].role, Role::Tool);
        assert_eq!(fixed_messages[1].tool_call_id, Some("call_1".to_string()));
        assert_eq!(fixed_messages[2].role, Role::Tool);
        assert_eq!(fixed_messages[2].tool_call_id, Some("call_2".to_string()));
    }

    #[test]
    fn test_partial_tool_responses() {
        let validator = MessageValidator::new();
        
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: "I'll use multiple tools".to_string(),
                tool_call_id: None,
                tool_calls: Some(vec![
                    create_test_tool_call("call_1", "tool_1"),
                    create_test_tool_call("call_2", "tool_2"),
                ]),
            },
            Message {
                role: Role::Tool,
                content: "First tool result".to_string(),
                tool_call_id: Some("call_1".to_string()),
                tool_calls: None,
            },
            // Missing response for call_2
            Message {
                role: Role::User,
                content: "What about the second tool?".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = validator.validate_and_fix(messages);
        assert!(result.is_ok());
        
        let fixed_messages = result.unwrap();
        assert_eq!(fixed_messages.len(), 4); // Original 3 + 1 synthetic tool response
        
        // Find the synthetic tool response for call_2
        let synthetic_responses: Vec<_> = fixed_messages.iter()
            .filter(|msg| msg.role == Role::Tool && msg.content.contains("interrupted"))
            .collect();
        
        assert_eq!(synthetic_responses.len(), 1);
        assert_eq!(synthetic_responses[0].tool_call_id, Some("call_2".to_string()));
    }

    #[test]
    fn test_validator_without_auto_fix() {
        let validator = MessageValidator::new().with_auto_fix(false);
        
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: "I'll use a tool".to_string(),
                tool_call_id: None,
                tool_calls: Some(vec![create_test_tool_call("call_1", "test_tool")]),
            },
            Message {
                role: Role::User,
                content: "What happened?".to_string(),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = validator.validate_and_fix(messages);
        assert!(result.is_err());
        
        if let Err(AgentError::ValidationError(msg)) = result {
            assert!(msg.contains("Message validation failed"));
        } else {
            panic!("Expected ValidationError");
        }
    }
}
