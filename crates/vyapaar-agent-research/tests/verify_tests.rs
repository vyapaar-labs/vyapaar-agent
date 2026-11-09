//! Integration tests for the static strategy verification backend
//! (`vyapaar-agent-research::verify`). All tests use the public API.

use vyapaar_agent_research::verify::{
    StrategyManifest, StrategyVerifier, VerificationError, VerificationReport, Violation,
};

fn clean_report(report: VerificationReport) {
    assert!(report.clean);
    assert!(report.violations.is_empty());
}

#[test]
fn test_clean_strategy_passes() {
    let verifier = StrategyVerifier;
    let manifest = StrategyManifest::new(
        "sma_cross",
        r#"
        pub fn on_bar(bar: &Bar) -> Decision {
            if bar.close > bar.sma(20) {
                return Decision::buy("NSE:RELIANCE-EQ", 50);
            }
            Decision::hold()
        }
        "#,
    );
    let report = verifier.verify_manifest(&manifest, false).expect("clean");
    clean_report(report);
}

#[test]
fn test_clean_strategy_strict_passes() {
    let verifier = StrategyVerifier;
    let manifest = StrategyManifest::new(
        "sma_cross",
        r#"
        pub fn on_bar(bar: &Bar) -> Decision {
            if bar.close > bar.sma(20) {
                return Decision::buy("NSE:RELIANCE-EQ", 50);
            }
            Decision::hold()
        }
        "#,
    );
    let report = verifier
        .verify_manifest(&manifest, true)
        .expect("clean strict");
    clean_report(report);
}

#[test]
fn test_lookahead_bias_rejected() {
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
fn test_data_leakage_ml_fit_rejected() {
    let verifier = StrategyVerifier;
    let manifest = StrategyManifest::new("ml_strategy", "let scaler = StandardScaler().fit(X);\n")
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
fn test_data_leakage_non_ml_fit_allowed() {
    let verifier = StrategyVerifier;
    let manifest =
        StrategyManifest::new("indicator", "let s = StandardScaler().fit(X);\n").with_ml(false);
    // Non-ML `.fit` is not flagged by the scanner.
    let report = verifier
        .verify_manifest(&manifest, false)
        .expect("clean non-ml");
    clean_report(report);
}

#[test]
fn test_train_test_split_leakage_rejected() {
    let verifier = StrategyVerifier;
    let manifest = StrategyManifest::new("split", "let s = train_test_split(X, y);\n");
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
fn test_nofx_network_rejected() {
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

#[test]
fn test_nofx_filesystem_rejected() {
    let verifier = StrategyVerifier;
    let manifest = StrategyManifest::new("fs", r#"let c = std::fs::read_to_string(path);"#);
    let res = verifier.verify_manifest(&manifest, false);
    assert!(res.is_err());
    match res.unwrap_err() {
        VerificationError::ViolationsFound(v) => {
            assert!(v.iter().any(|v| matches!(v, Violation::NoFXSafety { .. })));
        }
        VerificationError::ParseError(m) => panic!("unexpected parse error: {m}"),
    }
}

#[test]
fn test_report_clean_helper() {
    let report = VerificationReport::clean();
    assert!(report.clean);
    assert!(report.violations.is_empty());
}

#[test]
fn test_manifest_builder() {
    let manifest = StrategyManifest::new("s", "src")
        .with_instruments(vec!["NSE:RELIANCE-EQ".to_string()])
        .with_timeframe("1min")
        .with_ml(true);
    assert_eq!(manifest.id, "s");
    assert_eq!(manifest.source_code, "src");
    assert_eq!(manifest.instruments, vec!["NSE:RELIANCE-EQ"]);
    assert_eq!(manifest.timeframe, "1min");
    assert!(manifest.uses_ml);
}
