use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::errors::AgentError;
use crate::llm::ToolMetadata;
use crate::tools::Tool;

/// Reason for reporting progress
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressReason {
    AwaitingInput,
    PendingChoice,
    NeedClarification,
    ResponseComplete,
    ResultsDisplayed,
}

/// Parameters for the report_progress tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportProgressParams {
    /// Reason for reporting progress
    pub reason: ProgressReason,
    /// Optional context about the current state
    pub context: Option<String>,
}

/// Response from the report_progress tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportProgressResponse {
    /// Echo of the reason
    pub reason: ProgressReason,
    /// Echo of the context
    pub context: Option<String>,
    /// Flag indicating this is a progress report
    pub is_progress_report: bool,
}

/// Tool that allows the LLM to report progress to the user
pub struct ReportProgressTool;

impl ReportProgressTool {
    pub fn new() -> Self {
        Self
    }
    
    /// Create the progress response
    fn create_response(params: ReportProgressParams) -> ReportProgressResponse {
        ReportProgressResponse {
            reason: params.reason,
            context: params.context,
            is_progress_report: true,
        }
    }
}

impl Default for ReportProgressTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ReportProgressTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "report_progress".to_string(),
            description: "Report current progress or status to the user interface.".to_string(),
            input_schema: json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "reason": {
                        "type": "string",
                        "enum": ["awaiting_input", "pending_choice", "need_clarification", "response_complete", "results_displayed"],
                        "description": "Type of progress being reported"
                    },
                    "context": {
                        "type": "string",
                        "description": "Optional context about the current state"
                    }
                },
                "required": ["reason"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        // Parse the arguments
        let params: ReportProgressParams = serde_json::from_value(arguments)
            .map_err(|e| AgentError::ToolError {
                tool_name: "report_progress".to_string(),
                message: format!("Invalid parameters: {}", e),
            })?;
        
        // Log the progress event
        log::info!(
            "Progress reported: reason={:?}, context={:?}", 
            params.reason, 
            params.context
        );
        
        // Create the response
        let response = Self::create_response(params);
        
        // Return the response as JSON
        serde_json::to_string(&response).map_err(|e| AgentError::ToolError {
            tool_name: "report_progress".to_string(),
            message: format!("Failed to serialize response: {}", e),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_report_progress_awaiting_input() {
        let tool = ReportProgressTool::new();
        let args = json!({
            "reason": "awaiting_input",
            "context": "Need departure airport code"
        });
        
        let result = tool.execute(args).await.unwrap();
        let response: ReportProgressResponse = serde_json::from_str(&result).unwrap();
        
        assert!(matches!(response.reason, ProgressReason::AwaitingInput));
        assert_eq!(response.context, Some("Need departure airport code".to_string()));
        assert!(response.is_progress_report);
    }

    #[tokio::test]
    async fn test_report_progress_response_complete() {
        let tool = ReportProgressTool::new();
        let args = json!({
            "reason": "response_complete"
        });
        
        let result = tool.execute(args).await.unwrap();
        let response: ReportProgressResponse = serde_json::from_str(&result).unwrap();
        
        assert!(matches!(response.reason, ProgressReason::ResponseComplete));
        assert_eq!(response.context, None);
        assert!(response.is_progress_report);
    }

    #[tokio::test]
    async fn test_report_progress_metadata() {
        let tool = ReportProgressTool::new();
        let metadata = tool.metadata();
        
        assert_eq!(metadata.name, "report_progress");
        assert!(metadata.description.contains("progress"));
        assert!(metadata.input_schema.is_object());
    }
}