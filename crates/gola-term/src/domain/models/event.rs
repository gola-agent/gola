use super::AgentResponse;
use super::Message;
use tui_textarea::Input;

#[derive(Debug)]
pub enum Event {
    AgentMessage(Message),
    AgentPromptResponse(AgentResponse),
    KeyboardCharInput(Input),
    KeyboardCTRLC,
    KeyboardCTRLO,
    KeyboardCTRLR,
    KeyboardEnter,
    KeyboardPaste(String),
    UITick,
    UIScrollDown,
    UIScrollUp,
    UIScrollPageDown,
    UIScrollPageUp,
}
