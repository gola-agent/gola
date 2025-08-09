//! Schema validation utilities for agent input and output
//! 
//! This module provides JSON schema validation functionality for agent
//! requests and responses, ensuring data integrity and format compliance.

use crate::config::types::{InputSchemaConfig, OutputSchemaConfig, SchemaValidationConfig};
use crate::errors::AgentError;
use jsonschema::{JSONSchema, ValidationError};
use serde_json::Value;
use std::collections::HashMap;

/// Schema validator for agent input and output
#[derive(Debug)]
pub struct SchemaValidator {
    input_schema: Option<JSONSchema>,
    output_schema: Option<JSONSchema>,
    config: SchemaValidationConfig,
}

impl SchemaValidator {
    /// Create a new schema validator
    pub fn new(
        input_config: Option<&InputSchemaConfig>,
        output_config: Option<&OutputSchemaConfig>,
        validation_config: SchemaValidationConfig,
    ) -> Result<Self, AgentError> {
        let input_schema = if let Some(config) = input_config {
            Some(
                JSONSchema::compile(&config.schema)
                    .map_err(|e| AgentError::ConfigError(format!("Invalid input schema: {}", e)))?,
            )
        } else {
            None
        };

        let output_schema = if let Some(config) = output_config {
            Some(
                JSONSchema::compile(&config.schema)
                    .map_err(|e| AgentError::ConfigError(format!("Invalid output schema: {}", e)))?,
            )
        } else {
            None
        };

        Ok(Self {
            input_schema,
            output_schema,
            config: validation_config,
        })
    }

    /// Validate input data against the input schema
    pub fn validate_input(&self, data: &Value) -> Result<(), AgentError> {
        if let Some(schema) = &self.input_schema {
            let validation_result = schema.validate(data);
            
            if let Err(errors) = validation_result {
                let error_messages = self.format_validation_errors(errors);
                let error_msg = format!("Input validation failed: {}", error_messages.join("; "));
                
                if self.config.log_errors {
                    log::error!("{}", error_msg);
                    if self.config.include_schema_in_errors {
                        log::debug!("Input schema validation failed");
                        log::debug!("Invalid input data: {}", serde_json::to_string_pretty(data).unwrap_or_default());
                    }
                }
                
                return Err(AgentError::ConfigError(error_msg));
            }
        }
        
        Ok(())
    }

    /// Validate output data against the output schema
    pub fn validate_output(&self, data: &Value) -> Result<(), AgentError> {
        if let Some(schema) = &self.output_schema {
            let validation_result = schema.validate(data);
            
            if let Err(errors) = validation_result {
                let error_messages = self.format_validation_errors(errors);
                let error_msg = format!("Output validation failed: {}", error_messages.join("; "));
                
                if self.config.log_errors {
                    log::error!("{}", error_msg);
                    if self.config.include_schema_in_errors {
                        log::debug!("Output schema validation failed");
                        log::debug!("Invalid output data: {}", serde_json::to_string_pretty(data).unwrap_or_default());
                    }
                }
                
                return Err(AgentError::ConfigError(error_msg));
            }
        }
        
        Ok(())
    }

    /// Validate output with retry attempts and optional auto-correction
    pub fn validate_output_with_retry(
        &self,
        data: &Value,
        auto_correct: bool,
    ) -> Result<Value, AgentError> {
        let mut current_data = data.clone();
        let mut attempts = 0;
        
        while attempts < self.config.max_validation_attempts {
            match self.validate_output(&current_data) {
                Ok(()) => return Ok(current_data),
                Err(e) => {
                    attempts += 1;
                    
                    if attempts >= self.config.max_validation_attempts {
                        return Err(e);
                    }
                    
                    if auto_correct {
                        // Attempt basic auto-correction
                        if let Ok(corrected) = self.attempt_auto_correction(&current_data) {
                            current_data = corrected;
                            continue;
                        }
                    }
                    
                    // If no auto-correction or it failed, return the error
                    return Err(e);
                }
            }
        }
        
        Err(AgentError::ConfigError(
            "Maximum validation attempts exceeded".to_string(),
        ))
    }

    /// Check if input validation is enabled
    pub fn has_input_schema(&self) -> bool {
        self.input_schema.is_some()
    }

