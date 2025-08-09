//! Web server for hosting Gola agents with HTTP/WebSocket interfaces
//!
//! This binary provides a production-ready server for deploying Gola agents as
//! network services. The design prioritizes accessibility by exposing agents through
//! standard web protocols, enabling integration with existing infrastructure and
//! tooling. By decoupling agent execution from client interfaces, this architecture
//! supports diverse deployment scenarios from local development to cloud-native
//! orchestration.

use anyhow::Result;
use clap::{Parser, Subcommand};
use gola_core::{config::ConfigLoader, AgentFactory, GitHubConfigLoader};
use gola_ag_ui_server::{AgUiServer, ServerConfig, shutdown_signal, AgentHandler, ToolAuthorizationMode};
use log::LevelFilter;
use std::net::SocketAddr;

#[derive(Parser, Debug)]
#[clap(author, version, about = "Gola Server - Run the Gola agent server")]
struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,

    #[clap(long, short, default_value = "gola.yaml", help = "Configuration source: file path, URL, or GitHub repository (e.g., github:owner/repo@branch)")]
    config: String,

    #[clap(long, default_value = "127.0.0.1:3001")]
    bind_addr: String,

    #[clap(long, short, default_value = "info")]
    log_level: String,

    #[clap(long, help = "Disable tool authorization checks (tools will be allowed to run without prompting)")]
    no_tool_auth: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the Gola server (default command)
    Run {
        #[clap(long, short, default_value = "gola.yaml", help = "Configuration source: file path, URL, or GitHub repository")]
        config: Option<String>,
        
        #[clap(long, default_value = "127.0.0.1:3001")]
        bind_addr: Option<String>,

        #[clap(long, help = "Disable tool authorization checks")]
        no_tool_auth: bool,
    },
    /// Manage GitHub repository cache
    Cache {
        #[clap(subcommand)]
        action: CacheCommands,
    },
}

#[derive(Subcommand, Debug)]
enum CacheCommands {
    /// List cached GitHub repositories
    List,
    /// Clear the GitHub repository cache
    Clear,
    /// Show cache directory location
    Info,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logger
    let log_level_filter = cli.log_level.parse().unwrap_or(LevelFilter::Info);
    env_logger::Builder::new()
        .filter_level(log_level_filter)
        .init();

    match cli.command {
        Some(Commands::Run { config, bind_addr, no_tool_auth }) => {
            let config = config.unwrap_or(cli.config);
            let bind_addr = bind_addr.unwrap_or(cli.bind_addr);
            run_server(config, bind_addr, no_tool_auth || cli.no_tool_auth).await
        }
        Some(Commands::Cache { action }) => {
            handle_cache_command(action).await
        }
        None => {
            // Default behavior: run the server
            run_server(cli.config, cli.bind_addr, cli.no_tool_auth).await
        }
    }
}

async fn run_server(config: String, bind_addr: String, no_tool_auth: bool) -> Result<()> {
    log::info!("Loading configuration from: {}", config);

    // Load configuration from various sources
    let gola_config = if config.starts_with("github:") {
        log::info!("Loading configuration from GitHub repository: {}", config);
        ConfigLoader::from_source(&config).await?
    } else if config.starts_with("http://") || config.starts_with("https://") {
        log::info!("Loading configuration from URL: {}", config);
        ConfigLoader::from_source(&config).await?
    } else {
        log::info!("Loading configuration from file: {}", config);
        ConfigLoader::from_file(&config).await?
    };
    log::info!("Configuration loaded successfully for agent: {}", gola_config.agent.name);

    // Create agent handler using the factory
    let factory_config = gola_core::agent_factory::AgentFactoryConfig {
        gola_config,
        local_runtimes: true,
        non_interactive: false,
    };
    let agent_handler = AgentFactory::create_from_config(factory_config).await?;
    log::info!("GolaAgentHandler created.");

    // Parse bind address
    let bind_socket_addr: SocketAddr = bind_addr.parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address '{}': {}", bind_addr, e))?;

    // Configure the AgUiServer
    let server_config = ServerConfig::default()
        .with_bind_addr(bind_socket_addr)
        .with_logging(true); 

    if no_tool_auth {
        log::warn!("Tool authorization is disabled. All tool calls will be allowed without prompting.");
        agent_handler.set_authorization_config(gola_ag_ui_server::AuthorizationConfig {
            mode: ToolAuthorizationMode::AlwaysAllow,
            ..Default::default()
        }).await?;
    }

    log::info!("Starting Gola server on {}...", bind_socket_addr);

    let server = AgUiServer::with_config(agent_handler, server_config);

    // Run the server
    if let Err(e) = server.serve_with_shutdown(shutdown_signal()).await {
        log::error!("Server failed: {}", e);
        return Err(e.into());
    }

    log::info!("Gola server shut down gracefully.");
    Ok(())
}

async fn handle_cache_command(action: CacheCommands) -> Result<()> {
    let github_loader = GitHubConfigLoader::new()?;

    match action {
        CacheCommands::List => {
            let cached_repos = github_loader.list_cached_repos()?;
            if cached_repos.is_empty() {
                println!("No cached GitHub repositories found.");
            } else {
                println!("Cached GitHub repositories:");
                for repo in cached_repos {
                    println!("  {}", repo);
                }
            }
        }
        CacheCommands::Clear => {
            github_loader.clear_cache()?;
            println!("GitHub repository cache cleared.");
        }
        CacheCommands::Info => {
            let cache_dir = dirs::cache_dir()
                .unwrap_or_else(|| dirs::home_dir().unwrap().join(".cache"))
                .join("gola")
                .join("github-repos");
            println!("GitHub repository cache directory: {}", cache_dir.display());
            
            let cached_repos = github_loader.list_cached_repos()?;
            println!("Number of cached repositories: {}", cached_repos.len());
        }
    }

    Ok(())
}