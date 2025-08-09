use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::errors::AgentError;
use crate::llm::ToolMetadata;
use crate::tools::Tool;

/// Completion status categories
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionStatus {
    Success,
    PartialSuccess,
    Error,
    UserAbort,
}

/// Parameters for the assistant_done tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantDoneParams {
    /// Human-readable summary of what was accomplished or why stopping
    pub summary: String,
    /// Reference to final output (session_id, file path, etc.)
    pub final_artifact_id: Option<String>,
    /// Performance metrics for this session
    pub metrics: Option<HashMap<String, f64>>,
    /// Completion status category
    pub status: CompletionStatus,
}

/// Response from the assistant_done tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantDoneResponse {
    /// Echo of the completion status
    pub status: CompletionStatus,
    /// Echo of the summary
    pub summary: String,
    /// Final artifact reference if provided
    pub final_artifact_id: Option<String>,
    /// Metrics if provided
    pub metrics: Option<HashMap<String, f64>>,
    /// Timestamp of completion
    pub completed_at: String,
    /// Flag indicating this is a completion signal
    pub is_completion: bool,
}

/// Tool that allows the LLM to signal completion of the current task
pub struct AssistantDoneTool;

impl AssistantDoneTool {
    pub fn new() -> Self {
        Self
    }
    
    /// Validate the parameters
    fn validate_params(params: &AssistantDoneParams) -> Result<(), AgentError> {
        if params.summary.trim().is_empty() {
            return Err(AgentError::ToolError {
                tool_name: "assistant_done".to_string(),
                message: "Summary cannot be empty".to_string(),
            });
        }
        
        // Summary should be reasonable length (not too short or too long)
        let summary_len = params.summary.len();
        if summary_len < 10 {
            return Err(AgentError::ToolError {
                tool_name: "assistant_done".to_string(),
                message: "Summary too short (minimum 10 characters)".to_string(),
            });
        }
        
        if summary_len > 1000 {
            return Err(AgentError::ToolError {
                tool_name: "assistant_done".to_string(),
                message: "Summary too long (maximum 1000 characters)".to_string(),
            });
        }
        
        Ok(())
    }
    
    /// Create the completion response
    fn create_response(params: AssistantDoneParams) -> AssistantDoneResponse {
        AssistantDoneResponse {
            status: params.status,
            summary: params.summary,
            final_artifact_id: params.final_artifact_id,
            metrics: params.metrics,
            completed_at: chrono::Utc::now().to_rfc3339(),
            is_completion: true,
        }
    }
}

impl Default for AssistantDoneTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for AssistantDoneTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "assistant_done".to_string(),
            description: "Mark the current task or conversation as complete.".to_string(),
            input_schema: json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "Human-readable summary of what was accomplished or why stopping (10-1000 characters)"
                    },
                    "final_artifact_id": {
                        "type": "string",
                        "description": "Reference to final output (session_id, file path, etc.)"
                    },
                    "metrics": {
                        "type": "object",
                        "description": "Performance metrics for this session",
                        "additionalProperties": {
                            "type": "number"
                        }
                    },
                    "status": {
                        "type": "string",
                        "enum": ["success", "partial_success", "error", "user_abort"],
                        "description": "Completion status category"
                    }
                },
                "required": ["summary", "status"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        // Parse the arguments
        let params: AssistantDoneParams = serde_json::from_value(arguments)
            .map_err(|e| AgentError::ToolError {
                tool_name: "assistant_done".to_string(),
                message: format!("Invalid parameters: {}", e),
            })?;
        
        // Validate parameters
        Self::validate_params(&params)?;
        
        // Log the completion event
        log::info!(
            "Assistant completion signaled: status={:?}, summary='{}'", 
            params.status, 
            params.summary
        );
        
        if let Some(ref artifact_id) = params.final_artifact_id {
            log::info!("Final artifact: {}", artifact_id);
        }
        
        if let Some(ref metrics) = params.metrics {
            log::info!("Completion metrics: {:?}", metrics);
        }
        
        // Create the response
        let response = Self::create_response(params);
        
        // Return the response as JSON
        serde_json::to_string(&response).map_err(|e| AgentError::ToolError {
            tool_name: "assistant_done".to_string(),
            message: format!("Failed to serialize response: {}", e),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_assistant_done_success() {
        let tool = AssistantDoneTool::new();
        let args = json!({
            "summary": "Successfully completed travel itinerary planning",
            "status": "success",
            "final_artifact_id": "session_123",
            "metrics": {
                "api_calls": 15.0,
                "duration_seconds": 45.0
            }
        });
        
        let result = tool.execute(args).await.unwrap();
        let response: AssistantDoneResponse = serde_json::from_str(&result).unwrap();
        
        assert!(matches!(response.status, CompletionStatus::Success));
        assert_eq!(response.summary, "Successfully completed travel itinerary planning");
        assert_eq!(response.final_artifact_id, Some("session_123".to_string()));
        assert!(response.is_completion);
        assert!(response.metrics.is_some());
    }

    #[tokio::test]
    async fn test_assistant_done_error() {
        let tool = AssistantDoneTool::new();
        let args = json!({
            "summary": "Unable to complete due to flight API authentication failure",
            "status": "error"
        });
        
        let result = tool.execute(args).await.unwrap();
        let response: AssistantDoneResponse = serde_json::from_str(&result).unwrap();
        
        assert!(matches!(response.status, CompletionStatus::Error));
        assert_eq!(response.summary, "Unable to complete due to flight API authentication failure");
        assert_eq!(response.final_artifact_id, None);
        assert!(response.is_completion);
    }

    #[tokio::test]
    async fn test_assistant_done_validation_empty_summary() {
        let tool = AssistantDoneTool::new();
        let args = json!({
            "summary": "",
            "status": "success"
        });
        
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Summary cannot be empty"));
    }

    #[tokio::test]
    async fn test_assistant_done_validation_short_summary() {
        let tool = AssistantDoneTool::new();
        let args = json!({
            "summary": "Short",
            "status": "success"
        });
        
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Summary too short"));
    }

    #[tokio::test]
    async fn test_assistant_done_metadata() {
        let tool = AssistantDoneTool::new();
        let metadata = tool.metadata();
        
        assert_eq!(metadata.name, "assistant_done");
        assert!(metadata.description.contains("Mark the current task or conversation as complete"));
        assert!(metadata.input_schema.is_object());
    }
}