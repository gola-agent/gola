//! Binary installation management
//!
//! This module provides binary installation capabilities through multiple strategies:
//! - GitHub releases (primary)
//! - Docker registries (fallback)
//! - Source building (last resort)

pub mod manager;
pub mod strategies;
pub mod cache;
pub mod docker_manager;
pub mod orchestrator;

// Re-exports
pub use manager::BinaryManagerImpl;
pub use strategies::*;
pub use docker_manager::DockerManagerImpl;
pub use orchestrator::InstallationOrchestrator;