use crate::core_types::{LLMResponse, ToolCall};
use crate::errors::AgentError;
use serde_json::Value;

pub struct ResponseParser;

impl ResponseParser {
    pub fn parse_openai_response(response: Value) -> Result<LLMResponse, AgentError> {
        let choices = response["choices"]
            .as_array()
            .ok_or_else(|| AgentError::ParsingError("No choices in response".to_string()))?;

        if choices.is_empty() {
            return Err(AgentError::ParsingError("Empty choices array".to_string()));
        }

        let choice = &choices[0];
        let message = &choice["message"];

        let content = message["content"].as_str().map(|s| s.to_string());

        let tool_calls = Self::parse_tool_calls(&message["tool_calls"])?;

        // Validate that we have either content or tool calls
        if content.is_none() && tool_calls.is_none() {
            return Err(AgentError::ParsingError(
                "Response has neither content nor tool calls".to_string(),
            ));
        }

        Ok(LLMResponse {
            content,
            tool_calls,
            finish_reason: None,
            usage: None,
        })
    }

    pub fn parse_gemini_response(response: Value) -> Result<LLMResponse, AgentError> {
        let candidates = response["candidates"].as_array().ok_or_else(|| {
            AgentError::ParsingError("No candidates in Gemini response".to_string())
        })?;

        if candidates.is_empty() {
            return Err(AgentError::ParsingError(
                "Empty candidates array".to_string(),
            ));
        }

        let candidate = &candidates[0];
        let content_obj = &candidate["content"];

        // Extract text content
        let content = if let Some(parts) = content_obj["parts"].as_array() {
            let mut text_parts = Vec::new();
            for part in parts {
                if let Some(text) = part["text"].as_str() {
                    text_parts.push(text);
                }
            }
            if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join(""))
            }
        } else {
            None
        };

        // Extract function calls (Gemini format)
        let tool_calls = if let Some(parts) = content_obj["parts"].as_array() {
            let mut calls = Vec::new();
            for part in parts {
                if let Some(function_call) = part["functionCall"].as_object() {
                    if let (Some(name), Some(args)) = (
                        function_call["name"].as_str(),
                        function_call["args"].as_object(),
                    ) {
                        calls.push(ToolCall {
                            id: None,
                            name: name.to_string(),
                            arguments: Value::Object(args.clone()),
                        });
                    }
                }
            }
            if calls.is_empty() {
                None
            } else {
                Some(calls)
            }
        } else {
            None
        };

        if content.is_none() && tool_calls.is_none() {
            return Err(AgentError::ParsingError(
                "Gemini response has neither content nor function calls".to_string(),
            ));
        }

        Ok(LLMResponse {
            content,
            tool_calls,
            finish_reason: None,
            usage: None,
        })
    }

    fn parse_tool_calls(tool_calls_value: &Value) -> Result<Option<Vec<ToolCall>>, AgentError> {
        if let Some(calls) = tool_calls_value.as_array() {
            let mut parsed_calls = Vec::new();
            for call in calls {
                if let (Some(id), Some(function)) =
                    (call["id"].as_str(), call["function"].as_object())
                {
                    if let (Some(name), Some(arguments_str)) =
                        (function["name"].as_str(), function["arguments"].as_str())
                    {
                        let arguments: Value =
                            serde_json::from_str(arguments_str).map_err(|e| {
                                AgentError::ParsingError(format!(
                                    "Invalid tool call arguments JSON: {}",
                                    e
                                ))
                            })?;

                        parsed_calls.push(ToolCall {
                            id: Some(id.to_string()),
                            name: name.to_string(),
                            arguments,
                        });
                    }
                }
            }
            if parsed_calls.is_empty() {
                Ok(None)
            } else {
                Ok(Some(parsed_calls))
            }
        } else {
            Ok(None)
        }
    }

    pub fn parse_text_for_tool_calls(text: &str) -> Option<Vec<ToolCall>> {
        use regex::Regex;

        // Pattern to match function calls in text like: function_name({"arg": "value"})
        let re = Regex::new(r"(\w+)\((\{[^}]*\})\)").ok()?;

        let mut tool_calls = Vec::new();
        for cap in re.captures_iter(text) {
            if let (Some(name), Some(args_str)) = (cap.get(1), cap.get(2)) {
                if let Ok(arguments) = serde_json::from_str::<Value>(args_str.as_str()) {
                    tool_calls.push(ToolCall {
                        id: None,
                        name: name.as_str().to_string(),
                        arguments,
                    });
                }
            }
        }

        if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        }
    }

    pub fn extract_final_answer(text: &str) -> Option<String> {
        use regex::Regex;

        // Look for patterns like "Final Answer:", "Answer:", etc.
        let patterns = [
            r"(?i)final\s+answer:\s*(.+)",
            r"(?i)answer:\s*(.+)",
            r"(?i)conclusion:\s*(.+)",
            r"(?i)result:\s*(.+)",
        ];

        for pattern in &patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(cap) = re.captures(text) {
                    if let Some(answer) = cap.get(1) {
                        return Some(answer.as_str().trim().to_string());
                    }
                }
            }
        }

        None
    }

    pub fn contains_tool_intent(text: &str) -> bool {
        let tool_keywords = [
            "use tool",
            "call function",
            "execute",
            "search",
            "calculate",
            "I need to",
            "I should",
            "let me",
            "I'll use",
            "I will use",
        ];

        let text_lower = text.to_lowercase();
        tool_keywords
            .iter()
            .any(|keyword| text_lower.contains(keyword))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_openai_response_with_content() {
        let response = json!({
            "choices": [{
                "message": {
                    "content": "Hello, how can I help you?"
                }
            }]
        });

        let parsed = ResponseParser::parse_openai_response(response).unwrap();
        assert_eq!(
            parsed.content,
            Some("Hello, how can I help you?".to_string())
        );
        assert!(parsed.tool_calls.is_none());
    }

    #[test]
    fn test_parse_openai_response_with_tool_calls() {
        let response = json!({
            "choices": [{
                "message": {
                    "tool_calls": [{
                        "id": "call_123",
                        "function": {
                            "name": "calculator",
                            "arguments": "{\"operation\": \"add\", \"a\": 5, \"b\": 3}"
                        }
                    }]
                }
            }]
        });

        let parsed = ResponseParser::parse_openai_response(response).unwrap();
        assert!(parsed.content.is_none());
        assert!(parsed.tool_calls.is_some());

        let tool_calls = parsed.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "calculator");
        assert_eq!(tool_calls[0].id, Some("call_123".to_string()));
    }

    #[test]
    fn test_extract_final_answer() {
        let text = "After thinking about this, the Final Answer: 42 is the result.";
        let answer = ResponseParser::extract_final_answer(text);
        assert_eq!(answer, Some("42 is the result.".to_string()));
    }

    #[test]
    fn test_parse_text_for_tool_calls() {
        let text = r#"I need to calculator({"operation": "add", "a": 5, "b": 3}) to solve this."#;
        let tool_calls = ResponseParser::parse_text_for_tool_calls(text);

        assert!(tool_calls.is_some());
        let calls = tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "calculator");
    }

    #[test]
    fn test_contains_tool_intent() {
        assert!(ResponseParser::contains_tool_intent(
            "I need to search for information"
        ));
        assert!(ResponseParser::contains_tool_intent(
            "Let me calculate this"
        ));
        assert!(!ResponseParser::contains_tool_intent(
            "This is just a regular response"
        ));
    }
}

