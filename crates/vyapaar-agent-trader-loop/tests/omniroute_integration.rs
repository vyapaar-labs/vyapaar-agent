//! Live OmniRoute trader-loop integration test.
//!
//! Ignored by default (requires a running OmniRoute / OpenAI-compatible router
//! and `VYAPAAR_LLM_BASE_URL` / `VYAPAAR_LLM_MODEL` env vars). Run manually:
//!
//! ```text
//! VYAPAAR_LLM_BASE_URL=http://localhost:11434/v1 \
//! VYAPAAR_LLM_MODEL=gpt-4o-mini \
//! cargo test -p vyapaar-agent-trader-loop -- --ignored
//! ```

use std::sync::Arc;

use vyapaar_agent_llm::openai_provider_from_env;
use vyapaar_agent_trader_loop::{TraderConfig, TraderLoop};
use vyapaar_core::Decision;
use vyapaar_risk::{OtrConfig, RiskEngine, RiskLimits};

#[tokio::test]
#[ignore = "requires a running OmniRoute router and VYAPAAR_LLM_* env vars"]
async fn test_trader_loop_decision_against_omniroute() {
    let provider = openai_provider_from_env().expect("VYAPAAR_LLM_* env vars");
    let config = TraderConfig {
        name: "omniroute-int".to_string(),
        universe: vec!["TCS".to_string()],
        llm: Arc::new(provider),
        risk_limits: RiskLimits::default(),
    };
    let trader = TraderLoop::new(config);

    // Structural validation only (skips the risk gate): a live model must
    // produce a single closed-grammar Decision JSON that parses.
    let decision = trader.run_decision(None, None).await.expect("run_decision");
    assert!(matches!(
        decision,
        Decision::Hold { .. }
            | Decision::PlaceOrder { .. }
            | Decision::ModifyOrder { .. }
            | Decision::CancelOrder { .. }
            | Decision::CancelAll { .. }
    ));

    // Full pipeline through the pre-trade risk gate must also complete (a
    // model issuing an unregistered trade intent is expected to be refused,
    // which is still a successful pipeline run).
    let mut risk_engine = RiskEngine::new(RiskLimits::default(), OtrConfig::default());
    let result = trader.run(&mut risk_engine, chrono::Utc::now()).await;
    assert!(
        result.is_ok(),
        "risk gate result (accept or refusal) must be a valid outcome: {result:?}"
    );
}