    /// Check if output validation is enabled
    pub fn has_output_schema(&self) -> bool {
        self.output_schema.is_some()
    }

    /// Get validation configuration
    pub fn config(&self) -> &SchemaValidationConfig {
        &self.config
    }

    /// Format validation errors into human-readable messages
    fn format_validation_errors<'a>(&self, errors: impl Iterator<Item = ValidationError<'a>>) -> Vec<String> {
        errors
            .map(|error| {
                let path = if error.instance_path.to_string().is_empty() {
                    "root".to_string()
                } else {
                    error.instance_path.to_string()
                };
                format!("At '{}': {}", path, error)
            })
            .collect()
    }

    /// Attempt basic auto-correction of validation errors
    fn attempt_auto_correction(&self, data: &Value) -> Result<Value, AgentError> {
        let mut corrected = data.clone();
        
        // Basic auto-correction strategies
        if let Some(obj) = corrected.as_object_mut() {
            // Remove null values that might be causing issues
            obj.retain(|_, v| !v.is_null());
            
            // Convert string numbers to actual numbers if schema expects numbers
            for (_key, value) in obj.iter_mut() {
                if let Value::String(s) = value {
                    if let Ok(num) = s.parse::<f64>() {
                        if let Some(number) = serde_json::Number::from_f64(num) { *value = Value::Number(number); }
                    } else if let Ok(int_val) = s.parse::<i64>() {
                        *value = Value::Number(serde_json::Number::from(int_val));
                    } else if s.eq_ignore_ascii_case("true") {
                        *value = Value::Bool(true);
                    } else if s.eq_ignore_ascii_case("false") {
                        *value = Value::Bool(false);
                    }
                }
            }
        }
        
        Ok(corrected)
    }
}

/// Utility functions for schema validation
pub struct SchemaUtils;

