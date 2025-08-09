//! GitHub release strategy for binary installation

use crate::errors::AgentError;
use crate::installation::traits::{InstallationStrategy, Platform, priority};
use crate::installation::errors::InstallationError;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// GitHub release strategy implementation
#[derive(Debug, Clone)]
pub struct GitHubReleaseStrategy {
    pub org: String,
    pub repo: String,
    pub asset_pattern: Option<String>,
    pub version: Option<String>,
    pub client: Client,
}

impl GitHubReleaseStrategy {
    /// Create a new GitHub release strategy
    pub fn new(org: String, repo: String) -> Self {
        Self {
            org,
            repo,
            asset_pattern: None,
            version: None,
            client: Client::new(),
        }
    }

    /// Set the asset pattern for matching release assets
    pub fn with_asset_pattern(mut self, pattern: String) -> Self {
        self.asset_pattern = Some(pattern);
        self
    }

    /// Set a specific version to download
    pub fn with_version(mut self, version: String) -> Self {
        self.version = Some(version);
        self
    }

    /// Get the GitHub API URL for releases
    fn get_releases_url(&self) -> String {
        if let Some(version) = &self.version {
            format!("https://api.github.com/repos/{}/{}/releases/tags/{}", 
                    self.org, self.repo, version)
        } else {
            format!("https://api.github.com/repos/{}/{}/releases/latest", 
                    self.org, self.repo)
        }
    }

    /// Find the best matching asset for the current platform
    fn find_matching_asset<'a>(&self, release: &'a GitHubRelease, binary_name: &str) -> Option<&'a GitHubAsset> {
        let platform = Platform::current();
        let platform_str = platform.to_asset_format();
        
        // Try custom pattern first
        if let Some(pattern) = &self.asset_pattern {
            let expected_name = pattern
                .replace("{name}", binary_name)
                .replace("{platform}", &platform_str)
                .replace("{version}", &release.tag_name);
            
            if let Some(asset) = release.assets.iter().find(|a| a.name == expected_name) {
                return Some(asset);
            }
        }
        
        // Fallback to heuristic matching
        release.assets.iter().find(|asset| {
            let name = asset.name.to_lowercase();
            
            // Must contain the binary name
            if !name.contains(&binary_name.to_lowercase()) {
                return false;
            }
            
            // Must match platform
            let platform_matches = name.contains(&platform.os.to_lowercase()) || 
                                  name.contains("darwin") && platform.os == "macos" ||
                                  name.contains("linux") && platform.os == "linux" ||
                                  name.contains("windows") && platform.os == "windows";
            
            // Must match architecture
            let arch_matches = name.contains(&platform.arch) ||
                             name.contains("amd64") && platform.arch == "x86_64" ||
                             name.contains("arm64") && platform.arch == "aarch64";
            
            platform_matches && arch_matches
        })
    }

    /// Download and extract the binary from the asset
    async fn download_and_extract(&self, asset: &GitHubAsset, binary_name: &str, target_dir: &Path) -> Result<PathBuf, InstallationError> {
        // Download the asset
        let response = self.client.get(&asset.browser_download_url)
            .send()
            .await?;
        
        if !response.status().is_success() {
            return Err(InstallationError::GitHubApiError {
                message: format!("Failed to download asset: {}", response.status()),
            });
        }
        
        let asset_bytes = response.bytes().await?;
        
        // Determine the file type and extract accordingly
        let binary_path = if asset.name.ends_with(".tar.gz") || asset.name.ends_with(".tgz") {
            self.extract_tar_gz(&asset_bytes, binary_name, target_dir).await?
        } else if asset.name.ends_with(".zip") {
            self.extract_zip(&asset_bytes, binary_name, target_dir).await?
        } else {
            // Assume it's a raw binary
            let binary_path = target_dir.join(binary_name);
            fs::write(&binary_path, &asset_bytes).await?;
            self.make_executable(&binary_path).await?;
            binary_path
        };
        
        Ok(binary_path)
    }

    /// Extract binary from tar.gz archive
    async fn extract_tar_gz(&self, data: &[u8], binary_name: &str, target_dir: &Path) -> Result<PathBuf, InstallationError> {
        use async_compression::tokio::bufread::GzipDecoder;
        use tokio_tar::Archive;
        use futures_util::StreamExt;
        
        let cursor = std::io::Cursor::new(data);
        let buf_reader = tokio::io::BufReader::new(cursor);
        let gz_decoder = GzipDecoder::new(buf_reader);
        let mut archive = Archive::new(gz_decoder);
        
        let mut entries = archive.entries().map_err(|e| InstallationError::IoError {
            message: format!("Failed to read tar.gz entries: {}", e),
        })?;
        
        while let Some(entry) = entries.next().await {
            let mut entry = entry.map_err(|e| InstallationError::IoError {
                message: format!("Failed to read tar.gz entry: {}", e),
            })?;
            
            let path = entry.path().map_err(|e| InstallationError::IoError {
                message: format!("Failed to get entry path: {}", e),
            })?;
            
            // Look for the binary in the archive
            if let Some(filename) = path.file_name() {
                if filename == binary_name {
                    let binary_path = target_dir.join(binary_name);
                    entry.unpack(&binary_path).await.map_err(|e| InstallationError::IoError {
                        message: format!("Failed to extract binary: {}", e),
                    })?;
                    self.make_executable(&binary_path).await?;
                    return Ok(binary_path);
                }
            }
        }
        
        Err(InstallationError::BinaryNotFound {
            name: binary_name.to_string(),
        })
    }

    /// Extract binary from zip archive
    async fn extract_zip(&self, data: &[u8], binary_name: &str, target_dir: &Path) -> Result<PathBuf, InstallationError> {
        use zip::ZipArchive;
        use std::io::{Cursor, Read};
        
        let cursor = Cursor::new(data);
        let mut archive = ZipArchive::new(cursor).map_err(|e| InstallationError::IoError {
            message: format!("Failed to read zip archive: {}", e),
        })?;
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| InstallationError::IoError {
                message: format!("Failed to read zip entry: {}", e),
            })?;
            
            if let Some(path) = file.enclosed_name() {
                if let Some(filename) = path.file_name() {
                    if filename == binary_name {
                        let binary_path = target_dir.join(binary_name);
                    
                        // Read the file content into a buffer
                        let mut buffer = Vec::new();
                        file.read_to_end(&mut buffer).map_err(|e| InstallationError::IoError {
                            message: format!("Failed to read zip entry content: {}", e),
                        })?;
                        
                        // Write to the output file
                        fs::write(&binary_path, &buffer).await?;
                        self.make_executable(&binary_path).await?;
                        return Ok(binary_path);
                    }
                }
            }
        }
        
        Err(InstallationError::BinaryNotFound {
            name: binary_name.to_string(),
        })
    }

    /// Make the binary executable (Unix only)
    async fn make_executable(&self, binary_path: &Path) -> Result<(), InstallationError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(binary_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(binary_path, permissions).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl InstallationStrategy for GitHubReleaseStrategy {
    async fn is_available(&self, binary_name: &str) -> Result<bool, AgentError> {
        let url = self.get_releases_url();
        
        let response = self.client.get(&url)
            .header("User-Agent", "gola-binary-installer")
            .send()
            .await
            .map_err(|e| AgentError::RuntimeError(format!("GitHub API request failed: {}", e)))?;
        
        if response.status().is_success() {
            let release: GitHubRelease = response.json().await
                .map_err(|e| AgentError::RuntimeError(format!("Failed to parse GitHub release: {}", e)))?;
            
            Ok(self.find_matching_asset(&release, binary_name).is_some())
        } else {
            Ok(false)
        }
    }

    async fn install(&self, binary_name: &str, target_dir: &Path) -> Result<PathBuf, AgentError> {
        let url = self.get_releases_url();
        
        let response = self.client.get(&url)
            .header("User-Agent", "gola-binary-installer")
            .send()
            .await
            .map_err(|e| AgentError::RuntimeError(format!("GitHub API request failed: {}", e)))?;
        
        if !response.status().is_success() {
            return Err(AgentError::RuntimeError(format!(
                "GitHub API returned status: {}", response.status()
            )));
        }
        
        let release: GitHubRelease = response.json().await
            .map_err(|e| AgentError::RuntimeError(format!("Failed to parse GitHub release: {}", e)))?;
        
        let asset = self.find_matching_asset(&release, binary_name)
            .ok_or_else(|| AgentError::RuntimeError(format!(
                "No matching asset found for binary '{}' on platform '{}'", 
                binary_name, Platform::current().to_asset_format()
            )))?;
        
        // Ensure target directory exists
        fs::create_dir_all(target_dir).await
            .map_err(|e| AgentError::IoError(format!("Failed to create target directory: {}", e)))?;
        
        let binary_path = self.download_and_extract(asset, binary_name, target_dir)
            .await
            .map_err(|e| AgentError::from(e))?;
        
        log::info!("Successfully installed {} from GitHub release {}", binary_name, release.tag_name);
        Ok(binary_path)
    }

    fn get_priority(&self) -> u8 {
        priority::GITHUB_RELEASES
    }

    fn get_name(&self) -> &'static str {
        "github-releases"
    }
}

