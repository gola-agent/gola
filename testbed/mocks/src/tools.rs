//! Mock Tool implementations for testing

use async_trait::async_trait;
use gola_core::tools::Tool;
use serde_json::{json, Value};
use gola_core::errors::AgentError;

/// Mock tool for testing
pub struct MockTool {
    name: String,
    description: String,
    response: Value,
    should_fail: bool,
}

impl MockTool {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Mock {} tool", name),
            response: json!({"result": "success"}),
            should_fail: false,
        }
    }

    pub fn with_response(name: &str, response: Value) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Mock {} tool", name),
            response,
            should_fail: false,
        }
    }

    pub fn with_error(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: format!("Mock {} tool", name),
            response: json!({}),
            should_fail: true,
        }
    }
}

#[async_trait]
impl Tool for MockTool {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn description(&self) -> String {
        self.description.clone()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Test input"
                }
            }
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, AgentError> {
        if self.should_fail {
            return Err(AgentError::ToolError(format!("Mock tool {} failed", self.name)));
        }
        Ok(self.response.clone())
    }
}

/// Mock calculator tool for testing
pub struct MockCalculatorTool;

impl MockCalculatorTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MockCalculatorTool {
    fn name(&self) -> String {
        "calculator".to_string()
    }

    fn description(&self) -> String {
        "Performs basic arithmetic operations".to_string()
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["add", "subtract", "multiply", "divide"]
                },
                "a": {"type": "number"},
                "b": {"type": "number"}
            },
            "required": ["operation", "a", "b"]
        })
    }

    async fn execute(&self, input: Value) -> Result<Value, AgentError> {
        let operation = input["operation"].as_str().unwrap_or("add");
        let a = input["a"].as_f64().unwrap_or(0.0);
        let b = input["b"].as_f64().unwrap_or(0.0);

        let result = match operation {
            "add" => a + b,
            "subtract" => a - b,
            "multiply" => a * b,
            "divide" => {
                if b == 0.0 {
                    return Err(AgentError::ToolError("Division by zero".to_string()));
                }
                a / b
            }
            _ => return Err(AgentError::ToolError("Unknown operation".to_string())),
        };

        Ok(json!({"result": result}))
    }
}