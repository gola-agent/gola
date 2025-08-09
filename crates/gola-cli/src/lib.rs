//! Unified command-line interface orchestrating all Gola deployment modes
//!
//! This crate serves as the primary entry point for the Gola ecosystem, providing
//! a single binary that can operate in multiple modes based on user needs. The
//! design philosophy of mode unification reduces cognitive load by eliminating the
//! need to remember different commands for different use cases. Whether running
//! agents locally, deploying servers, or executing one-off tasks, users interact
//! with a consistent interface that adapts to their workflow.

pub mod direct_gola_term_client;
pub mod embedded_ui;
pub mod remote_gola_term_client;
