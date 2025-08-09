//! Runtime manager for MCP servers
//
// This module is responsible for resolving the runtime for an MCP server,
// detecting available toolchains, and constructing the final command to execute.

use crate::config::types::McpExecutionType;
use crate::errors::AgentError;
use async_trait::async_trait;
use dialoguer::{theme::ColorfulTheme, Confirm};
use std::env;
use std::fs::File;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use which::which;
use zip::ZipArchive;

// ----------------- Runtime Trait and Implementations -----------------

#[async_trait]
trait Runtime {
    async fn ensure_installed(&self, entry_point: &str) -> Result<(), AgentError>;
    fn get_command(&self, entry_point: &str, args: &[String]) -> Result<Command, AgentError>;
}

struct BunRuntime {
    manager: RuntimeManager,
}
#[async_trait]
impl Runtime for BunRuntime {
    async fn ensure_installed(&self, _entry_point: &str) -> Result<(), AgentError> {
        self.manager.ensure_bun_installed().await
    }

    fn get_command(&self, entry_point: &str, args: &[String]) -> Result<Command, AgentError> {
        let bun_path = self.manager.find_tool("bun").ok_or_else(|| {
            AgentError::RuntimeError("Could not find bun executable.".to_string())
        })?;

        let mut cmd = Command::new(bun_path);
        cmd.arg("x").arg("--yes").arg(entry_point).args(args);
        Ok(cmd)
    }
}

struct UvRuntime {
    manager: RuntimeManager,
}
#[async_trait]
impl Runtime for UvRuntime {
    async fn ensure_installed(&self, _entry_point: &str) -> Result<(), AgentError> {
        // Ensure UV is installed (which also installs uvx)
        self.manager.ensure_uv_installed().await?;
        
        // Verify uvx is available
        if self.manager.find_tool("uvx").is_none() {
            return Err(AgentError::RuntimeError(
                "uvx binary not found after UV installation".to_string(),
            ));
        }
        
        Ok(())
    }

    fn get_command(&self, entry_point: &str, args: &[String]) -> Result<Command, AgentError> {
        // Use 'uvx' binary for automatic package installation and execution
        // This is similar to how 'bun x --yes' works for Node.js packages
        let uvx_path = self
            .manager
            .find_tool("uvx")
            .ok_or_else(|| AgentError::RuntimeError("Could not find uvx executable.".to_string()))?;

        let mut cmd = Command::new(uvx_path);
        cmd.arg(entry_point).args(args);
        Ok(cmd)
    }
}

struct CargoRuntime {
    manager: RuntimeManager,
}
#[async_trait]
impl Runtime for CargoRuntime {
    async fn ensure_installed(&self, entry_point: &str) -> Result<(), AgentError> {
        self.manager.ensure_cargo_installed().await?;
        let cargo_path = self.manager.find_tool("cargo").ok_or_else(|| {
            AgentError::RuntimeError("Could not find cargo executable.".to_string())
        })?;

        let bin_name = entry_point
            .split('/')
            .last()
            .unwrap_or(entry_point)
            .replace(".git", "");
        if self.manager.find_tool(&bin_name).is_some() {
            return Ok(());
        }

        log::info!("Installing Rust package '{}' with cargo...", entry_point);
        let mut cmd = Command::new(cargo_path);
        cmd.arg("install").arg("--git").arg(entry_point);

        let output = cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::RuntimeError(format!(
                "cargo install failed for '{}': {}",
                entry_point, stderr
            )));
        }
        log::info!("Package '{}' installed successfully.", entry_point);
        Ok(())
    }

    fn get_command(&self, entry_point: &str, args: &[String]) -> Result<Command, AgentError> {
        let bin_name = entry_point
            .split('/')
            .last()
            .unwrap_or(entry_point)
            .replace(".git", "");
        let bin_path = self.manager.find_tool(&bin_name).ok_or_else(|| {
            AgentError::RuntimeError(format!(
                "Could not find installed binary '{}' after installation.",
                bin_name
            ))
        })?;

        let mut cmd = Command::new(bin_path);
        cmd.args(args);
        Ok(cmd)
    }
}

// ----------------- RuntimeManager -----------------

#[derive(Clone)]
pub struct RuntimeManager {
    local_runtimes: bool,
    non_interactive: bool,
    gola_home: Option<PathBuf>,
    cargo_home: Option<PathBuf>,
}

