//! Installation strategies for binary acquisition

pub mod github;
pub mod docker;
pub mod source;

// Re-exports
pub use github::GitHubReleaseStrategy;
pub use docker::DockerRegistryStrategy;
pub use source::SourceBuildStrategy;