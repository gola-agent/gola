//! Core framework for building autonomous AI agents with tool integration.
//!
//! This crate provides the foundational infrastructure for creating, configuring,
//! and orchestrating AI agents capable of multi-step reasoning, tool execution,
//! and context-aware decision making. The architecture emphasizes modularity,
//! extensibility, and production-grade reliability.
//!
//! # Architecture Overview
//!
//! The framework is organized around several key subsystems:
//!
//! - **Agent orchestration**: Lifecycle management and conversation flow control
//! - **Language model integration**: Provider-agnostic LLM interfaces supporting multiple backends
//! - **Memory management**: Conversation history retention with multiple eviction strategies
//! - **Tool ecosystem**: Extensible tool registry with MCP protocol support
//! - **Retrieval augmentation**: Semantic search and document embedding capabilities
//! - **Execution environments**: Sandboxed code execution via Docker and native runtimes
//! - **Authorization framework**: Fine-grained permission control for tool invocations
//! - **Configuration system**: Hierarchical configuration with environment-aware defaults

pub mod agent;
pub mod agent_factory;
pub mod core_types;
pub mod errors;
pub mod executors;
pub mod installation;
pub mod llm;
pub mod loop_detection;
pub mod memory;
pub mod tools;
pub mod guardrails;
pub mod sse_authorization_handler;
pub mod polling_authorization_handler;
pub mod authorization_client;
pub mod trace;
pub mod tracing;
pub mod rag;
pub mod ag_ui_handler;
pub mod config;

pub use authorization_client::AuthorizationClient;
pub use agent::{Agent, AgentConfig};
pub use agent_factory::AgentFactory;
pub use errors::AgentError;
pub use executors::CodeExecutor;
pub use rag::Rag;
pub use config::*;
pub use llm::LLM;
pub use ag_ui_handler::GolaAgentHandler;


#[cfg(test)]
pub mod test_utils;


#[cfg(test)]
mod test_rag_integration;
