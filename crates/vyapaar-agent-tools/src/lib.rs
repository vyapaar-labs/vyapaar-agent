//! Typed agent tool registry for screener queries, regime checks, and risk
//! inspection.
//!
//! Tools are pure and side-effect free: they validate their arguments and
//! return structured [`ToolOutput`] payloads. Concrete data access (market
//! feeds, portfolio stores) is injected by the caller, never reached into
//! from inside a tool.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A typed tool invocation from the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Canonical tool name (e.g. `screener.query`).
    pub name: String,
    /// JSON arguments for the tool.
    pub arguments: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool call.
    pub fn new(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}

/// The structured result of executing a tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Tool that produced this output.
    pub tool: String,
    /// Whether execution succeeded.
    pub ok: bool,
    /// Structured payload (validated arguments, computed values).
    pub payload: serde_json::Value,
}

impl ToolOutput {
    /// Create a successful tool output.
    pub fn ok(tool: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            tool: tool.into(),
            ok: true,
            payload,
        }
    }

    /// Create a failed tool output.
    pub fn failed(tool: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            tool: tool.into(),
            ok: false,
            payload,
        }
    }
}

/// Errors raised while executing a tool.
#[derive(Debug, Error)]
pub enum ToolError {
    /// No tool with the requested name is registered.
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    /// The tool rejected the call arguments.
    #[error("invalid arguments for `{tool}`: {message}")]
    InvalidArguments { tool: String, message: String },
    /// The tool failed during execution.
    #[error("tool `{0}` failed: {1}")]
    Execution(String, String),
}

/// A typed, side-effect-free agent tool.
pub trait AgentTool: Send + Sync {
    /// Canonical tool name.
    fn name(&self) -> &str;
    /// Human-readable description for prompt building.
    fn description(&self) -> &str;
    /// Execute the tool against a validated call.
    fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolError>;
}

/// Registry of available tools, resolved by name.
#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Box<dyn AgentTool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. Later registrations shadow earlier ones with the
    /// same name.
    pub fn register(&mut self, tool: Box<dyn AgentTool>) {
        self.tools.retain(|t| t.name() != tool.name());
        self.tools.push(tool);
    }

    /// Names of all registered tools, in registration order.
    pub fn names(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name()).collect()
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn AgentTool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
    }

    /// Execute a call against the matching tool.
    pub fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        match self.get(&call.name) {
            Some(tool) => tool.execute(call),
            None => Err(ToolError::UnknownTool(call.name.clone())),
        }
    }
}

/// Screener query tool: validates a symbol universe and metric name.
#[derive(Debug, Default)]
pub struct ScreenerQueryTool;

impl AgentTool for ScreenerQueryTool {
    fn name(&self) -> &str {
        "screener.query"
    }

    fn description(&self) -> &str {
        "Query a screener universe for a metric (e.g. rsi_14, sma_20)."
    }

    fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        let universe = call
            .arguments
            .get("universe")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: self.name().to_string(),
                message: "`universe` must be an array of symbols".into(),
            })?;
        let metric = call
            .arguments
            .get("metric")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: self.name().to_string(),
                message: "`metric` must be a string".into(),
            })?;
        if universe.is_empty() {
            return Err(ToolError::InvalidArguments {
                tool: self.name().to_string(),
                message: "`universe` must not be empty".into(),
            });
        }
        Ok(ToolOutput::ok(
            self.name(),
            serde_json::json!({
                "universe_size": universe.len(),
                "metric": metric,
            }),
        ))
    }
}

/// Regime check tool: validates a regime label against the known set.
#[derive(Debug, Default)]
pub struct RegimeCheckTool;

impl RegimeCheckTool {
    const KNOWN: &'static [&'static str] = &["trending", "mean_reverting", "choppy", "volatile"];
}

impl AgentTool for RegimeCheckTool {
    fn name(&self) -> &str {
        "regime.check"
    }

    fn description(&self) -> &str {
        "Classify the current market regime (trending, mean_reverting, choppy, volatile)."
    }

    fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        let regime = call
            .arguments
            .get("regime")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: self.name().to_string(),
                message: "`regime` must be a string".into(),
            })?;
        if !Self::KNOWN.contains(&regime) {
            return Err(ToolError::InvalidArguments {
                tool: self.name().to_string(),
                message: format!(
                    "unknown regime `{regime}`; expected one of {:?}",
                    Self::KNOWN
                ),
            });
        }
        Ok(ToolOutput::ok(
            self.name(),
            serde_json::json!({ "regime": regime }),
        ))
    }
}

/// Risk inspection tool: validates a risk metric name.
#[derive(Debug, Default)]
pub struct RiskInspectionTool;

impl AgentTool for RiskInspectionTool {
    fn name(&self) -> &str {
        "risk.inspect"
    }

    fn description(&self) -> &str {
        "Inspect a pre-trade risk metric (otr, price_band, kill_switch)."
    }

    fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        let metric = call
            .arguments
            .get("metric")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| ToolError::InvalidArguments {
                tool: self.name().to_string(),
                message: "`metric` must be a string".into(),
            })?;
        Ok(ToolOutput::ok(
            self.name(),
            serde_json::json!({ "metric": metric }),
        ))
    }
}
