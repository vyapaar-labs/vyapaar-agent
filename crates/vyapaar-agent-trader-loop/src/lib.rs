//! Async trader loop per strategy instance.
//!
//! Coordinates the closed-grammar decision loop: builds a trading-decision
//! prompt via [`vyapaar_agent_prompt`], calls the configured [`LlmProvider`],
//! and pushes the raw response through [`DecisionParser`]'s validation before
//! the pre-trade [`vyapaar_risk::RiskEngine`] gate (the parser's validation step
//! is always wired in — nothing bypasses straight to the risk gate).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use vyapaar_agent_core::{DecisionError, DecisionParser};
use vyapaar_agent_llm::{LlmError, LlmProvider};
use vyapaar_agent_prompt::render_trading_decision_system_prompt;
use vyapaar_core::{Decision, Price, PriceBand};
use vyapaar_risk::{RiskEngine, RiskLimits, RiskRefusal};

/// Errors that can occur while running a trader loop.
#[derive(Debug, thiserror::Error)]
pub enum TraderError {
    /// LLM transport or parse failure.
    #[error("llm: {0}")]
    Llm(#[from] LlmError),
    /// Closed-grammar decision validation failure.
    #[error("decision: {0}")]
    Decision(#[from] DecisionError),
}

/// Configuration for an async trader loop.
pub struct TraderConfig {
    /// Strategy / loop identifier
    pub name: String,
    /// Trading universe symbols
    pub universe: Vec<String>,
    /// Language model provider
    pub llm: Arc<dyn LlmProvider>,
    /// Pre-trade risk limits
    pub risk_limits: RiskLimits,
}

/// Strategy trader loop coordinating LLM generation and risk gate evaluation.
pub struct TraderLoop {
    /// Loop configuration
    pub config: TraderConfig,
    /// Closed-grammar decision parser (validates every model response).
    pub parser: DecisionParser,
}

impl TraderLoop {
    /// Create a new trader loop with the specified configuration.
    pub fn new(config: TraderConfig) -> Self {
        Self {
            config,
            parser: DecisionParser::new(),
        }
    }

    /// Build the trading-decision system prompt for this loop.
    pub fn build_prompt(&self) -> String {
        render_trading_decision_system_prompt(&self.config.name, &self.config.universe)
    }

    /// Evaluates a parsed AI/LLM decision against the parser's validation and
    /// then the [`vyapaar_risk::RiskEngine`] pre-trade gate.
    #[allow(clippy::result_large_err)]
    pub fn evaluate_decision(
        &self,
        decision: &Decision,
        risk_engine: &mut RiskEngine,
        now: DateTime<Utc>,
    ) -> Result<(), RiskRefusal> {
        // Wire the parser's validation step in (0004 §3 Phase 1): a decision
        // must pass closed-grammar validation before the risk gate sees it.
        match self.parser.evaluate_with_risk(decision, risk_engine, now) {
            Ok(()) => Ok(()),
            Err(DecisionError::RiskRejected(refusal)) => Err(refusal),
            Err(other) => Err(RiskRefusal::InvalidOrder {
                error: other.to_string(),
            }),
        }
    }

    /// Run one closed-grammar decision turn: build prompt → generate → parse
    /// and validate structurally. Reference price / band grounding is optional
    /// and passed through when the caller has market context.
    pub async fn run_decision(
        &self,
        reference_price: Option<Price>,
        price_band: Option<&PriceBand>,
    ) -> Result<Decision, TraderError> {
        let prompt = self.build_prompt();
        tracing::info!(name = %self.config.name, "trader loop decision turn");
        let (_raw_response, decision) = self.config.llm.generate(&prompt).await?;
        self.parser
            .validate_decision(&decision, reference_price, price_band)?;
        Ok(decision)
    }

    /// Run one decision turn and push the result through the pre-trade risk gate.
    pub async fn run(
        &self,
        risk_engine: &mut RiskEngine,
        now: DateTime<Utc>,
    ) -> Result<Decision, TraderError> {
        let decision = self.run_decision(None, None).await?;
        self.parser
            .evaluate_with_risk(&decision, risk_engine, now)?;
        tracing::info!(name = %self.config.name, decision = ?decision, "decision passed risk gate");
        Ok(decision)
    }
}
