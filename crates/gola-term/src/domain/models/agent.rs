use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::{Display, EnumIter, EnumString, EnumVariantNames};

#[derive(
    Debug,
    Default,
    Clone,
    PartialEq,
    Eq,
    Display,
    EnumString,
    EnumVariantNames,
    Serialize,
    Deserialize,
    EnumIter,
)]
#[strum(serialize_all = "kebab-case")]
pub enum AgentName {
    #[default]
    GolaAgUI,
}

impl AgentName {
    #[allow(dead_code)]
    pub fn parse(s: String) -> Option<AgentName> {
        AgentName::iter().find(|e| e.to_string() == s)
    }
}

#[derive(Debug, Default, Clone)]
pub struct AgentPrompt {
    pub text: String,
    pub agent_context: String,
    pub editor_context: String,
}

impl AgentPrompt {
    pub fn new(text: String, agent_context: String) -> Self {
        Self {
            text,
            agent_context,
            ..Default::default()
        }
    }

    pub fn append_chat_context(&mut self, editor_context: &Option<super::EditorContext>) {
        if let Some(ctx) = editor_context {
            self.editor_context = ctx.format();
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct AgentResponse {
    pub author: super::Author,
    pub text: String,
    pub done: bool,
    pub context: Option<String>,
}
