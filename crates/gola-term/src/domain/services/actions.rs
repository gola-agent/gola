use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::clipboard::ClipboardService;
use crate::domain::models::AcceptType;
use crate::domain::models::Action;
use crate::domain::models::AgentClientBox;
use crate::domain::models::AgentPrompt;
use crate::domain::models::Author;
use crate::domain::models::EditorContext;
use crate::domain::models::EditorName;
use crate::domain::models::Event;
use crate::domain::models::Message;
use crate::domain::models::MessageType;
use crate::domain::models::SlashCommand;
use crate::infrastructure::editors::EditorManager;

pub fn help_text() -> String {
    let text = r#"
COMMANDS:
- /append (/a) [CODE_BLOCK_NUMBER?] - Appends code blocks to an editor. See Code Actions for more details.
- /replace (/r) [CODE_BLOCK_NUMBER?] - Replaces selections with code blocks in an editor. See Code Actions for more details.
- /copy (/c) [CODE_BLOCK_NUMBER?] - Copies the entire chat history to your clipboard. When a `CODE_BLOCK_NUMBER` is used, only the specified copy blocks are copied to clipboard. See Code Actions for more details.
- /clear - Clears the current session's memory.
- /quit /exit (/q) - Exit Gola.
- /help (/h) - Provides this help menu.
- /about - Displays information about gola-term.

HOTKEYS:
- Up arrow - Scroll up.
- Down arrow - Scroll down.
- CTRL+U - Page up.
- CTRL+D - Page down.
- CTRL+C - Interrupt waiting for prompt response if in progress, otherwise exit.
- CTRL+O - Insert a line break at the cursor position.
- CTRL+R - Resubmit your last message to the backend.

CODE ACTIONS:
When working with models that provide code, and using an editor integration, Gola has the capabilities to read selected code from an editor, and submit model provided code back in to an editor. Each code block provided by a model is indexed with a (NUMBER) at the beginning of the block to make it easily identifiable.

- /append (/a) [CODE_BLOCK_NUMBER?] will append one-to-many model provided code blocks to the open file in your editor.
- /replace (/r) [CODE_BLOCK_NUMBER?] - will replace selected code in your editor with one-to-many model provided code blocks.
- /copy (/c) [CODE_BLOCK_NUMBER?] - Copies the entire chat history to your clipboard. When a `CODE_BLOCK_NUMBER` is used it will append one-to-many model provided code blocks to your clipboard, no matter the editor integration.

The `CODE_BLOCK_NUMBER` allows you to select several code blocks to send back to your editor at once. The parameter can be set as follows:
- `1` - Selects the first code block
- `1,3,5` - Selects code blocks 1, 3, and 5.
- `2..5`- Selects an inclusive range of code blocks between 2 and 5.
- None - Selects the last provided code block.
        "#;

    text.trim().to_string()
}

