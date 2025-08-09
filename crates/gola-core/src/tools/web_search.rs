//! Web search integration for real-time information retrieval
//!
//! This module bridges the gap between static training data and current information
//! by providing agents with web search capabilities. The abstraction over multiple
//! search providers ensures resilience and allows for provider-specific optimizations.
//! This tool is essential for tasks requiring up-to-date information, fact-checking,
//! and research that extends beyond the agent's knowledge cutoff.

use crate::errors::AgentError;
use crate::llm::ToolMetadata;
use crate::tools::Tool;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

pub struct WebSearchTool {
    client: Client,
    api_key: Option<String>,
    search_engine: SearchEngine,
}

#[derive(Debug, Clone)]
pub enum SearchEngine {
    DuckDuckGo,
    Tavily,
    Serper,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_key: None,
            search_engine: SearchEngine::DuckDuckGo,
        }
    }

    pub fn with_tavily_api_key(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_key: Some(api_key),
            search_engine: SearchEngine::Tavily,
        }
    }

    pub fn with_serper_api_key(api_key: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_key: Some(api_key),
            search_engine: SearchEngine::Serper,
        }
    }

    async fn search_duckduckgo(&self, query: &str, max_results: usize) -> Result<String, AgentError> {
        // DuckDuckGo Instant Answer API (free but limited)
        let url = format!(
            "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "rusty-smol-agent/1.0")
            .send()
            .await
            .map_err(|e| AgentError::ToolError {
                tool_name: "web_search".to_string(),
                message: format!("DuckDuckGo API request failed: {}", e),
            })?;

        let data: Value = response.json().await.map_err(|e| AgentError::ToolError {
            tool_name: "web_search".to_string(),
            message: format!("Failed to parse DuckDuckGo response: {}", e),
        })?;

        let mut results = Vec::new();

        // Extract instant answer
        if let Some(answer) = data["Answer"].as_str() {
            if !answer.is_empty() {
                results.push(format!("Instant Answer: {}", answer));
            }
        }

        // Extract abstract
        if let Some(abstract_text) = data["Abstract"].as_str() {
            if !abstract_text.is_empty() {
                results.push(format!("Abstract: {}", abstract_text));
            }
        }

        // Extract related topics
        if let Some(topics) = data["RelatedTopics"].as_array() {
            for (i, topic) in topics.iter().take(max_results.saturating_sub(results.len())).enumerate() {
                if let Some(text) = topic["Text"].as_str() {
                    if !text.is_empty() {
                        results.push(format!("Related {}: {}", i + 1, text));
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(format!("No specific results found for '{}'. You may want to try a more specific query or use a different search approach.", query))
        } else {
            Ok(format!("Search results for '{}':\n\n{}", query, results.join("\n\n")))
        }
    }

    async fn search_tavily(&self, query: &str, max_results: usize) -> Result<String, AgentError> {
        let api_key = self.api_key.as_ref().ok_or_else(|| AgentError::ToolError {
            tool_name: "web_search".to_string(),
            message: "Tavily API key not configured".to_string(),
        })?;

        let payload = json!({
            "api_key": api_key,
            "query": query,
            "search_depth": "basic",
            "include_answer": true,
            "include_images": false,
            "include_raw_content": false,
            "max_results": max_results
        });

        let response = self
            .client
            .post("https://api.tavily.com/search")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::ToolError {
                tool_name: "web_search".to_string(),
                message: format!("Tavily API request failed: {}", e),
            })?;

        let data: Value = response.json().await.map_err(|e| AgentError::ToolError {
            tool_name: "web_search".to_string(),
            message: format!("Failed to parse Tavily response: {}", e),
        })?;

        let mut results = Vec::new();

        // Extract answer if available
        if let Some(answer) = data["answer"].as_str() {
            if !answer.is_empty() {
                results.push(format!("Answer: {}", answer));
            }
        }

        // Extract search results
        if let Some(search_results) = data["results"].as_array() {
            for (i, result) in search_results.iter().enumerate() {
                if let (Some(title), Some(content), Some(url)) = (
                    result["title"].as_str(),
                    result["content"].as_str(),
                    result["url"].as_str(),
                ) {
                    results.push(format!(
                        "Result {}: {}\nContent: {}\nURL: {}",
                        i + 1,
                        title,
                        content,
                        url
                    ));
                }
            }
        }

        if results.is_empty() {
            Ok(format!("No results found for '{}'", query))
        } else {
            Ok(format!("Search results for '{}':\n\n{}", query, results.join("\n\n")))
        }
    }

    async fn search_serper(&self, query: &str, max_results: usize) -> Result<String, AgentError> {
        let api_key = self.api_key.as_ref().ok_or_else(|| AgentError::ToolError {
            tool_name: "web_search".to_string(),
            message: "Serper API key not configured".to_string(),
        })?;

        let payload = json!({
            "q": query,
            "num": max_results
        });

        let response = self
            .client
            .post("https://google.serper.dev/search")
            .header("X-API-KEY", api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::ToolError {
                tool_name: "web_search".to_string(),
                message: format!("Serper API request failed: {}", e),
            })?;

        let data: Value = response.json().await.map_err(|e| AgentError::ToolError {
            tool_name: "web_search".to_string(),
            message: format!("Failed to parse Serper response: {}", e),
        })?;

        let mut results = Vec::new();

        // Extract knowledge graph if available
        if let Some(knowledge_graph) = data["knowledgeGraph"].as_object() {
            if let (Some(title), Some(description)) = (
                knowledge_graph["title"].as_str(),
                knowledge_graph["description"].as_str(),
            ) {
                results.push(format!("Knowledge: {} - {}", title, description));
            }
        }

        // Extract organic results
        if let Some(organic) = data["organic"].as_array() {
            for (i, result) in organic.iter().enumerate() {
                if let (Some(title), Some(snippet), Some(link)) = (
                    result["title"].as_str(),
                    result["snippet"].as_str(),
                    result["link"].as_str(),
                ) {
                    results.push(format!(
                        "Result {}: {}\nSnippet: {}\nURL: {}",
                        i + 1,
                        title,
                        snippet,
                        link
                    ));
                }
            }
        }

        if results.is_empty() {
            Ok(format!("No results found for '{}'", query))
        } else {
            Ok(format!("Search results for '{}':\n\n{}", query, results.join("\n\n")))
        }
    }
}

impl Default for WebSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: "web_search".to_string(),
            description: "Search the web for current information on any topic. Useful for finding recent news, facts, or general information.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to look up on the web"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of search results to return (default: 5)",
                        "minimum": 1,
                        "maximum": 10
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<String, AgentError> {
        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::ToolError {
                tool_name: "web_search".to_string(),
                message: "Missing or invalid 'query' parameter".to_string(),
            })?;

        let max_results = arguments
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        if max_results == 0 || max_results > 10 {
            return Err(AgentError::ToolError {
                tool_name: "web_search".to_string(),
                message: "max_results must be between 1 and 10".to_string(),
            });
        }

        log::info!("Web search: '{}' (max_results: {})", query, max_results);

        match self.search_engine {
            SearchEngine::DuckDuckGo => self.search_duckduckgo(query, max_results).await,
            SearchEngine::Tavily => self.search_tavily(query, max_results).await,
            SearchEngine::Serper => self.search_serper(query, max_results).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_web_search_tool_metadata() {
        let tool = WebSearchTool::new();
        let metadata = tool.metadata();
        assert_eq!(metadata.name, "web_search");
        assert!(metadata.description.contains("Search the web"));
    }

    #[tokio::test]
    async fn test_web_search_missing_query() {
        let tool = WebSearchTool::new();
        let args = json!({"max_results": 3});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    // Note: Actual API tests would require real API keys and network access
    // These would be integration tests rather than unit tests
}