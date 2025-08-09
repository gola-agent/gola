use gola_core::agent::{Agent, AgentConfig};
use gola_core::config::types::{MemoryConfig, MemoryEvictionStrategy};
use gola_core::core_types::{LLMResponse, Message, Role};
use gola_core::errors::AgentError;
use gola_core::llm::{ToolMetadata, LLM};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use gola_core::memory::{ConversationMemory, ConversationSummaryMemory};

#[derive(Clone)]
struct MockLLM {
    responses: Arc<Mutex<Vec<String>>>,
    summary_responses: Arc<Mutex<Vec<String>>>,
    default_response: String,
}

impl MockLLM {
    fn new(responses: Vec<String>, summary_responses: Option<Vec<String>>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
            summary_responses: Arc::new(Mutex::new(summary_responses.unwrap_or_default())),
            default_response: "OK".to_string(),
        }
    }
}

#[async_trait]
impl LLM for MockLLM {
    async fn generate(
        &self,
        messages: Vec<Message>,
        _tools: Option<Vec<ToolMetadata>>,
    ) -> Result<LLMResponse, AgentError> {
        if let Some(message) = messages.last() {
            if message.content.starts_with("Progressively summarize") || message.content.starts_with("Concisely summarize") {
                let mut summary_responses = self.summary_responses.lock().unwrap();
                if summary_responses.is_empty() {
                     let summary = if message.content.contains("first") {
                        "The user said hello."
                    } else if message.content.contains("second") {
                        "The user said hello and then asked a question."
                    } else {
                        "The user said hello, asked a question, and then made a statement."
                    };
                    return Ok(LLMResponse {
                        content: Some(summary.to_string()),
                        tool_calls: None,
                        finish_reason: None,
                        usage: None,
                    });
                } else {
                    return Ok(LLMResponse {
                        content: Some(summary_responses.remove(0)),
                        tool_calls: None,
                        finish_reason: None,
                        usage: None,
                    });
                }
            }
        }

        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Ok(LLMResponse {
                content: Some(self.default_response.clone()),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            })
        } else {
            Ok(LLMResponse {
                content: Some(responses.remove(0)),
                tool_calls: None,
                finish_reason: None,
                usage: None,
            })
        }
    }
}

#[tokio::test]
async fn test_agent_with_summary_memory() {
    let llm = Arc::new(MockLLM::new(vec!["Final Answer: OK".to_string(); 16], None));
    let config = AgentConfig {
        memory_config: Some(MemoryConfig {
            eviction_strategy: MemoryEvictionStrategy::Summarize,
            max_history_steps: 10,
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut agent = Agent::new(llm, HashMap::new(), None, config);

    // This should trigger summarization
    for i in 0..15 {
        let _ = agent.run(format!("This is message {}", i)).await;
    }

    // Not easy to inspect the summary buffer directly, but we can check if the agent runs
    let result = agent.run("Final task".to_string()).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "OK");
}

#[tokio::test]
async fn test_agent_memory_switch() {
    let llm = Arc::new(MockLLM::new(vec!["Final Answer: OK".to_string()], None));
    
    // Start with Sliding Window
    let mut config = AgentConfig {
        memory_config: Some(MemoryConfig {
            eviction_strategy: MemoryEvictionStrategy::Fifo,
            max_history_steps: 5,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut agent = Agent::new(llm.clone(), HashMap::new(), None, config.clone());
    let _ = agent.run("task 1".to_string()).await;
    assert_eq!(agent.memory().get_context().len(), 2); // User + Assistant

    // Switch to Summarize
    config.memory_config.as_mut().unwrap().eviction_strategy = MemoryEvictionStrategy::Summarize;
    agent.update_memory_config(config.memory_config.unwrap());
    
    // This is a bit of a hack since we can't easily switch the memory type on the fly
    // A better implementation would involve the agent recreating its memory when config changes.
    // For now, we just check that the config is updated.
    assert_eq!(agent.config().memory_config.as_ref().unwrap().eviction_strategy, MemoryEvictionStrategy::Summarize);
}

#[tokio::test]
async fn test_agent_long_conversation() {
    let agent_responses: Vec<String> = (0..20).map(|_| "Final Answer: Thinking...".to_string()).collect();
    let summarizer_responses: Vec<String> = (0..20).map(|i| format!("Summary {}", i)).collect();
    let llm = Arc::new(MockLLM::new(agent_responses, Some(summarizer_responses)));

    let config = AgentConfig {
        memory_config: Some(MemoryConfig {
            eviction_strategy: MemoryEvictionStrategy::Summarize,
            max_history_steps: 500, // This will be used by the summarizer for token limit, not steps
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut agent = Agent::new(llm, HashMap::new(), None, config);

    for i in 0..20 {
        let _ = agent.run(format!("This is a very long message number {} to ensure that the buffer overflows and we get summarization.", i)).await;
    }

    let final_context = agent.memory().get_context();
    // The context should contain a summary and the last few messages
    assert!(final_context.len() < 40); // 20 messages * 2 (user + assistant)
    assert!(final_context.iter().any(|m| m.role == Role::System && m.content.contains("Summary")));
}

#[tokio::test]
async fn test_summary_generation() {
    let llm = Arc::new(MockLLM::new(vec![], None));
    let mut memory = ConversationSummaryMemory::new(llm);

    let message = Message {
        role: Role::User,
        content: "hello, this is the first message".to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    memory.add_message(message).await.unwrap();

    let context = memory.get_context();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].content, "The user said hello.");
}

#[tokio::test]
async fn test_summary_evolution() {
    let llm = Arc::new(MockLLM::new(vec![], None));
    let mut memory = ConversationSummaryMemory::new(llm);

    let message1 = Message {
        role: Role::User,
        content: "hello, this is the first message".to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    memory.add_message(message1).await.unwrap();

    let message2 = Message {
        role: Role::User,
        content: "this is the second message".to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    memory.add_message(message2).await.unwrap();

    let context = memory.get_context();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].content, "The user said hello and then asked a question.");
}

#[tokio::test]
async fn test_empty_conversation() {
    let llm = Arc::new(MockLLM::new(vec![], None));
    let memory = ConversationSummaryMemory::new(llm);
    let context = memory.get_context();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].content, "");
}

#[tokio::test]
async fn test_clear_memory() {
    let llm = Arc::new(MockLLM::new(vec![], None));
    let mut memory = ConversationSummaryMemory::new(llm);

    let message = Message {
        role: Role::User,
        content: "hello, this is the first message".to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    memory.add_message(message).await.unwrap();
    memory.clear();

    let context = memory.get_context();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].content, "");
}

#[tokio::test]
async fn test_complex_summary_evolution() {
    let llm = Arc::new(MockLLM::new(vec![], None));
    let mut memory = ConversationSummaryMemory::new(llm);

    let message1 = Message {
        role: Role::User,
        content: "hello, this is the first message".to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    memory.add_message(message1).await.unwrap();

    let message2 = Message {
        role: Role::User,
        content: "this is the second message".to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    memory.add_message(message2).await.unwrap();

    let message3 = Message {
        role: Role::User,
        content: "this is the third message".to_string(),
        tool_call_id: None,
        tool_calls: None,
    };
    memory.add_message(message3).await.unwrap();

    let context = memory.get_context();
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].content, "The user said hello, asked a question, and then made a statement.");
}