//! Similarity analysis for tool call arguments
//! 
//! This module provides functions to calculate similarity between tool call arguments
//! to detect patterns where calls are similar but not identical.

use serde_json::Value;
use std::collections::HashSet;

/// Calculate similarity score between two tool call arguments
/// Returns a score between 0.0 (completely different) and 1.0 (identical)
pub fn calculate_argument_similarity(args1: &Value, args2: &Value) -> f64 {
    if args1 == args2 {
        return 1.0;
    }
    
    match (args1, args2) {
        (Value::Object(obj1), Value::Object(obj2)) => calculate_object_similarity(obj1, obj2),
        (Value::Array(arr1), Value::Array(arr2)) => calculate_array_similarity(arr1, arr2),
        (Value::String(s1), Value::String(s2)) => calculate_string_similarity(s1, s2),
        (Value::Number(n1), Value::Number(n2)) => calculate_number_similarity(n1, n2),
        (Value::Bool(b1), Value::Bool(b2)) => if b1 == b2 { 1.0 } else { 0.0 },
        (Value::Null, Value::Null) => 1.0,
        _ => 0.0,
    }
}

/// Calculate similarity between two JSON objects
fn calculate_object_similarity(obj1: &serde_json::Map<String, Value>, obj2: &serde_json::Map<String, Value>) -> f64 {
    if obj1.is_empty() && obj2.is_empty() {
        return 1.0;
    }
    
    let keys1: HashSet<_> = obj1.keys().collect();
    let keys2: HashSet<_> = obj2.keys().collect();
    
    // Calculate key overlap (structural similarity)
    let common_keys: HashSet<_> = keys1.intersection(&keys2).collect();
    let all_keys: HashSet<_> = keys1.union(&keys2).collect();
    
    if all_keys.is_empty() {
        return 1.0;
    }
    
    let key_similarity = common_keys.len() as f64 / all_keys.len() as f64;
    
    // Calculate value similarity for common keys
    let mut value_similarity_sum = 0.0;
    let mut common_key_count = 0;
    
    for key in common_keys {
        if let (Some(val1), Some(val2)) = (obj1.get(*key), obj2.get(*key)) {
            value_similarity_sum += calculate_argument_similarity(val1, val2);
            common_key_count += 1;
        }
    }
    
    let value_similarity = if common_key_count > 0 {
        value_similarity_sum / common_key_count as f64
    } else {
        0.0
    };
    
    // Weight structural similarity higher than value similarity
    key_similarity * 0.6 + value_similarity * 0.4
}

/// Calculate similarity between two JSON arrays
fn calculate_array_similarity(arr1: &Vec<Value>, arr2: &Vec<Value>) -> f64 {
    if arr1.is_empty() && arr2.is_empty() {
        return 1.0;
    }
    
    if arr1.len() != arr2.len() {
        // Arrays of different lengths are less similar
        let size_similarity = 1.0 - (arr1.len() as f64 - arr2.len() as f64).abs() / 
                              (arr1.len().max(arr2.len()) as f64);
        
        // Still compare elements that exist in both
        let min_len = arr1.len().min(arr2.len());
        if min_len == 0 {
            return size_similarity * 0.5;
        }
        
        let mut element_similarity_sum = 0.0;
        for i in 0..min_len {
            element_similarity_sum += calculate_argument_similarity(&arr1[i], &arr2[i]);
        }
        
        let element_similarity = element_similarity_sum / min_len as f64;
        return size_similarity * 0.3 + element_similarity * 0.7;
    }
    
    // Same length arrays
    let mut similarity_sum = 0.0;
    for (val1, val2) in arr1.iter().zip(arr2.iter()) {
        similarity_sum += calculate_argument_similarity(val1, val2);
    }
    
    similarity_sum / arr1.len() as f64
}

/// Calculate similarity between two strings
fn calculate_string_similarity(s1: &str, s2: &str) -> f64 {
    if s1 == s2 {
        return 1.0;
    }
    
    if s1.is_empty() && s2.is_empty() {
        return 1.0;
    }
    
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    
    // Use a simple character overlap approach
    let chars1: HashSet<char> = s1.chars().collect();
    let chars2: HashSet<char> = s2.chars().collect();
    
    let common_chars = chars1.intersection(&chars2).count();
    let total_chars = chars1.union(&chars2).count();
    
    if total_chars == 0 {
        return 1.0;
    }
    
    let char_similarity = common_chars as f64 / total_chars as f64;
    
    // Also consider length similarity
    let len_similarity = 1.0 - (s1.len() as f64 - s2.len() as f64).abs() / 
                         (s1.len().max(s2.len()) as f64);
    
    // Weight character similarity higher
    char_similarity * 0.7 + len_similarity * 0.3
}

