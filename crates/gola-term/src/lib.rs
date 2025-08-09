//! Terminal user interface for the Gola agent system.
//!
//! This crate provides a rich terminal-based interface for interacting with AI agents,
//! featuring message bubbles, syntax highlighting, editor integration, and session management.
//! It serves as the primary user-facing component of the Gola ecosystem.

pub mod application;
pub mod configuration;
pub mod domain;
pub mod infrastructure;
pub use application::ui::{destruct_terminal_for_panic, start_loop};
pub use configuration::{Config, ConfigKey};
pub use domain::models::{
    Action, AgentClient, AgentName, AgentPrompt, AgentResponse, Author, Event,
};
pub use domain::services::{AppStateProps, Sessions};
pub use infrastructure::editors::EditorManager;
