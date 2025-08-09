//! Binary and container runtime installation management.
//!
//! Provides automated acquisition and lifecycle management for external binaries
//! and Docker containers required by agent execution environments.

use crate::config::types::BinaryConfig;
use crate::errors::AgentError;
#[allow(deprecated)]
use bollard::image::CreateImageOptions;
#[allow(deprecated)]
use bollard::container::{RemoveContainerOptions, CreateContainerOptions, StartContainerOptions};
use bollard::models::{ContainerCreateResponse, ContainerCreateBody};
use bollard::Docker;
use futures_util::stream::StreamExt;
use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub struct Installer {
    gola_home: PathBuf,
}

impl Installer {
    pub fn new(gola_home: PathBuf) -> Self {
        Self { gola_home }
    }

    pub async fn ensure_binary(&self, config: &BinaryConfig) -> Result<PathBuf, AgentError> {
        let bin_dir = self.gola_home.join("bin");
        fs::create_dir_all(&bin_dir)?;
        let bin_path = bin_dir.join(&config.name);

        if bin_path.exists() {
            return Ok(bin_path);
        }

        match &config.source {
            crate::config::types::BinarySource::Docker { image, tag, container_name } => {
                self.pull_and_run_docker_image(image, tag, container_name).await?;
                Ok(bin_path)
            }
            crate::config::types::BinarySource::GitHub { repo, asset_name } => {
                let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
                let client = reqwest::Client::new();
                let response = client.get(&url).header("User-Agent", "gola-agent").send().await?;
                let release: serde_json::Value = response.json().await?;
                let assets = release["assets"].as_array().ok_or_else(|| AgentError::InstallerError("No assets found in release".to_string()))?;
                let asset = assets.iter().find(|a| a["name"].as_str() == Some(asset_name)).ok_or_else(|| AgentError::InstallerError(format!("Asset '{}' not found", asset_name)))?;
                let download_url = asset["browser_download_url"].as_str().ok_or_else(|| AgentError::InstallerError("No download URL for asset".to_string()))?;

                let response = reqwest::get(download_url).await?;
                let bytes = response.bytes().await?;
                let cursor = Cursor::new(bytes);

                if asset_name.ends_with(".zip") {
                    self.extract_zip(cursor, &bin_dir, &config.name)?;
                } else {
                    fs::write(&bin_path, cursor.into_inner())?;
                }

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let mut perms = fs::metadata(&bin_path)?.permissions();
                    perms.set_mode(0o755);
                    fs::set_permissions(&bin_path, perms)?;
                }

                Ok(bin_path)
            }
        }
    }

    fn extract_zip(&self, reader: Cursor<impl AsRef<[u8]>>, out_dir: &Path, _binary_name: &str) -> Result<(), AgentError> {
        let mut archive = ZipArchive::new(reader).map_err(|e| AgentError::InstallerError(e.to_string()))?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| AgentError::InstallerError(e.to_string()))?;
            let outpath = out_dir.join(file.name());

            if (*file.name()).ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(&p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }
        }
        Ok(())
    }

    async fn pull_and_run_docker_image(&self, image: &str, tag: &str, container_name: &str) -> Result<(), AgentError> {
        let docker = Docker::connect_with_local_defaults().map_err(|e| AgentError::InstallerError(e.to_string()))?;
        let image_with_tag = format!("{}:{}", image, tag);

        #[allow(deprecated)]
        let create_result = docker
            .create_image(
                Some(CreateImageOptions {
                    from_image: image_with_tag.clone(),
                    ..Default::default()
                }),
                None,
                None,
            )
;
        create_result.for_each(|info| async {
                match info {
                    Ok(info) => log::debug!("Pulling image: {:?}", info),
                    Err(e) => log::error!("Error pulling image: {}", e),
                }
            })
            .await;

        #[allow(deprecated)]
        let _ = docker.remove_container(container_name, Some(RemoveContainerOptions {
            force: true,
            ..Default::default()
        })).await;

        #[allow(deprecated)]
        let options = Some(CreateContainerOptions {
            name: container_name,
            ..Default::default()
        });

        let container_config = ContainerCreateBody {
            image: Some(image_with_tag.to_string()),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            open_stdin: Some(true),
            ..Default::default()
        };

        let _response: ContainerCreateResponse = docker.create_container(options, container_config).await.map_err(|e| AgentError::InstallerError(e.to_string()))?;
        #[allow(deprecated)]
        docker.start_container(container_name, None::<StartContainerOptions<String>>).await.map_err(|e| AgentError::InstallerError(e.to_string()))?;

        Ok(())
    }
}
