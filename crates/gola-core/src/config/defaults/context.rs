//! Context detection for intelligent defaults
//!
//! This module provides functionality to detect project structure, environment,
//! and other context information to inform default configuration choices.

use super::traits::{DefaultsContext, ProjectInfo, GitInfo};
use crate::errors::AgentError;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

impl DefaultsContext {
    /// Build a complete context from the current environment
    pub fn build(
        working_dir: Option<PathBuf>,
        environment: Option<String>,
        active_profile: Option<String>,
    ) -> Result<Self, AgentError> {
        let working_dir = working_dir
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        
        let project_info = ProjectInfo::detect(&working_dir)?;
        let env_vars = env::vars().collect();
        let environment = environment.or_else(|| detect_environment(&env_vars));
        
        Ok(Self {
            working_dir,
            environment,
            project_info,
            env_vars,
            active_profile,
        })
    }
    
    /// Get the current environment name
    pub fn get_environment(&self) -> &str {
        self.environment.as_deref().unwrap_or("development")
    }
    
    /// Check if we're in a development environment
    pub fn is_development(&self) -> bool {
        matches!(self.get_environment(), "development" | "dev" | "local")
    }
    
    /// Check if we're in a production environment
    pub fn is_production(&self) -> bool {
        matches!(self.get_environment(), "production" | "prod")
    }
    
    /// Get an environment variable value
    pub fn get_env_var(&self, key: &str) -> Option<&String> {
        self.env_vars.get(key)
    }
    
    /// Get the project name, preferring detected name over directory name
    pub fn get_project_name(&self) -> String {
        self.project_info
            .project_name
            .clone()
            .unwrap_or_else(|| {
                self.working_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("gola-project")
                    .to_string()
            })
    }
    
    /// Check if a specific tool/binary is available in the environment
    pub fn has_tool(&self, tool_name: &str) -> bool {
        Command::new("which")
            .arg(tool_name)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

impl ProjectInfo {
    /// Detect project information from the filesystem
    pub fn detect(working_dir: &PathBuf) -> Result<Self, AgentError> {
        let mut info = ProjectInfo::default();
        
        // Check for various project types
        info.is_rust_project = working_dir.join("Cargo.toml").exists();
        info.is_node_project = working_dir.join("package.json").exists();
        info.is_python_project = working_dir.join("pyproject.toml").exists() 
            || working_dir.join("setup.py").exists()
            || working_dir.join("requirements.txt").exists();
        
        // Try to extract project name from various sources
        info.project_name = detect_project_name(working_dir)?;
        
        // Detect git information
        info.git_info = GitInfo::detect(working_dir).ok();
        
        Ok(info)
    }
}

impl GitInfo {
    /// Detect git repository information
    pub fn detect(working_dir: &PathBuf) -> Result<Self, AgentError> {
        // Check if we're in a git repository
        let git_dir = working_dir.join(".git");
        if !git_dir.exists() {
            return Err(AgentError::ConfigError("Not a git repository".to_string()));
        }
        
        // Get current branch
        let branch_output = Command::new("git")
            .args(&["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(working_dir)
            .output()
            .map_err(|e| AgentError::ConfigError(format!("Failed to get git branch: {}", e)))?;
        
        let branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();
        
        // Get origin URL if available
        let origin_output = Command::new("git")
            .args(&["config", "--get", "remote.origin.url"])
            .current_dir(working_dir)
            .output()
            .ok();
        
        let origin_url = origin_output
            .and_then(|output| {
                if output.status.success() {
                    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
                } else {
                    None
                }
            });
        
        // Check for uncommitted changes
        let status_output = Command::new("git")
            .args(&["status", "--porcelain"])
            .current_dir(working_dir)
            .output()
            .map_err(|e| AgentError::ConfigError(format!("Failed to get git status: {}", e)))?;
        
        let has_changes = !status_output.stdout.is_empty();
        
        Ok(Self {
            branch,
            origin_url,
            has_changes,
        })
    }
}

/// Detect the current environment from environment variables
fn detect_environment(env_vars: &HashMap<String, String>) -> Option<String> {
    // Common environment variable names in order of preference
    let env_var_names = [
        "GOLA_ENV",
        "NODE_ENV",
        "RAILS_ENV",
        "FLASK_ENV",
        "DJANGO_SETTINGS_MODULE",
        "ENVIRONMENT",
        "ENV",
    ];
    
    for var_name in &env_var_names {
        if let Some(value) = env_vars.get(*var_name) {
            return Some(value.to_lowercase());
        }
    }
    
    // Check for common CI/CD environment indicators
    if env_vars.contains_key("CI") {
        return Some("ci".to_string());
    }
    
    // Check for Docker environment
    if env_vars.contains_key("DOCKER_CONTAINER") 
        || fs::metadata("/.dockerenv").is_ok() {
        return Some("docker".to_string());
    }
    
    None
}

/// Detect project name from various sources
fn detect_project_name(working_dir: &PathBuf) -> Result<Option<String>, AgentError> {
    // Try Cargo.toml first
    if let Ok(cargo_content) = fs::read_to_string(working_dir.join("Cargo.toml")) {
        if let Ok(cargo_toml) = toml::from_str::<toml::Value>(&cargo_content) {
            if let Some(package) = cargo_toml.get("package") {
                if let Some(name) = package.get("name") {
                    if let Some(name_str) = name.as_str() {
                        return Ok(Some(name_str.to_string()));
                    }
                }
            }
        }
    }
    
    // Try package.json
    if let Ok(package_content) = fs::read_to_string(working_dir.join("package.json")) {
        if let Ok(package_json) = serde_json::from_str::<serde_json::Value>(&package_content) {
            if let Some(name) = package_json.get("name") {
                if let Some(name_str) = name.as_str() {
                    return Ok(Some(name_str.to_string()));
                }
            }
        }
    }
    
    // Try pyproject.toml
    if let Ok(pyproject_content) = fs::read_to_string(working_dir.join("pyproject.toml")) {
        if let Ok(pyproject_toml) = toml::from_str::<toml::Value>(&pyproject_content) {
            if let Some(project) = pyproject_toml.get("project") {
                if let Some(name) = project.get("name") {
                    if let Some(name_str) = name.as_str() {
                        return Ok(Some(name_str.to_string()));
                    }
                }
            }
        }
    }
    
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::File;
    use std::io::Write;
    
    #[test]
    fn test_project_detection_rust() {
        let temp_dir = TempDir::new().unwrap();
        let cargo_toml = temp_dir.path().join("Cargo.toml");
        let mut file = File::create(&cargo_toml).unwrap();
        writeln!(file, "[package]\nname = \"test-project\"\nversion = \"0.1.0\"").unwrap();
        
        let project_info = ProjectInfo::detect(&temp_dir.path().to_path_buf()).unwrap();
        assert!(project_info.is_rust_project);
        assert_eq!(project_info.project_name, Some("test-project".to_string()));
    }
    
    #[test]
    fn test_environment_detection() {
        let mut env_vars = HashMap::new();
        env_vars.insert("NODE_ENV".to_string(), "production".to_string());
        
        let env = detect_environment(&env_vars);
        assert_eq!(env, Some("production".to_string()));
    }
    
    #[test]
    fn test_context_building() {
        let temp_dir = TempDir::new().unwrap();
        let context = DefaultsContext::build(
            Some(temp_dir.path().to_path_buf()),
            Some("test".to_string()),
            Some("test-profile".to_string()),
        ).unwrap();
        
        assert_eq!(context.get_environment(), "test");
        assert_eq!(context.active_profile, Some("test-profile".to_string()));
    }
}