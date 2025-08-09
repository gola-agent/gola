//! Control plane tools for agent orchestration and lifecycle management
//!
//! This module provides meta-level tools that enable agents to manage their own
//! execution lifecycle and communicate progress to external systems. The control
//! plane abstraction is essential for building reliable, observable agent systems
//! that can be monitored, interrupted, and coordinated. This design enables
//! enterprise-grade deployments where agent behavior must be predictable and auditable.

pub mod assistant_done;
pub mod report_progress;
pub mod server;

pub use assistant_done::AssistantDoneTool;
pub use report_progress::ReportProgressTool;
pub use server::ControlPlaneServer;

use std::sync::Arc;
use crate::tools::{Tool, ToolRegistry};

/// Factory for creating control plane tools
pub struct ControlPlaneFactory;

impl ControlPlaneFactory {
    /// Create the assistant_done tool
    pub fn create_assistant_done() -> Arc<dyn Tool> {
        Arc::new(AssistantDoneTool::new())
    }
    
    /// Create the report_progress tool
    pub fn create_report_progress() -> Arc<dyn Tool> {
        Arc::new(ReportProgressTool::new())
    }
    
    /// Create a tool registry containing all control plane tools
    pub fn create_registry() -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        registry.register_tool(Self::create_assistant_done());
        registry.register_tool(Self::create_report_progress());
        registry
    }
    
    /// Add control plane tools to an existing registry
    pub fn register_tools(registry: &mut ToolRegistry) {
        registry.register_tool(Self::create_assistant_done());
        registry.register_tool(Self::create_report_progress());
    }
}