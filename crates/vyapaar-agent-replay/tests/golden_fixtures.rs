//! Golden-fixture oracle tests (0004 Phase 3).
//!
//! Every fixture in `tests/golden/omniroute_decision_fixtures.json` must:
//! 1. Re-parse through `DecisionParser` and match the recorded decision
//!    (guards against stale golden data), and
//! 2. Pass `RiskEngine::check_decision` — the execute-and-verify oracle — so
//!    only risk-approved fixtures accumulate in the set that doubles as the
//!    future local-SLM fine-tuning/eval dataset.

use chrono::Utc;
use vyapaar_agent_core::DecisionParser;
use vyapaar_agent_replay::{FixtureWorkload, OmnirouteFixture, OmnirouteFixtureSet};
use vyapaar_core::Decision;
use vyapaar_risk::{OtrConfig, RiskEngine, RiskLimits};

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/golden/omniroute_decision_fixtures.json"
);

#[test]
fn test_golden_fixtures_reparse_and_pass_risk_gate() {
    let set = OmnirouteFixtureSet::load_json(GOLDEN_PATH).expect("load golden fixtures");
    assert!(!set.is_empty(), "golden fixture set must not be empty");

    let parser = DecisionParser::new();
    for fixture in &set.fixtures {
        assert!(
            fixture.passes_risk_gate,
            "fixture must have been risk-approved at capture time"
        );

        let reparsed = parser
            .parse_str(&fixture.raw_response)
            .expect("raw response re-parses through the closed grammar");
        assert_eq!(
            reparsed, fixture.parsed_decision,
            "recorded decision must match a fresh parse of raw_response"
        );

        let mut risk_engine = RiskEngine::new(RiskLimits::default(), OtrConfig::default());
        risk_engine
            .check_decision(&fixture.parsed_decision, Utc::now())
            .expect("fixture passes the pre-trade risk gate");
    }
}

#[test]
fn test_accept_candidate_accepts_risk_approved_fixture() {
    let mut set = OmnirouteFixtureSet::new();
    let candidate = OmnirouteFixture::new(
        FixtureWorkload::TraderLoop,
        "prompt",
        "{\"action\":\"hold\",\"reason\":\"flat\"}",
        Decision::Hold {
            reason: Some("flat".into()),
        },
    );

    let accepted = set
        .accept_candidate(candidate, RiskLimits::default())
        .expect("hold fixture accepted");
    assert!(accepted.passes_risk_gate);
    assert_eq!(set.len(), 1);
}

#[test]
fn test_accept_candidate_rejects_malformed_fixture() {
    let mut set = OmnirouteFixtureSet::new();
    let candidate = OmnirouteFixture::new(
        FixtureWorkload::TraderLoop,
        "prompt",
        "not json at all",
        Decision::Hold { reason: None },
    );

    let err = set
        .accept_candidate(candidate, RiskLimits::default())
        .expect_err("malformed raw response must be rejected");
    assert!(err.to_string().contains("parsing"));
    assert!(set.is_empty());
}

#[test]
fn test_accept_candidate_rejects_stale_decision() {
    let mut set = OmnirouteFixtureSet::new();
    // raw_response says hold, but the recorded decision claims a buy order —
    // a stale/doctored fixture must be rejected by the re-parse oracle.
    let candidate = OmnirouteFixture::new(
        FixtureWorkload::TraderLoop,
        "prompt",
        "{\"action\":\"hold\",\"reason\":\"flat\"}",
        Decision::PlaceOrder {
            symbol: "TCS".to_string(),
            side: vyapaar_core::OrderSide::Buy,
            qty: vyapaar_core::Quantity(Default::default()),
            price_limit: None,
            algo_id: "ALGO_01".to_string(),
            rationale: None,
        },
    );

    let err = set
        .accept_candidate(candidate, RiskLimits::default())
        .expect_err("stale decision must be rejected");
    assert!(err.to_string().contains("checksum"));
    assert!(set.is_empty());
}
