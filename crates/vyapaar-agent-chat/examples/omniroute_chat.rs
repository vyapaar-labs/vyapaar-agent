//! End-to-end chat assistant against a live OmniRoute (or any OpenAI-compatible)
//! endpoint configured through `VYAPAAR_LLM_*` env vars.
//!
//! This is the demo/bootstrap entry point for the chat workload's real
//! provider injection (0004 Phase 2): the turn loop and tool dispatch in
//! `vyapaar-agent-chat` already run against `Arc<dyn LlmProvider>`; the only
//! change needed was constructing the agent with the env-configured
//! [`OpenAiCompatibleProvider`].
//!
//! ```text
//! VYAPAAR_LLM_BASE_URL=http://localhost:11434/v1 \
//! VYAPAAR_LLM_MODEL=gpt-4o-mini \
//! cargo run -p vyapaar-agent-chat --example omniroute_chat -- "Summarise NIFTY 50 in one line"
//! ```

use std::sync::Arc;

use vyapaar_agent_chat::{
    ChatAgent, InMemorySessionStore, SessionEvent, SessionSelector, ToolRegistry, run_turn_headless,
};
use vyapaar_agent_core::{RuntimeMode, SessionScope};
use vyapaar_agent_llm::openai_provider_from_env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Give a one-line overview of NIFTY 50 today.".to_string());

    let provider = Arc::new(openai_provider_from_env()?);
    let store = Arc::new(InMemorySessionStore::new());
    let agent = ChatAgent::builder(provider, ToolRegistry::new(), store.clone()).build()?;

    let scope = SessionScope::new("omniroute-demo", RuntimeMode::Paper);
    let (_outcome, events) = run_turn_headless(&agent, SessionSelector::New(scope), &input).await?;

    for event in events {
        if let SessionEvent::TextDelta { text, .. } = event {
            print!("{text}");
        }
    }
    println!();
    Ok(())
}
