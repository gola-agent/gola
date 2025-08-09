use super::AcceptType;
use super::AgentPrompt;
use super::EditorContext;
use super::Message;

#[derive(Debug, Clone)]
pub enum Action {
    AcceptCodeBlock(Option<EditorContext>, String, AcceptType),
    CopyMessages(Vec<Message>),
    AgentAbort(),
    AgentRequest(AgentPrompt),
    AgentClearMemory,
}
