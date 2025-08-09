use anyhow::Result;
use clap::{Parser, Subcommand};
use gola_ag_ui_server::{
    shutdown_signal, AgUiServer, AgentHandler, ServerConfig, ToolAuthorizationMode,
};
use gola_core::{config::ConfigLoader, AgentFactory, GitHubConfigLoader};
use log::LevelFilter;
use std::net::SocketAddr;

mod direct_gola_term_client;
mod embedded_ui;
mod remote_gola_term_client;

#[derive(Debug, Clone)]
enum RunMode {
    Embedded,
    ServerOnly,
    TerminalOnly,
    Task,
}

#[derive(Parser, Debug)]
#[clap(name = "Gola", author, version = "0.1.0", about = "Gola LLM Agent Runtime")]
struct Cli {
    #[clap(subcommand)]
    command: Option<Commands>,

    #[clap(
        long,
        short,
        default_value = "gola.yaml",
        help = "Configuration source: file path, URL, or GitHub repository (e.g., github:owner/repo@branch)"
    )]
    config: String,

    #[clap(long, default_value = "127.0.0.1:3001")]
    bind_addr: String,

    #[clap(long, short, default_value = "info")]
    log_level: String,

    #[clap(long, help = "Enable the debug panel in the Gola Terminal UI")]
    term_debug_panel: bool,


    #[clap(
        long,
        help = "Run both server and terminal UI in the same process (default mode)"
    )]
    embedded: bool,

    #[clap(long, help = "Server only mode - no terminal UI")]
    server_only: bool,

    #[clap(long, help = "Terminal only mode - no server")]
    terminal_only: bool,

    #[clap(
        long,
        help = "Server URL to connect to (for --terminal-only mode)",
        default_value = "http://127.0.0.1:3001"
    )]
    server_url: String,

    #[clap(long, help = "Execute a single task and output result to stdout")]
    task: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the Gola server (default command)
    Run {
        #[clap(
            long,
            short,
            default_value = "gola.yaml",
            help = "Configuration source: file path, URL, or GitHub repository"
        )]
        config: Option<String>,

        #[clap(long, default_value = "127.0.0.1:3001")]
        bind_addr: Option<String>,

        #[clap(long, help = "Enable the debug panel in the Gola Terminal UI")]
        term_debug_panel: bool,


        #[clap(
            long,
            help = "Run both server and terminal UI in the same process (default mode)"
        )]
        embedded: bool,

        #[clap(long, help = "Server only mode - no terminal UI")]
        server_only: bool,

        #[clap(long, help = "Terminal only mode - no server")]
        terminal_only: bool,

        #[clap(
            long,
            help = "Server URL to connect to (for --terminal-only mode)",
            default_value = "http://127.0.0.1:3001"
        )]
        server_url: Option<String>,

        #[clap(long, help = "Execute a single task and output result to stdout")]
        task: Option<String>,
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

    // Determine mode first to configure logging appropriately
    let mode = match &cli.command {
        Some(Commands::Run {
            embedded,
            server_only,
            terminal_only,
            task,
            ..
        }) => {
            // Check for conflicting flags first
            let task_specified = task.is_some() || cli.task.is_some();
            let flags_count = [
                *embedded || cli.embedded,
                *server_only || cli.server_only,
                *terminal_only || cli.terminal_only,
                task_specified,
            ]
            .iter()
            .filter(|&&x| x)
            .count();
            if flags_count > 1 {
                anyhow::bail!("Conflicting mode flags specified. Only one of --embedded, --server-only, --terminal-only, or --task can be used at a time.");
            }

            if task_specified {
                RunMode::Task
            } else if *terminal_only || cli.terminal_only {
                RunMode::TerminalOnly
            } else if *embedded || cli.embedded {
                RunMode::Embedded
            } else if *server_only || cli.server_only {
                RunMode::ServerOnly
            } else {
                RunMode::Embedded
            }
        }
        _ => {
            // Check for conflicting flags first
            let task_specified = cli.task.is_some();
            let flags_count = [
                cli.embedded,
                cli.server_only,
                cli.terminal_only,
                task_specified,
            ]
            .iter()
            .filter(|&&x| x)
            .count();
            if flags_count > 1 {
                anyhow::bail!("Conflicting mode flags specified. Only one of --embedded, --server-only, --terminal-only, or --task can be used at a time.");
            }

            if task_specified {
                RunMode::Task
            } else if cli.terminal_only {
                RunMode::TerminalOnly
            } else if cli.embedded {
                RunMode::Embedded
            } else if cli.server_only {
                RunMode::ServerOnly
            } else {
                RunMode::Embedded
            }
        }
    };

    // Initialize logger with appropriate target based on mode
    let log_level_filter = cli.log_level.parse().unwrap_or(LevelFilter::Info);

    match mode {
        RunMode::Embedded => {
            // Show progress message immediately when embedded mode starts
            use std::io::{self, Write};
            print!("Starting gola embedded mode");
            io::stdout().flush().unwrap();

            // Set environment variable to signal embedded mode to core components
            std::env::set_var("GOLA_EMBEDDED_MODE", "1");

            // Add first dot immediately after basic setup
            print!(".");
            io::stdout().flush().unwrap();

            // In embedded mode, redirect logs to file to keep terminal UI clean
            use std::fs::OpenOptions;

            let log_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("gola.log")
                .expect("Failed to create gola.log file");

            env_logger::Builder::new()
                .filter_level(log_level_filter)
                .target(env_logger::Target::Pipe(Box::new(log_file)))
                .init();
        }
        RunMode::TerminalOnly => {
            // Show progress message for terminal-only mode
            use std::io::{self, Write};
            print!("Starting gola terminal-only mode");
            io::stdout().flush().unwrap();

            // In terminal-only mode, redirect logs to file to keep terminal UI clean
            use std::fs::OpenOptions;

            let log_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("gola.log")
                .expect("Failed to create gola.log file");

            env_logger::Builder::new()
                .filter_level(log_level_filter)
                .target(env_logger::Target::Pipe(Box::new(log_file)))
                .init();

            print!(".");
            io::stdout().flush().unwrap();
        }
        RunMode::Task => {
            // For task mode, redirect logs to file to keep stdout clean for task output
            use std::fs::OpenOptions;

            let log_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open("gola.log")
                .expect("Failed to create gola.log file");

            env_logger::Builder::new()
                .filter_level(log_level_filter)
                .target(env_logger::Target::Pipe(Box::new(log_file)))
                .init();
        }
        _ => {
            // For server-only mode, use normal console logging
            env_logger::Builder::new()
                .filter_level(log_level_filter)
                .init();
        }
    }

    match cli.command {
        Some(Commands::Run {
            config,
            bind_addr,
            term_debug_panel,
            embedded: _,
            server_only: _,
            terminal_only: _,
            server_url,
            task,
        }) => {
            let config = config.unwrap_or(cli.config);
            let bind_addr = bind_addr.unwrap_or(cli.bind_addr);
            let server_url = server_url.unwrap_or(cli.server_url.clone());
            let task_prompt = task.or(cli.task.clone());

            run_gola(
                config,
                bind_addr,
                mode,
                term_debug_panel || cli.term_debug_panel,
                server_url,
                task_prompt,
            )
            .await
        }
        Some(Commands::Cache { action }) => handle_cache_command(action).await,
        None => {
            // This case uses the mode already determined above
            let mode = mode;

            run_gola(
                cli.config,
                cli.bind_addr,
                mode,
                cli.term_debug_panel,
                cli.server_url,
                cli.task,
            )
            .await
        }
    }
}

