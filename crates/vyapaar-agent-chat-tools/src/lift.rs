//! Lifting pure synchronous `AgentTool`s into conversational `ChatTool`s (§4.4).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use vyapaar_agent_chat::permission::{ToolRisk, ToolTraits};
use vyapaar_agent_chat::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};
use vyapaar_agent_core::{ToolError, ToolOutput, ToolSpec};
use vyapaar_agent_tools::{
    AgentTool, RegimeCheckTool, RiskInspectionTool, ScreenerQueryTool, ToolCall,
};

use crate::ports::snapshot::{SnapshotNeeds, SnapshotSource};

/// Adapter lifting pure synchronous `AgentTool` into async conversational `ChatTool` (§4.4).
pub struct PureToolAdapter<T: AgentTool> {
    inner: Arc<T>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
    spec: ToolSpec,
    legacy_name: String,
}

impl<T: AgentTool> PureToolAdapter<T> {
    /// Create a new adapter wrapping a pure tool.
    pub fn new(
        inner: Arc<T>,
        spec: ToolSpec,
        legacy_name: impl Into<String>,
        snapshots: Option<Arc<dyn SnapshotSource>>,
    ) -> Self {
        Self {
            inner,
            snapshots,
            spec,
            legacy_name: legacy_name.into(),
        }
    }

    /// Access the underlying pure tool.
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

#[async_trait]
impl<T: AgentTool + 'static> ChatTool for PureToolAdapter<T> {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        // Pure tools are always ReadOnly and concurrency safe (§4.4, §5.5)
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, _input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        Ok(ToolPlan::new(format!(
            "Execute lifted pure tool `{}`",
            self.spec.name
        )))
    }

    async fn execute(&self, mut input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        // If snapshot source is available, optionally ground inputs with snapshot data (§4.4)
        if let Some(ref snapshots) = self.snapshots {
            let needs = SnapshotNeeds::for_tool(&self.legacy_name);
            if let Ok(snap) = snapshots.snapshot(&needs).await {
                if let Value::Object(ref mut map) = input {
                    if self.legacy_name == "screener.query"
                        && !map.contains_key("universe")
                        && let Some(ref universe) = snap.universe
                    {
                        map.insert(
                            "universe".to_string(),
                            serde_json::to_value(universe).unwrap_or_default(),
                        );
                    }
                    if self.legacy_name == "regime.check"
                        && !map.contains_key("regime")
                        && let Some(ref regime) = snap.regime
                    {
                        map.insert(
                            "regime".to_string(),
                            serde_json::to_value(regime).unwrap_or_default(),
                        );
                    }
                }
            }
        }

        let call = ToolCall::new(&self.legacy_name, input);
        let tool = Arc::clone(&self.inner);
        let tool_name = self.spec.name.clone();

        // Spawn pure execution onto blocking reactor (§4.4)
        let output = tokio::task::spawn_blocking(move || tool.execute(&call))
            .await
            .map_err(|e| {
                ToolError::Execution(
                    tool_name.clone(),
                    format!("task join error in pure tool adapter: {e}"),
                )
            })?
            .map_err(|e| ToolError::Execution(tool_name.clone(), format!("{e}")))?;

        if output.ok {
            Ok(ToolOutput::ok(&self.spec.name, output.payload))
        } else {
            Ok(ToolOutput::failed(&self.spec.name, output.payload))
        }
    }
}

/// Lift [`ScreenerQueryTool`] as model-facing `screener_scan` tool (§6.5).
pub fn lift_screener(
    tool: Arc<ScreenerQueryTool>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
) -> PureToolAdapter<ScreenerQueryTool> {
    let spec = ToolSpec::new(
        "screener_scan",
        "Scan an instrument universe for technical metrics (e.g. rsi_14, sma_20).",
        serde_json::json!({
            "type": "object",
            "properties": {
                "universe": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of instrument symbols. If omitted, uses default active universe."
                },
                "metric": {
                    "type": "string",
                    "description": "Metric to compute (e.g. 'rsi_14', 'sma_20')."
                }
            },
            "required": ["metric"],
            "additionalProperties": false
        }),
    );
    PureToolAdapter::new(tool, spec, "screener.query", snapshots)
}

/// Lift [`RegimeCheckTool`] as model-facing `regime_check` tool (§6.5).
pub fn lift_regime(
    tool: Arc<RegimeCheckTool>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
) -> PureToolAdapter<RegimeCheckTool> {
    let spec = ToolSpec::new(
        "regime_check",
        "Classify or verify the market regime (trending, mean_reverting, choppy, volatile).",
        serde_json::json!({
            "type": "object",
            "properties": {
                "regime": {
                    "type": "string",
                    "enum": ["trending", "mean_reverting", "choppy", "volatile"],
                    "description": "Market regime label to check or validate. If omitted, checks current state."
                }
            },
            "additionalProperties": false
        }),
    );
    PureToolAdapter::new(tool, spec, "regime.check", snapshots)
}

/// Lift [`RiskInspectionTool`] as model-facing `risk_inspect` tool (§6.5).
pub fn lift_risk(
    tool: Arc<RiskInspectionTool>,
    snapshots: Option<Arc<dyn SnapshotSource>>,
) -> PureToolAdapter<RiskInspectionTool> {
    let spec = ToolSpec::new(
        "risk_inspect",
        "Inspect pre-trade risk controls and metrics (otr, price_band, kill_switch).",
        serde_json::json!({
            "type": "object",
            "properties": {
                "metric": {
                    "type": "string",
                    "description": "Risk metric to inspect (e.g. 'otr', 'price_band', 'kill_switch')."
                }
            },
            "required": ["metric"],
            "additionalProperties": false
        }),
    );
    PureToolAdapter::new(tool, spec, "risk.inspect", snapshots)
}