/// GitHub release API response
#[derive(Debug, Deserialize, Serialize)]
struct GitHubRelease {
    tag_name: String,
    name: String,
    assets: Vec<GitHubAsset>,
}

/// GitHub release asset
#[derive(Debug, Deserialize, Serialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_strategy_creation() {
        let strategy = GitHubReleaseStrategy::new("owner".to_string(), "repo".to_string());
        assert_eq!(strategy.org, "owner");
        assert_eq!(strategy.repo, "repo");
        assert_eq!(strategy.get_name(), "github-releases");
        assert_eq!(strategy.get_priority(), priority::GITHUB_RELEASES);
    }

    #[test]
    fn test_github_strategy_builder() {
        let strategy = GitHubReleaseStrategy::new("owner".to_string(), "repo".to_string())
            .with_asset_pattern("{name}-{platform}".to_string())
            .with_version("v1.0.0".to_string());
        
        assert_eq!(strategy.asset_pattern, Some("{name}-{platform}".to_string()));
        assert_eq!(strategy.version, Some("v1.0.0".to_string()));
    }

    #[test]
    fn test_releases_url_generation() {
        let strategy = GitHubReleaseStrategy::new("owner".to_string(), "repo".to_string());
        assert_eq!(strategy.get_releases_url(), "https://api.github.com/repos/owner/repo/releases/latest");
        
        let strategy = strategy.with_version("v1.0.0".to_string());
        assert_eq!(strategy.get_releases_url(), "https://api.github.com/repos/owner/repo/releases/tags/v1.0.0");
    }
}