impl RuntimeManager {
    pub fn new(local_runtimes: bool, non_interactive: bool) -> Self {
        Self {
            local_runtimes,
            non_interactive,
            gola_home: None,
            cargo_home: None,
        }
    }

    pub fn with_gola_home(mut self, gola_home: PathBuf) -> Self {
        self.gola_home = Some(gola_home);
        self
    }

    pub fn with_cargo_home(mut self, cargo_home: PathBuf) -> Self {
        self.cargo_home = Some(cargo_home);
        self
    }

    pub async fn resolve_command(
        &self,
        execution_type: &McpExecutionType,
    ) -> Result<Command, AgentError> {
        match execution_type {
            McpExecutionType::Command { command } => {
                let mut cmd = Command::new(&command.run);
                cmd.args(&command.args);
                if let Some(cwd) = &command.working_dir {
                    cmd.current_dir(cwd);
                }
                cmd.envs(command.env.clone());
                Ok(cmd)
            }
            McpExecutionType::Runtime {
                runtime,
                entry_point,
                args,
                env,
                working_dir,
                ..
            } => {
                let runtime_handler = self.get_runtime_handler(runtime)?;
                runtime_handler.ensure_installed(entry_point).await?;
                let mut cmd = runtime_handler.get_command(entry_point, args)?;

                if let Some(cwd) = working_dir {
                    cmd.current_dir(cwd);
                }
                cmd.envs(env.clone());
                Ok(cmd)
            }
            McpExecutionType::NativeBinary {
                binary_name,
                installation_config: _,
                args,
                env,
                working_dir,
            } => {
                // TODO: Implement native binary installation in future phases
                // For now, just try to find the binary in PATH
                let mut cmd = Command::new(binary_name);
                cmd.args(args);
                if let Some(cwd) = working_dir {
                    cmd.current_dir(cwd);
                }
                cmd.envs(env.clone());
                Ok(cmd)
            }
        }
    }

    fn get_runtime_handler(&self, runtime: &str) -> Result<Box<dyn Runtime>, AgentError> {
        match runtime {
            "nodejs" => Ok(Box::new(BunRuntime {
                manager: self.clone(),
            })),
            "python" => Ok(Box::new(UvRuntime {
                manager: self.clone(),
            })),
            "rust" => Ok(Box::new(CargoRuntime {
                manager: self.clone(),
            })),
            _ => Err(AgentError::ConfigError(format!(
                "Unsupported runtime: {}",
                runtime
            ))),
        }
    }

    fn find_tool(&self, tool: &str) -> Option<PathBuf> {
        self.find_tool_in_gola_home(tool)
            .or_else(|| self.find_tool_in_cargo_home(tool))
            .or_else(|| {
                // Skip system PATH check if we're in local_runtimes mode for testing
                if self.local_runtimes {
                    None
                } else {
                    which(tool).ok()
                }
            })
    }

    fn find_tool_in_gola_home(&self, tool: &str) -> Option<PathBuf> {
        if let Ok(gola_home) = self.get_gola_home() {
            let tool_path = gola_home.join("bin").join(tool);
            if tool_path.exists() {
                return Some(tool_path);
            }
        }
        None
    }

    fn find_tool_in_cargo_home(&self, tool: &str) -> Option<PathBuf> {
        let cargo_home = self.get_cargo_home().ok()?;

        let tool_path = cargo_home.join("bin").join(tool);
        if tool_path.exists() {
            Some(tool_path)
        } else {
            None
        }
    }

    async fn ensure_bun_installed(&self) -> Result<(), AgentError> {
        if self.find_tool("bun").is_some() {
            return Ok(());
        }

        if self.non_interactive {
            self.install_bun().await
        } else if self.confirm_install("bun", "Node.js")? {
            self.install_bun().await
        } else {
            Err(AgentError::RuntimeError(
                "bun installation cancelled.".to_string(),
            ))
        }
    }

    async fn ensure_uv_installed(&self) -> Result<(), AgentError> {
        if self.find_tool("uv").is_some() {
            // UV is installed, now ensure Python is available
            return self.ensure_python_via_uv().await;
        }

        if self.non_interactive {
            self.install_uv().await?;
            self.ensure_python_via_uv().await
        } else if self.confirm_install("uv", "Python")? {
            self.install_uv().await?;
            self.ensure_python_via_uv().await
        } else {
            Err(AgentError::RuntimeError(
                "uv installation cancelled.".to_string(),
            ))
        }
    }

