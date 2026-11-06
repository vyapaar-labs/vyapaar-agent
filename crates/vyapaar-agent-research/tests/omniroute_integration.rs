//! Live OmniRoute research-loop integration test.
//!
//! Ignored by default (requires a running OmniRoute / OpenAI-compatible router
//! and `VYAPAAR_LLM_BASE_URL` / `VYAPAAR_LLM_MODEL` env vars). Run manually:
//!
//! ```text
//! VYAPAAR_LLM_BASE_URL=http://localhost:11434/v1 \
//! VYAPAAR_LLM_MODEL=gpt-4o-mini \
//! cargo test -p vyapaar-agent-research --features llm -- --ignored
//! ```

use std::sync::Arc;

use rust_decimal_macros::dec;
use vyapaar_agent_llm::openai_provider_from_env;
use vyapaar_agent_research::enhancer::FailureRegimeDiagnostic;
use vyapaar_agent_research::{MarketRegime, ResearchLlm};

fn mean_reversion_diagnostic() -> FailureRegimeDiagnostic {
    FailureRegimeDiagnostic {
        strategy_name: "intraday_momentum".to_string(),
        failure_regime: MarketRegime::MeanReversion,
        drawdown_pct: dec!(12.4),
        loss_rate: dec!(0.62),
        max_loss_streak: 5,
        loss_clusters: Vec::new(),
        avg_mae: dec!(0.8),
        avg_mfe: dec!(1.1),
        trade_efficiency: dec!(-0.2),
        diagnosis_summary: "Strategy suffers losses in sideways chop".to_string(),
    }
}

#[tokio::test]
#[ignore = "requires a running OmniRoute router and VYAPAAR_LLM_* env vars"]
async fn test_research_llm_suggestion_against_omniroute() {
    let provider = Arc::new(openai_provider_from_env().expect("VYAPAAR_LLM_* env vars"));
    let research_llm = ResearchLlm::new(provider);

    let diagnostic = mean_reversion_diagnostic();
    let suggestion = research_llm
        .suggest_enhancement(&diagnostic)
        .await
        .expect("suggestion from live router");
    assert!(!suggestion.trim().is_empty());
}
