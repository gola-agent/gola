// src/executors/docker.rs
use async_trait::async_trait;
use bollard::container::LogOutput; // For LogOutput::StdOut, LogOutput::StdErr
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::{
    CreateContainerOptions as BollardCreateContainerOptionsQuery,
    LogsOptions as BollardLogsOptionsQuery,
    StartContainerOptions as BollardStartContainerOptionsQuery,
    StopContainerOptions as BollardStopContainerOptionsQuery,
    WaitContainerOptions as BollardWaitContainerOptionsQuery,
};
use bollard::Docker;
use futures_util::stream::StreamExt;
use std::default::Default;
use tempfile::Builder;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::{CodeExecutor, ExecutionResult};
use crate::errors::DockerExecutorError;

pub struct DockerCodeExecutor {
    docker: Docker,
    timeout_seconds: u64,
}

impl DockerCodeExecutor {
    pub async fn new(timeout_seconds: u64) -> Result<Self, DockerExecutorError> {
        let docker = Docker::connect_with_local_defaults()?;
        // let mut language_to_image = std::collections::HashMap::new();
        // language_to_image.insert("python".to_string(), "python:3.10-slim".to_string());
        // language_to_image.insert("javascript".to_string(), "node:18-slim".to_string());
        Ok(Self {
            docker,
            /* language_to_image, */ timeout_seconds,
        })
    }

    fn get_image_and_command(
        &self,
        language: &str,
        script_path_in_container: &str,
    ) -> (String, Vec<String>) {
        match language.to_lowercase().as_str() {
            "python" | "python3" => (
                "python:3.10-slim".to_string(),
                vec!["python".to_string(), script_path_in_container.to_string()],
            ),
            "javascript" | "node" | "nodejs" => (
                "node:18-slim".to_string(),
                vec!["node".to_string(), script_path_in_container.to_string()],
            ),
            // Add more languages here
            _ => (
                "alpine:latest".to_string(),
                vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    format!("echo 'Unsupported language: {}'; exit 1", language),
                ],
            ),
        }
    }
}

#[async_trait]
impl CodeExecutor for DockerCodeExecutor {
    async fn execute_code(
        &self,
        language: &str,
        code: &str,
    ) -> Result<ExecutionResult, DockerExecutorError> {
        let temp_dir = Builder::new().prefix("code-exec-").tempdir()?;
        let host_temp_dir_path = temp_dir
            .path()
            .to_str()
            .ok_or_else(|| DockerExecutorError::TempFileError("Invalid temp path".to_string()))?
            .to_string();

        let script_extension = match language.to_lowercase().as_str() {
            "python" | "python3" => "py",
            "javascript" | "node" | "nodejs" => "js",
            _ => "sh",
        };
        let script_filename = format!("script_{}.{}", Uuid::new_v4(), script_extension);
        let host_script_path = temp_dir.path().join(&script_filename);

        let mut file = fs::File::create(&host_script_path).await?;
        file.write_all(code.as_bytes()).await?;
        file.flush().await?; // Ensure data is written

        let container_work_dir = "/app";
        let script_path_in_container = format!("{}/{}", container_work_dir, script_filename);

        let (image_name, cmd_strings) =
            self.get_image_and_command(language, &script_path_in_container);
        // cmd_strs is no longer needed as ContainerCreateBody takes Vec<String> for Cmd

        let options = Some(BollardCreateContainerOptionsQuery {
            name: Some(format!("code-exec-{}", Uuid::new_v4())),
            ..Default::default()
        });

        let config = ContainerCreateBody {
            image: Some(image_name),
            cmd: Some(cmd_strings),
            working_dir: Some(container_work_dir.to_string()),
            host_config: Some(bollard::models::HostConfig {
                binds: Some(vec![format!(
                    "{}:{}",
                    host_temp_dir_path, container_work_dir
                )]),
                auto_remove: Some(true),
                ..Default::default()
            }),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let container = self.docker.create_container(options, config).await?;
        self.docker
            .start_container(&container.id, None::<BollardStartContainerOptionsQuery>)
            .await?;

        // wait_container returns a stream. We need to await the next item for the result.
        let mut exec_stream = self
            .docker
            .wait_container(&container.id, None::<BollardWaitContainerOptionsQuery>);
        let timeout_future =
            tokio::time::sleep(tokio::time::Duration::from_secs(self.timeout_seconds));

        let wait_outcome = tokio::select! {
            res = exec_stream.next() => res,
            _ = timeout_future => {
                // Timeout occurred, try to stop and remove the container
                log::warn!("Execution timed out for container {}", container.id);
                let _ = self.docker.stop_container(&container.id, None::<BollardStopContainerOptionsQuery>).await;
                // AutoRemove should handle removal, but explicit removal can be added if needed.
                return Err(DockerExecutorError::Timeout);
            }
        };

        // Process the outcome of the wait operation
        let container_wait_response = match wait_outcome {
            Some(Ok(response)) => response,
            Some(Err(e)) => return Err(DockerExecutorError::BollardError(e)),
            None => {
                return Err(DockerExecutorError::ContainerFailed {
                    // Should not happen if container started
                    exit_code: None,
                    stdout: "Container wait stream ended unexpectedly".to_string(),
                    stderr: "".to_string(),
                })
            }
        };

        let mut output_stream = self.docker.logs(
            &container.id,
            Some(BollardLogsOptionsQuery {
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        );

        let mut stdout = String::new();
        let mut stderr = String::new();
        while let Some(log_result) = output_stream.next().await {
            match log_result {
                Ok(log_output) => match log_output {
                    LogOutput::StdOut { message } => {
                        stdout.push_str(std::str::from_utf8(&message)?)
                    }
                    LogOutput::StdErr { message } => {
                        stderr.push_str(std::str::from_utf8(&message)?)
                    }
                    _ => {}
                },
                Err(e) => return Err(DockerExecutorError::BollardError(e)),
            }
        }

        let exit_code = container_wait_response.status_code;

        if exit_code != 0 {
            return Err(DockerExecutorError::ContainerFailed {
                exit_code: Some(exit_code),
                stdout,
                stderr,
            });
        }

        Ok(ExecutionResult { stdout, stderr })
    }
}
