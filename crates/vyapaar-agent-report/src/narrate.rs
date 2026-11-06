//! LLM narrative narration for tearsheet reports (feature: `llm`).
//!
//! Streams the rendered Markdown report to any [`vyapaar_agent_llm::LlmProvider`]
//! via its chat-completions surface and collects a plain-English narrative
//! summary. Wired in behind the optional `llm` feature so the crate stays
//! dependency-light for embedders that only render reports.

use futures::StreamExt;
use vyapaar_agent_llm::{ChatMessage, ChatRequest, ChatStreamEvent, LlmError, LlmProvider};

use crate::TearsheetReport;

/// Errors that can occur while narrating a report.
#[derive(Debug, thiserror::Error)]
pub enum ReportLlmError {
    /// LLM transport / stream failure.
    #[error("llm: {0}")]
    Llm(#[from] LlmError),
    /// The provider returned no narrative text.
    #[error("empty narrative response")]
    Empty,
}

/// System prompt used for report narration.
pub const NARRATE_SYSTEM_PROMPT: &str = "\
You are Vyapaar's quantitative report narrator. Write a concise, plain-English \
narrative summary (2-4 sentences) of the trading performance report. Never \
invent figures that are not present in the report; if a figure is missing, \
say so plainly.";

/// Generate a plain-English narrative summary of a [`TearsheetReport`].
///
/// Uses the provider's chat-completions stream and returns the accumulated
/// assistant text. Deterministic replay of the narrative is possible through
/// the caller's audit recorder; this function itself only returns text.
pub async fn narrate_report(
    provider: &dyn LlmProvider,
    report: &TearsheetReport,
) -> Result<String, ReportLlmError> {
    let markdown = report.to_markdown();
    let request = ChatRequest {
        messages: vec![ChatMessage::user(format!(
            "Narrate this trading performance report:\n\n{markdown}"
        ))],
        system_prompt: Some(NARRATE_SYSTEM_PROMPT.to_string()),
        tools: Vec::new(),
        temperature: None,
        max_tokens: Some(300),
    };

    let mut stream = provider.chat_stream(&request).await?;
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        if let ChatStreamEvent::TextDelta { text: delta } = event? {
            text.push_str(&delta);
        }
    }

    if text.trim().is_empty() {
        return Err(ReportLlmError::Empty);
    }
    Ok(text)
}
