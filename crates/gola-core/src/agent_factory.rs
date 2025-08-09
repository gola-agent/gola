//! Agent factory for creating configured agents from GolaConfig

use crate::executors::runtime_manager::RuntimeManager;
use crate::tracing::TracingTraceHandler;
use crate::ag_ui_handler::GolaAgentHandler; // Added import
use crate::agent::{Agent, AgentConfig};
use crate::config::GolaConfig;
use crate::errors::AgentError;
use crate::executors::{docker::DockerCodeExecutor, CodeExecutor};
use crate::guardrails::AuthorizationMode;
use crate::llm::{LLM, utils::LLMFactory};
use crate::rag::{
    embeddings::{
        EmbeddingGenerator, EmbeddingProvider as RagEmbeddingProvider, RestEmbeddingConfig,
        RestEmbeddingFactory,
    },
    splitter::TextSplitter,
    vector_store::{InMemoryVectorStore, PersistentVectorStore, VectorStore},
    Rag, RagConfig, RagDocument, RagSystem,
};
use crate::tools::{CalculatorTool, MCPToolFactory, RMCPClient, Tool, WebSearchTool};
use crate::tools::control_plane::ControlPlaneServer;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex; // Added import

/// Factory for creating configured agents from GolaConfig
pub struct AgentFactory;

pub struct AgentFactoryConfig {
    pub gola_config: GolaConfig,
    pub local_runtimes: bool,
    pub non_interactive: bool,
}

impl AgentFactory {
    /// Create a new GolaAgentHandler (ag-ui compatible agent) from a GolaConfig
    pub async fn create_from_config(
        factory_config: AgentFactoryConfig,
    ) -> Result<GolaAgentHandler, AgentError> {
        let config = factory_config.gola_config;
        let local_runtimes = factory_config.local_runtimes;
        let non_interactive = factory_config.non_interactive;

        let llm = Self::configure_llm(&config)?;
        let tools = Self::configure_tools(&config, local_runtimes, non_interactive).await?;
        let code_executor = Self::configure_code_executor(&config).await?;
        let mut agent_core_config = Self::configure_agent_config(&config); // Renamed for clarity
                                                                       //
        // For the GolaAgentHandler, we ALWAYS want to start in Ask mode
        // so that the PollingAuthorizationHandler can manage the state.
        agent_core_config.authorization_mode = AuthorizationMode::Ask;

        let mut agent_instance = if config.rag.as_ref().map_or(false, |r| r.enabled) {
            let rag_system = Self::configure_rag_system(&config).await?;
            Agent::with_rag(llm.clone(), tools, code_executor, agent_core_config, rag_system)
        } else {
            Agent::new(llm.clone(), tools, code_executor, agent_core_config)
        };

        if config.tracing.enabled {
            let tracing_llm = if config.tracing.model_provider == "default" {
                llm
            } else {
                // In the future, we can configure a separate LLM for Tracing here
                llm
            };
            let tracing_handler = TracingTraceHandler::new(config.tracing.clone(), tracing_llm)
                .map_err(|e| AgentError::IoError(e.to_string()))?;
            agent_instance.set_trace_handler(Box::new(tracing_handler));
        }

        let agent_arc_mutex = Arc::new(Mutex::new(agent_instance));
        let gola_config_arc = Arc::new(config);

        let handler = GolaAgentHandler::new(agent_arc_mutex, gola_config_arc);
        Ok(handler)
    }

    fn configure_llm(config: &GolaConfig) -> Result<Arc<dyn LLM>, AgentError> {
        match &config.llm {
            Some(llm_config) => {
                // Use the new provider system with all wrappers applied consistently
                LLMFactory::create_llm_with_config(llm_config)
            }
            None => {
                // Auto-detect LLM provider from environment variables
                use crate::config::defaults::providers::environment::EnvironmentLlmProvider;
                use crate::config::defaults::traits::{DefaultProvider, DefaultsContext, ProjectInfo};
                use std::collections::HashMap;
                
                let context = DefaultsContext {
                    working_dir: std::env::current_dir().unwrap_or_default(),
                    environment: std::env::var("ENVIRONMENT").ok(),
                    project_info: ProjectInfo::default(),
                    env_vars: std::env::vars().collect::<HashMap<String, String>>(),
                    active_profile: None,
                };
                
                let env_provider = EnvironmentLlmProvider::new();
                let llm_config = env_provider.provide_defaults(&context)?;
                LLMFactory::create_llm_with_config(&llm_config)
            }
        }
    }

