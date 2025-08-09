//! Code execution environments for sandboxed runtime evaluation.
//!
//! Provides secure execution contexts for running untrusted code through
//! containerized environments (Docker) or isolated native runtimes. Supports
//! multiple programming languages with resource constraints and timeout management.

use async_trait::async_trait;
use crate::errors::DockerExecutorError; 

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait CodeExecutor: Send + Sync {
    async fn execute_code(
        &self,
        language: &str,
        code: &str,
    ) -> Result<ExecutionResult, DockerExecutorError>;
}

pub mod docker;
pub mod installer;
pub mod runtime_manager;

#[cfg(test)]
mod runtime_bootstrapping_test;
