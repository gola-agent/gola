//! Pattern detection engine for identifying tool call loops
//! 
//! This module implements the core pattern detection logic that analyzes
//! tool call history to identify exact and similar loops.

use super::{LoopDetectionConfig, LoopPattern, ToolCallRecord};
use crate::loop_detection::similarity::calculate_argument_similarity;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

/// Main pattern detector that analyzes tool call history for loops
#[derive(Debug)]
pub struct PatternDetector {
    config: LoopDetectionConfig,
    call_history: VecDeque<ToolCallRecord>,
    exact_match_counts: HashMap<u64, u32>,
    similar_pattern_tracking: HashMap<String, Vec<(usize, f64)>>,
}

impl PatternDetector {
    /// Create a new pattern detector with the given configuration
    pub fn new(config: LoopDetectionConfig) -> Self {
        Self {
            config,
            call_history: VecDeque::new(),
            exact_match_counts: HashMap::new(),
            similar_pattern_tracking: HashMap::new(),
        }
    }
    
    /// Add a new tool call and analyze for patterns
    pub fn add_tool_call(&mut self, tool_name: String, arguments: Value, step_index: usize) -> LoopPattern {
        let record = ToolCallRecord::new(tool_name.clone(), arguments, step_index);
        
        // Maintain sliding window
        self.call_history.push_back(record.clone());
        if self.call_history.len() > self.config.detection_window_size {
            if let Some(removed) = self.call_history.pop_front() {
                // Decrement count for removed record
                if let Some(count) = self.exact_match_counts.get_mut(&removed.signature_hash) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.exact_match_counts.remove(&removed.signature_hash);
                    }
                }
            }
        }
        
        // Update exact match counts
        *self.exact_match_counts.entry(record.signature_hash).or_insert(0) += 1;
        
        // Analyze patterns
        self.detect_patterns(&record)
    }
    
    /// Detect patterns for the most recent tool call
    fn detect_patterns(&mut self, latest_record: &ToolCallRecord) -> LoopPattern {
        // Layer 1: Exact Match Detection
        if let Some(exact_pattern) = self.detect_exact_matches(latest_record) {
            return exact_pattern;
        }
        
        // Layer 2: Semantic Similarity Detection
        if let Some(similar_pattern) = self.detect_similar_patterns(latest_record) {
            return similar_pattern;
        }
        
        // Check for suspicious patterns
        if let Some(suspicious_pattern) = self.detect_suspicious_patterns(latest_record) {
            return suspicious_pattern;
        }
        
        LoopPattern::NoLoop
    }
    
    /// Layer 1: Detect exact matches (identical tool name + arguments)
    fn detect_exact_matches(&self, latest_record: &ToolCallRecord) -> Option<LoopPattern> {
        let exact_count = *self.exact_match_counts.get(&latest_record.signature_hash).unwrap_or(&0);
        
        // Use generic threshold for all tools
        let threshold = self.config.exact_match_threshold;
        
        if exact_count >= threshold {
            // Find the first occurrence of this signature
            let first_occurrence_index = self.call_history
                .iter()
                .find(|record| record.signature_hash == latest_record.signature_hash)
                .map(|record| record.step_index)
                .unwrap_or(latest_record.step_index);
            
            return Some(LoopPattern::ExactLoop {
                tool_name: latest_record.tool_name.clone(),
                count: exact_count,
                first_occurrence_index,
            });
        }
        
        None
    }
    
    /// Layer 2: Detect similar patterns (same tool, similar arguments)
    fn detect_similar_patterns(&mut self, latest_record: &ToolCallRecord) -> Option<LoopPattern> {
        let tool_name = &latest_record.tool_name;
        
        // Count similar calls for this tool
        let mut similar_calls = Vec::new();
        let mut total_similarity = 0.0;
        
        for record in self.call_history.iter().rev().take(self.config.detection_window_size) {
            if record.tool_name == *tool_name && record.step_index != latest_record.step_index {
                let similarity = calculate_argument_similarity(&record.arguments, &latest_record.arguments);
                
                // Consider it similar if above threshold (0.7 = 70% similar)
                if similarity >= 0.7 {
                    similar_calls.push((record.step_index, similarity));
                    total_similarity += similarity;
                }
            }
        }
        
        // Check if we have enough similar calls
        let similar_count = similar_calls.len() as u32 + 1; // +1 for current call
        
        if similar_count >= self.config.similar_match_threshold {
            let avg_similarity = if similar_calls.is_empty() {
                1.0 // Only one call, so perfectly similar to itself
            } else {
                total_similarity / similar_calls.len() as f64
            };
            
            let first_occurrence_index = similar_calls
                .iter()
                .map(|(index, _)| *index)
                .min()
                .unwrap_or(latest_record.step_index);
            
            return Some(LoopPattern::SimilarLoop {
                tool_name: tool_name.clone(),
                count: similar_count,
                similarity_score: avg_similarity,
                first_occurrence_index,
            });
        }
        
        None
    }
    
    /// Detect suspicious patterns that might develop into loops
    fn detect_suspicious_patterns(&self, latest_record: &ToolCallRecord) -> Option<LoopPattern> {
        let tool_name = &latest_record.tool_name;
        
        // Count calls to the same tool in recent history
        let recent_same_tool_count = self.call_history
            .iter()
            .rev()
            .take(5) // Look at last 5 calls
            .filter(|record| record.tool_name == *tool_name)
            .count() as u32;
        
        // If we're seeing the same tool frequently, it might be suspicious
        if recent_same_tool_count >= 3 {
            return Some(LoopPattern::SuspiciousPattern {
                tool_name: tool_name.clone(),
                count: recent_same_tool_count,
                pattern_type: "frequent_same_tool".to_string(),
            });
        }
        
        // Check for rapid-fire calls (same tool called multiple times in quick succession)
        let rapid_fire_count = self.count_rapid_fire_calls(latest_record);
        if rapid_fire_count >= 3 {
            return Some(LoopPattern::SuspiciousPattern {
                tool_name: tool_name.clone(),
                count: rapid_fire_count,
                pattern_type: "rapid_fire".to_string(),
            });
        }
        
        None
    }
    
    /// Count how many times the same tool was called within the time window
    fn count_rapid_fire_calls(&self, latest_record: &ToolCallRecord) -> u32 {
        let time_threshold = latest_record.timestamp - self.config.time_window_seconds;
        
        self.call_history
            .iter()
            .filter(|record| {
                record.tool_name == latest_record.tool_name && 
                record.timestamp >= time_threshold
            })
            .count() as u32
    }
    
    /// Get current statistics about the detection state
    pub fn get_statistics(&self) -> DetectionStatistics {
        let total_calls = self.call_history.len();
        let unique_signatures = self.exact_match_counts.len();
        let tool_distribution = self.calculate_tool_distribution();
        
        DetectionStatistics {
            total_calls,
            unique_signatures,
            window_utilization: total_calls as f64 / self.config.detection_window_size as f64,
            tool_distribution,
        }
    }
    
    /// Calculate distribution of tool calls
    fn calculate_tool_distribution(&self) -> HashMap<String, u32> {
        let mut distribution = HashMap::new();
        
        for record in &self.call_history {
            *distribution.entry(record.tool_name.clone()).or_insert(0) += 1;
        }
        
        distribution
    }
    
    /// Clear all tracking data (useful for testing or resetting state)
    pub fn clear(&mut self) {
        self.call_history.clear();
        self.exact_match_counts.clear();
        self.similar_pattern_tracking.clear();
    }
    
    /// Get the current call history (for debugging/inspection)
    pub fn get_call_history(&self) -> &VecDeque<ToolCallRecord> {
        &self.call_history
    }
}

