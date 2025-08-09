//! Docker Manager implementation for binary installation

use crate::errors::AgentError;
use crate::installation::traits::{DockerManager, DockerRegistry, SourceBuildConfig};
use async_trait::async_trait;
use bollard::container::LogOutput;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{
    CreateContainerOptions as BollardCreateContainerOptionsQuery,
    CreateImageOptions as BollardCreateImageOptionsQuery,
    LogsOptions as BollardLogsOptionsQuery,
    RemoveContainerOptions as BollardRemoveContainerOptionsQuery,
    StartContainerOptions as BollardStartContainerOptionsQuery,
    StopContainerOptions as BollardStopContainerOptionsQuery,
    WaitContainerOptions as BollardWaitContainerOptionsQuery,
    DownloadFromContainerOptions as BollardDownloadFromContainerOptionsQuery,
};
use bollard::Docker;
use futures_util::stream::StreamExt;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Docker Manager implementation using bollard
pub struct DockerManagerImpl {
    docker: Docker,
    timeout_seconds: u64,
}

impl DockerManagerImpl {
    /// Create a new Docker Manager
    pub async fn new(timeout_seconds: u64) -> Result<Self, AgentError> {
        let docker = Docker::connect_with_local_defaults()
            .map_err(|e| AgentError::RuntimeError(format!("Failed to connect to Docker: {}", e)))?;
        
        Ok(Self {
            docker,
            timeout_seconds,
        })
    }

    /// Get the full image name for a registry
    fn get_image_name(&self, registry: &DockerRegistry, org: &str, repo: &str) -> String {
        match registry {
            DockerRegistry::DockerHub => format!("{}/{}", org, repo),
            DockerRegistry::GitHubContainerRegistry => format!("ghcr.io/{}/{}", org, repo),
            DockerRegistry::Custom(url) => format!("{}/{}/{}", url, org, repo),
        }
    }

    /// Copy a file from container to host
    async fn copy_file_from_container(
        &self,
        container_id: &str,
        container_path: &str,
        host_path: &Path,
    ) -> Result<(), AgentError> {
        // Create a temporary tar archive to extract the file
        let options = BollardDownloadFromContainerOptionsQuery {
            path: container_path.to_string(),
        };
        let mut stream = self.docker.download_from_container(container_id, Some(options));
        let mut archive_data = Vec::new();
        
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| AgentError::RuntimeError(format!("Failed to download file: {}", e)))?;
            archive_data.extend_from_slice(&chunk);
        }
        
        // Extract the file from the tar archive
        let cursor = std::io::Cursor::new(archive_data);
        let mut archive = tar::Archive::new(cursor);
        
        for entry in archive.entries().map_err(|e| AgentError::RuntimeError(format!("Failed to read tar archive: {}", e)))? {
            let mut entry = entry.map_err(|e| AgentError::RuntimeError(format!("Failed to read tar entry: {}", e)))?;
            
            // Extract the file to the host path
            entry.unpack(host_path).map_err(|e| AgentError::RuntimeError(format!("Failed to extract file: {}", e)))?;
            break; // We only expect one file
        }
        
        Ok(())
    }
}

#[async_trait]
impl DockerManager for DockerManagerImpl {
    async fn is_available(&self) -> bool {
        // Test if Docker is available by pinging it
        self.docker.ping().await.is_ok()
    }

