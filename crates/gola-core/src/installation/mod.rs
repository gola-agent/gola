//! Installation system for binaries and runtimes
//!
//! This module provides a unified system for installing MCP server binaries
//! through multiple strategies: GitHub releases, Docker registries, and source building.

pub mod traits;
pub mod errors;
pub mod binary;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;

// Re-exports for convenience
pub use traits::*;
pub use errors::*;
pub use binary::*;

/// Creates a default installation orchestrator with standard configuration
pub async fn create_default_orchestrator(
    installation_dir: std::path::PathBuf,
    org: String,
    repo: String,
) -> Result<Box<dyn BinaryManager>, InstallationError> {
    let orchestrator = InstallationOrchestrator::create_default(installation_dir, org, repo)
        .await
        .map_err(|e| InstallationError::IoError { message: e.to_string() })?;
    
    Ok(Box::new(orchestrator))
}