//! Algorithmic strategy backtest, tuning, and trade suggestion chat tools (§4, §6).

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vyapaar_agent_chat::permission::{ToolRisk, ToolTraits};
use vyapaar_agent_chat::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};
use vyapaar_agent_core::{ToolError, ToolOutput, ToolSpec};

use crate::ports::strategy::{DefaultStrategyRunner, StrategyRunner};

// ============================================================================
// 1. Strategy Run Tool
// ============================================================================

/// Inputs for strategy backtest simulation tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyRunInput {
    /// Strategy identifier (e.g. "turtle_breakout", "sma_crossover", "rsi_reversal").
    pub strategy: String,
    /// Target trading symbol (e.g. "NSE:NIFTY50-INDEX", "NSE:RELIANCE-EQ").
    pub symbol: String,
    /// Account capital to deploy in INR (defaults to 1,000,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capital: Option<Decimal>,
    /// Parameter overrides mapping (e.g. fast_period, slow_period).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, Decimal>>,
}

/// Strategy Backtest Execution Tool (`strategy_run`).
pub struct StrategyRunTool {
    spec: ToolSpec,
    runner: Arc<dyn StrategyRunner>,
}

impl StrategyRunTool {
    /// Construct a new tool with default in-process simulation runner.
    pub fn new() -> Self {
        Self::with_runner(Arc::new(DefaultStrategyRunner::new()))
    }

    /// Construct a new tool with an explicit strategy runner port.
    pub fn with_runner(runner: Arc<dyn StrategyRunner>) -> Self {
        let schema = schemars::schema_for!(StrategyRunInput);
        let spec = ToolSpec::new(
            "strategy_run",
            "Simulate a backtest for an algorithmic trading strategy (e.g. 'turtle_breakout', 'sma_crossover', 'rsi_reversal') and report Net PnL, Total Return %, Sharpe Ratio, Max Drawdown %, and Win Rate.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { spec, runner }
    }
}

impl Default for StrategyRunTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChatTool for StrategyRunTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let strat = input.get("strategy").and_then(|v| v.as_str()).unwrap_or("strategy");
        let sym = input.get("symbol").and_then(|v| v.as_str()).unwrap_or("symbol");
        Ok(ToolPlan::new(format!("Run backtest simulation for '{strat}' on {sym}")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: StrategyRunInput = serde_json::from_value(input)
            .map_err(|e| ToolError::InvalidArguments { tool: "strategy_run".to_string(), message: format!("invalid strategy_run input: {e}") })?;

        let capital = args.capital.unwrap_or(Decimal::new(1000000, 0));
        let res = self
            .runner
            .run_backtest(&args.strategy, &args.symbol, capital, args.params)
            .await
            .map_err(|e| ToolError::Execution("strategy_run".to_string(), e))?;

        Ok(ToolOutput::ok("strategy_run", res))
    }
}

// ============================================================================
// 2. Strategy Tune Tool
// ============================================================================

/// Inputs for strategy parameter optimization tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StrategyTuneInput {
    /// Strategy identifier (e.g. "turtle_breakout", "sma_crossover", "rsi_reversal").
    pub strategy: String,
    /// Target trading symbol (e.g. "NSE:NIFTY50-INDEX", "NSE:RELIANCE-EQ").
    pub symbol: String,
    /// Objective metric to maximize: "sharpe" or "pnl" (defaults to "sharpe").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    /// Account capital to deploy in INR (defaults to 1,000,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capital: Option<Decimal>,
}

/// Strategy Parameter Tuning & Overfitting Audit Tool (`strategy_tune`).
pub struct StrategyTuneTool {
    spec: ToolSpec,
    runner: Arc<dyn StrategyRunner>,
}

impl StrategyTuneTool {
    /// Construct a new tuning tool with default simulation runner.
    pub fn new() -> Self {
        Self::with_runner(Arc::new(DefaultStrategyRunner::new()))
    }