    async fn configure_tools(
        config: &GolaConfig,
        local_runtimes: bool,
        non_interactive: bool,
    ) -> Result<HashMap<String, Arc<dyn Tool>>, AgentError> {
        let mut tools_map: HashMap<String, Arc<dyn Tool>> = HashMap::new();
        let runtime_manager = RuntimeManager::new(local_runtimes, non_interactive);

        if config.tools.calculator {
            tools_map.insert("calculator".to_string(), Arc::new(CalculatorTool::new()));
        }

        if let Some(ws_config) = &config.tools.web_search {
            if ws_config.enabled {
                let ws_tool = match ws_config.provider {
                    crate::config::WebSearchProvider::DuckDuckGo => WebSearchTool::new(),
                    crate::config::WebSearchProvider::Tavily => {
                        let api_key = ws_config.auth.api_key.clone().ok_or_else(|| {
                            AgentError::ConfigError(
                                "Tavily API key not found for web search".to_string(),
                            )
                        })?;
                        WebSearchTool::with_tavily_api_key(api_key)
                    }
                    crate::config::WebSearchProvider::Serper => {
                        let api_key = ws_config.auth.api_key.clone().ok_or_else(|| {
                            AgentError::ConfigError(
                                "Serper API key not found for web search".to_string(),
                            )
                        })?;
                        WebSearchTool::with_serper_api_key(api_key)
                    }
                };
                tools_map.insert("web_search".to_string(), Arc::new(ws_tool));
            }
        }

        for mcp_server_config in &config.mcp_servers {
            if !mcp_server_config.enabled {
                log::debug!(
                    "MCP server {} is disabled, skipping",
                    mcp_server_config.name
                );
                continue;
            }

            // Determine which configuration format to use and convert to execution_type for runtime manager
            let execution_type = match (&mcp_server_config.command, &mcp_server_config.execution_type, &mcp_server_config.execution_environment) {
                // execution_environment format - clearest semantics about where code runs
                (None, None, Some(exec_env)) => {
                    // Convert execution_environment to execution_type for runtime manager compatibility
                    Self::convert_execution_environment_to_type(exec_env)
                }
                // Legacy command format - direct command execution
                (Some(cmd), None, None) => {
                    // Convert legacy command format to execution_type
                    crate::config::types::McpExecutionType::Command { command: cmd.clone() }
                }
                // execution_type format - backwards compatibility
                (None, Some(exec_type), None) => {
                    // Use execution_type directly
                    exec_type.clone()
                }
                _ => {
                    // Multiple formats specified - should have been caught by validation
                    log::error!("MCP server {} has invalid configuration: multiple execution formats specified", mcp_server_config.name);
                    continue;
                }
            };

            let command_result = runtime_manager.resolve_command(&execution_type).await;

            let command = match command_result {
                Ok(cmd) => cmd,
                Err(e) => {
                    log::error!("Failed to resolve command for MCP server {}: {}", mcp_server_config.name, e);
                    continue;
                }
            };

            let mcp_command = crate::config::types::McpCommand {
                run: command.as_std().get_program().to_string_lossy().to_string(),
                args: command.as_std().get_args().map(|s| s.to_string_lossy().to_string()).collect(),
                env: command.as_std().get_envs().map(|(k, v)| (k.to_string_lossy().to_string(), v.unwrap_or_default().to_string_lossy().to_string())).collect(),
                working_dir: command.as_std().get_current_dir().map(|p| p.to_path_buf()),
                token_limit: 2000,
            };

            match RMCPClient::new_with_mcp_command(&mcp_command).await {
                Ok(rmcp_client) => {
                    log::info!(
                        "Successfully connected to MCP server: {}",
                        mcp_server_config.name
                    );
                    let factory = MCPToolFactory::new(
                        Arc::new(rmcp_client),
                        mcp_server_config.description_token_limit,
                    );
                    log::info!(
                        "About to call create_all_tools for: {}",
                        mcp_server_config.name
                    );

                    // Use longer timeout for Gmail MCP server which may need OAuth setup
                    let timeout_duration = std::time::Duration::from_secs(60); // 60 seconds for Gmail

                    match tokio::time::timeout(timeout_duration, factory.create_all_tools())
                        .await
                    {
                        Ok(Ok(mcp_tools_vec)) => {
                            log::info!(
                                "Successfully created {} tools from MCP server: {}",
                                mcp_tools_vec.len(),
                                mcp_server_config.name
                            );
                            for tool in mcp_tools_vec {
                                let tool_name = tool.metadata().name.clone();
                                tools_map.insert(tool_name.clone(), tool);
                                log::debug!("Registered MCP tool: {}", tool_name);
                            }
                        }
                        Ok(Err(e)) => {
                            log::error!(
                                "Failed to create tools from MCP server {}: {}",
                                mcp_server_config.name,
                                e
                            );
                        }
                        Err(_) => {
                            log::error!(
                                "Timeout creating tools from MCP server: {}. Skipping.",
                                mcp_server_config.name
                            );
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "Failed to connect to MCP server {}: {}",
                        mcp_server_config.name,
                        e
                    );
                    return Err(AgentError::ConfigError(format!(
                        "Failed to initialize MCP server '{}': {}. This is a critical error that prevents agent startup.",
                        mcp_server_config.name,
                        e
                    )));
                }
            }
        }
        // Add control plane tools to the tools map
        let control_plane = ControlPlaneServer::new();
        for tool_name in control_plane.list_tools() {
            if let Some(control_tool) = control_plane.get_tool(&tool_name) {
                log::info!("Adding control plane tool: {}", tool_name);
                tools_map.insert(tool_name, control_tool);
            }
        }
        
        Ok(tools_map)
    }

    async fn configure_code_executor(
        config: &GolaConfig,
    ) -> Result<Option<Arc<dyn CodeExecutor>>, AgentError> {
        if let Some(ce_config) = &config.tools.code_execution {
            if ce_config.enabled {
                match ce_config.backend {
                    crate::config::CodeExecutionBackend::Docker => {
                        let executor =
                            DockerCodeExecutor::new(ce_config.timeout)
                                .await
                                .map_err(|e| {
                                    AgentError::ConfigError(format!(
                                        "Failed to create DockerCodeExecutor: {}",
                                        e
                                    ))
                                })?;
                        return Ok(Some(Arc::new(executor)));
                    }
                    crate::config::CodeExecutionBackend::Local => {
                        log::warn!("Local code executor is not yet implemented. Skipping.");
                    }
                }
            }
        }
        Ok(None)
    }

    fn configure_agent_config(config: &GolaConfig) -> AgentConfig {
        let agent_gola_config = &config.agent;
        let rag_enabled = config.rag.as_ref().map_or(false, |r| r.enabled);

        let agent_rag_config = if rag_enabled {
            config.rag.as_ref().map(|gola_rag_conf| RagConfig {
                chunk_size: gola_rag_conf.text_processing.chunk_size,
                chunk_overlap: gola_rag_conf.text_processing.chunk_overlap,
                top_k: gola_rag_conf.retrieval.top_k,
                similarity_threshold: gola_rag_conf.retrieval.similarity_threshold,
                embedding_model_name: gola_rag_conf.embeddings.model.clone(),
                reranker_model_name: gola_rag_conf.retrieval.reranker_model.clone(),
                persistent_vector_store_path: gola_rag_conf
                    .vector_store
                    .persistence
                    .as_ref()
                    .map(|p| p.vector_store_path.to_string_lossy().into_owned()),
                embedding_cache: gola_rag_conf.embedding_cache.clone(),
            })
        } else {
            None
        };

        // Extract memory configuration from agent behavior
        let memory_config = Some(agent_gola_config.behavior.memory.clone());

        let mut system_prompt = None;

        if let Some(prompt_config) = &config.prompts {
            if let Some(roles) = &prompt_config.roles {
                system_prompt = roles.assembled.clone();
            }
        }

        AgentConfig {
            max_steps: agent_gola_config.max_steps,
            enable_rag: rag_enabled,
            rag_config: agent_rag_config,
            system_prompt,
            memory_config,
            authorization_mode: AuthorizationMode::default(),
        }
    }

    async fn configure_rag_system(config: &GolaConfig) -> Result<Box<dyn Rag>, AgentError> {
        let rag_gola_config = config.rag.as_ref().ok_or_else(|| {
            AgentError::ConfigError("RAG config expected but not found".to_string())
        })?;

        log::info!(
            "DEBUG: rag_gola_config.embedding_cache before use: {:?}",
            rag_gola_config.embedding_cache
        );

        let agent_rag_conf = RagConfig {
            chunk_size: rag_gola_config.text_processing.chunk_size,
            chunk_overlap: rag_gola_config.text_processing.chunk_overlap,
            top_k: rag_gola_config.retrieval.top_k,
            similarity_threshold: rag_gola_config.retrieval.similarity_threshold,
            embedding_model_name: rag_gola_config.embeddings.model.clone(),
            reranker_model_name: rag_gola_config.retrieval.reranker_model.clone(),
            persistent_vector_store_path: rag_gola_config
                .vector_store
                .persistence
                .as_ref()
                .map(|p| p.vector_store_path.to_string_lossy().into_owned()),
            embedding_cache: rag_gola_config.embedding_cache.clone(),
        };

        let embedding_generator: Box<dyn EmbeddingGenerator> = match &rag_gola_config
            .embeddings
            .provider
        {
            crate::config::EmbeddingProvider::Simple => {
                Box::new(crate::rag::DummyEmbeddingGenerator::with_dimension(
                    rag_gola_config.embeddings.dimension,
                ))
            }
            provider => {
                let rest_config = RestEmbeddingConfig {
                    api_base_url: match provider {
                        crate::config::EmbeddingProvider::Custom { base_url } => base_url.clone(),
                        crate::config::EmbeddingProvider::OpenAI => {
                            "https://api.openai.com/v1".to_string()
                        }
                        crate::config::EmbeddingProvider::Cohere => {
                            "https://api.cohere.ai/v1".to_string()
                        }
                        crate::config::EmbeddingProvider::HuggingFace => {
                            "https://api-inference.huggingface.co/pipeline/feature-extraction"
                                .to_string()
                        }
                        _ => {
                            return Err(AgentError::ConfigError(
                                "Unsupported REST embedding provider configuration for base URL"
                                    .to_string(),
                            ))
                        }
                    },
                    api_key: rag_gola_config.embeddings.auth.api_key.clone(),
                    model_name: rag_gola_config.embeddings.model.clone(),
                    embedding_dimension: rag_gola_config.embeddings.dimension,
                    timeout_seconds: 30,
                    max_batch_size: rag_gola_config.embeddings.batch_size,
                    provider: match provider {
                        crate::config::EmbeddingProvider::OpenAI => RagEmbeddingProvider::OpenAI,
                        crate::config::EmbeddingProvider::Cohere => RagEmbeddingProvider::Cohere,
                        crate::config::EmbeddingProvider::HuggingFace => {
                            RagEmbeddingProvider::HuggingFace
                        }
                        crate::config::EmbeddingProvider::Custom { .. } => {
                            RagEmbeddingProvider::Custom
                        }
                        crate::config::EmbeddingProvider::Simple => {
                            return Err(AgentError::ConfigError(
                                "'Simple' provider should use DummyEmbeddingGenerator directly"
                                    .to_string(),
                            ))
                        }
                    },
                };
                match RestEmbeddingFactory::create_from_provider(
                    match provider {
                        crate::config::EmbeddingProvider::OpenAI => "openai",
                        crate::config::EmbeddingProvider::Cohere => "cohere",
                        crate::config::EmbeddingProvider::HuggingFace => "huggingface",
                        crate::config::EmbeddingProvider::Custom { .. } => "custom",
                        _ => "openai",
                    },
                    Some(rag_gola_config.embeddings.model.clone()),
                ) {
                    Ok(client) => Box::new(client),
                    Err(_) => Box::new(
                        crate::rag::embeddings::RestEmbeddingClient::new(rest_config).map_err(
                            |e| {
                                AgentError::ConfigError(format!(
                                    "Failed to create REST embedding client: {}",
                                    e
                                ))
                            },
                        )?,
                    ),
                }
            }
        };

        let vector_store: Box<dyn VectorStore> = match rag_gola_config.vector_store.store_type {
            crate::config::VectorStoreType::InMemory => Box::new(InMemoryVectorStore::new()),
            crate::config::VectorStoreType::Persistent => {
                let path = rag_gola_config
                    .vector_store
                    .persistence
                    .as_ref()
                    .ok_or_else(|| {
                        AgentError::ConfigError(
                            "Persistent vector store path not configured".to_string(),
                        )
                    })?
                    .vector_store_path
                    .clone();

                let mode = rag_gola_config
                    .vector_store
                    .persistence
                    .as_ref()
                    .map(|p| &p.mode)
                    .unwrap_or(&crate::config::PersistenceMode::CreateOrLoad);

                match mode {
                    crate::config::PersistenceMode::Load => {
                        Box::new(PersistentVectorStore::load(&path).await.map_err(|e| {
                            AgentError::ConfigError(format!(
                                "Failed to load persistent vector store: {}",
                                e
                            ))
                        })?)
                    }
                    crate::config::PersistenceMode::Create => {
                        Box::new(PersistentVectorStore::new().with_file_path(path))
                    }
                    crate::config::PersistenceMode::CreateOrLoad => {
                        match PersistentVectorStore::load(&path).await {
                            Ok(store) => Box::new(store),
                            Err(_) => Box::new(PersistentVectorStore::new().with_file_path(path)),
                        }
                    }
                }
            }
        };

        let text_splitter = TextSplitter::new(
            rag_gola_config.text_processing.chunk_size,
            rag_gola_config.text_processing.chunk_overlap,
        );

        // Use with_components_cached instead of with_components to enable caching
        let mut rag_system = RagSystem::with_components_cached(
            agent_rag_conf,
            vector_store,
            embedding_generator,
            text_splitter,
        )
        .await?;

        for source_config in &rag_gola_config.document_sources {
            match &source_config.source_type {
                crate::config::DocumentSourceType::Files { paths } => {
                    let path_strs: Vec<String> = paths
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                    rag_system
                        .add_documents_from_paths(&path_strs)
                        .await
                        .map_err(|e| {
                            AgentError::ConfigError(format!(
                                "Failed to add documents from paths: {}",
                                e
                            ))
                        })?;
                }
                crate::config::DocumentSourceType::Directory { path, recursive: _ } => {
                    rag_system
                        .add_documents_from_paths(&[path.to_string_lossy().into_owned()])
                        .await
                        .map_err(|e| {
                            AgentError::ConfigError(format!(
                                "Failed to add documents from directory: {}",
                                e
                            ))
                        })?;
                }
                crate::config::DocumentSourceType::Url { url } => {
                    let content = reqwest::get(url)
                        .await
                        .map_err(|e| {
                            AgentError::ConfigError(format!("Failed to fetch URL {}: {}", url, e))
                        })?
                        .text()
                        .await
                        .map_err(|e| {
                            AgentError::ConfigError(format!(
                                "Failed to read content from URL {}: {}",
                                url, e
                            ))
                        })?;
                    rag_system
                        .add_documents(vec![RagDocument::new(content, url.clone())])
                        .await
                        .map_err(|e| {
                            AgentError::ConfigError(format!(
                                "Failed to add document from URL: {}",
                                e
                            ))
                        })?;
                }
                crate::config::DocumentSourceType::Inline { content, name } => {
                    rag_system
                        .add_documents(vec![RagDocument::new(content.clone(), name.clone())])
                        .await
                        .map_err(|e| {
                            AgentError::ConfigError(format!("Failed to add inline document: {}", e))
                        })?;
                }
            }
        }

        // Explicitly save the cache after initial document loading
        let current_rag_config = rag_system.config();
        if current_rag_config.embedding_cache.persistent {
            if let Some(cache_path_str) = &current_rag_config.embedding_cache.cache_file_path {
                let cache_path = std::path::PathBuf::from(cache_path_str);
                log::info!(
                    "Attempting to save embedding cache to: {}",
                    cache_path.display()
                );
                if let Err(e) = rag_system.save_cache(&cache_path).await {
                    log::warn!(
                        "Failed to save embedding cache after document loading: {}",
                        e
                    );
                } else {
                    log::info!(
                        "Successfully saved embedding cache after document loading to: {}",
                        cache_path.display()
                    );
                }
            } else {
                log::warn!("Persistent embedding cache enabled, but no cache_file_path configured. Cannot save.");
            }
        }

        // Log cache size if available
        if let Some(cache_size) = rag_system.cache_size().await {
            log::info!(
                "RAG system initialized. Embedding cache final size: {}",
                cache_size
            );
        }

        Ok(Box::new(rag_system))
    }

    /// Convert execution_environment to execution_type for backward compatibility with runtime manager
    fn convert_execution_environment_to_type(exec_env: &crate::config::types::McpExecutionEnvironment) -> crate::config::types::McpExecutionType {
        use crate::config::types::{McpExecutionEnvironment, McpExecutionType};
        
        match exec_env {
            McpExecutionEnvironment::Host { runtime, entry_point, args, env, working_dir } => {
                // Host environment: assumes runtime is already available on the system
                // Map to Command type with direct execution
                let command_args = match runtime.as_str() {
                    "python" => {
                        let mut cmd_args = vec![entry_point.clone()];
                        cmd_args.extend(args.clone());
                        cmd_args
                    }
                    "nodejs" => {
                        let mut cmd_args = vec![entry_point.clone()];
                        cmd_args.extend(args.clone());
                        cmd_args
                    }
                    "rust" => {
                        // For rust, entry_point is usually a git URL or path
                        let mut cmd_args = vec!["install".to_string(), "--git".to_string(), entry_point.clone()];
                        cmd_args.extend(args.clone());
                        cmd_args
                    }
                    _ => {
                        let mut cmd_args = vec![entry_point.clone()];
                        cmd_args.extend(args.clone());
                        cmd_args
                    }
                };

                McpExecutionType::Command {
                    command: crate::config::types::McpCommand {
                        run: runtime.clone(),
                        args: command_args,
                        env: env.clone(),
                        working_dir: working_dir.clone(),
                        token_limit: crate::config::types::default_mcp_token_limit(),
                    }
                }
            }
            McpExecutionEnvironment::Local { runtime, entry_point, args, env, working_dir } => {
                // Local environment: system downloads and manages runtime
                // Map to Runtime type (current behavior)
                McpExecutionType::Runtime {
                    runtime: runtime.clone(),
                    entry_point: entry_point.clone(),
                    args: args.clone(),
                    env: env.clone(),
                    working_dir: working_dir.clone(),
                }
            }
            McpExecutionEnvironment::Container { image, tag, entry_point, args, env, working_dir, volumes } => {
                // Container environment: run in Docker container
                // For now, map to Command type with docker command until we implement container support in runtime manager
                let image_with_tag = format!("{}:{}", image, tag);
                let mut docker_args = vec!["run".to_string(), "--rm".to_string()];
                
                // Add volume mounts
                for volume in volumes {
                    docker_args.push("-v".to_string());
                    docker_args.push(volume.clone());
                }
                
                // Add environment variables
                for (key, value) in env {
                    docker_args.push("-e".to_string());
                    docker_args.push(format!("{}={}", key, value));
                }
                
                // Add working directory if specified
                if let Some(wd) = working_dir {
                    docker_args.push("-w".to_string());
                    docker_args.push(wd.to_string_lossy().to_string());
                }
                
                docker_args.push(image_with_tag);
                docker_args.push(entry_point.clone());
                docker_args.extend(args.clone());
                
                McpExecutionType::Command {
                    command: crate::config::types::McpCommand {
                        run: "docker".to_string(),
                        args: docker_args,
                        env: std::collections::HashMap::new(),
                        working_dir: None,
                        token_limit: crate::config::types::default_mcp_token_limit(),
                    }
                }
            }
        }
    }
}