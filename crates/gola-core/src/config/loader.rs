//! Configuration loader for YAML files and environment resolution
//!
//! This module handles loading configuration from YAML files and resolving
//! environment variables and other dynamic values.

use crate::config::github::GitHubConfigLoader;
use crate::config::types::SchemaSourceType; // Additional schema imports
use crate::config::types::*;
use crate::errors::AgentError;
use reqwest;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use tokio::fs; // For URL schema loading

/// Configuration loader with environment resolution
pub struct ConfigLoader;
impl ConfigLoader {
    /// Load configuration from a source (file path, URL, or GitHub repository)
    pub async fn from_source(source: &str) -> Result<GolaConfig, AgentError> {
        if source.starts_with("github:") {
            // Parse GitHub reference and load from repository
            let github_ref = GitHubConfigLoader::parse_github_ref(source)?;
            let github_loader = GitHubConfigLoader::new()?;
            let (mut config, repo_dir) = github_loader.load_from_github(&github_ref).await?;
            Self::resolve_prompts(&mut config, Some(&repo_dir)).await?;
            Ok(config)
        } else if source.starts_with("http://") || source.starts_with("https://") {
            // Load from URL
            Self::from_url(source).await
        } else {
            // Load from file path
            Self::from_file(source).await
        }
    }

    /// Load configuration from a URL
    pub async fn from_url(url: &str) -> Result<GolaConfig, AgentError> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await.map_err(|e| {
            AgentError::ConfigError(format!(
                "Failed to fetch configuration from URL {}: {}",
                url, e
            ))
        })?;

        if !response.status().is_success() {
            return Err(AgentError::ConfigError(format!(
                "Failed to fetch configuration: HTTP {} from URL {}",
                response.status(),
                url
            )));
        }

        let content = response.text().await.map_err(|e| {
            AgentError::ConfigError(format!(
                "Failed to read configuration response from URL {}: {}",
                url, e
            ))
        })?;

