use serde::Deserialize;
use serde::Serialize;

use super::Message;

#[derive(Serialize, Deserialize)]
pub struct State {
    pub agent_name: String,
    pub agent_context: String,
    pub editor_language: String,
    pub messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub version: String,
    pub timestamp: String,
    pub state: State,
}