/// Calculate similarity between two numbers
fn calculate_number_similarity(n1: &serde_json::Number, n2: &serde_json::Number) -> f64 {
    // Convert to f64 for comparison
    let f1 = n1.as_f64().unwrap_or(0.0);
    let f2 = n2.as_f64().unwrap_or(0.0);
    
    if f1 == f2 {
        return 1.0;
    }
    
    // Calculate relative difference
    let max_val = f1.abs().max(f2.abs());
    if max_val == 0.0 {
        return 1.0; // Both are zero
    }
    
    let relative_diff = (f1 - f2).abs() / max_val;
    
    // Return similarity (1 - difference), clamped to 0.0
    (1.0 - relative_diff).max(0.0)
}

/// Check if two tool calls are semantically similar
/// This is a higher-level function that combines argument similarity with other factors
pub fn are_tool_calls_similar(
    tool1: &str, 
    args1: &Value, 
    tool2: &str, 
    args2: &Value,
    similarity_threshold: f64
) -> bool {
    // Must be the same tool
    if tool1 != tool2 {
        return false;
    }
    
    let arg_similarity = calculate_argument_similarity(args1, args2);
    arg_similarity >= similarity_threshold
}

/// Calculate how similar a tool call is to a pattern of previous calls
pub fn calculate_pattern_similarity(
    target_tool: &str,
    target_args: &Value,
    pattern_calls: &[(String, Value)]
) -> f64 {
    if pattern_calls.is_empty() {
        return 0.0;
    }
    
    let mut similarity_sum = 0.0;
    let mut count = 0;
    
    for (tool_name, args) in pattern_calls {
        if tool_name == target_tool {
            similarity_sum += calculate_argument_similarity(target_args, args);
            count += 1;
        }
    }
    
    if count == 0 {
        return 0.0;
    }
    
    similarity_sum / count as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_identical_arguments() {
        let args1 = json!({"timezone": "America/New_York"});
        let args2 = json!({"timezone": "America/New_York"});
        
        let similarity = calculate_argument_similarity(&args1, &args2);
        assert_eq!(similarity, 1.0);
    }
    
    #[test]
    fn test_different_values_same_structure() {
        let args1 = json!({"timezone": "America/New_York"});
        let args2 = json!({"timezone": "Europe/London"});
        
        let similarity = calculate_argument_similarity(&args1, &args2);
        assert!(similarity > 0.5); // Same structure, different values
        assert!(similarity < 1.0);
    }
    
    #[test]
    fn test_different_structure() {
        let args1 = json!({"timezone": "America/New_York"});
        let args2 = json!({"city": "New York"});
        
        let similarity = calculate_argument_similarity(&args1, &args2);
        assert!(similarity < 0.5); // Different structure
    }
    
    #[test]
    fn test_complex_objects() {
        let args1 = json!({
            "origin": "JFK",
            "destination": "FCO", 
            "departure_date": "2025-08-15",
            "trip_type": "round-trip"
        });
        
        let args2 = json!({
            "origin": "JFK",
            "destination": "CDG",
            "departure_date": "2025-08-15", 
            "trip_type": "round-trip"
        });
        
        let similarity = calculate_argument_similarity(&args1, &args2);
        assert!(similarity > 0.7); // Most fields same, one different
        assert!(similarity < 1.0);
    }
    
    #[test]
    fn test_array_similarity() {
        let args1 = json!({"tags": ["travel", "flight", "booking"]});
        let args2 = json!({"tags": ["travel", "hotel", "booking"]});
        
        let similarity = calculate_argument_similarity(&args1, &args2);
        assert!(similarity > 0.6); // 2/3 array elements same
        assert!(similarity < 1.0);
    }
    
    #[test]
    fn test_string_similarity() {
        let args1 = json!({"message": "Hello world"});
        let args2 = json!({"message": "Hello there"});
        
        let similarity = calculate_argument_similarity(&args1, &args2);
        assert!(similarity > 0.4); // Some character overlap
        assert!(similarity < 1.0);
    }
    
    #[test]
    fn test_number_similarity() {
        let args1 = json!({"count": 10});
        let args2 = json!({"count": 12});
        
        let similarity = calculate_argument_similarity(&args1, &args2);
        assert!(similarity > 0.8); // Numbers close to each other
        assert!(similarity < 1.0);
    }
    
    #[test]
    fn test_tool_calls_similar() {
        let args1 = json!({"timezone": "America/New_York"});
        let args2 = json!({"timezone": "America/Chicago"});
        
        assert!(are_tool_calls_similar("get_current_time", &args1, "get_current_time", &args2, 0.7));
        assert!(!are_tool_calls_similar("get_current_time", &args1, "different_tool", &args2, 0.7));
    }
    
    #[test]
    fn test_pattern_similarity() {
        let target_args = json!({"timezone": "America/New_York"});
        let pattern = vec![
            ("get_current_time".to_string(), json!({"timezone": "America/Chicago"})),
            ("get_current_time".to_string(), json!({"timezone": "America/Denver"})),
            ("other_tool".to_string(), json!({"param": "value"})),
        ];
        
        let similarity = calculate_pattern_similarity("get_current_time", &target_args, &pattern);
        assert!(similarity > 0.6); // Similar to pattern calls
        assert!(similarity < 1.0);
    }
}