        Self::from_str(&content, None).await
    }

    /// Load configuration from a YAML file
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<GolaConfig, AgentError> {
        let path = path.as_ref();

        // Read the file
        let content = fs::read_to_string(path).await.map_err(|e| {
            AgentError::ConfigError(format!(
                "Failed to read config file {}: {}",
                path.display(),
                e
            ))
        })?;

        let base_dir = path.parent();
        Self::from_str(&content, base_dir).await
    }

    /// Load configuration from a YAML string
    pub async fn from_str(
        content: &str,
        base_dir: Option<&Path>,
    ) -> Result<GolaConfig, AgentError> {
        let mut config: GolaConfig = serde_yaml::from_str(content)
            .map_err(|e| AgentError::ConfigError(format!("Failed to parse YAML config: {}", e)))?;

        // Resolve environment variables
        Self::resolve_environment(&mut config).await?;

        // Resolve prompts
        Self::resolve_prompts(&mut config, base_dir).await?;

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Resolve environment variables in the configuration
    async fn resolve_environment(config: &mut GolaConfig) -> Result<(), AgentError> {
        // Load environment files if specified
        for env_file in &config.environment.env_files {
            if env_file.exists() {
                Self::load_env_file(env_file)?;
            }
        }

        // Set environment variables from config
        for (key, value) in &config.environment.variables {
            env::set_var(key, value);
        }

        // Resolve LLM authentication if LLM config is present
        if let Some(ref mut llm_config) = config.llm {
            Self::resolve_llm_auth(&mut llm_config.auth)?;
        }

        // Resolve RAG embedding authentication
        if let Some(rag) = &mut config.rag {
            Self::resolve_embedding_auth(&mut rag.embeddings.auth)?;
        }

        // Resolve web search authentication
        if let Some(web_search) = &mut config.tools.web_search {
            Self::resolve_web_search_auth(&mut web_search.auth)?;
        }

        // Resolve MCP server environment variables
        for server in &mut config.mcp_servers {
            if let Some(ref mut cmd) = server.command {
                Self::resolve_mcp_env(&mut cmd.env)?;
            }
        }

        // Resolve schema files
        Self::resolve_schema_files(&mut config.agent.schema).await?;

        Ok(())
    }

    async fn resolve_prompts(
        config: &mut GolaConfig,
        base_dir: Option<&Path>,
    ) -> Result<(), AgentError> {
        let base_dir = match base_dir {
            Some(dir) => dir,
            None => return Ok(()),
        };

        // Auto-detect conventional prompt structure if no prompts config exists
        if config.prompts.is_none() {
            let prompts_dir = base_dir.join("prompts");
            if prompts_dir.exists() {
                log::info!("Auto-detecting conventional prompt structure in: {}", prompts_dir.display());
                
                let mut detected_config = PromptConfig {
                    template_vars: None,
                    fragments: None,
                    roles: None,
                    purposes: None,
                };

                // Check for system prompt: prompts/system/main.md
                let system_prompt_path = prompts_dir.join("system/main.md");
                if system_prompt_path.exists() {
                    log::info!("Found system prompt: {}", system_prompt_path.display());
                    detected_config.roles = Some(RolePrompts {
                        system: Some(vec![PromptSource::File { 
                            file: "prompts/system/main.md".to_string() 
                        }]),
                        assembled: None,
                    });
                }

                // Check for ice breaker: prompts/user/ice_breaker.md
                let ice_breaker_path = prompts_dir.join("user/ice_breaker.md");
                if ice_breaker_path.exists() {
                    log::info!("Found ice breaker prompt: {}", ice_breaker_path.display());
                    let mut purposes = HashMap::new();
                    purposes.insert("ice_breaker".to_string(), PurposePrompt {
                        role: "user".to_string(),
                        assembly: Some(vec![PromptSource::File { 
                            file: "prompts/user/ice_breaker.md".to_string() 
                        }]),
                    });
                    detected_config.purposes = Some(purposes);
                }

                // Only set if we found something
                if detected_config.roles.is_some() || detected_config.purposes.is_some() {
                    config.prompts = Some(detected_config);
                    log::info!("Auto-configured prompts based on conventional structure");
                }
            }
        }

        if let Some(prompt_config) = &mut config.prompts {

            // Step 1: Gather all variables, starting with defaults from YAML
            let mut template_vars = prompt_config.template_vars.clone().unwrap_or_default();

            // Step 2: Override with environment variables
            for (key, value) in env::vars() {
                if let Some(stripped_key) = key.strip_prefix("GOLA_TPL_") {
                    template_vars.insert(stripped_key.to_lowercase(), value);
                }
            }
            
            // Add runtime variables
            template_vars.insert("gola.runtime.date".to_string(), chrono::Utc::now().to_rfc3339());
            template_vars.insert("gola.runtime.os".to_string(), env::consts::OS.to_string());


            // Step 3: Load all fragments into a map and perform substitution on them
            let mut loaded_fragments = HashMap::new();
            if let Some(fragments) = &prompt_config.fragments {
                for (name, file_path) in fragments {
                    let path = base_dir.join(file_path);
                    let mut content = fs::read_to_string(&path).await.map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Failed to read fragment file {}: {}",
                            path.display(),
                            e
                        ))
                    })?;
                    
                    for (key, value) in &template_vars {
                        content = content.replace(&format!("{{{{{}}}}}", key), value);
                    }

                    loaded_fragments.insert(name.clone(), content);
                }
            }

            // Step 4: Assemble final prompts by resolving files and fragments
            if let Some(roles) = &mut prompt_config.roles {
                if let Some(system_sources) = &roles.system {
                    let mut assembled_sources = vec![];
                    for source in system_sources.iter() {
                        let mut content = match source {
                        PromptSource::File { file } => {
                            let path = base_dir.join(file);
                            log::info!("Loading system prompt from file: {}", path.display());
                            fs::read_to_string(&path).await.map_err(|e| {
                                AgentError::ConfigError(format!(
                                    "Failed to read prompt file {}: {}",
                                    path.display(),
                                    e
                                ))
                            })
                        }
                        PromptSource::Fragment { fragment } => {
                            loaded_fragments.get(fragment).cloned().ok_or_else(|| {
                                AgentError::ConfigError(format!(
                                    "Fragment '{}' not found in definitions",
                                    fragment
                                ))
                            })
                        }
                    }?;

                    for (key, value) in &template_vars {
                        content = content.replace(&format!("{{{{{}}}}}", key), value);
                    }
                    assembled_sources.push(content);
                }
                let final_prompt = assembled_sources.join("\n\n");
                log::info!("System prompt submitted");
                roles.assembled = Some(final_prompt);
            }
        }

        if let Some(purposes) = &mut prompt_config.purposes {
            for purpose in purposes.values_mut() {
                if let Some(assembly) = &mut purpose.assembly {
                    let mut assembled_sources = vec![];
                    for source in assembly.iter() {
                        let mut content = match source {
                            PromptSource::File { file } => {
                                let path = base_dir.join(file);
                                fs::read_to_string(&path).await.map_err(|e| {
                                    AgentError::ConfigError(format!(
                                        "Failed to read prompt file {}: {}",
                                        path.display(),
                                        e
                                    ))
                                })
                            }
                            PromptSource::Fragment { fragment } => {
                                loaded_fragments.get(fragment).cloned().ok_or_else(|| {
                                    AgentError::ConfigError(format!(
                                        "Fragment '{}' not found in definitions",
                                        fragment
                                    ))
                                })
                            }
                        }?;
                        for (key, value) in &template_vars {
                            content = content.replace(&format!("{{{{{}}}}}", key), value);
                        }
                        assembled_sources.push(content);
                    }
                let final_prompt = assembled_sources.join("\n\n");
                log::info!("System prompt submitted");
                *assembly = vec![PromptSource::File { file: final_prompt }];
            }
        }
    }
}
Ok(())
}

    /// Resolve schema files and URLs
    async fn resolve_schema_files(schema_config: &mut SchemaConfig) -> Result<(), AgentError> {
        // Resolve input schema
        if let Some(input_schema) = &mut schema_config.input {
            if let SchemaSourceType::File = input_schema.source.source_type {
                if let Some(file_path) = &input_schema.source.location {
                    let schema_content = fs::read_to_string(file_path).await.map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Failed to read input schema file {}: {}",
                            file_path, e
                        ))
                    })?;

                    input_schema.schema = serde_json::from_str(&schema_content).map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Invalid JSON in input schema file {}: {}",
                            file_path, e
                        ))
                    })?;
                }
            } else if let SchemaSourceType::Url = input_schema.source.source_type {
                if let Some(url) = &input_schema.source.location {
                    let response = reqwest::get(url).await.map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Failed to fetch input schema from URL {}: {}",
                            url, e
                        ))
                    })?;

                    let schema_content = response.text().await.map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Failed to read input schema response from URL {}: {}",
                            url, e
                        ))
                    })?;

                    input_schema.schema = serde_json::from_str(&schema_content).map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Invalid JSON in input schema from URL {}: {}",
                            url, e
                        ))
                    })?;
                }
            }
        }

        // Resolve output schema
        if let Some(output_schema) = &mut schema_config.output {
            if let SchemaSourceType::File = output_schema.source.source_type {
                if let Some(file_path) = &output_schema.source.location {
                    let schema_content = fs::read_to_string(file_path).await.map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Failed to read output schema file {}: {}",
                            file_path, e
                        ))
                    })?;

                    output_schema.schema = serde_json::from_str(&schema_content).map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Invalid JSON in output schema file {}: {}",
                            file_path, e
                        ))
                    })?;
                }
            } else if let SchemaSourceType::Url = output_schema.source.source_type {
                if let Some(url) = &output_schema.source.location {
                    let response = reqwest::get(url).await.map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Failed to fetch output schema from URL {}: {}",
                            url, e
                        ))
                    })?;

                    let schema_content = response.text().await.map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Failed to read output schema response from URL {}: {}",
                            url, e
                        ))
                    })?;

                    output_schema.schema = serde_json::from_str(&schema_content).map_err(|e| {
                        AgentError::ConfigError(format!(
                            "Invalid JSON in output schema from URL {}: {}",
                            url, e
                        ))
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Load environment variables from a .env file
    fn load_env_file<P: AsRef<Path>>(path: P) -> Result<(), AgentError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| {
            AgentError::ConfigError(format!(
                "Failed to read env file {}: {}",
                path.as_ref().display(),
                e
            ))
        })?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                env::set_var(key, value);
            }
        }

        Ok(())
    }

    /// Resolve LLM authentication from environment
    fn resolve_llm_auth(auth: &mut LlmAuth) -> Result<(), AgentError> {
        // Resolve API key from environment variable
        if let Some(env_var) = &auth.api_key_env {
            if let Ok(api_key) = env::var(env_var) {
                auth.api_key = Some(api_key);
            }
        }

        // If no API key is set, try common environment variables
        if auth.api_key.is_none() && auth.api_key_env.is_none() {
            if let Ok(api_key) = env::var("OPENAI_API_KEY") {
                auth.api_key = Some(api_key);
            } else if let Ok(api_key) = env::var("GEMINI_API_KEY") {
                auth.api_key = Some(api_key);
            } else if let Ok(api_key) = env::var("CUSTOM_API_KEY") {
                auth.api_key = Some(api_key);
            }
        }

        Ok(())
    }

    /// Resolve embedding authentication from environment
    fn resolve_embedding_auth(auth: &mut EmbeddingAuth) -> Result<(), AgentError> {
        if let Some(env_var) = &auth.api_key_env {
            if let Ok(api_key) = env::var(env_var) {
                auth.api_key = Some(api_key);
            }
        }

        // Try common embedding API keys
        if auth.api_key.is_none() && auth.api_key_env.is_none() {
            if let Ok(api_key) = env::var("OPENAI_API_KEY") {
                auth.api_key = Some(api_key);
            } else if let Ok(api_key) = env::var("COHERE_API_KEY") {
                auth.api_key = Some(api_key);
            } else if let Ok(api_key) = env::var("HUGGINGFACE_API_KEY") {
                auth.api_key = Some(api_key);
            }
        }

        Ok(())
    }

    /// Resolve web search authentication from environment
    fn resolve_web_search_auth(auth: &mut WebSearchAuth) -> Result<(), AgentError> {
        if let Some(env_var) = &auth.api_key_env {
            if let Ok(api_key) = env::var(env_var) {
                auth.api_key = Some(api_key);
            }
        }

        // Try common search API keys
        if auth.api_key.is_none() && auth.api_key_env.is_none() {
            if let Ok(api_key) = env::var("TAVILY_API_KEY") {
                auth.api_key = Some(api_key);
            } else if let Ok(api_key) = env::var("SERPER_API_KEY") {
                auth.api_key = Some(api_key);
            }
        }

        Ok(())
    }

    /// Resolve MCP environment variables
    fn resolve_mcp_env(env_vars: &mut HashMap<String, String>) -> Result<(), AgentError> {
        let mut resolved = HashMap::new();

        for (key, value) in env_vars.iter() {
            let resolved_value = if value.starts_with("$") {
                let env_var = &value[1..];
                env::var(env_var).unwrap_or_else(|_| value.clone())
            } else {
                value.clone()
            };
            resolved.insert(key.clone(), resolved_value);
        }

        *env_vars = resolved;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_basic_config() {
        let yaml_content = r#"
agent:
  name: "test_agent"
  description: "A test agent"
  max_steps: 5

llm:
  provider: "openai"
  model: "gpt-4.1-mini"
  parameters:
    temperature: 0.8
    max_tokens: 1000

environment:
  variables:
    TEST_VAR: "test_value"
"#;

        let config = ConfigLoader::from_str(yaml_content, None).await.unwrap();
        assert_eq!(config.agent.name, "test_agent");
        assert_eq!(config.agent.max_steps, 5);
        assert_eq!(config.llm.as_ref().unwrap().model, "gpt-4.1-mini");
        assert_eq!(config.llm.as_ref().unwrap().parameters.temperature, 0.8);
    }

    #[tokio::test]
    async fn test_load_config_with_rag() {
        let yaml_content = r#"
agent:
  name: "rag_agent"

llm:
  provider: "openai"
  model: "gpt-4"

rag:
  enabled: true
  embeddings:
    provider: "openai"
    model: "text-embedding-3-small"
    dimension: 1536
  text_processing:
    chunk_size: 500
    chunk_overlap: 100
  retrieval:
    top_k: 3
    similarity_threshold: 0.8
"#;

        let config = ConfigLoader::from_str(yaml_content, None).await.unwrap();
        assert_eq!(config.agent.name, "rag_agent");

        let rag = config.rag.unwrap();
        assert!(rag.enabled);
        assert_eq!(rag.embeddings.model, "text-embedding-3-small");
        assert_eq!(rag.text_processing.chunk_size, 500);
        assert_eq!(rag.retrieval.top_k, 3);
    }

    #[tokio::test]
    async fn test_load_config_with_mcp() {
        let yaml_content = r#"
agent:
  name: "mcp_agent"

llm:
  provider: "openai"
  model: "gpt-4"

mcp_servers:
  - name: "git_server"
    command:
      run: "uvx"
      args: ["mcp-server-git"]
    timeout: 30
    enabled: true
  - name: "filesystem_server"
    command:
      run: "npx"
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    tools: 
      - "read_file"
      - "write_file"
"#;

        let config = ConfigLoader::from_str(yaml_content, None).await.unwrap();
        assert_eq!(config.mcp_servers.len(), 2);

        let git_server = &config.mcp_servers[0];
        assert_eq!(git_server.name, "git_server");
        assert_eq!(git_server.command.as_ref().unwrap().run, "uvx");
        assert_eq!(git_server.command.as_ref().unwrap().args, vec!["mcp-server-git"]);

        let fs_server = &config.mcp_servers[1];
        assert_eq!(fs_server.name, "filesystem_server");
        assert_eq!(
            fs_server.command.as_ref().unwrap().args,
            vec!["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
        );
    }

    #[tokio::test]
    async fn test_env_resolution() {
        env::set_var("TEST_API_KEY", "secret123");

        let yaml_content = r#"
agent:
  name: "env_test"

llm:
  provider: "openai"
  model: "gpt-4.1-mini"
  auth:
    api_key_env: "TEST_API_KEY"
"#;

        let config = ConfigLoader::from_str(yaml_content, None).await.unwrap();
        assert_eq!(config.llm.as_ref().unwrap().auth.api_key, Some("secret123".to_string()));

        env::remove_var("TEST_API_KEY");
    }

    #[tokio::test]
    async fn test_load_from_file() {
        let yaml_content = r#"
agent:
  name: "file_test"

llm:
  provider: "openai"
  model: "gpt-4.1-mini"
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();

        let config = ConfigLoader::from_file(temp_file.path()).await.unwrap();
        assert_eq!(config.agent.name, "file_test");
    }
}
