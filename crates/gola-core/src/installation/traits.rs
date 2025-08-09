//! Core traits and abstractions for the installation system

use crate::errors::AgentError;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Represents a strategy for installing a binary from a specific source
#[async_trait]
pub trait InstallationStrategy: Send + Sync {
    /// Check if this strategy can provide the requested binary
    async fn is_available(&self, binary_name: &str) -> Result<bool, AgentError>;
    
    /// Install the binary to the specified target directory
    /// Returns the path to the installed binary
    async fn install(&self, binary_name: &str, target_dir: &Path) -> Result<PathBuf, AgentError>;
    
    /// Get the priority of this strategy (lower number = higher priority)
    fn get_priority(&self) -> u8;
    
    /// Get a human-readable name for this strategy
    fn get_name(&self) -> &'static str;
}

/// Manages binary installation using multiple strategies
#[async_trait]
pub trait BinaryManager: Send + Sync {
    /// Ensure a binary is available, installing it if necessary
    /// Returns the path to the binary
    async fn ensure_binary(&self, binary_name: &str) -> Result<PathBuf, AgentError>;
    
    /// Find an already installed binary
    async fn find_binary(&self, binary_name: &str) -> Option<PathBuf>;
    
    /// Install a binary using the provided strategies
    /// Strategies are tried in priority order
    async fn install_binary(
        &self,
        binary_name: &str,
        strategies: Vec<Box<dyn InstallationStrategy>>,
    ) -> Result<PathBuf, AgentError>;
    
    /// Get the installation directory for binaries
    fn get_installation_dir(&self) -> &Path;
}

/// Manages Docker operations for binary installation
#[async_trait]
pub trait DockerManager: Send + Sync {
    /// Check if Docker is available on the system
    async fn is_available(&self) -> bool;
    
    /// Extract a binary from a Docker image
    /// Returns the path to the extracted binary
    async fn extract_binary_from_image(
        &self,
        image: &str,
        binary_name: &str,
        target_dir: &Path,
    ) -> Result<PathBuf, AgentError>;
    
    /// Search for an image in a registry
    /// Returns the full image name if found
    async fn search_image(
        &self,
        registry: &DockerRegistry,
        org: &str,
        repo: &str,
    ) -> Result<Option<String>, AgentError>;
    
    /// Build a binary from source using a Docker build environment
    /// Returns the path to the built binary
    async fn build_from_source(
        &self,
        source_config: &SourceBuildConfig,
        target_dir: &Path,
    ) -> Result<PathBuf, AgentError>;
}

/// Docker registry types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerRegistry {
    /// GitHub Container Registry (ghcr.io)
    GitHubContainerRegistry,
    /// Docker Hub (docker.io)
    DockerHub,
    /// Custom registry
    Custom(String),
}

impl DockerRegistry {
    /// Get the registry URL
    pub fn url(&self) -> &str {
        match self {
            DockerRegistry::GitHubContainerRegistry => "ghcr.io",
            DockerRegistry::DockerHub => "docker.io",
            DockerRegistry::Custom(url) => url,
        }
    }
}

/// Configuration for building from source
#[derive(Debug, Clone)]
pub struct SourceBuildConfig {
    /// Repository URL (git or archive)
    pub repository: String,
    /// Git reference (branch, tag, or commit hash)
    pub git_ref: Option<String>,
    /// Docker image to use for building
    pub build_image: String,
    /// Build command to execute
    pub build_command: String,
    /// Path to the binary within the build output
    pub binary_path: String,
    /// Additional environment variables for the build
    pub build_env: std::collections::HashMap<String, String>,
}

/// Platform detection utilities
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    pub os: String,
    pub arch: String,
}

impl Platform {
    /// Get the current platform
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
    
    /// Convert to a common format used in release assets
    pub fn to_asset_format(&self) -> String {
        let os = match self.os.as_str() {
            "macos" => "darwin",
            "windows" => "windows",
            "linux" => "linux",
            other => other,
        };
        
        let arch = match self.arch.as_str() {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            other => other,
        };
        
        format!("{}-{}", os, arch)
    }
}

/// Priority constants for strategies
pub mod priority {
    /// GitHub releases have highest priority (fastest, most reliable)
    pub const GITHUB_RELEASES: u8 = 10;
    /// Docker registries have medium priority
    pub const DOCKER_REGISTRY: u8 = 20;
    /// Source building has lowest priority (slowest, most complex)
    pub const SOURCE_BUILD: u8 = 30;
}