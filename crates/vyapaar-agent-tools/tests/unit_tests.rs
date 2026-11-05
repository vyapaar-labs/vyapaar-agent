//! Unit tests for `vyapaar-agent-tools`.

use vyapaar_agent_tools::{
    AgentTool, RegimeCheckTool, RiskInspectionTool, ScreenerQueryTool, ToolCall, ToolError,
    ToolRegistry,
};

#[test]
fn registry_resolves_tools_by_name() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ScreenerQueryTool));
    registry.register(Box::new(RegimeCheckTool));
    registry.register(Box::new(RiskInspectionTool));

    assert_eq!(
        registry.names(),
        vec!["screener.query", "regime.check", "risk.inspect"]
    );
    assert!(registry.get("screener.query").is_some());
    assert!(registry.get("nope").is_none());
}

#[test]
fn screener_query_validates_universe_and_metric() {
    let tool = ScreenerQueryTool;
    let out = tool
        .execute(&ToolCall::new(
            "screener.query",
            serde_json::json!({ "universe": ["RELIANCE", "TCS"], "metric": "rsi_14" }),
        ))
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["universe_size"], 2);
    assert_eq!(out.payload["metric"], "rsi_14");
}

#[test]
fn screener_query_rejects_missing_metric() {
    let tool = ScreenerQueryTool;
    let err = tool
        .execute(&ToolCall::new(
            "screener.query",
            serde_json::json!({ "universe": ["RELIANCE"] }),
        ))
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArguments { .. }));
}

#[test]
fn screener_query_rejects_empty_universe() {
    let tool = ScreenerQueryTool;
    let err = tool
        .execute(&ToolCall::new(
            "screener.query",
            serde_json::json!({ "universe": [], "metric": "rsi_14" }),
        ))
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArguments { .. }));
}

#[test]
fn regime_check_accepts_known_regimes_only() {
    let tool = RegimeCheckTool;
    let out = tool
        .execute(&ToolCall::new(
            "regime.check",
            serde_json::json!({ "regime": "trending" }),
        ))
        .unwrap();
    assert!(out.ok);

    let err = tool
        .execute(&ToolCall::new(
            "regime.check",
            serde_json::json!({ "regime": "sideways" }),
        ))
        .unwrap_err();
    assert!(matches!(err, ToolError::InvalidArguments { .. }));
}

#[test]
fn risk_inspection_echoes_metric() {
    let tool = RiskInspectionTool;
    let out = tool
        .execute(&ToolCall::new(
            "risk.inspect",
            serde_json::json!({ "metric": "otr" }),
        ))
        .unwrap();
    assert!(out.ok);
    assert_eq!(out.payload["metric"], "otr");
}

#[test]
fn registry_execute_reports_unknown_tool() {
    let registry = ToolRegistry::new();
    let err = registry
        .execute(&ToolCall::new("ghost.tool", serde_json::json!({})))
        .unwrap_err();
    assert!(matches!(err, ToolError::UnknownTool(name) if name == "ghost.tool"));
}

#[test]
fn registry_register_shadows_duplicate_names() {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(ScreenerQueryTool));
    registry.register(Box::new(ScreenerQueryTool));
    assert_eq!(registry.names(), vec!["screener.query"]);
}
