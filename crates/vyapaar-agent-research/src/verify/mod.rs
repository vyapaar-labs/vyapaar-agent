//! Static strategy verification backend (§2.3 of plan 0002).
//!
//! `StrategyVerifier` performs a static source scan of a candidate strategy before any
//! backtest runs, rejecting strategies that exhibit lookahead bias, in-sample data
//! leakage, or NoFX safety violations (direct unsandboxed network/FS/process access).
//!
//! This is intentionally a dependency-light, deterministic scanner operating on the
//! strategy source string rather than a full Rust parser, so it runs in any profile
//! (including WASM) without a compiler toolchain.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A strategy submitted for static verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategyManifest {
    /// Strategy identifier.
    pub id: String,
    /// Strategy source code to scan.
    pub source_code: String,
    /// Instruments the strategy references.
    pub instruments: Vec<String>,
    /// Timeframe (e.g. "1min", "1day").
    pub timeframe: String,
    /// Whether the strategy uses machine learning.
    pub uses_ml: bool,
}

impl StrategyManifest {
    pub fn new(id: impl Into<String>, source_code: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source_code: source_code.into(),
            instruments: Vec::new(),
            timeframe: "1day".to_string(),
            uses_ml: false,
        }
    }

    pub fn with_instruments(mut self, instruments: Vec<String>) -> Self {
        self.instruments = instruments;
        self
    }

    pub fn with_timeframe(mut self, timeframe: impl Into<String>) -> Self {
        self.timeframe = timeframe.into();
        self
    }

    pub fn with_ml(mut self, uses_ml: bool) -> Self {
        self.uses_ml = uses_ml;
        self
    }
}

/// A single static verification violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Violation {
    /// Future information used while forming a signal (bar[i + k], k >= 0).
    LookaheadBias { detail: String, line: usize },
    /// A scaler / model fitted before the train/test split or on the full dataset.
    DataLeakage { detail: String, line: usize },
    /// Direct unsandboxed network / filesystem / process access.
    NoFXSafety { detail: String, line: usize },
}

/// Result of static verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    /// `true` when no violations were detected.
    pub clean: bool,
    /// All detected violations, in source order.
    pub violations: Vec<Violation>,
}

impl VerificationReport {
    /// A clean report with no violations.
    pub fn clean() -> Self {
        Self {
            clean: true,
            violations: Vec::new(),
        }
    }
}

/// Errors from static verification.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum VerificationError {
    /// One or more violations were found; see the attached report.
    #[error("verification failed: {} violation(s) found", .0.len())]
    ViolationsFound(Vec<Violation>),
    /// The manifest could not be parsed / normalized.
    #[error("parse error: {0}")]
    ParseError(String),
}

/// Static strategy verifier.
pub struct StrategyVerifier;

impl Default for StrategyVerifier {
    fn default() -> Self {
        Self
    }
}

impl StrategyVerifier {
    /// Statically verify a strategy manifest.
    ///
    /// Returns the clean `VerificationReport` when no violations are detected, or
    /// `VerificationError::ViolationsFound` otherwise.
    pub fn verify_manifest(
        &self,
        manifest: &StrategyManifest,
        _strict: bool,
    ) -> Result<VerificationReport, VerificationError> {
        let mut violations: Vec<Violation> = Vec::new();
        for (i, raw_line) in manifest.source_code.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let lineno = i + 1;
            if let Some(detail) = scan_lookahead_bias(line) {
                violations.push(Violation::LookaheadBias {
                    detail,
                    line: lineno,
                });
            }
            if let Some(detail) = scan_data_leakage(line, manifest.uses_ml) {
                violations.push(Violation::DataLeakage {
                    detail,
                    line: lineno,
                });
            }
            if let Some(detail) = scan_nofx_safety(line) {
                violations.push(Violation::NoFXSafety {
                    detail,
                    line: lineno,
                });
            }
        }
        let report = VerificationReport {
            clean: violations.is_empty(),
            violations,
        };
        if report.clean {
            Ok(report)
        } else {
            Err(VerificationError::ViolationsFound(
                report.violations.clone(),
            ))
        }
    }
}

