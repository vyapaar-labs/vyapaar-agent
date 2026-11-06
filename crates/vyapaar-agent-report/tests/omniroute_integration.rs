//! Live OmniRoute report-narration integration test.
//!
//! Ignored by default (requires a running OmniRoute / OpenAI-compatible router
//! and `VYAPAAR_LLM_BASE_URL` / `VYAPAAR_LLM_MODEL` env vars). Run manually:
//!
//! ```text
//! VYAPAAR_LLM_BASE_URL=http://localhost:11434/v1 \
//! VYAPAAR_LLM_MODEL=gpt-4o-mini \
//! cargo test -p vyapaar-agent-report --features llm -- --ignored
//! ```

use vyapaar_agent_llm::openai_provider_from_env;
use vyapaar_agent_report::{Metric, TearsheetReport, narrate_report};

#[tokio::test]
#[ignore = "requires a running OmniRoute router and VYAPAAR_LLM_* env vars"]
async fn test_report_narrate_against_omniroute() {
    let provider = openai_provider_from_env().expect("VYAPAAR_LLM_* env vars");

    let mut report = TearsheetReport::new("TCS momentum");
    report.add_metric(Metric::new("Sharpe", "1.42"));
    report.add_metric(Metric::new("Max drawdown", "-8.3%"));

    let narrative = narrate_report(&provider, &report)
        .await
        .expect("narrative from live router");
    assert!(!narrative.trim().is_empty());
    assert!(narrative.contains("TCS"), "narrative mentions the strategy");
}
