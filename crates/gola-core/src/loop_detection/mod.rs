//! Loop detection system for preventing infinite tool call patterns
//! 
//! This module provides functionality to detect when an agent is stuck in loops
//! by analyzing patterns in tool calls and their arguments.

use serde_json::Value;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod pattern_detector;
pub mod similarity;

pub use pattern_detector::*;
pub use similarity::*;

/// Configuration for loop detection
#[derive(Debug, Clone)]
pub struct LoopDetectionConfig {
    /// Number of recent tool calls to analyze
    pub detection_window_size: usize,
    /// Threshold for exact match detection
    pub exact_match_threshold: u32,
    /// Threshold for similar match detection  
    pub similar_match_threshold: u32,
    /// Time window in seconds for temporal grouping
    pub time_window_seconds: u64,
    /// Tool-specific thresholds
    pub tool_specific_thresholds: HashMap<String, u32>,
}

impl Default for LoopDetectionConfig {
    fn default() -> Self {
        Self {
            detection_window_size: 10,
            exact_match_threshold: 5,
            similar_match_threshold: 8,
            time_window_seconds: 30,
            tool_specific_thresholds: HashMap::new(),
        }
    }
}

/// Types of loop patterns that can be detected
#[derive(Debug, Clone, PartialEq)]
pub enum LoopPattern {
    /// Exact same tool calls with identical arguments
    ExactLoop {
        tool_name: String,
        count: u32,
        first_occurrence_index: usize,
    },
    /// Similar tool calls with nearly identical arguments
    SimilarLoop {
        tool_name: String,
        count: u32,
        similarity_score: f64,
        first_occurrence_index: usize,
    },
    /// Suspicious pattern that might develop into a loop
    SuspiciousPattern {
        tool_name: String,
        count: u32,
        pattern_type: String,
    },
    /// No loop detected
    NoLoop,
}

impl LoopPattern {
    /// Check if this pattern indicates a problematic loop
    pub fn is_problematic(&self) -> bool {
        matches!(self, LoopPattern::ExactLoop { .. } | LoopPattern::SimilarLoop { .. })
    }
    
    /// Get the tool name involved in the pattern
    pub fn tool_name(&self) -> Option<&str> {
        match self {
            LoopPattern::ExactLoop { tool_name, .. } => Some(tool_name),
            LoopPattern::SimilarLoop { tool_name, .. } => Some(tool_name),
            LoopPattern::SuspiciousPattern { tool_name, .. } => Some(tool_name),
            LoopPattern::NoLoop => None,
        }
    }
    
    /// Get the count of repetitions
    pub fn count(&self) -> u32 {
        match self {
            LoopPattern::ExactLoop { count, .. } => *count,
            LoopPattern::SimilarLoop { count, .. } => *count,
            LoopPattern::SuspiciousPattern { count, .. } => *count,
            LoopPattern::NoLoop => 0,
        }
    }
}

/// Represents a tool call with timing information
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub arguments: Value,
    pub timestamp: u64,
    pub step_index: usize,
    pub signature_hash: u64,
    pub argument_structure_hash: u64,
}

impl ToolCallRecord {
    pub fn new(tool_name: String, arguments: Value, step_index: usize) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        let signature_hash = Self::calculate_signature_hash(&tool_name, &arguments);
        let argument_structure_hash = Self::calculate_structure_hash(&arguments);
        
        Self {
            tool_name,
            arguments,
            timestamp,
            step_index,
            signature_hash,
            argument_structure_hash,
        }
    }
    
    /// Calculate hash for exact matching (tool_name + arguments)
    fn calculate_signature_hash(tool_name: &str, arguments: &Value) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        tool_name.hash(&mut hasher);
        
        // Create a canonical string representation of the JSON for hashing
        let canonical_json = serde_json::to_string(arguments).unwrap_or_default();
        canonical_json.hash(&mut hasher);
        
        hasher.finish()
    }
    
    /// Calculate hash for argument structure (keys and types, not values)
    fn calculate_structure_hash(arguments: &Value) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Self::hash_json_structure(arguments, &mut hasher);
        hasher.finish()
    }
    
    /// Recursively hash the structure of a JSON value
    fn hash_json_structure(value: &Value, hasher: &mut impl Hasher) {
        match value {
            Value::Object(map) => {
                "object".hash(hasher);
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort(); // Ensure consistent ordering
                for key in keys {
                    key.hash(hasher);
                    if let Some(val) = map.get(key) {
                        Self::hash_json_structure(val, hasher);
                    }
                }
            }
            Value::Array(arr) => {
                "array".hash(hasher);
                arr.len().hash(hasher);
                if let Some(first) = arr.first() {
                    Self::hash_json_structure(first, hasher);
                }
            }
            Value::String(_) => "string".hash(hasher),
            Value::Number(_) => "number".hash(hasher),
            Value::Bool(_) => "bool".hash(hasher),
            Value::Null => "null".hash(hasher),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_call_record_creation() {
        let args = json!({"timezone": "America/New_York"});
        let record = ToolCallRecord::new("get_current_time".to_string(), args.clone(), 1);
        
        assert_eq!(record.tool_name, "get_current_time");
        assert_eq!(record.arguments, args);
        assert_eq!(record.step_index, 1);
        assert!(record.timestamp > 0);
        assert!(record.signature_hash > 0);
        assert!(record.argument_structure_hash > 0);
    }
    
    #[test]
    fn test_signature_hash_consistency() {
        let args1 = json!({"timezone": "America/New_York"});
        let args2 = json!({"timezone": "America/New_York"});
        let args3 = json!({"timezone": "Europe/London"});
        
        let record1 = ToolCallRecord::new("get_current_time".to_string(), args1, 1);
        let record2 = ToolCallRecord::new("get_current_time".to_string(), args2, 2);
        let record3 = ToolCallRecord::new("get_current_time".to_string(), args3, 3);
        
        // Same tool + same args should have same signature hash
        assert_eq!(record1.signature_hash, record2.signature_hash);
        
        // Same tool + different args should have different signature hash
        assert_ne!(record1.signature_hash, record3.signature_hash);
    }
    
    #[test]
    fn test_structure_hash_consistency() {
        let args1 = json!({"timezone": "America/New_York", "format": "iso"});
        let args2 = json!({"timezone": "Europe/London", "format": "rfc"});
        let args3 = json!({"city": "New York"});
        
        let record1 = ToolCallRecord::new("get_time".to_string(), args1, 1);
        let record2 = ToolCallRecord::new("get_time".to_string(), args2, 2);
        let record3 = ToolCallRecord::new("get_time".to_string(), args3, 3);
        
        // Same structure (same keys) should have same structure hash
        assert_eq!(record1.argument_structure_hash, record2.argument_structure_hash);
        
        // Different structure should have different structure hash
        assert_ne!(record1.argument_structure_hash, record3.argument_structure_hash);
    }
    
    #[test]
    fn test_loop_pattern_methods() {
        let exact_loop = LoopPattern::ExactLoop {
            tool_name: "test_tool".to_string(),
            count: 5,
            first_occurrence_index: 0,
        };
        
        assert!(exact_loop.is_problematic());
        assert_eq!(exact_loop.tool_name(), Some("test_tool"));
        assert_eq!(exact_loop.count(), 5);
        
        let no_loop = LoopPattern::NoLoop;
        assert!(!no_loop.is_problematic());
        assert_eq!(no_loop.tool_name(), None);
        assert_eq!(no_loop.count(), 0);
    }
}