/// Statistics about the current detection state
#[derive(Debug)]
pub struct DetectionStatistics {
    pub total_calls: usize,
    pub unique_signatures: usize,
    pub window_utilization: f64,
    pub tool_distribution: HashMap<String, u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_test_config() -> LoopDetectionConfig {
        LoopDetectionConfig {
            detection_window_size: 5,
            exact_match_threshold: 3,
            similar_match_threshold: 3,
            time_window_seconds: 10,
            tool_specific_thresholds: HashMap::new(),
        }
    }

    #[test]
    fn test_exact_loop_detection() {
        let mut detector = PatternDetector::new(create_test_config());
        let args = json!({"timezone": "America/New_York"});
        
        // Add the same call multiple times
        let pattern1 = detector.add_tool_call("get_current_time".to_string(), args.clone(), 1);
        assert_eq!(pattern1, LoopPattern::NoLoop);
        
        let pattern2 = detector.add_tool_call("get_current_time".to_string(), args.clone(), 2);
        assert_eq!(pattern2, LoopPattern::NoLoop);
        
        let pattern3 = detector.add_tool_call("get_current_time".to_string(), args.clone(), 3);
        match pattern3 {
            LoopPattern::ExactLoop { tool_name, count, .. } => {
                assert_eq!(tool_name, "get_current_time");
                assert_eq!(count, 3);
            }
            _ => panic!("Expected ExactLoop pattern"),
        }
    }
    
