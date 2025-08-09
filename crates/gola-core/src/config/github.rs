//! GitHub repository configuration loading
//! 
//! This module handles downloading and caching configurations from GitHub repositories,
//! similar to how Nix flakes work.

use crate::errors::AgentError;
use std::path::{Path, PathBuf};
use std::fs;
use reqwest;
use tar::Archive;
use flate2::read::GzDecoder;
use sha2::{Sha256, Digest};

use super::{DocumentSourceType, GitHubRef, GolaConfig, SchemaSourceType};

/// GitHub repository configuration loader
pub struct GitHubConfigLoader {
    cache_dir: PathBuf,
    client: reqwest::Client,
}

impl GitHubConfigLoader {
    /// Create a new GitHub configuration loader
    pub fn new() -> Result<Self, AgentError> {
        let cache_dir = Self::get_cache_dir()?;
        
        // Ensure cache directory exists
        fs::create_dir_all(&cache_dir)
            .map_err(|e| AgentError::ConfigError(format!("Failed to create cache directory {}: {}", cache_dir.display(), e)))?;

        let mut client_builder = reqwest::Client::builder()
            .user_agent("gola-cli/0.1.0");
        
        // For testing with self-signed certificates (e.g., github-mock)
        if std::env::var("GOLA_ACCEPT_INVALID_CERTS").is_ok() {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }
        
        let client = client_builder
            .build()
            .map_err(|e| AgentError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            cache_dir,
            client,
        })
    }

    /// Parse a GitHub repository reference from a string
    /// Supports formats like:
    /// - github:owner/repo
    /// - github:owner/repo@branch
    /// - github:owner/repo@tag
    /// - github:owner/repo@commit-sha
    /// - github:owner/repo/path/to/config.yaml
    pub fn parse_github_ref(input: &str) -> Result<GitHubRef, AgentError> {
        if !input.starts_with("github:") {
            return Err(AgentError::ConfigError(format!("Invalid GitHub reference: {}", input)));
        }

        let without_prefix = &input[7..]; // Remove "github:" prefix
        
        // Split by @ to separate repo from ref
        let (repo_part, ref_and_path) = if let Some(at_pos) = without_prefix.find('@') {
            (&without_prefix[..at_pos], Some(&without_prefix[at_pos + 1..]))
        } else {
            (without_prefix, None)
        };

        // Parse owner/repo from the repo part
        let repo_parts: Vec<&str> = repo_part.split('/').collect();
        if repo_parts.len() < 2 {
            return Err(AgentError::ConfigError(format!("Invalid GitHub repository format: {}", input)));
        }

        let owner = repo_parts[0].to_string();
        let repo = repo_parts[1].to_string();
        
        // Handle additional path components in repo_part (for cases without @)
        let mut config_path = if repo_parts.len() > 2 {
            repo_parts[2..].join("/")
        } else {
            "gola.yaml".to_string()
        };

        let git_ref = if let Some(ref_and_path) = ref_and_path {
            // Split ref_and_path by / to separate ref from path
            let ref_parts: Vec<&str> = ref_and_path.split('/').collect();
            let git_ref = ref_parts[0].to_string();
            
            // If there are path components after the ref, use them as config_path
            if ref_parts.len() > 1 {
                config_path = ref_parts[1..].join("/");
            }
            
            git_ref
        } else {
            "main".to_string()
        };

        Ok(GitHubRef {
            owner,
            repo,
            git_ref,
            config_path,
        })
    }

    pub async fn load_from_github(&self, github_ref: &GitHubRef) -> Result<(GolaConfig, PathBuf), AgentError> {
        // Download and extract the repository
        let repo_dir = self.download_and_extract_repo(github_ref).await?;
        
        // Load the configuration file
        let config_file_path = repo_dir.join(&github_ref.config_path);
        if !config_file_path.exists() {
            return Err(AgentError::ConfigError(format!(
                "Configuration file {} not found in repository {}",
                github_ref.config_path,
                format!("{}/{}", github_ref.owner, github_ref.repo)
            )));
        }

        // Load the configuration with the repository directory as context
        let mut config = super::ConfigLoader::from_file(&config_file_path).await?;
        
        // Resolve relative paths in the configuration to be relative to the repo directory
        self.resolve_relative_paths(&mut config, &repo_dir)?;

        Ok((config, repo_dir))
    }

    async fn download_and_extract_repo(&self, github_ref: &GitHubRef) -> Result<PathBuf, AgentError> {
        // Generate cache key based on repo and ref
        let cache_key = self.generate_cache_key(github_ref);
        let cache_path = self.cache_dir.join(&cache_key);

        // Check if already cached
        if cache_path.exists() {
            log::debug!("Using cached repository: {}", cache_path.display());
            return Ok(cache_path);
        }

        log::info!("Downloading repository {}/{}@{}", github_ref.owner, github_ref.repo, github_ref.git_ref);

        // Download the tarball
        let download_url = format!(
            "https://github.com/{}/{}/archive/{}.tar.gz",
            github_ref.owner, github_ref.repo, github_ref.git_ref
        );

        let response = self.client.get(&download_url)
            .send()
            .await
            .map_err(|e| AgentError::ConfigError(format!("Failed to download repository: {}", e)))?;

        if !response.status().is_success() {
            return Err(AgentError::ConfigError(format!(
                "Failed to download repository: HTTP {}. Repository {}/{}@{} may not exist or be accessible.",
                response.status(),
                github_ref.owner,
                github_ref.repo,
                github_ref.git_ref
            )));
        }

        let bytes = response.bytes()
            .await
            .map_err(|e| AgentError::ConfigError(format!("Failed to read repository data: {}", e)))?;

        // Extract the tarball
        let tar_gz = GzDecoder::new(&bytes[..]);
        let mut archive = Archive::new(tar_gz);

        // Create temporary extraction directory
        let temp_dir = self.cache_dir.join(format!("{}.tmp", cache_key));
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir)
                .map_err(|e| AgentError::ConfigError(format!("Failed to clean temporary directory: {}", e)))?;
        }
        fs::create_dir_all(&temp_dir)
            .map_err(|e| AgentError::ConfigError(format!("Failed to create temporary directory: {}", e)))?;

        // Extract archive
        archive.unpack(&temp_dir)
            .map_err(|e| AgentError::ConfigError(format!("Failed to extract repository: {}", e)))?;

        // Find the extracted directory (GitHub creates a directory named repo-ref)
        let entries = fs::read_dir(&temp_dir)
            .map_err(|e| AgentError::ConfigError(format!("Failed to read extracted directory: {}", e)))?;

        let mut extracted_dir = None;
        for entry in entries {
            let entry = entry.map_err(|e| AgentError::ConfigError(format!("Failed to read directory entry: {}", e)))?;
            if entry.file_type().map_err(|e| AgentError::ConfigError(format!("Failed to get file type: {}", e)))?.is_dir() {
                extracted_dir = Some(entry.path());
                break;
            }
        }

        let extracted_dir = extracted_dir.ok_or_else(|| {
            AgentError::ConfigError("No directory found in extracted archive".to_string())
        })?;

        // Move to final cache location
        fs::rename(&extracted_dir, &cache_path)
            .map_err(|e| AgentError::ConfigError(format!("Failed to move extracted repository to cache: {}", e)))?;

        // Clean up temporary directory
        if temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }

        log::debug!("Repository cached at: {}", cache_path.display());
        Ok(cache_path)
    }

    fn generate_cache_key(&self, github_ref: &GitHubRef) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}/{}/{}", github_ref.owner, github_ref.repo, github_ref.git_ref));
        let hash = hasher.finalize();
        format!("{}_{}_{}_{:x}", github_ref.owner, github_ref.repo, github_ref.git_ref, hash)
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect()
    }

    fn resolve_relative_paths(&self, config: &mut GolaConfig, repo_dir: &Path) -> Result<(), AgentError> {
        // Resolve RAG document sources
        if let Some(rag) = &mut config.rag {
            for doc_source in &mut rag.document_sources {
                match &mut doc_source.source_type {
                    DocumentSourceType::Files { paths } => {
                        for path in paths {
                            if path.is_relative() {
                                *path = repo_dir.join(&path);
                            }
                        }
                    }
                    DocumentSourceType::Directory { path, .. } => {
                        if path.is_relative() {
                            *path = repo_dir.join(&path);
                        }
                    }
                    _ => {} // URLs and inline content don't need path resolution
                }
            }

            // Resolve persistence paths
            if let Some(persistence) = &mut rag.vector_store.persistence {
                if persistence.system_path.is_relative() {
                    persistence.system_path = repo_dir.join(&persistence.system_path);
                }
                if persistence.vector_store_path.is_relative() {
                    persistence.vector_store_path = repo_dir.join(&persistence.vector_store_path);
                }
            }
        }

        // Resolve prompt file paths
        if let Some(prompts) = &mut config.prompts {
            if let Some(fragments) = &mut prompts.fragments {
                for file_path in fragments.values_mut() {
                    let path = PathBuf::from(&file_path);
                    if path.is_relative() {
                        *file_path = repo_dir.join(path).to_string_lossy().to_string();
                    }
                }
            }
            if let Some(roles) = &mut prompts.roles {
                if let Some(system) = &mut roles.system {
                    for source in system {
                        if let super::types::PromptSource::File { file } = source {
                            let path = PathBuf::from(&file);
                            if path.is_relative() {
                                *file = repo_dir.join(path).to_string_lossy().to_string();
                            }
                        }
                    }
                }
            }
            if let Some(purposes) = &mut prompts.purposes {
                for purpose in purposes.values_mut() {
                    if let Some(assembly) = &mut purpose.assembly {
                        for source in assembly {
                            if let super::types::PromptSource::File { file } = source {
                                let path = PathBuf::from(&file);
                                if path.is_relative() {
                                    *file = repo_dir.join(path).to_string_lossy().to_string();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Resolve schema file paths
        if let Some(input_schema) = &mut config.agent.schema.input {
            if input_schema.source.source_type == SchemaSourceType::File {
                if let Some(location) = &input_schema.source.location {
                    let path = PathBuf::from(location);
                    if path.is_relative() {
                        input_schema.source.location = Some(repo_dir.join(path).to_string_lossy().to_string());
                    }
                }
            }
        }

        if let Some(output_schema) = &mut config.agent.schema.output {
            if output_schema.source.source_type == SchemaSourceType::File {
                if let Some(location) = &output_schema.source.location {
                    let path = PathBuf::from(location);
                    if path.is_relative() {
                        output_schema.source.location = Some(repo_dir.join(path).to_string_lossy().to_string());
                    }
                }
            }
        }

        // Resolve environment file paths
        for env_file in &mut config.environment.env_files {
            if env_file.is_relative() {
                *env_file = repo_dir.join(&env_file);
            }
        }

        // Resolve MCP server working directories
        for server in &mut config.mcp_servers {
            if let Some(ref mut cmd) = server.command {
                if let Some(working_dir) = &mut cmd.working_dir {
                    if working_dir.is_relative() {
                        *working_dir = repo_dir.join(&working_dir);
                    }
                }
            }
        }

        // Resolve logging file path
        if let Some(log_file) = &mut config.logging.file {
            if log_file.is_relative() {
                *log_file = repo_dir.join(&log_file);
            }
        }

        Ok(())
    }

    /// Get the cache directory for GitHub repositories
    fn get_cache_dir() -> Result<PathBuf, AgentError> {
        let cache_dir = if let Some(cache_home) = dirs::cache_dir() {
            cache_home.join("gola").join("github-repos")
        } else {
            // Fallback to home directory
            dirs::home_dir()
                .ok_or_else(|| AgentError::ConfigError("Unable to determine home directory".to_string()))?
                .join(".cache")
                .join("gola")
                .join("github-repos")
        };

        Ok(cache_dir)
    }

    /// Clear the GitHub repository cache
    pub fn clear_cache(&self) -> Result<(), AgentError> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir)
                .map_err(|e| AgentError::ConfigError(format!("Failed to clear cache: {}", e)))?;
        }
        Ok(())
    }

    /// List cached repositories
    pub fn list_cached_repos(&self) -> Result<Vec<String>, AgentError> {
        if !self.cache_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(&self.cache_dir)
            .map_err(|e| AgentError::ConfigError(format!("Failed to read cache directory: {}", e)))?;

        let mut repos = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| AgentError::ConfigError(format!("Failed to read cache entry: {}", e)))?;
            if entry.file_type().map_err(|e| AgentError::ConfigError(format!("Failed to get file type: {}", e)))?.is_dir() {
                repos.push(entry.file_name().to_string_lossy().to_string());
            }
        }

        Ok(repos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_ref_basic() {
        let github_ref = GitHubConfigLoader::parse_github_ref("github:owner/repo").unwrap();
        assert_eq!(github_ref.owner, "owner");
        assert_eq!(github_ref.repo, "repo");
        assert_eq!(github_ref.git_ref, "main");
        assert_eq!(github_ref.config_path, "gola.yaml");
    }

    #[test]
    fn test_parse_github_ref_with_branch() {
        let github_ref = GitHubConfigLoader::parse_github_ref("github:owner/repo@develop").unwrap();
        assert_eq!(github_ref.owner, "owner");
        assert_eq!(github_ref.repo, "repo");
        assert_eq!(github_ref.git_ref, "develop");
        assert_eq!(github_ref.config_path, "gola.yaml");
    }

    #[test]
    fn test_parse_github_ref_with_path() {
        let github_ref = GitHubConfigLoader::parse_github_ref("github:owner/repo/configs/agent.yaml").unwrap();
        assert_eq!(github_ref.owner, "owner");
        assert_eq!(github_ref.repo, "repo");
        assert_eq!(github_ref.git_ref, "main");
        assert_eq!(github_ref.config_path, "configs/agent.yaml");
    }

    #[test]
    fn test_parse_github_ref_with_branch_and_path() {
        let github_ref = GitHubConfigLoader::parse_github_ref("github:owner/repo@v1.0.0/configs/agent.yaml").unwrap();
        assert_eq!(github_ref.owner, "owner");
        assert_eq!(github_ref.repo, "repo");
        assert_eq!(github_ref.git_ref, "v1.0.0");
        assert_eq!(github_ref.config_path, "configs/agent.yaml");
    }

    #[test]
    fn test_parse_github_ref_invalid() {
        assert!(GitHubConfigLoader::parse_github_ref("invalid:owner/repo").is_err());
        assert!(GitHubConfigLoader::parse_github_ref("github:owner").is_err());
        assert!(GitHubConfigLoader::parse_github_ref("github:").is_err());
    }

    #[test]
    fn test_generate_cache_key() {
        let loader = GitHubConfigLoader::new().unwrap();
        let github_ref = GitHubRef {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            git_ref: "main".to_string(),
            config_path: "gola.yaml".to_string(),
        };

        let cache_key = loader.generate_cache_key(&github_ref);
        assert!(cache_key.starts_with("owner_repo_main_"));
        assert!(cache_key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
    }
}