/// Detect lookahead bias: accessing future bars relative to the current bar.
fn scan_lookahead_bias(line: &str) -> Option<String> {
    let l = line.to_ascii_lowercase();
    let patterns = [
        "i + ",
        "i+",
        "shift(-)",
        "future",
        "tomorrow",
        "lookahead",
        "iloc[i +",
        "iloc[i+",
        ".shift(-)",
    ];
    for p in &patterns {
        if l.contains(p) {
            return Some(format!("possible future bar access: '{line}'"));
        }
    }
    None
}

/// Detect data leakage: fitting on the full dataset before the train/test split.
fn scan_data_leakage(line: &str, uses_ml: bool) -> Option<String> {
    let l = line.to_ascii_lowercase();
    let always_leaky = ["train_test_split", "shuffle=true", "shuffle=true"];
    for p in &always_leaky {
        if l.contains(p) {
            return Some(format!("possible train/test leakage: '{line}'"));
        }
    }
    if uses_ml {
        let ml_fit = [".fit(", "fit_transform(", "standardscaler", "minmaxscaler"];
        for p in &ml_fit {
            if l.contains(p) {
                return Some(format!("possible ML fit-before-split leakage: '{line}'"));
            }
        }
    }
    None
}

/// Detect NoFX safety violations: direct unsandboxed I/O.
fn scan_nofx_safety(line: &str) -> Option<String> {
    let l = line.to_ascii_lowercase();
    let patterns = [
        "std::net",
        "tcpstream",
        "udpsocket",
        "reqwest",
        "ureq",
        "std::process::command",
        "command::new",
        "std::fs::",
        "fs::read",
        "std::env::var",
        "std::command",
    ];
    for p in &patterns {
        if l.contains(p) {
            return Some(format!("unsandboxed system access: '{line}'"));
        }
    }
    None
}

#[cfg(test)]
mod verify_unit_tests {
    use super::*;

    #[test]
    fn test_clean_strategy_passes() {
        let verifier = StrategyVerifier;
        let manifest = StrategyManifest::new(
            "sma_cross",
            r#"
            pub fn on_bar(bar: &Bar) -> Decision {
                if bar.close > bar.sma(20) {
                    return Decision::buy(symbol, qty);
                }
                Decision::hold()
            }
            "#,
        );
        let report = verifier.verify_manifest(&manifest, false).expect("clean");
        assert!(report.clean);
        assert!(report.violations.is_empty());
    }

    #[test]
    fn test_lookahead_bias_detected() {
        let verifier = StrategyVerifier;
        let manifest = StrategyManifest::new(
            "lookback",
            r#"
            let future_close = close[i + 1];
            let signal = future_close > open;
            "#,
        );
        let res = verifier.verify_manifest(&manifest, false);
        assert!(res.is_err());
        match res.unwrap_err() {
            VerificationError::ViolationsFound(v) => {
                assert!(!v.is_empty());
                assert!(
                    v.iter()
                        .any(|v| matches!(v, Violation::LookaheadBias { .. }))
                );
            }
            VerificationError::ParseError(m) => panic!("unexpected parse error: {m}"),
        }
    }

    #[test]
    fn test_data_leakage_detected_when_ml() {
        let verifier = StrategyVerifier;
        let manifest =
            StrategyManifest::new("ml_strategy", "let scaler = StandardScaler().fit(X);\n")
                .with_ml(true);
        let res = verifier.verify_manifest(&manifest, false);
        assert!(res.is_err());
        match res.unwrap_err() {
            VerificationError::ViolationsFound(v) => {
                assert!(v.iter().any(|v| matches!(v, Violation::DataLeakage { .. })));
            }
            VerificationError::ParseError(m) => panic!("unexpected parse error: {m}"),
        }
    }

    #[test]
    fn test_nofx_safety_detected() {
        let verifier = StrategyVerifier;
        let manifest = StrategyManifest::new("net", r#"let s = TcpStream::connect(addr);"#);
        let res = verifier.verify_manifest(&manifest, false);
        assert!(res.is_err());
        match res.unwrap_err() {
            VerificationError::ViolationsFound(v) => {
                assert!(v.iter().any(|v| matches!(v, Violation::NoFXSafety { .. })));
            }
            VerificationError::ParseError(m) => panic!("unexpected parse error: {m}"),
        }
    }
}
