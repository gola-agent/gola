//! Consolidated test mocks for the Gola project
//!
//! This crate provides reusable mock implementations for testing across
//! all Gola crates, preventing duplication and ensuring consistent behavior.

pub mod llm;
pub mod agent;
pub mod auth;
pub mod tools;

pub use llm::*;
pub use agent::*;
pub use auth::*;
pub use tools::*;