impl SchemaUtils {
    /// Create a basic JSON schema for common data types
    pub fn create_basic_schema(schema_type: &str) -> Value {
        match schema_type {
            "string" => serde_json::json!({
                "type": "string"
            }),
            "number" => serde_json::json!({
                "type": "number"
            }),
            "integer" => serde_json::json!({
                "type": "integer"
            }),
            "boolean" => serde_json::json!({
                "type": "boolean"
            }),
            "array" => serde_json::json!({
                "type": "array",
                "items": {}
            }),
            "object" => serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": true
            }),
            _ => serde_json::json!({
                "type": "object",
                "additionalProperties": true
            }),
        }
    }

    /// Validate that a JSON value is a valid JSON schema
    pub fn validate_schema(schema: &Value) -> Result<(), AgentError> {
        JSONSchema::compile(schema)
            .map_err(|e| AgentError::ConfigError(format!("Invalid JSON schema: {}", e)))?;
        Ok(())
    }

    /// Extract schema information for debugging
    pub fn extract_schema_info(schema: &Value) -> HashMap<String, String> {
        let mut info = HashMap::new();
        
        if let Some(obj) = schema.as_object() {
            if let Some(title) = obj.get("title").and_then(|v| v.as_str()) {
                info.insert("title".to_string(), title.to_string());
            }
            
            if let Some(description) = obj.get("description").and_then(|v| v.as_str()) {
                info.insert("description".to_string(), description.to_string());
            }
            
            if let Some(schema_type) = obj.get("type").and_then(|v| v.as_str()) {
                info.insert("type".to_string(), schema_type.to_string());
            }
            
            if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
                info.insert("property_count".to_string(), properties.len().to_string());
                info.insert("properties".to_string(), properties.keys().cloned().collect::<Vec<_>>().join(", "));
            }
            
            if let Some(required) = obj.get("required").and_then(|v| v.as_array()) {
                let required_fields: Vec<String> = required
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect();
                info.insert("required_fields".to_string(), required_fields.join(", "));
            }
        }
        
        info
    }

    /// Generate a sample data structure that would validate against the schema
    pub fn generate_sample_data(schema: &Value) -> Result<Value, AgentError> {
        if let Some(obj) = schema.as_object() {
            match obj.get("type").and_then(|v| v.as_str()) {
                Some("object") => {
                    let mut sample = serde_json::Map::new();
                    
                    if let Some(properties) = obj.get("properties").and_then(|v| v.as_object()) {
                        for (key, prop_schema) in properties {
                            sample.insert(key.clone(), Self::generate_sample_data(prop_schema)?);
                        }
                    }
                    
                    Ok(Value::Object(sample))
                }
                Some("array") => {
                    if let Some(items_schema) = obj.get("items") {
                        Ok(Value::Array(vec![Self::generate_sample_data(items_schema)?]))
                    } else {
                        Ok(Value::Array(vec![]))
                    }
                }
                Some("string") => Ok(Value::String("sample_string".to_string())),
                Some("number") => Ok(Value::Number(serde_json::Number::from_f64(42.0).unwrap())),
                Some("integer") => Ok(Value::Number(serde_json::Number::from(42))),
                Some("boolean") => Ok(Value::Bool(true)),
                Some("null") => Ok(Value::Null),
                _ => Ok(Value::Object(serde_json::Map::new())),
            }
        } else {
            Ok(Value::Object(serde_json::Map::new()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_schema_validator_creation() {
        let input_config = InputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                },
                "required": ["name"]
            }),
            strict: true,
            error_message: None,
            source: crate::config::types::SchemaSource::default(),
        };

        let validation_config = SchemaValidationConfig::default();
        
        let validator = SchemaValidator::new(
            Some(&input_config),
            None,
            validation_config,
        );
        
        assert!(validator.is_ok());
        let validator = validator.unwrap();
        assert!(validator.has_input_schema());
        assert!(!validator.has_output_schema());
    }

    #[test]
    fn test_input_validation_success() {
        let input_config = InputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                },
                "required": ["name"]
            }),
            strict: true,
            error_message: None,
            source: crate::config::types::SchemaSource::default(),
        };

        let validator = SchemaValidator::new(
            Some(&input_config),
            None,
            SchemaValidationConfig::default(),
        ).unwrap();

        let valid_data = json!({"name": "test"});
        assert!(validator.validate_input(&valid_data).is_ok());
    }

    #[test]
    fn test_input_validation_failure() {
        let input_config = InputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"}
                },
                "required": ["name"]
            }),
            strict: true,
            error_message: None,
            source: crate::config::types::SchemaSource::default(),
        };

        let validator = SchemaValidator::new(
            Some(&input_config),
            None,
            SchemaValidationConfig::default(),
        ).unwrap();

        let invalid_data = json!({"age": 25}); // Missing required "name" field
        assert!(validator.validate_input(&invalid_data).is_err());
    }

    #[test]
    fn test_schema_utils_create_basic_schema() {
        let string_schema = SchemaUtils::create_basic_schema("string");
        assert_eq!(string_schema["type"], "string");

        let object_schema = SchemaUtils::create_basic_schema("object");
        assert_eq!(object_schema["type"], "object");
        assert!(object_schema["properties"].is_object());
    }

    #[test]
    fn test_schema_utils_validate_schema() {
        let valid_schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"}
            }
        });
        
        assert!(SchemaUtils::validate_schema(&valid_schema).is_ok());

        let invalid_schema = json!({
            "type": "invalid_type"
        });
        
        assert!(SchemaUtils::validate_schema(&invalid_schema).is_err());
    }

    #[test]
    fn test_generate_sample_data() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"},
                "active": {"type": "boolean"}
            }
        });

        let sample = SchemaUtils::generate_sample_data(&schema).unwrap();
        assert!(sample.is_object());
        
        let obj = sample.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("age"));
        assert!(obj.contains_key("active"));
        assert!(obj["name"].is_string());
        assert!(obj["age"].is_number());
        assert!(obj["active"].is_boolean());
    }

    #[test]
    fn test_auto_correction() {
        let output_config = OutputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "count": {"type": "number"}
                },
                "required": ["count"]
            }),
            strict: false,
            error_message: None,
            source: crate::config::types::SchemaSource::default(),
            auto_correct: true,
        };

        let validator = SchemaValidator::new(
            None,
            Some(&output_config),
            SchemaValidationConfig::default(),
        ).unwrap();

        // Data with string number that should be auto-corrected
        let data_with_string_number = json!({"count": "42"});
        let result = validator.validate_output_with_retry(&data_with_string_number, true);
        
        assert!(result.is_ok());
        let corrected = result.unwrap();
        assert!(corrected["count"].is_number());
    }
}