pub fn about_text() -> String {
    let text = r#"
gola-term is a fork of the oatmeal project (https://github.com/dustinblackman/oatmeal), refactored to serve as a terminal client for autonomous agents.

Oatmeal Author: Dustin Blackman
License: MIT
"#;
    text.trim().to_string()
}

async fn accept_codeblock(
    context: Option<EditorContext>,
    codeblock: String,
    accept_type: AcceptType,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> Result<()> {
    let editor_name = EditorName::default();
    let editor = EditorManager::get(editor_name.clone())?;
    let mut context_mut = context;

    if editor_name == EditorName::Clipboard || editor_name == EditorName::None {
        context_mut = Some(EditorContext::default());
    }

    if let Some(editor_context) = context_mut {
        let res = editor
            .send_codeblock(editor_context, codeblock, accept_type)
            .await;

        if let Err(err) = res {
            event_tx.send(Event::AgentMessage(Message::new_with_type(
                Author::Gola,
                MessageType::Error,
                &format!("Failed to commuicate with editor:\n\n{err}"),
            )))?;
        }
    }

    if editor_name == EditorName::Clipboard {
        event_tx.send(Event::AgentMessage(Message::new(
            Author::Gola,
            "Copied codeblocks to clipboard.",
        )))?;
    }

    Ok(())
}

fn copy_messages(messages: Vec<Message>, event_tx: &mpsc::UnboundedSender<Event>) -> Result<()> {
    let mut payload = messages[0].text.to_string();
    if messages.len() > 1 {
        payload = messages
            .iter()
            .map(|message| format!("{}: {}", message.author, message.text))
            .collect::<Vec<String>>()
            .join("\n\n");
    }

    if let Err(err) = ClipboardService::set(payload) {
        event_tx.send(Event::AgentMessage(Message::new_with_type(
            Author::Gola,
            MessageType::Error,
            &format!("Failed to copy to clipboard:\n\n{err}"),
        )))?;

        return Ok(());
    }
    event_tx.send(Event::AgentMessage(Message::new(
        Author::Gola,
        "Copied chat log to clipboard.",
    )))?;

    Ok(())
}

fn worker_error(err: anyhow::Error, event_tx: &mpsc::UnboundedSender<Event>) -> Result<()> {
    event_tx.send(Event::AgentMessage(Message::new_with_type(
        Author::Gola,
        MessageType::Error,
        &format!(
            "The agent client failed with the following error: {:?}",
            err
        ),
    )))?;

    Ok(())
}

async fn send_prompt_to_agent(
    agent_client: &AgentClientBox,
    prompt: AgentPrompt,
    event_tx: &mpsc::UnboundedSender<Event>,
) -> Result<()> {
    agent_client.send_prompt(prompt, event_tx).await
}

fn help(event_tx: &mpsc::UnboundedSender<Event>) -> Result<()> {
    event_tx.send(Event::AgentMessage(Message::new(
        Author::Gola,
        &help_text(),
    )))?;

    Ok(())
}

fn about(event_tx: &mpsc::UnboundedSender<Event>) -> Result<()> {
    event_tx.send(Event::AgentMessage(Message::new(
        Author::Gola,
        &about_text(),
    )))?;

    Ok(())
}

pub struct ActionsService {}

const GOLA_CONNECT_MESSAGE: &str = "gola-connect-HACK";

impl ActionsService {
    pub async fn start(
        agent_client: AgentClientBox,
        action_tx: mpsc::UnboundedSender<Action>,
        event_tx: mpsc::UnboundedSender<Event>,
        rx: &mut mpsc::UnboundedReceiver<Action>,
    ) -> Result<()> {
        let agent_client_arc = Arc::new(agent_client);

        #[allow(unused_assignments)]
        let mut worker: JoinHandle<Result<()>> = tokio::spawn(async { Ok(()) });

        let connect_prompt = AgentPrompt {
            text: GOLA_CONNECT_MESSAGE.to_string(),
            ..Default::default()
        };

        let client_worker = agent_client_arc.clone();
        let worker_event_tx = event_tx.clone();
        worker = tokio::spawn(async move {
            if let Err(err) =
                send_prompt_to_agent(&client_worker, connect_prompt, &worker_event_tx).await
            {
                worker_error(err, &worker_event_tx)?;
            }
            Ok(())
        });

        loop {
            if let Some(event) = rx.recv().await {
                let worker_event_tx = event_tx.clone();
                match event {
                    Action::AcceptCodeBlock(context, codeblock, accept_type) => {
                        accept_codeblock(context, codeblock, accept_type, &event_tx).await?;
                    }
                    Action::CopyMessages(messages) => {
                        copy_messages(messages, &event_tx)?;
                    }
                    Action::AgentAbort() => {
                        worker.abort();
                    }
                    Action::AgentClearMemory => {
                        agent_client_arc.clear_memory().await?;
                        event_tx.send(Event::AgentMessage(Message::new(
                            Author::Gola,
                            "Memory cleared.",
                        )))?;
                    }
                    Action::AgentRequest(prompt) => {
                        if let Some(command) = SlashCommand::parse(&prompt.text) {
                            if command.is_help() {
                                help(&event_tx)?;
                                continue;
                            }
                            if command.is_about() {
                                about(&event_tx)?;
                                continue;
                            }
                            if command.is_clear() {
                                action_tx.send(Action::AgentClearMemory)?;
                                continue;
                            }
                        }

                        let client_worker = agent_client_arc.clone();
                        worker = tokio::spawn(async move {
                            if let Err(err) =
                                send_prompt_to_agent(&client_worker, prompt, &worker_event_tx).await
                            {
                                worker_error(err, &worker_event_tx)?;
                            }
                            Ok(())
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{AgentClient, AgentName, AgentPrompt};
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    struct MockAgentClient {
        prompt_fn: Box<dyn Fn(AgentPrompt) -> Result<()> + Send + Sync>,
    }

    #[async_trait]
    impl AgentClient for MockAgentClient {
        fn name(&self) -> AgentName {
            AgentName::GolaAgUI
        }

        async fn health_check(&self) -> Result<()> {
            Ok(())
        }

        async fn send_prompt(
            &self,
            prompt: AgentPrompt,
            _tx: &mpsc::UnboundedSender<Event>,
        ) -> Result<()> {
            (self.prompt_fn)(prompt)
        }

        async fn clear_memory(&self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_start_sends_connect_message() {
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

        let (completion_tx, mut completion_rx) = mpsc::unbounded_channel::<String>();

        let agent_client = MockAgentClient {
            prompt_fn: Box::new(move |prompt| {
                completion_tx.send(prompt.text).unwrap();
                Ok(())
            }),
        };

        tokio::spawn(async move {
            ActionsService::start(Box::new(agent_client), action_tx, event_tx, &mut action_rx)
                .await
                .unwrap();
        });

        let received_prompt = completion_rx.recv().await.unwrap();
        assert_eq!(received_prompt, GOLA_CONNECT_MESSAGE);
    }
}