    #[test]
    fn test_similar_loop_detection() {
        let mut detector = PatternDetector::new(create_test_config());
        
        // Add similar but not identical calls
        let args1 = json!({"timezone": "America/New_York"});
        let args2 = json!({"timezone": "America/Chicago"});
        let args3 = json!({"timezone": "America/Denver"});
        
        detector.add_tool_call("get_current_time".to_string(), args1, 1);
        detector.add_tool_call("get_current_time".to_string(), args2, 2);
        let pattern = detector.add_tool_call("get_current_time".to_string(), args3, 3);
        
        match pattern {
            LoopPattern::SimilarLoop { tool_name, count, similarity_score, .. } => {
                assert_eq!(tool_name, "get_current_time");
                assert_eq!(count, 3);
                assert!(similarity_score > 0.7);
            }
            _ => panic!("Expected SimilarLoop pattern, got {:?}", pattern),
        }
    }
    
    #[test]
    fn test_no_loop_different_tools() {
        let mut detector = PatternDetector::new(create_test_config());
        let args = json!({"param": "value"});
        
        // Different tools shouldn't trigger loop detection
        detector.add_tool_call("tool1".to_string(), args.clone(), 1);
        detector.add_tool_call("tool2".to_string(), args.clone(), 2);
        let pattern = detector.add_tool_call("tool3".to_string(), args, 3);
        
        assert_eq!(pattern, LoopPattern::NoLoop);
    }
    
    #[test]
    fn test_suspicious_pattern_detection() {
        let mut detector = PatternDetector::new(create_test_config());
        let args = json!({"param": "value"});
        
        // Add same tool multiple times but not enough for exact/similar loop
        detector.add_tool_call("test_tool".to_string(), args.clone(), 1);
        detector.add_tool_call("other_tool".to_string(), json!({}), 2);
        detector.add_tool_call("test_tool".to_string(), args.clone(), 3);
        detector.add_tool_call("test_tool".to_string(), args.clone(), 4);
        let pattern = detector.add_tool_call("test_tool".to_string(), args, 5);
        
        match pattern {
            LoopPattern::SuspiciousPattern { tool_name, count, pattern_type } => {
                assert_eq!(tool_name, "test_tool");
                assert!(count >= 3);
                assert_eq!(pattern_type, "frequent_same_tool");
            }
            _ => {},
        }
    }
    
    #[test]
    fn test_sliding_window() {
        let mut detector = PatternDetector::new(create_test_config());
        let args = json!({"param": "value"});
        
        // Fill beyond window size
        for i in 1..=10 {
            detector.add_tool_call(format!("tool_{}", i), args.clone(), i);
        }
        
        let stats = detector.get_statistics();
        assert_eq!(stats.total_calls, 5); // Should be limited by window size
    }
    
    #[test]
    fn test_statistics() {
        let mut detector = PatternDetector::new(create_test_config());
        let args = json!({"param": "value"});
        
        detector.add_tool_call("tool1".to_string(), args.clone(), 1);
        detector.add_tool_call("tool1".to_string(), args.clone(), 2);
        detector.add_tool_call("tool2".to_string(), args, 3);
        
        let stats = detector.get_statistics();
        assert_eq!(stats.total_calls, 3);
        assert_eq!(stats.tool_distribution.get("tool1"), Some(&2));
        assert_eq!(stats.tool_distribution.get("tool2"), Some(&1));
        assert!(stats.window_utilization > 0.5);
    }
    
    #[test]
    fn test_generic_threshold_applies_to_all_tools() {
        let config = create_test_config();
        let mut detector = PatternDetector::new(config);
        let args = json!({"param": "value"});
        
        // Test that loop detection works the same for different tools
        // All tools should use the same generic threshold (3)
        
        // Test with get_current_time
        detector.add_tool_call("get_current_time".to_string(), args.clone(), 1);
        detector.add_tool_call("get_current_time".to_string(), args.clone(), 2);
        let pattern = detector.add_tool_call("get_current_time".to_string(), args.clone(), 3);
        
        match pattern {
            LoopPattern::ExactLoop { count, .. } => assert_eq!(count, 3),
            _ => panic!("Expected ExactLoop with generic threshold for get_current_time"),
        }
        
        // Reset detector for next test
        detector.clear();
        
        // Test with search_flights - should behave identically
        detector.add_tool_call("search_flights".to_string(), args.clone(), 1);
        detector.add_tool_call("search_flights".to_string(), args.clone(), 2);
        let pattern = detector.add_tool_call("search_flights".to_string(), args, 3);
        
        match pattern {
            LoopPattern::ExactLoop { count, .. } => assert_eq!(count, 3),
            _ => panic!("Expected ExactLoop with generic threshold for search_flights"),
        }
    }
}