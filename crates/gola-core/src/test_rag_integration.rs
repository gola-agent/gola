//! Integration test for RAG functionality

#[cfg(test)]
mod rag_integration_tests {
    use crate::agent::{Agent, AgentConfig};
    use crate::core_types::{LLMResponse, Message};
    use crate::errors::AgentError;
    use crate::guardrails::AuthorizationMode;
    use crate::llm::{ToolMetadata, LLM};
    use crate::rag::{Rag, RagSystem, RagConfig, RagDocument};
    use crate::tools::{Tool, rag_search_tool::RagSearchTool};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct MockLLM {
        responses: Mutex<Vec<String>>,
    }

    impl MockLLM {
        fn new(responses: Vec<String>) -> Self {
            let mut reversed = responses;
            reversed.reverse(); // So we can pop from the end
            Self {
                responses: Mutex::new(reversed),
            }
        }
    }

    #[async_trait]
    impl LLM for MockLLM {
        async fn generate(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<ToolMetadata>>,
        ) -> Result<LLMResponse, AgentError> {
            let response = self.responses.lock().unwrap().pop()
                .unwrap_or_else(|| "No more responses".to_string());
            
            Ok(LLMResponse {
                content: Some(response),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            })
        }
    }

    #[tokio::test]
    async fn test_agent_with_rag_integration() {
        let mut rag_system = RagSystem::new_with_dummy();
        let documents = vec![
            RagDocument::new(
                "Rust is a systems programming language focused on safety and performance.".to_string(),
                "rust.md".to_string(),
            ),
            RagDocument::new(
                "Machine learning enables computers to learn without explicit programming.".to_string(),
                "ml.md".to_string(),
            ),
        ];
        rag_system.add_documents(documents).await.unwrap();

        let mock_llm = Arc::new(MockLLM::new(vec![
            "Final Answer: Based on the retrieved context, Rust is a systems programming language focused on safety and performance.".to_string(),
        ]));

        let config = AgentConfig {
            max_steps: 3,
            enable_rag: true,
            rag_config: Some(RagConfig::default()),
            system_prompt: None,
            memory_config: None,
            authorization_mode: AuthorizationMode::default(),
        };

        let mut tools: HashMap<String, Arc<dyn crate::tools::Tool>> = HashMap::new();
        let rag_search_tool = Arc::new(RagSearchTool::new(Arc::new(rag_system) as Arc<dyn crate::rag::Rag>));
        tools.insert("rag_search".to_string(), rag_search_tool.clone());

        let mut agent_rag_system = RagSystem::new_with_dummy();
        let agent_documents = vec![
            RagDocument::new(
                "Rust is a systems programming language focused on safety and performance.".to_string(),
                "rust.md".to_string(),
            ),
        ];
        agent_rag_system.add_documents(agent_documents).await.unwrap();

        let mut agent = Agent::with_rag(
            mock_llm,
            tools,
            None,
            config,
            Box::new(agent_rag_system),
        );

        assert!(agent.is_rag_enabled());

        let result = agent.run("What is Rust programming language?".to_string()).await;
        assert!(result.is_ok());

        let answer = result.unwrap();
        assert!(answer.contains("Rust"));
        assert!(answer.contains("systems programming") || answer.contains("safety"));

        let history = agent.history().get_history();
        assert!(!history.is_empty());
        
        if let Some(first_step) = history.first() {
            match first_step {
                crate::core_types::HistoryStep::UserTask(task) => {
                    assert!(task.contains("What is Rust programming language?"));
                    // The task might be enhanced with RAG context
                }
                _ => panic!("First step should be UserTask"),
            }
        }
    }

    #[tokio::test]
    async fn test_rag_search_tool_integration() {
        let mut rag_system = RagSystem::new_with_dummy();
        let documents = vec![
            RagDocument::new(
                "Vector databases store high-dimensional vectors for similarity search.".to_string(),
                "vectors.md".to_string(),
            ),
        ];
        rag_system.add_documents(documents).await.unwrap();

        let rag_search_tool = RagSearchTool::new(Arc::new(rag_system));

        let metadata = rag_search_tool.metadata();
        assert_eq!(metadata.name, "rag_search");
        assert!(metadata.description.contains("Search") || metadata.description.contains("knowledge"));

        let args = serde_json::json!({
            "query": "vector databases",
            "top_k": 3
        });

        let result = rag_search_tool.execute(args).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.contains("relevant documents") || response.contains("vector"));
    }

    #[tokio::test]
    async fn test_agent_without_rag() {
        // Create mock LLM
        let mock_llm = Arc::new(MockLLM::new(vec![
            "Final Answer: I don't have specific information about that topic.".to_string(),
        ]));

        let config = AgentConfig {
            max_steps: 3,
            enable_rag: false,
            rag_config: None,
            system_prompt: None,
            memory_config: None,
            authorization_mode: AuthorizationMode::default(),
        };

        let mut agent = Agent::new(mock_llm, HashMap::new(), None, config);

        assert!(!agent.is_rag_enabled());

        let result = agent.run("What is machine learning?".to_string()).await;
        assert!(result.is_ok());

        let answer = result.unwrap();
        assert!(answer.contains("don't have specific information"));
    }

    #[tokio::test]
    async fn test_rag_enable_disable() {
        let mock_llm = Arc::new(MockLLM::new(vec![]));
        let config = AgentConfig::default();
        let mut agent = Agent::new(mock_llm, HashMap::new(), None, config);

        assert!(!agent.is_rag_enabled());

        let rag_system = Box::new(RagSystem::new_with_dummy());
        agent.enable_rag(rag_system);
        assert!(agent.is_rag_enabled());

        agent.disable_rag();
        assert!(!agent.is_rag_enabled());
    }
}
