//! LLM-based text summarization
use crate::core_types::{Message, Role};
use crate::errors::AgentError;
use crate::llm::LLM;
use std::sync::Arc;
use log::{info, warn};

/// Summarizes the content of a message if it exceeds a specified token count.
///
/// This function uses the provided LLM to generate a concise summary of the message
/// content. It's designed to reduce the context size for large tool outputs or
/// other lengthy messages.
///
/// # Arguments
///
/// * `llm` - An `Arc<dyn LLM>` instance to use for generating the summary.
/// * `content` - The string content of the message to potentially summarize.
/// * `token_threshold` - The number of tokens above which the content should be summarized.
///
/// # Returns
///
/// A `Result` containing either the summarized content (if summarization was
/// performed) or the original content (if it was below the threshold).
pub async fn summarize_message_content(
    llm: Arc<dyn LLM>,
    content: &str,
    token_threshold: usize,
) -> Result<String, AgentError> {
    // A simple heuristic to estimate token count (4 chars/token)
    let estimated_tokens = content.len() / 4;

    if estimated_tokens <= token_threshold {
        return Ok(content.to_string());
    }

    info!(
        "Content with ~{} tokens exceeds threshold of {}, summarizing...",
        estimated_tokens, token_threshold
    );

    let prompt = format!(
        "Summarize the following content concisely. Focus on the key results and main points. \
        Do not omit important identifiers, names, or numbers. The summary should be significantly shorter than the original. \
        \n\nCONTENT:\n{}\n\nSUMMARY:",
        content
    );

    let messages = vec![Message {
        role: Role::System,
        content: prompt,
        tool_call_id: None,
        tool_calls: None,
    }];

    match llm.generate(messages, None).await {
        Ok(response) => {
            let summary = response.content.unwrap_or_else(|| {
                warn!("Summarization LLM call returned no content, using fallback.");
                "Content was too long and summarization failed.".to_string()
            });
            Ok(format!(
                "[Content summarized to fit context]:\n{}",
                summary
            ))
        }
        Err(e) => {
            warn!("Summarization LLM call failed: {}. Using fallback.", e);
            Ok(
                "[Content was too long and summarization failed. Please check logs.]".to_string()
            )
        }
    }
}
