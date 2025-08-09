//! Mathematical computation tool for safe expression evaluation
//!
//! This module provides agents with numerical computation capabilities, enabling
//! data analysis, financial calculations, and scientific computations. The design
//! emphasizes safety by sandboxing expression evaluation to prevent code injection
//! while maintaining flexibility for complex mathematical operations. This tool
//! serves as a foundation for quantitative reasoning tasks.

use crate::errors::AgentError;
use crate::llm::ToolMetadata;
use crate::tools::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct CalculatorTool;

impl CalculatorTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CalculatorTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CalculatorTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "calculator".to_string(),
            description: "Performs basic arithmetic operations including addition, subtraction, multiplication, division, and exponentiation".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["add", "subtract", "multiply", "divide", "power", "sqrt"],
                        "description": "The arithmetic operation to perform"
                    },
                    "a": {
                        "type": "number",
                        "description": "The first number"
                    },
                    "b": {
                        "type": "number",
                        "description": "The second number (not required for sqrt operation)"
                    }
                },
                "required": ["operation", "a"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        let operation = arguments
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::ToolError {
                tool_name: "calculator".to_string(),
                message: "Missing or invalid 'operation' parameter".to_string(),
            })?;

        let a = arguments
            .get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| AgentError::ToolError {
                tool_name: "calculator".to_string(),
                message: "Missing or invalid parameter 'a'".to_string(),
            })?;

        let result = match operation {
            "add" => {
                let b = arguments
                    .get("b")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| AgentError::ToolError {
                        tool_name: "calculator".to_string(),
                        message: "Missing or invalid parameter 'b' for addition".to_string(),
                    })?;
                a + b
            }
            "subtract" => {
                let b = arguments
                    .get("b")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| AgentError::ToolError {
                        tool_name: "calculator".to_string(),
                        message: "Missing or invalid parameter 'b' for subtraction".to_string(),
                    })?;
                a - b
            }
            "multiply" => {
                let b = arguments
                    .get("b")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| AgentError::ToolError {
                        tool_name: "calculator".to_string(),
                        message: "Missing or invalid parameter 'b' for multiplication".to_string(),
                    })?;
                a * b
            }
            "divide" => {
                let b = arguments
                    .get("b")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| AgentError::ToolError {
                        tool_name: "calculator".to_string(),
                        message: "Missing or invalid parameter 'b' for division".to_string(),
                    })?;
                if b == 0.0 {
                    return Err(AgentError::ToolError {
                        tool_name: "calculator".to_string(),
                        message: "Division by zero is not allowed".to_string(),
                    });
                }
                a / b
            }
            "power" => {
                let b = arguments
                    .get("b")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| AgentError::ToolError {
                        tool_name: "calculator".to_string(),
                        message: "Missing or invalid parameter 'b' for exponentiation".to_string(),
                    })?;
                a.powf(b)
            }
            "sqrt" => {
                if a < 0.0 {
                    return Err(AgentError::ToolError {
                        tool_name: "calculator".to_string(),
                        message: "Cannot calculate square root of negative number".to_string(),
                    });
                }
                a.sqrt()
            }
            _ => {
                return Err(AgentError::ToolError {
                    tool_name: "calculator".to_string(),
                    message: format!("Unknown operation: {}", operation),
                });
            }
        };

        // Format the result nicely
        let formatted_result = if result.fract() == 0.0 {
            format!("{}", result as i64)
        } else {
            format!("{:.6}", result).trim_end_matches('0').trim_end_matches('.').to_string()
        };

        log::info!("Calculator: {} {} {} = {}", a, operation, 
                  arguments.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0), 
                  formatted_result);

        Ok(formatted_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_calculator_addition() {
        let calc = CalculatorTool::new();
        let args = json!({"operation": "add", "a": 5, "b": 3});
        let result = calc.execute(args).await.unwrap();
        assert_eq!(result, "8");
    }

    #[tokio::test]
    async fn test_calculator_division() {
        let calc = CalculatorTool::new();
        let args = json!({"operation": "divide", "a": 10, "b": 2});
        let result = calc.execute(args).await.unwrap();
        assert_eq!(result, "5");
    }

    #[tokio::test]
    async fn test_calculator_division_by_zero() {
        let calc = CalculatorTool::new();
        let args = json!({"operation": "divide", "a": 10, "b": 0});
        let result = calc.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_calculator_sqrt() {
        let calc = CalculatorTool::new();
        let args = json!({"operation": "sqrt", "a": 16});
        let result = calc.execute(args).await.unwrap();
        assert_eq!(result, "4");
    }

    #[tokio::test]
    async fn test_calculator_power() {
        let calc = CalculatorTool::new();
        let args = json!({"operation": "power", "a": 2, "b": 3});
        let result = calc.execute(args).await.unwrap();
        assert_eq!(result, "8");
    }

    #[tokio::test]
    async fn test_calculator_metadata() {
        let calc = CalculatorTool::new();
        let metadata = calc.metadata();
        assert_eq!(metadata.name, "calculator");
        assert!(metadata.description.contains("arithmetic"));
    }
}