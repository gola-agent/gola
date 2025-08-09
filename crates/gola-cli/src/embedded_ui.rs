use anyhow::Result;
use std::io;
use tokio::sync::mpsc;
use tokio::task;

use crossterm::{
    cursor,
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use gola_term::application::ui::{destruct_terminal_for_panic, start_loop};
use gola_term::configuration::{Config, ConfigKey};
use gola_term::domain::models::{Action, AgentClientBox, EditorName, Event};
use gola_term::domain::services::{AppStateProps, Sessions};
use gola_term::infrastructure::editors::EditorManager;
use ratatui::{backend::CrosstermBackend, Terminal};

/// Start gola-term UI with a custom injected AgentClient
pub async fn start_embedded_ui(
    agent_client: AgentClientBox,
    tx: mpsc::UnboundedSender<Action>,
    rx: mpsc::UnboundedReceiver<Event>,
) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    enable_raw_mode()?;
    crossterm::execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;

    let term_backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(term_backend)?;
    let editor_name = EditorName::parse(Config::get(ConfigKey::Editor)).unwrap_or(EditorName::None);

    let mut session_id = None;
    if !Config::get(ConfigKey::SessionID).is_empty() {
        session_id = Some(Config::get(ConfigKey::SessionID));
    }

    let editor = EditorManager::get(editor_name.clone())
        .unwrap_or_else(|_| EditorManager::get(EditorName::None).unwrap());

    let app_state_props = AppStateProps {
        agent_client,
        editor,
        theme_name: Config::get(ConfigKey::Theme),
        theme_file: Config::get(ConfigKey::ThemeFile),
        session_id,
        sessions_service: Sessions::default(),
    };

    let result = start_loop(&mut terminal, app_state_props, tx, rx).await;

    let editor = EditorManager::get(editor_name)?;
    if editor.health_check().await.is_ok() {
        let _ = editor.clear_context().await;
    }

    disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste
    )?;

    let _ = crossterm::execute!(io::stdout(), cursor::Show);

    result
}

async fn complete_startup_progress() {
    

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    println!(" ready!");
}

/// Initialize embedded UI environment and start with direct agent client
pub async fn run_embedded_ui(
    agent_handler: gola_core::ag_ui_handler::GolaAgentHandler,
) -> Result<()> {
    complete_startup_progress().await;

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

    // Set up the same panic handler as gola-term
    std::panic::set_hook(Box::new(|panic_info| {
        destruct_terminal_for_panic();
        better_panic::Settings::auto().create_panic_handler()(panic_info);
    }));

    // Create the gola-term compatible client using the agent handler
    let direct_client = gola_agent_client::AgentClientFactory::create_direct_client(
        std::sync::Arc::new(agent_handler.clone()),
    );
    let gola_term_client = Box::new(crate::direct_gola_term_client::DirectGolaTermClient::new(
        direct_client,
    ));

    // Create channels for communication (same as gola-term main.rs)
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();
    let (event_tx, event_rx) = mpsc::unbounded_channel::<Event>();

    let mut background_futures = task::JoinSet::new();

    // Start actions service (same as gola-term main.rs)
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

    // Create another instance of our direct client for the UI layer using the same agent handler
    // This ensures no HTTP client is involved anywhere
    let ui_direct_client = gola_agent_client::AgentClientFactory::create_direct_client(
        std::sync::Arc::new(agent_handler.clone()),
    );
    let ui_client = Box::new(crate::direct_gola_term_client::DirectGolaTermClient::new(
        ui_direct_client,
    ));

    // Start the UI
    let ui_future = start_embedded_ui(ui_client, action_tx, event_rx);

    let result = tokio::select!(
        res = background_futures.join_next() => res.unwrap().unwrap(),
        res = ui_future => res,
    );

    if result.is_err() {
        destruct_terminal_for_panic();
    }

    result
}
