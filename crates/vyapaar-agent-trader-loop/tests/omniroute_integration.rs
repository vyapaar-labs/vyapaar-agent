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
//!
//! Set `VYAPAAR_WRITE_FIXTURES=1` to append the live capture to the golden
//! fixture set in `vyapaar-agent-replay/tests/golden/` (0004 Phase 3 — the
//! fixture is only accepted after it passes the pre-trade risk gate).

use std::path::PathBuf;
use std::sync::Arc;

use vyapaar_agent_core::DecisionParser;
use vyapaar_agent_llm::{LlmProvider, openai_provider_from_env};
use vyapaar_agent_replay::{FixtureWorkload, OmnirouteFixture, OmnirouteFixtureSet};
use vyapaar_agent_trader_loop::{TraderConfig, TraderLoop};
use vyapaar_core::Decision;
use vyapaar_risk::{OtrConfig, RiskEngine, RiskLimits};

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../vyapaar-agent-replay/tests/golden/omniroute_decision_fixtures.json"
);

#[tokio::test]
#[ignore = "requires a running OmniRoute router and VYAPAAR_LLM_* env vars"]
async fn test_trader_loop_decision_against_omniroute() {
    let provider = Arc::new(openai_provider_from_env().expect("VYAPAAR_LLM_* env vars"));
    let config = TraderConfig {
        name: "omniroute-int".to_string(),
        universe: vec!["TCS".to_string()],
        llm: provider.clone(),
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

    // Capture the live (prompt -> decision) pair as a golden fixture, accepted
    // only after the risk gate oracle approves it.
    let prompt = trader.build_prompt();
    let (raw_response, _) = provider.generate(&prompt).await.expect("generate");
    let parsed = DecisionParser::new()
        .parse_str(&raw_response)
        .expect("live response parses through closed grammar");

    let mut set = OmnirouteFixtureSet::new();
    set.accept_candidate(
        OmnirouteFixture::new(FixtureWorkload::TraderLoop, &prompt, &raw_response, parsed),
        RiskLimits::default(),
    )
    .expect("captured fixture passes the risk gate oracle");

    let pretty = set.to_json_pretty().expect("serialize fixture");
    println!("captured fixture:\n{pretty}");

    if std::env::var("VYAPAAR_WRITE_FIXTURES").is_ok_and(|v| v == "1") {
        let mut on_disk = match OmnirouteFixtureSet::load_json(GOLDEN_PATH) {
            Ok(existing) => existing,
            Err(_) => OmnirouteFixtureSet::new(),
        };
        for fixture in set.fixtures {
            on_disk
                .accept_candidate(fixture, RiskLimits::default())
                .expect("append to golden set");
        }
        let path = PathBuf::from(GOLDEN_PATH);
        on_disk.save_json(&path).expect("write golden fixtures");
        println!("wrote golden fixtures to {}", path.display());
    }
}
