//! Async trader loop per strategy instance.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use vyapaar_agent_llm::{LlmError, LlmProvider};
use vyapaar_core::Decision;
use vyapaar_risk::{RiskEngine, RiskLimits, RiskRefusal};

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
}

impl TraderLoop {
    /// Create a new trader loop with the specified configuration.
    pub fn new(config: TraderConfig) -> Self {
        Self { config }
    }

    /// Evaluates an AI/LLM decision directly against [`vyapaar_risk::RiskEngine`].
    #[allow(clippy::result_large_err)]
    pub fn evaluate_decision(
        &self,
        decision: &Decision,
        risk_engine: &mut RiskEngine,
        now: DateTime<Utc>,
    ) -> Result<(), RiskRefusal> {
        risk_engine.check_decision(decision, now)
    }

    /// Start and run the async trader loop.
    pub async fn run(&self) -> Result<(), LlmError> {
        tracing::info!(name = %self.config.name, "starting trader loop");
        Ok(())
    }
}
