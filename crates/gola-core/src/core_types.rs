//! Core type definitions for agent-LLM communication protocol
//!
//! This module defines the fundamental data structures that form the contract
//! between agents and language models. The design prioritizes compatibility with
//! OpenAI's function calling format while remaining extensible for other providers.
//! These types serve as the lingua franca for all agent-LLM interactions, ensuring
//! consistent message formatting and tool invocation across different implementations.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool, 
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>, 
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub arguments: Value,
}

// OpenAI-style function call structure (for compatibility)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: Value,
}

// Extended ToolCall structure for more detailed function calling
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DetailedToolCall {
    pub id: String,
    pub function: FunctionCall,
}

// Usage statistics structure
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LLMResponse {
    pub content: Option<String>,           
    pub tool_calls: Option<Vec<ToolCall>>, 
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub tool_call_id: Option<String>, 
    pub content: String,
    pub success: bool,
}

#[derive(Debug, Clone)]
pub enum HistoryStep {
    UserTask(String),
    Thought(String),
    Action(ToolCall),
    Observation(Observation),
    LLMError(String),      
    ToolError(String),     
    ExecutorError(String), 
}