    async fn ensure_cargo_installed(&self) -> Result<(), AgentError> {
        if self.find_tool("cargo").is_some() {
            return Ok(());
        }

        if self.non_interactive {
            self.install_rustup().await
        } else if self.confirm_install("rustup", "Rust")? {
            self.install_rustup().await
        } else {
            Err(AgentError::RuntimeError(
                "Rust installation cancelled.".to_string(),
            ))
        }
    }

    fn confirm_install(&self, tool: &str, runtime_name: &str) -> Result<bool, AgentError> {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "No {} runtime found. Would you like to install {} into ~/.gola?",
                runtime_name, tool
            ))
            .interact()
            .map_err(|e| AgentError::IoError(e.to_string()))
    }

    pub fn get_cargo_home(&self) -> Result<PathBuf, AgentError> {
        if let Some(cargo_home) = &self.cargo_home {
            Ok(cargo_home.clone())
        } else if let Ok(path) = env::var("CARGO_HOME") {
            Ok(PathBuf::from(path))
        } else {
            dirs::home_dir()
                .map(|home| home.join(".cargo"))
                .ok_or_else(|| {
                    AgentError::InternalError("Could not determine home directory.".to_string())
                })
        }
    }

    pub fn get_gola_home(&self) -> Result<PathBuf, AgentError> {
        if let Some(gola_home) = &self.gola_home {
            Ok(gola_home.clone())
        } else {
            dirs::home_dir()
                .map(|home| home.join(".gola"))
                .ok_or_else(|| {
                    AgentError::InternalError("Could not determine home directory.".to_string())
                })
        }
    }

    async fn install_bun(&self) -> Result<(), AgentError> {
        let gola_home = self.get_gola_home()?;
        let bin_dir = gola_home.join("bin");
        std::fs::create_dir_all(&bin_dir)?;

        log::info!("Installing bun to {}...", gola_home.display());

        // Dynamically detect OS and architecture at runtime using uname
        let os_suffix = std::env::consts::OS;
        let runtime_arch = self.detect_runtime_arch().await?;

        let (os_name, arch, dir_format) = match os_suffix {
            "linux" => {
                let arch = match runtime_arch.as_str() {
                    "x86_64" => "x64",
                    "aarch64" => "aarch64",
                    _ => {
                        return Err(AgentError::RuntimeError(format!(
                            "Unsupported architecture '{}' for bun installation",
                            runtime_arch
                        )))
                    }
                };
                // Use GNU libc instead of musl for better compatibility with Debian-based systems
                ("linux", arch, format!("bun-linux-{}", arch))
            }
            "macos" => {
                let arch = match runtime_arch.as_str() {
                    "x86_64" => "x64",
                    "aarch64" => "aarch64",
                    _ => {
                        return Err(AgentError::RuntimeError(format!(
                            "Unsupported architecture '{}' for bun installation",
                            runtime_arch
                        )))
                    }
                };
                ("darwin", arch, format!("bun-darwin-{}", arch))
            }
            _ => {
                return Err(AgentError::RuntimeError(format!(
                    "Unsupported operating system '{}' for bun installation",
                    os_suffix
                )))
            }
        };

        let url = format!(
            "https://github.com/oven-sh/bun/releases/latest/download/bun-{}-{}.zip",
            os_name, arch
        );
        log::info!("Downloading bun from: {}", url);

        // Download the binary
        let response = reqwest::get(&url).await?;
        if !response.status().is_success() {
            return Err(AgentError::RuntimeError(format!(
                "Failed to download bun: HTTP {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await?;

        // Extract the zip file using the zip crate
        log::info!("Extracting bun archive...");
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|e| AgentError::RuntimeError(format!("Failed to read zip archive: {}", e)))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| {
                AgentError::RuntimeError(format!("Failed to read file from archive: {}", e))
            })?;

            let outpath = match file.enclosed_name() {
                Some(path) => bin_dir.join(path),
                None => continue,
            };

            if file.name().ends_with('/') {
                // Directory
                std::fs::create_dir_all(&outpath)?;
            } else {
                // File
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        std::fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }

            // Set permissions on Unix systems
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
                }
            }
        }

        log::info!("bun archive extracted successfully");

        // Find the extracted bun binary and move it to the correct location
        let extracted_dir = bin_dir.join(&dir_format);
        let extracted_bun = extracted_dir.join("bun");
        let final_bun_path = bin_dir.join("bun");

        if extracted_bun.exists() {
            std::fs::rename(extracted_bun, &final_bun_path)?;
            // Clean up extracted directory
            let _ = std::fs::remove_dir_all(&extracted_dir);
        } else {
            // List contents for debugging
            let mut debug_info = String::new();
            if let Ok(entries) = std::fs::read_dir(&bin_dir) {
                debug_info.push_str("Contents of bin_dir after extraction:\n");
                for entry in entries.flatten() {
                    debug_info.push_str(&format!("  {}\n", entry.path().display()));
                }
            }
            return Err(AgentError::RuntimeError(format!(
                "bun binary not found after extraction. Expected at: {}\n{}",
                extracted_bun.display(),
                debug_info
            )));
        }

        // Make the binary executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&final_bun_path, std::fs::Permissions::from_mode(0o755))?;
        }

        log::info!(
            "bun installed successfully at: {}",
            final_bun_path.display()
        );
        let path = env::var("PATH").unwrap_or_default();
        env::set_var("PATH", format!("{}:{}", bin_dir.display(), path));
        Ok(())
    }

    async fn install_uv(&self) -> Result<(), AgentError> {
        let gola_home = self.get_gola_home()?;
        let bin_dir = gola_home.join("bin");
        std::fs::create_dir_all(&bin_dir)?;

        log::info!("Installing uv to {}...", bin_dir.display());
        let script_content = reqwest::get("https://astral.sh/uv/install.sh")
            .await?
            .text()
            .await?;
        let mut child = Command::new("sh")
            .arg("-s")
            .env("UV_INSTALL_DIR", bin_dir.to_str().unwrap())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(script_content.as_bytes()).await?;
        }
        let output = child.wait_with_output().await?;

        // Log both stdout and stderr for debugging
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        log::info!("uv installation stdout: {}", stdout_str);
        log::info!("uv installation stderr: {}", stderr_str);

        if !output.status.success() {
            return Err(AgentError::RuntimeError(format!(
                "uv installation failed with exit code {:?}: {}",
                output.status.code(),
                stderr_str
            )));
        }

        // Verify the binary was installed correctly
        let uv_path = bin_dir.join("uv");
        if !uv_path.exists() {
            // List contents of bin_dir for debugging
            let mut debug_info = String::new();
            if let Ok(entries) = std::fs::read_dir(&bin_dir) {
                debug_info.push_str("Contents of bin_dir:\n");
                for entry in entries.flatten() {
                    debug_info.push_str(&format!("  {}\n", entry.path().display()));
                }
            }

            return Err(AgentError::RuntimeError(format!(
                "uv binary not found at expected location: {}\nDebug info:\n{}",
                uv_path.display(),
                debug_info
            )));
        }

        log::info!("uv installed successfully at: {}", uv_path.display());
        let path = env::var("PATH").unwrap_or_default();
        env::set_var("PATH", format!("{}:{}", bin_dir.display(), path));
        Ok(())
    }

    async fn ensure_python_via_uv(&self) -> Result<(), AgentError> {
        let uv_path = self.find_tool("uv").ok_or_else(|| {
            AgentError::RuntimeError("UV not found - should be installed first".to_string())
        })?;

        // Check if Python is already installed and pinned
        let output = Command::new(&uv_path)
            .arg("python")
            .arg("list")
            .output()
            .await
            .map_err(|e| {
                AgentError::RuntimeError(format!("Failed to check Python installations: {}", e))
            })?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() && stdout.contains("3.12") {
                log::info!("Python 3.12 already available via UV");
                return Ok(());
            }
        }

        log::info!("Installing Python 3.12 via UV...");

        // Step 1: Install Python 3.12 with architecture-specific version
        let runtime_arch = self.detect_runtime_arch().await?;
        let python_version = match runtime_arch.as_str() {
            "aarch64" => "cpython-3.12.11-linux-aarch64-gnu",
            "x86_64" => "cpython-3.12.11-linux-x86_64-gnu",
            _ => "3.12",
        };

        log::info!(
            "Installing Python {} for architecture {}",
            python_version,
            runtime_arch
        );
        let output = Command::new(&uv_path)
            .arg("python")
            .arg("install")
            .arg(python_version)
            .output()
            .await
            .map_err(|e| {
                AgentError::RuntimeError(format!("Failed to install Python via UV: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::RuntimeError(format!(
                "Python installation via UV failed: {}",
                stderr
            )));
        }

        log::info!("Python 3.12 installed successfully via UV");

        // Step 2: Pin Python 3.12 for this environment in the gola home directory
        let gola_home = self.get_gola_home()?;
        let output = Command::new(&uv_path)
            .arg("python")
            .arg("pin")
            .arg("3.12")
            .current_dir(&gola_home)
            .output()
            .await
            .map_err(|e| AgentError::RuntimeError(format!("Failed to pin Python via UV: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::RuntimeError(format!(
                "Python pinning via UV failed: {}",
                stderr
            )));
        }

        log::info!("Python 3.12 pinned successfully via UV");
        Ok(())
    }

    async fn detect_runtime_arch(&self) -> Result<String, AgentError> {
        // Try to use uname command first, but fallback to Rust's built-in detection if it fails
        let arch_result = Command::new("uname")
            .arg("-m")
            .output()
            .await;

        let arch_output = match arch_result {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            _ => {
                // Fallback to Rust's built-in architecture detection
                log::debug!("uname command failed, using Rust's built-in architecture detection");
                std::env::consts::ARCH.to_string()
            }
        };

        match arch_output.as_str() {
            "x86_64" => Ok("x86_64".to_string()),
            "aarch64" | "arm64" => Ok("aarch64".to_string()),
            _ => Err(AgentError::RuntimeError(format!(
                "Unsupported architecture detected: {}",
                arch_output
            ))),
        }
    }

    async fn install_rustup(&self) -> Result<(), AgentError> {
        let gola_home = self.get_gola_home()?;
        let rustup_home = gola_home.join("rustup");
        let cargo_home = self.get_cargo_home()?;
        std::fs::create_dir_all(&rustup_home)?;
        std::fs::create_dir_all(&cargo_home.join("bin"))?;

        log::info!("Installing Rust toolchain to {}...", rustup_home.display());
        let script_content = reqwest::get("https://sh.rustup.rs").await?.text().await?;
        let mut child = Command::new("sh")
            .arg("-s")
            .arg("--")
            .arg("-y") // Run non-interactively
            .arg("--no-modify-path")
            .env("RUSTUP_HOME", &rustup_home)
            .env("CARGO_HOME", &cargo_home)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(script_content.as_bytes()).await?;
        }
        let output = child.wait_with_output().await?;

        // Log both stdout and stderr for debugging
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        log::info!("rustup installation stdout: {}", stdout_str);
        log::info!("rustup installation stderr: {}", stderr_str);

        if !output.status.success() {
            return Err(AgentError::RuntimeError(format!(
                "Rustup installation failed with exit code {:?}: {}",
                output.status.code(),
                stderr_str
            )));
        }

        // Verify the binary was installed correctly
        let cargo_path = cargo_home.join("bin").join("cargo");
        if !cargo_path.exists() {
            // List contents of cargo_home/bin for debugging
            let mut debug_info = String::new();
            let cargo_bin_dir = cargo_home.join("bin");
            if let Ok(entries) = std::fs::read_dir(&cargo_bin_dir) {
                debug_info.push_str("Contents of cargo/bin:\n");
                for entry in entries.flatten() {
                    debug_info.push_str(&format!("  {}\n", entry.path().display()));
                }
            }

            return Err(AgentError::RuntimeError(format!(
                "cargo binary not found at expected location: {}\nDebug info:\n{}",
                cargo_path.display(),
                debug_info
            )));
        }

        log::info!(
            "Rust toolchain installed successfully. cargo at: {}",
            cargo_path.display()
        );
        let path = env::var("PATH").unwrap_or_default();
        env::set_var(
            "PATH",
            format!("{}:{}", cargo_home.join("bin").display(), path),
        );
        env::set_var("RUSTUP_HOME", &rustup_home);
        env::set_var("CARGO_HOME", &cargo_home);
        Ok(())
    }
}