    async fn extract_binary_from_image(
        &self,
        image: &str,
        binary_name: &str,
        target_dir: &Path,
    ) -> Result<PathBuf, AgentError> {
        // Ensure the target directory exists
        tokio::fs::create_dir_all(target_dir).await
            .map_err(|e| AgentError::IoError(format!("Failed to create target directory: {}", e)))?;

        // Create container to extract binary from
        let container_name = format!("extract-{}-{}", binary_name, Uuid::new_v4());
        let options = Some(BollardCreateContainerOptionsQuery {
            name: Some(container_name.clone()),
            ..Default::default()
        });

        let config = ContainerCreateBody {
            image: Some(image.to_string()),
            cmd: Some(vec!["sleep".to_string(), "30".to_string()]),
            ..Default::default()
        };

        let container = self.docker.create_container(options, config).await
            .map_err(|e| AgentError::RuntimeError(format!("Failed to create container: {}", e)))?;

        // Start the container
        self.docker
            .start_container(&container.id, None::<BollardStartContainerOptionsQuery>)
            .await
            .map_err(|e| AgentError::RuntimeError(format!("Failed to start container: {}", e)))?;

        // Common binary locations to search
        let binary_paths = vec![
            format!("/usr/bin/{}", binary_name),
            format!("/usr/local/bin/{}", binary_name),
            format!("/bin/{}", binary_name),
            format!("/app/{}", binary_name),
            format!("/{}", binary_name),
        ];

        let mut binary_found = false;
        let binary_output_path = target_dir.join(binary_name);

        for container_path in binary_paths {
            if let Ok(()) = self.copy_file_from_container(&container.id, &container_path, &binary_output_path).await {
                binary_found = true;
                break;
            }
        }

        // Clean up container
        let _ = self.docker.stop_container(&container.id, None::<BollardStopContainerOptionsQuery>).await;
        let _ = self.docker.remove_container(&container.id, None::<BollardRemoveContainerOptionsQuery>).await;

        if !binary_found {
            return Err(AgentError::RuntimeError(format!(
                "Binary '{}' not found in Docker image '{}'", binary_name, image
            )));
        }

        // Make binary executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(&binary_output_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            tokio::fs::set_permissions(&binary_output_path, permissions).await?;
        }

