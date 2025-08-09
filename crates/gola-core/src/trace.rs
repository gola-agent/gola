// src/trace.rs

use serde::{Deserialize, Serialize};
use crate::core_types::{ToolCall, Observation};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStep {
    pub step_number: usize,
    pub thought: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_results: Option<Vec<Observation>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExecution {
    pub steps: Vec<AgentStep>,
    pub final_result: Option<String>,
    pub error: Option<String>,
}

use tokio::task::JoinHandle;
/// A trait for handling agent execution traces.
/// This allows for decoupled monitoring, logging, or other features
/// without modifying the core agent logic.
pub trait AgentTraceHandler: Send + Sync {
    /// Called after each step of the agent's execution.
    fn on_step_complete(&mut self, step: &AgentStep) -> Option<JoinHandle<()>>;

    /// Called when the agent's execution is complete.
    fn on_execution_complete(&mut self, execution: &AgentExecution);
}