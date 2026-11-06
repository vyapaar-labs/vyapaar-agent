//! Golden (prompt → `Decision`) fixtures captured from live OmniRoute runs.
//!
//! Phase 3 of 0004: fixtures double as the fine-tuning / eval dataset for the
//! future local-SLM stack. A fixture is only accepted after the raw response
//! re-parses through [`vyapaar_agent_core::DecisionParser`] and passes
//! [`vyapaar_risk::RiskEngine::check_decision`] — the "execute-and-verify"
//! oracle the platform already owns.

use std::path::Path;

use serde::{Deserialize, Serialize};
use vyapaar_agent_core::DecisionParser;
use vyapaar_core::Decision;
use vyapaar_risk::{OtrConfig, RiskEngine, RiskLimits};

use crate::error::ReplayError;

/// Which of the four LLM workloads produced the fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureWorkload {
    /// Closed-grammar trader decision loop.
    TraderLoop,
    /// Conversational chat assistant.
    Chat,
    /// Autonomous research loop (enhancer suggestion prompt).
    Research,
    /// Report narrative narration.
    Report,
}

/// One captured (prompt → decision) fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmnirouteFixture {
    /// Workload that produced the fixture.
    pub workload: FixtureWorkload,
    /// Exact prompt sent to the model.
    pub prompt: String,
    /// Raw model completion.
    pub raw_response: String,
    /// Closed-grammar decision parsed from `raw_response`.
    pub parsed_decision: Decision,
    /// Whether the fixture passed the pre-trade risk gate when captured.
    pub passes_risk_gate: bool,
}

impl OmnirouteFixture {
    /// Build a fixture from a raw live capture, recording parse + risk verdict.
    pub fn new(
        workload: FixtureWorkload,
        prompt: impl Into<String>,
        raw_response: impl Into<String>,
        parsed_decision: Decision,
    ) -> Self {
        Self {
            workload,
            prompt: prompt.into(),
            raw_response: raw_response.into(),
            parsed_decision,
            passes_risk_gate: false,
        }
    }

    /// Serialize as pretty JSON.
    pub fn to_json_pretty(&self) -> Result<String, ReplayError> {
        serde_json::to_string_pretty(self).map_err(|e| ReplayError::Json(e.to_string()))
    }

    /// Deserialize from JSON.
    pub fn from_json_str(json: &str) -> Result<Self, ReplayError> {
        serde_json::from_str(json).map_err(|e| ReplayError::Json(e.to_string()))
    }
}

/// An ordered set of accepted golden fixtures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmnirouteFixtureSet {
    /// Captured fixtures.
    pub fixtures: Vec<OmnirouteFixture>,
}

impl OmnirouteFixtureSet {
    /// Create an empty fixture set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of fixtures in the set.
    pub fn len(&self) -> usize {
        self.fixtures.len()
    }

    /// `true` when the set holds no fixtures.
    pub fn is_empty(&self) -> bool {
        self.fixtures.is_empty()
    }

    /// Serialize as pretty JSON.
    pub fn to_json_pretty(&self) -> Result<String, ReplayError> {
        serde_json::to_string_pretty(self).map_err(|e| ReplayError::Json(e.to_string()))
    }

    /// Deserialize from JSON.
    pub fn from_json_str(json: &str) -> Result<Self, ReplayError> {
        serde_json::from_str(json).map_err(|e| ReplayError::Json(e.to_string()))
    }

    /// Load a fixture set from a JSON file.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ReplayError> {
        let contents = std::fs::read_to_string(path).map_err(|e| ReplayError::Io(e.to_string()))?;
        Self::from_json_str(&contents)
    }

    /// Persist the fixture set as pretty JSON.
    pub fn save_json(&self, path: impl AsRef<Path>) -> Result<(), ReplayError> {
        let contents = self.to_json_pretty()?;
        std::fs::write(path, contents).map_err(|e| ReplayError::Io(e.to_string()))
    }

    /// Append a candidate fixture, accepting it only after it passes the
    /// pre-trade risk gate (`RiskEngine::check_decision`) with the given
    /// limits. Decisions that parse but are refused by the gate are rejected
    /// with [`ReplayError::RiskRejected`] — malformed input is rejected with
    /// [`ReplayError::DecisionParse`].
    ///
    /// `workload_1` (`TraderLoop`) fixtures are checked against a fresh
    /// [`RiskEngine`] built from `limits`; other workloads' decision payloads
    /// go through the same gate for parity.
    pub fn accept_candidate(
        &mut self,
        fixture: OmnirouteFixture,
        limits: RiskLimits,
    ) -> Result<&OmnirouteFixture, ReplayError> {
        // Re-parse the raw response to prove the recorded decision is what the
        // grammar yields today (guards against stale golden data).
        let reparsed = DecisionParser::new()
            .parse_str(&fixture.raw_response)
            .map_err(|e| ReplayError::DecisionParse(e.to_string()))?;
        if reparsed != fixture.parsed_decision {
            let expected = serde_json::to_string(&fixture.parsed_decision)
                .unwrap_or_else(|_| format!("{:?}", fixture.parsed_decision));
            let actual =
                serde_json::to_string(&reparsed).unwrap_or_else(|_| format!("{reparsed:?}"));
            return Err(ReplayError::ChecksumMismatch {
                record_id: fixture.workload.to_string(),
                expected,
                actual,
            });
        }

        let mut risk_engine = RiskEngine::new(limits, OtrConfig::default());
        risk_engine
            .check_decision(&fixture.parsed_decision, chrono::Utc::now())
            .map_err(|refusal| ReplayError::RiskRejected(refusal.to_string()))?;

        let mut accepted = fixture;
        accepted.passes_risk_gate = true;
        self.fixtures.push(accepted);
        Ok(self.fixtures.last().unwrap())
    }
}

impl std::fmt::Display for FixtureWorkload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::TraderLoop => "trader_loop",
            Self::Chat => "chat",
            Self::Research => "research",
            Self::Report => "report",
        };
        f.write_str(name)
    }
}