        Ok(binary_output_path)
    }

    async fn search_image(
        &self,
        registry: &DockerRegistry,
        org: &str,
        repo: &str,
    ) -> Result<Option<String>, AgentError> {
        let image_name = self.get_image_name(registry, org, repo);
        
        // Try to pull the image to check if it exists
        let pull_options = Some(BollardCreateImageOptionsQuery {
            from_image: Some(image_name.clone()),
            ..Default::default()
        });

        let mut pull_stream = self.docker.create_image(pull_options, None, None);
        let mut pull_successful = false;

        while let Some(result) = pull_stream.next().await {
            match result {
                Ok(_) => {
                    pull_successful = true;
                    break;
                }
                Err(e) => {
                    log::debug!("Failed to pull image {}: {}", image_name, e);
                    break;
                }
            }
        }

        if pull_successful {
            Ok(Some(image_name))
        } else {
            Ok(None)
        }
    }

    async fn build_from_source(
        &self,
        source_config: &SourceBuildConfig,
        target_dir: &Path,
    ) -> Result<PathBuf, AgentError> {
        // Ensure the target directory exists
        tokio::fs::create_dir_all(target_dir).await
            .map_err(|e| AgentError::IoError(format!("Failed to create target directory: {}", e)))?;

        // Create container for building
        let container_name = format!("build-{}", Uuid::new_v4());
        let options = Some(BollardCreateContainerOptionsQuery {
            name: Some(container_name.clone()),
            ..Default::default()
        });

        // Convert build environment to Docker format
        let mut env_vars = Vec::new();
        for (key, value) in &source_config.build_env {
            env_vars.push(format!("{}={}", key, value));
        }

        let config = ContainerCreateBody {
            image: Some(source_config.build_image.clone()),
            cmd: Some(vec![
                "sh".to_string(),
                "-c".to_string(),
                format!(
                    "git clone {} /workspace && cd /workspace && {} && {}",
                    source_config.repository,
                    source_config.git_ref.as_ref().map(|r| format!("git checkout {}", r)).unwrap_or_default(),
                    source_config.build_command
                ),
            ]),
            working_dir: Some("/workspace".to_string()),
            env: if env_vars.is_empty() { None } else { Some(env_vars) },
            ..Default::default()
        };

        let container = self.docker.create_container(options, config).await
            .map_err(|e| AgentError::RuntimeError(format!("Failed to create build container: {}", e)))?;

        // Start the container
        self.docker
            .start_container(&container.id, None::<BollardStartContainerOptionsQuery>)
            .await
            .map_err(|e| AgentError::RuntimeError(format!("Failed to start build container: {}", e)))?;

        // Wait for build to complete
        let mut wait_stream = self.docker.wait_container(&container.id, None::<BollardWaitContainerOptionsQuery>);
        let timeout_future = tokio::time::sleep(tokio::time::Duration::from_secs(self.timeout_seconds));

        let wait_outcome = tokio::select! {
            res = wait_stream.next() => res,
            _ = timeout_future => {
                log::warn!("Build timed out for container {}", container.id);
                let _ = self.docker.stop_container(&container.id, None::<BollardStopContainerOptionsQuery>).await;
                return Err(AgentError::RuntimeError("Build timed out".to_string()));
            }
        };

        let container_wait_response = match wait_outcome {
            Some(Ok(response)) => response,
            Some(Err(e)) => {
                let _ = self.docker.remove_container(&container.id, None::<BollardRemoveContainerOptionsQuery>).await;
                return Err(AgentError::RuntimeError(format!("Build failed: {}", e)));
            }
            None => {
                let _ = self.docker.remove_container(&container.id, None::<BollardRemoveContainerOptionsQuery>).await;
                return Err(AgentError::RuntimeError("Build failed unexpectedly".to_string()));
            }
        };

        // Check if build was successful
        if container_wait_response.status_code != 0 {
            // Get logs for debugging
            let mut output_stream = self.docker.logs(
                &container.id,
                Some(BollardLogsOptionsQuery {
                    stdout: true,
                    stderr: true,
                    ..Default::default()
                }),
            );

            let mut stderr = String::new();
            while let Some(log_result) = output_stream.next().await {
                if let Ok(log_output) = log_result {
                    if let LogOutput::StdErr { message } = log_output {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                }
            }

            let _ = self.docker.remove_container(&container.id, None::<BollardRemoveContainerOptionsQuery>).await;
            return Err(AgentError::RuntimeError(format!("Build failed with exit code {}: {}", container_wait_response.status_code, stderr)));
        }

        // Extract the built binary
        let container_binary_path = format!("/workspace/{}", source_config.binary_path);
        let host_binary_path = target_dir.join(
            PathBuf::from(&source_config.binary_path)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("binary"))
        );

        self.copy_file_from_container(&container.id, &container_binary_path, &host_binary_path).await?;

        // Clean up container
        let _ = self.docker.remove_container(&container.id, None::<BollardRemoveContainerOptionsQuery>).await;

        // Make binary executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = tokio::fs::metadata(&host_binary_path).await?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            tokio::fs::set_permissions(&host_binary_path, permissions).await?;
        }

        log::info!("Successfully built binary from source using Docker");
        Ok(host_binary_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_docker_manager_creation() {
        let manager = DockerManagerImpl::new(300).await;
        // This test might fail if Docker is not available
        if manager.is_ok() {
            let manager = manager.unwrap();
            // Just verify the manager was created successfully
            assert!(manager.timeout_seconds == 300);
        }
    }

    #[test]
    fn test_image_name_generation() {
        let manager = DockerManagerImpl {
            docker: Docker::connect_with_local_defaults().unwrap(),
            timeout_seconds: 300,
        };

        assert_eq!(
            manager.get_image_name(&DockerRegistry::DockerHub, "owner", "repo"),
            "owner/repo"
        );
        assert_eq!(
            manager.get_image_name(&DockerRegistry::GitHubContainerRegistry, "owner", "repo"),
            "ghcr.io/owner/repo"
        );
        assert_eq!(
            manager.get_image_name(&DockerRegistry::Custom("registry.example.com".to_string()), "owner", "repo"),
            "registry.example.com/owner/repo"
        );
    }

    #[test]
    fn test_source_build_config_usage() {
        let config = SourceBuildConfig {
            repository: "https://github.com/test/repo".to_string(),
            git_ref: Some("main".to_string()),
            build_image: "rust:latest".to_string(),
            build_command: "cargo build --release".to_string(),
            binary_path: "target/release/test-binary".to_string(),
            build_env: HashMap::new(),
        };

        assert_eq!(config.repository, "https://github.com/test/repo");
        assert_eq!(config.git_ref, Some("main".to_string()));
        assert_eq!(config.build_image, "rust:latest");
        assert_eq!(config.build_command, "cargo build --release");
        assert_eq!(config.binary_path, "target/release/test-binary");
    }
}