async fn run_gola(
    config: String,
    bind_addr: String,
    mode: RunMode,
    _term_debug_panel: bool,
    server_url: String,
    task_prompt: Option<String>,
) -> Result<()> {
    // Terminal-only mode doesn't need configuration or agent setup
    if matches!(mode, RunMode::TerminalOnly) {
        log::info!("Terminal-only mode: skipping configuration loading");
        return run_terminal_only_ui(&server_url).await;
    }

    // Task mode: execute single task and output result
    if matches!(mode, RunMode::Task) {
        if let Some(prompt) = task_prompt {
            return run_task_mode(&config, prompt).await;
        } else {
            anyhow::bail!("Task mode requires a task prompt. Use --task \"your task here\"");
        }
    }

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
    log::info!(
        "Configuration loaded successfully for agent: {}",
        gola_config.agent.name
    );

    // Add dot after configuration loading in embedded mode
    if matches!(mode, RunMode::Embedded) {
        use std::io::{self, Write};
        print!(".");
        io::stdout().flush().unwrap();
    }

    // Create agent handler using the factory
    let factory_config = gola_core::agent_factory::AgentFactoryConfig {
        gola_config,
        local_runtimes: true,
        non_interactive: matches!(mode, RunMode::ServerOnly),
    };
    let agent_handler = AgentFactory::create_from_config(factory_config).await?;
    log::info!("GolaAgentHandler created.");

    // Add another dot after agent creation (main time-consuming step) in embedded mode
    if matches!(mode, RunMode::Embedded) {
        use std::io::{self, Write};
        print!(".");
        io::stdout().flush().unwrap();
    }

    // Parse bind address
    let bind_socket_addr: SocketAddr = bind_addr
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid bind address '{}': {}", bind_addr, e))?;

    // Configure the AgUiServer
    // Disable server HTTP logging in embedded mode to keep terminal UI clean
    let enable_server_logging = !matches!(mode, RunMode::Embedded);
    let server_config = ServerConfig::default()
        .with_bind_addr(bind_socket_addr)
        .with_logging(enable_server_logging);

    // Disable tool authorization in embedded mode
    if matches!(mode, RunMode::Embedded) {
        let reason = "automatically disabled in embedded mode (gola-term doesn't support authorization UI)";
        log::warn!("Tool authorization is disabled ({}). All tool calls will be allowed without prompting.", reason);
        agent_handler
            .set_authorization_config(gola_ag_ui_server::AuthorizationConfig {
                mode: ToolAuthorizationMode::AlwaysAllow,
                ..Default::default()
            })
            .await?;

        // Add dot after authorization setup in embedded mode
        if matches!(mode, RunMode::Embedded) {
            use std::io::{self, Write};
            print!(".");
            io::stdout().flush().unwrap();
        }
    }

    log::info!("Starting Gola server on {}...", bind_socket_addr);

    match mode {
        RunMode::Embedded => {
            // Run embedded mode - server and gola-term TUI in same process
            let agent_handler_for_ui = agent_handler.clone();

            // Add another dot before starting server and UI
            use std::io::{self, Write};
            print!(".");
            io::stdout().flush().unwrap();

            // Run server in background task
            let server = AgUiServer::with_config(agent_handler, server_config);
            let server_task =
                tokio::spawn(async move { server.serve_with_shutdown(shutdown_signal()).await });

            // Add dot after server setup
            print!(".");
            io::stdout().flush().unwrap();

            // Run gola-term UI in foreground with direct agent handler
            let ui_result = embedded_ui::run_embedded_ui(agent_handler_for_ui).await;

            // If UI exits, shutdown server
            server_task.abort();

            ui_result
        }
        RunMode::ServerOnly => {
            // Just run the server
            let server = AgUiServer::with_config(agent_handler, server_config);
            server
                .serve_with_shutdown(shutdown_signal())
                .await
                .map_err(|e| e.into())
        }
        RunMode::TerminalOnly => {
            // This case is handled earlier in the function, should never reach here
            unreachable!("TerminalOnly mode is handled earlier in run_gola function");
        }
        RunMode::Task => {
            // This case is handled earlier in the function, should never reach here
            unreachable!("Task mode is handled earlier in run_gola function");
        }
    }
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

/// Run terminal-only UI connected to a remote gola server
async fn run_terminal_only_ui(server_url: &str) -> Result<()> {
    

    // Complete the startup progress
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    println!(" ready!");

    // Initialize gola-term configuration with defaults
    use gola_term::configuration::{Config, ConfigKey};
    for key in [
        ConfigKey::Editor,
        ConfigKey::Theme,
        ConfigKey::ThemeFile,
        ConfigKey::GolaAgUIURL,
        ConfigKey::SessionID,
        ConfigKey::ConfigFile,
    ] {
        if Config::get(key).is_empty() {
            Config::set(key, &Config::default(key));
        }
    }

    // Set up panic handler
    std::panic::set_hook(Box::new(|panic_info| {
        gola_term::application::ui::destruct_terminal_for_panic();
        better_panic::Settings::auto().create_panic_handler()(panic_info);
    }));

    // Create the remote client
    let remote_client =
        crate::remote_gola_term_client::RemoteGolaTermClient::new(server_url.to_string());

    // Test server connection
    if let Err(e) = remote_client.health_check_server().await {
        anyhow::bail!("Failed to connect to server at {}: {}", server_url, e);
    }

    let gola_term_client = Box::new(remote_client);

    // Create channels for communication (same as embedded UI)
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();

    let mut background_futures = tokio::task::JoinSet::new();

    // Start actions service
    let action_tx_clone = action_tx.clone();
    background_futures.spawn(async move {
        gola_term::domain::services::actions::ActionsService::start(
            gola_term_client,
            action_tx_clone,
            event_tx,
            &mut action_rx,
        )
        .await
    });

    // Start clipboard service if available
    use gola_term::domain::services::clipboard::ClipboardService;
    if ClipboardService::healthcheck().is_ok() {
        background_futures.spawn(async move { ClipboardService::start().await });
    }

    // Create another remote client for the UI layer
    let ui_remote_client =
        crate::remote_gola_term_client::RemoteGolaTermClient::new(server_url.to_string());
    let ui_client = Box::new(ui_remote_client);

    // Start the UI
    let ui_future = embedded_ui::start_embedded_ui(ui_client, action_tx, event_rx);

    let result = tokio::select!(
        res = background_futures.join_next() => res.unwrap().unwrap(),
        res = ui_future => res,
    );

    if result.is_err() {
        gola_term::application::ui::destruct_terminal_for_panic();
    }

    result
}

/// Run task mode - execute a single task and output result to stdout
async fn run_task_mode(config: &str, task_prompt: String) -> Result<()> {
    use futures_util::StreamExt;
    use gola_ag_ui_types::{Message, RunAgentInput};
    use uuid::Uuid;

    log::info!("Loading configuration from: {}", config);

    // Load configuration from various sources
    let gola_config = if config.starts_with("github:") {
        log::info!("Loading configuration from GitHub repository: {}", config);
        ConfigLoader::from_source(config).await?
    } else if config.starts_with("http://") || config.starts_with("https://") {
        log::info!("Loading configuration from URL: {}", config);
        ConfigLoader::from_source(config).await?
    } else {
        log::info!("Loading configuration from file: {}", config);
        ConfigLoader::from_file(config).await?
    };
    log::info!(
        "Configuration loaded successfully for agent: {}",
        gola_config.agent.name
    );

    // Create agent handler with headless settings
    let factory_config = gola_core::agent_factory::AgentFactoryConfig {
        gola_config,
        local_runtimes: true,
        non_interactive: true, // Task mode is non-interactive
    };
    let agent_handler = AgentFactory::create_from_config(factory_config).await?;
    log::info!("Agent handler created for task execution");

    // Disable tool authorization - auto-allow all tools in task mode
    log::info!("Disabling tool authorization for task mode (all tools will be auto-approved)");
    agent_handler
        .set_authorization_config(gola_ag_ui_server::AuthorizationConfig {
            mode: ToolAuthorizationMode::AlwaysAllow,
            ..Default::default()
        })
        .await?;

    // Create a user message with the task prompt, adding formatting instructions
    let formatted_prompt = format!(
        "{}\n\nPlease respond in plain text without LaTeX formatting or escaped characters.",
        task_prompt
    );
    let user_message = Message::new_user(Uuid::new_v4().to_string(), formatted_prompt);

    // Create the agent input
    let agent_input = RunAgentInput::new(
        Uuid::new_v4().to_string(), // thread_id
        Uuid::new_v4().to_string(), // run_id
        serde_json::Value::Null,    // state
        vec![user_message],         // messages
        vec![],                     // tools
        vec![],                     // context
        serde_json::Value::Null,    // forwarded_props
    );

    // Execute the task
    log::info!("Executing task: {}", task_prompt);

    // Get the event stream from the agent
    let mut event_stream = agent_handler
        .handle_input(agent_input)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to handle agent input: {}", e))?;

    // Process events and output to stdout
    while let Some(event) = event_stream.next().await {
        match event {
            gola_ag_ui_types::Event::TextMessageContent(content_event) => {
                print!("{}", content_event.delta);
                use std::io::{self, Write};
                io::stdout().flush().unwrap();
            }
            gola_ag_ui_types::Event::TextMessageChunk(chunk_event) => {
                if let Some(delta) = &chunk_event.delta {
                    print!("{}", delta);
                    use std::io::{self, Write};
                    io::stdout().flush().unwrap();
                }
            }
            gola_ag_ui_types::Event::RunFinished(_) => {
                // Task completed successfully - ensure output ends with newline
                println!();
                break;
            }
            gola_ag_ui_types::Event::RunError(error_event) => {
                log::error!("Task execution failed: {}", error_event.message);
                anyhow::bail!("Task execution failed: {}", error_event.message);
            }
            _ => {
                // Ignore other event types for now (RunStarted, etc.)
            }
        }
    }

    log::info!("Task completed successfully");
    Ok(())
}
