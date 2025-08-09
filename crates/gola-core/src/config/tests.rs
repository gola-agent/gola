//! Integration tests for configuration with schema validation

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::builder::{ConfigBuilder, SchemaBuilder};
    use crate::config::SchemaValidator;
    use serde_json::json;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_config_with_inline_schemas() {
        let input_schema = SchemaBuilder::new()
            .title("Test Input Schema")
            .add_string_property("query", Some("User query"))
            .add_number_property("priority", Some("Priority level"))
            .add_required("query")
            .build();

        let output_schema = SchemaBuilder::new()
            .title("Test Output Schema")
            .add_string_property("response", Some("Agent response"))
            .add_number_property("confidence", Some("Confidence score"))
            .add_required("response")
            .build();

        let config = ConfigBuilder::new()
            .agent_name("test_schema_agent")
            .input_schema(input_schema)
            .output_schema(output_schema)
            .input_schema_strict(true)
            .output_schema_strict(false)
            .enable_output_auto_correct()
            .build()
            .unwrap();

        assert!(config.agent.schema.enabled);
        assert!(config.agent.schema.input.is_some());
        assert!(config.agent.schema.output.is_some());
        
        let input_config = config.agent.schema.input.unwrap();
        assert!(input_config.strict);
        assert_eq!(input_config.source.source_type, SchemaSourceType::Inline);
        
        let output_config = config.agent.schema.output.unwrap();
        assert!(!output_config.strict);
        assert!(output_config.auto_correct);
    }

    #[tokio::test]
    async fn test_config_with_file_schemas() {
        // Create temporary schema files
        let mut input_file = NamedTempFile::new().unwrap();
        let input_schema = json!({
            "type": "object",
            "properties": {
                "message": {"type": "string"}
            },
            "required": ["message"]
        });
        writeln!(input_file, "{}", serde_json::to_string_pretty(&input_schema).unwrap()).unwrap();

        let mut output_file = NamedTempFile::new().unwrap();
        let output_schema = json!({
            "type": "object",
            "properties": {
                "reply": {"type": "string"},
                "status": {"type": "string"}
            },
            "required": ["reply"]
        });
        writeln!(output_file, "{}", serde_json::to_string_pretty(&output_schema).unwrap()).unwrap();

        let config = ConfigBuilder::new()
            .agent_name("file_schema_agent")
            .input_schema_from_file(input_file.path())
            .output_schema_from_file(output_file.path())
            .build_unchecked(); // Skip validation since files will be loaded later

        assert!(config.agent.schema.enabled);
        assert!(config.agent.schema.input.is_some());
        assert!(config.agent.schema.output.is_some());
        
        let input_config = config.agent.schema.input.unwrap();
        assert_eq!(input_config.source.source_type, SchemaSourceType::File);
        assert!(input_config.source.location.is_some());
        
        let output_config = config.agent.schema.output.unwrap();
        assert_eq!(output_config.source.source_type, SchemaSourceType::File);
        assert!(output_config.source.location.is_some());
    }

    #[test]
    fn test_schema_validator_creation_and_validation() {
        let input_config = InputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "age": {"type": "integer", "minimum": 0}
                },
                "required": ["name"]
            }),
            strict: true,
            error_message: None,
            source: SchemaSource::default(),
        };

        let output_config = OutputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "greeting": {"type": "string"},
                    "timestamp": {"type": "string"}
                },
                "required": ["greeting"]
            }),
            strict: false,
            error_message: None,
            source: SchemaSource::default(),
            auto_correct: true,
        };

        let validator = SchemaValidator::new(
            Some(&input_config),
            Some(&output_config),
            SchemaValidationConfig::default(),
        ).unwrap();

        // Test valid input
        let valid_input = json!({"name": "Alice", "age": 30});
        assert!(validator.validate_input(&valid_input).is_ok());

        // Test invalid input (missing required field)
        let invalid_input = json!({"age": 25});
        assert!(validator.validate_input(&invalid_input).is_err());

        // Test valid output
        let valid_output = json!({"greeting": "Hello", "timestamp": "2023-01-01T00:00:00Z"});
        assert!(validator.validate_output(&valid_output).is_ok());

        // Test invalid output (missing required field)
        let invalid_output = json!({"timestamp": "2023-01-01T00:00:00Z"});
        assert!(validator.validate_output(&invalid_output).is_err());
    }

    #[test]
    fn test_schema_builder_comprehensive() {
        let schema = SchemaBuilder::new()
            .title("Comprehensive Test Schema")
            .description("A schema for testing all property types")
            .add_string_property("name", Some("Person's name"))
            .add_number_property("score", Some("Numeric score"))
            .add_boolean_property("active", Some("Whether active"))
            .add_array_property("tags", json!({"type": "string"}), Some("List of tags"))
            .add_object_property("metadata", json!({
                "type": "object",
                "properties": {
                    "created": {"type": "string", "format": "date-time"}
                }
            }), Some("Metadata object"))
            .add_required("name")
            .add_required("score")
            .build();

        assert_eq!(schema["title"], "Comprehensive Test Schema");
        assert_eq!(schema["type"], "object");
        
        let properties = schema["properties"].as_object().unwrap();
        assert!(properties.contains_key("name"));
        assert!(properties.contains_key("score"));
        assert!(properties.contains_key("active"));
        assert!(properties.contains_key("tags"));
        assert!(properties.contains_key("metadata"));
        
        assert_eq!(properties["name"]["type"], "string");
        assert_eq!(properties["score"]["type"], "number");
        assert_eq!(properties["active"]["type"], "boolean");
        assert_eq!(properties["tags"]["type"], "array");
        assert_eq!(properties["metadata"]["type"], "object");
        
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("name")));
        assert!(required.contains(&json!("score")));
    }

    #[test]
    fn test_schema_utils_functionality() {
        // Test basic schema creation
        let string_schema = SchemaUtils::create_basic_schema("string");
        assert_eq!(string_schema["type"], "string");

        let object_schema = SchemaUtils::create_basic_schema("object");
        assert_eq!(object_schema["type"], "object");
        assert!(object_schema["properties"].is_object());

        // Test schema validation
        let valid_schema = json!({
            "type": "object",
            "properties": {
                "test": {"type": "string"}
            }
        });
        assert!(SchemaUtils::validate_schema(&valid_schema).is_ok());

        // Test schema info extraction
        let complex_schema = json!({
            "title": "Test Schema",
            "description": "A test schema",
            "type": "object",
            "properties": {
                "field1": {"type": "string"},
                "field2": {"type": "number"}
            },
            "required": ["field1"]
        });
        
        let info = SchemaUtils::extract_schema_info(&complex_schema);
        assert_eq!(info.get("title"), Some(&"Test Schema".to_string()));
        assert_eq!(info.get("type"), Some(&"object".to_string()));
        assert_eq!(info.get("property_count"), Some(&"2".to_string()));
        assert!(info.get("properties").unwrap().contains("field1"));
        assert!(info.get("properties").unwrap().contains("field2"));

        // Test sample data generation
        let sample = SchemaUtils::generate_sample_data(&complex_schema).unwrap();
        assert!(sample.is_object());
        let obj = sample.as_object().unwrap();
        assert!(obj.contains_key("field1"));
        assert!(obj.contains_key("field2"));
        assert!(obj["field1"].is_string());
        assert!(obj["field2"].is_number());
    }

    #[test]
    fn test_auto_correction_functionality() {
        let output_config = OutputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "count": {"type": "number"},
                    "active": {"type": "boolean"},
                    "name": {"type": "string"}
                },
                "required": ["count"]
            }),
            strict: false,
            error_message: None,
            source: SchemaSource::default(),
            auto_correct: true,
        };

        let validator = SchemaValidator::new(
            None,
            Some(&output_config),
            SchemaValidationConfig::default(),
        ).unwrap();

        // Test auto-correction of string numbers
        let data_with_string_number = json!({
            "count": "42",
            "active": "true",
            "name": "test"
        });
        
        let result = validator.validate_output_with_retry(&data_with_string_number, true);
        assert!(result.is_ok());
        
        let corrected = result.unwrap();
        assert!(corrected["count"].is_number());
        assert!(corrected["active"].is_boolean());
        assert!(corrected["name"].is_string());
    }

    #[tokio::test]
    async fn test_yaml_config_loading_with_schemas() {
        let yaml_content = r#"
agent:
  name: "yaml_test_agent"
  schema:
    enabled: true
    input:
      strict: true
      schema:
        type: "object"
        properties:
          query:
            type: "string"
        required: ["query"]
    output:
      strict: false
      auto_correct: true
      schema:
        type: "object"
        properties:
          response:
            type: "string"
        required: ["response"]
    validation:
      log_errors: true
      max_validation_attempts: 5

llm:
  provider: "openai"
  model: "gpt-4.1-mini"
"#;

        let config = ConfigLoader::from_str(yaml_content, None).await.unwrap();
        assert_eq!(config.agent.name, "yaml_test_agent");
        assert!(config.agent.schema.enabled);
        assert!(config.agent.schema.input.is_some());
        assert!(config.agent.schema.output.is_some());
        assert_eq!(config.agent.schema.validation.max_validation_attempts, 5);
        
        let input_config = config.agent.schema.input.unwrap();
        assert!(input_config.strict);
        assert_eq!(input_config.schema["type"], "object");
        
        let output_config = config.agent.schema.output.unwrap();
        assert!(!output_config.strict);
        assert!(output_config.auto_correct);
    }

    #[test]
    fn test_validation_error_handling() {
        let input_config = InputSchemaConfig {
            schema: json!({
                "type": "object",
                "properties": {
                    "email": {
                        "type": "string",
                        "format": "email"
                    },
                    "age": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 150
                    }
                },
                "required": ["email", "age"]
            }),
            strict: true,
            error_message: Some("Custom validation error message".to_string()),
            source: SchemaSource::default(),
        };

        let validation_config = SchemaValidationConfig {
            log_errors: true,
            include_schema_in_errors: true,
            max_validation_attempts: 1,
            validate_intermediate_steps: false,
        };

        let validator = SchemaValidator::new(
            Some(&input_config),
            None,
            validation_config,
        ).unwrap();

        // Test multiple validation errors
        let invalid_data = json!({
            "email": "not-an-email",
            "age": -5
        });

        let result = validator.validate_input(&invalid_data);
        assert!(result.is_err());
        
        let error = result.unwrap_err();
        match error {
            AgentError::ConfigError(msg) => {
                assert!(msg.contains("Input validation failed"));
            }
            _ => panic!("Expected ConfigError"),
        }
    }
}
