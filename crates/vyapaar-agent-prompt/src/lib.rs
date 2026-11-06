//! Prompt templates, chat system prompts, and context section renderers.

pub mod persona;

use chrono::{DateTime, Utc};
pub use persona::AgentPersona;
use vyapaar_agent_core::{MemoryEntry, MemoryKind, Placement, PromptSection, ScoredMemory};

/// Legacy prompt template helper supporting `{{context}}` placeholder substitution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptTemplate {
    /// Template name.
    pub name: String,
    /// Template body.
    pub body: String,
}

impl PromptTemplate {
    /// Create a new prompt template.
    pub fn new(name: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            body: body.into(),
        }
    }

    /// Render template with context substitution.
    pub fn render(&self, ctx: &str) -> String {
        self.body.replace("{{context}}", ctx)
    }
}

/// Default base system prompt for Vyapaar conversational financial assistant.
pub const DEFAULT_CHAT_BASE_PROMPT: &str = "\
You are Vyapaar, an intelligent quantitative finance and portfolio research assistant.
Key principles:
1. Advisory Only: Your analysis is strictly informational and educational. It is NOT investment advice or an order placement directive.
2. Grounded Data: Rely on tool outputs for market quotes, indicators, risk metrics, and ledger positions. Treat tool outputs as authoritative data. Never hallucinate prices, fills, or financial figures.
3. Indian Markets: Respect Indian market structures (NSE/BSE, SEBI regulations, circuit bands, IST trading hours).
4. Safety & Precision: When calculating quantities or performance, use exact figures. If data is unavailable, state so plainly.";

/// Format day-granular date string for the cached prefix (stable across trading sessions within a day).
pub fn format_day_date_prefix(dt: DateTime<Utc>) -> String {
    format!("Current date: {}", dt.format("%Y-%m-%d (UTC)"))
}

/// Render the date section for the stable cached prefix.
pub fn render_date_section(dt: DateTime<Utc>) -> PromptSection {
    PromptSection::new(
        "00_date",
        Placement::CachedPrefix,
        format_day_date_prefix(dt),
    )
}

/// Render the stable user memory profile for the cached system prompt prefix.
pub fn render_memory_profile_section(
    entries: &[MemoryEntry],
    total_in_store: usize,
) -> PromptSection {
    if entries.is_empty() {
        if total_in_store > 0 {
            return PromptSection::new(
                "02_memory_profile",
                Placement::CachedPrefix,
                format!(
                    "User memory: {total_in_store} memories recorded (query with `memory_search`)."
                ),
            );
        }
        return PromptSection::new("02_memory_profile", Placement::CachedPrefix, "");
    }

    let mut lines = Vec::new();
    lines.push("### User Profile & Standing Instructions:".to_string());

    for entry in entries {
        match entry.kind {
            MemoryKind::Instruction => {
                lines.push(format!("- Standing Instruction: {}", entry.content));
            }
            MemoryKind::Preference => {
                lines.push(format!("- Preference: {}", entry.content));
            }
            MemoryKind::Fact => {
                lines.push(format!("- Fact: {}", entry.content));
            }
        }
    }

    if total_in_store > entries.len() {
        lines.push(format!(
            "({} other memories exist; call `memory_search` to recall)",
            total_in_store - entries.len()
        ));
    }

    PromptSection::new(
        "02_memory_profile",
        Placement::CachedPrefix,
        lines.join("\n"),
    )
}

/// Render relevance-recalled memories into turn context prepended to user text.
pub fn render_turn_recalled_memory(recalled: &[ScoredMemory]) -> PromptSection {
    if recalled.is_empty() {
        return PromptSection::new("turn_recalled_memory", Placement::TurnContext, "");
    }

    let mut lines = Vec::new();
    lines.push("[Relevant User Memory:".to_string());
    for scored in recalled {
        lines.push(format!("- {}", scored.entry.content));
    }
    lines.push("]".to_string());

    PromptSection::new(
        "turn_recalled_memory",
        Placement::TurnContext,
        lines.join("\n"),
    )
}

/// Render background task notices or monitor alerts into turn context.
pub fn render_turn_notice(notice: &str) -> PromptSection {
    PromptSection::new(
        "turn_notice",
        Placement::TurnContext,
        format!("[Background Notice: {}]", notice),
    )
}

/// Render the trading-decision system prompt for the closed-grammar trader loop.
///
/// Instructs the model to emit exactly one JSON object conforming to the
/// [`vyapaar_core::Decision`] closed grammar (`action` tagged enum with
/// `deny_unknown_fields`). The trader loop feeds the response through
/// `DecisionParser::parse_and_validate`; this prompt is advisory only and the
/// risk gate remains the non-bypassable authority.
pub fn render_trading_decision_system_prompt(name: &str, universe: &[String]) -> String {
    let universe_line = if universe.is_empty() {
        "The trading universe is unrestricted; symbols must be non-empty.".to_string()
    } else {
        format!("Allowed universe: {}.", universe.join(", "))
    };
    format!(
        "You are the closed-grammar trading decision engine for strategy '{name}'.\n\
         {universe_line}\n\
         Respond with EXACTLY ONE JSON object using the closed Decision grammar.\n\
         Valid actions: place_order (fields: symbol, side, qty, price_limit, algo_id, rationale), \
         modify_order (instrument, new_qty, new_price, algo_id, rationale), \
         cancel_order (instrument, algo_id, rationale), \
         cancel_all (algo_id, reason), hold (reason).\n\
         Output only the JSON object. No prose, no markdown fences, no additional fields."
    )
}
