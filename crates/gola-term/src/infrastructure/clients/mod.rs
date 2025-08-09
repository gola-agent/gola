pub mod gola_ag_ui;

use anyhow::bail;
use anyhow::Result;

use crate::domain::models::AgentClientBox;
use crate::domain::models::AgentName;

pub struct AgentClientManager {}

impl AgentClientManager {
    pub fn get(name: AgentName) -> Result<AgentClientBox> {
        if name == AgentName::GolaAgUI {
            return Ok(Box::<gola_ag_ui::GolaAgUI>::default());
        }

        bail!(format!("No backend implemented for {name}"))
    }
}