    /// Construct a new tool with an explicit strategy runner port.
    pub fn with_runner(runner: Arc<dyn StrategyRunner>) -> Self {
        let schema = schemars::schema_for!(StrategyTuneInput);
        let spec = ToolSpec::new(
            "strategy_tune",
            "Tune strategy hyperparameters across declared search bounds using grid sweep optimization, computing Deflated Sharpe Ratio (DSR) to audit for backtest overfitting.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { spec, runner }
    }
}

impl Default for StrategyTuneTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChatTool for StrategyTuneTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let strat = input.get("strategy").and_then(|v| v.as_str()).unwrap_or("strategy");
        let sym = input.get("symbol").and_then(|v| v.as_str()).unwrap_or("symbol");
        Ok(ToolPlan::new(format!("Tune parameters and audit overfitting for '{strat}' on {sym}")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: StrategyTuneInput = serde_json::from_value(input)
            .map_err(|e| ToolError::InvalidArguments { tool: "strategy_tune".to_string(), message: format!("invalid strategy_tune input: {e}") })?;

        let capital = args.capital.unwrap_or(Decimal::new(1000000, 0));
        let metric = args.metric.as_deref().unwrap_or("sharpe");

        let res = self
            .runner
            .tune_strategy(&args.strategy, &args.symbol, capital, metric)
            .await
            .map_err(|e| ToolError::Execution("strategy_run".to_string(), e))?;

        Ok(ToolOutput::ok("strategy_run", res))
    }
}

// ============================================================================
// 3. Trade Suggestion Tool
// ============================================================================

/// Inputs for optimal trade recommendation generator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TradeSuggestionInput {
    /// Strategy identifier (e.g. "turtle_breakout", "sma_crossover", "rsi_reversal").
    pub strategy: String,
    /// Target trading symbol (e.g. "NSE:NIFTY50-INDEX", "NSE:RELIANCE-EQ").
    pub symbol: String,
    /// Total trading account capital in INR (defaults to 1,000,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capital: Option<Decimal>,
    /// Account percentage to risk on the trade (e.g. 1.0 for 1%, defaults to 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_pct: Option<Decimal>,
}

/// Optimal Trade Recommendation Generator Tool (`strategy_suggest`).
pub struct TradeSuggestionTool {
    spec: ToolSpec,
    runner: Arc<dyn StrategyRunner>,
}

impl TradeSuggestionTool {
    /// Construct a new trade suggestion tool with default simulation runner.
    pub fn new() -> Self {
        Self::with_runner(Arc::new(DefaultStrategyRunner::new()))
    }

    /// Construct a new tool with an explicit strategy runner port.
    pub fn with_runner(runner: Arc<dyn StrategyRunner>) -> Self {
        let schema = schemars::schema_for!(TradeSuggestionInput);
        let spec = ToolSpec::new(
            "strategy_suggest",
            "Synthesize an optimal, risk-budgeted trade recommendation: tunes parameters, identifies directional bias (BUY/SELL), computes ATR stop-loss and 1:2 / 1:3 profit targets, sizes share count to risk budget, and audits Indian statutory transaction costs (STT, GST, SEBI).",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { spec, runner }
    }
}

impl Default for TradeSuggestionTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChatTool for TradeSuggestionTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let strat = input.get("strategy").and_then(|v| v.as_str()).unwrap_or("strategy");
        let sym = input.get("symbol").and_then(|v| v.as_str()).unwrap_or("symbol");
        Ok(ToolPlan::new(format!("Generate optimal trade suggestion for '{strat}' on {sym}")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: TradeSuggestionInput = serde_json::from_value(input)
            .map_err(|e| ToolError::InvalidArguments { tool: "strategy_suggest".to_string(), message: format!("invalid strategy_suggest input: {e}") })?;

        let capital = args.capital.unwrap_or(Decimal::new(1000000, 0));
        let risk_pct = args.risk_pct.unwrap_or(Decimal::new(10, 1)); // 1.0%

        let res = self
            .runner
            .generate_trade_suggestion(&args.strategy, &args.symbol, capital, risk_pct)
            .await
            .map_err(|e| ToolError::Execution("strategy_run".to_string(), e))?;

        Ok(ToolOutput::ok("strategy_run", res))
    }
}
