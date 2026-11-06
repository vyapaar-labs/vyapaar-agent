//! Market data inspection chat tools (§6.4, §6.5).

use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use vyapaar_agent_chat::permission::{ToolRisk, ToolTraits};
use vyapaar_agent_chat::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};
use vyapaar_agent_core::{ToolError, ToolOutput, ToolSpec};
use vyapaar_core::{Exchange, InstrumentId};

use crate::ports::market::MarketReader;
use crate::ports::portfolio::DateRange;

/// Maximum allowable bars returned to conversation turn to respect token budgets (§6.5).
pub const MAX_BARS_LIMIT: usize = 500;

/// Parse an instrument string such as `"NSE:RELIANCE"` or `"INFY"` (defaults to NSE).
pub fn parse_instrument(s: &str) -> Result<InstrumentId, ToolError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(ToolError::InvalidArguments {
            tool: "market".to_string(),
            message: "instrument symbol cannot be empty".to_string(),
        });
    }

    if let Some((exch_str, sym)) = trimmed.split_once(':') {
        let exchange = Exchange::from_str(exch_str).map_err(|e| ToolError::InvalidArguments {
            tool: "market".to_string(),
            message: format!("unknown exchange `{exch_str}`: {e}"),
        })?;
        Ok(InstrumentId::new(exchange, sym))
    } else {
        Ok(InstrumentId::new(Exchange::Nse, trimmed))
    }
}

/// Helper to parse optional RFC3339 timestamp.
fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.to_utc())
}

// ——————————————————————————————————————————————————————————————————————————
// 1. market_quote
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `market_quote`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarketQuoteInput {
    /// Instrument symbol (e.g. "NSE:RELIANCE", "TCS").
    pub instrument: String,
}

/// Chat tool providing live/latest market quotes (§6.5).
pub struct MarketQuoteTool {
    reader: Arc<dyn MarketReader>,
    spec: ToolSpec,
}

impl MarketQuoteTool {
    /// Create new quote tool with injected market reader port.
    pub fn new(reader: Arc<dyn MarketReader>) -> Self {
        let schema = schemars::schema_for!(MarketQuoteInput);
        let spec = ToolSpec::new(
            "market_quote",
            "Fetch the latest quote, last traded price, best bid/ask, and volume for an instrument.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for MarketQuoteTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let inst = input
            .get("instrument")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(ToolPlan::new(format!("Fetch latest quote for `{inst}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: MarketQuoteInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "market_quote".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let inst = parse_instrument(&args.instrument)?;
        let quote = self
            .reader
            .quote(&inst)
            .await
            .map_err(|e| e.into_tool_error("market_quote"))?;

        Ok(ToolOutput::ok(
            "market_quote",
            serde_json::json!({
                "instrument": quote.instrument.to_string(),
                "last_price": quote.last_price.0,
                "bid": quote.bid.map(|p| p.0),
                "ask": quote.ask.map(|p| p.0),
                "volume": quote.volume.0,
                "as_of": quote.as_of,
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 2. market_history
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `market_history`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarketHistoryInput {
    /// Instrument symbol (e.g. "NSE:RELIANCE", "INFY").
    pub instrument: String,
    /// Inclusive start timestamp (RFC3339).
    pub start: Option<String>,
    /// Exclusive end timestamp (RFC3339).
    pub end: Option<String>,
    /// Maximum number of bars to return (capped at 500).
    pub limit: Option<usize>,
}

/// Chat tool providing historical OHLCV bars (§6.5).
pub struct MarketHistoryTool {
    reader: Arc<dyn MarketReader>,
    spec: ToolSpec,
}

impl MarketHistoryTool {
    /// Create new market history tool.
    pub fn new(reader: Arc<dyn MarketReader>) -> Self {
        let schema = schemars::schema_for!(MarketHistoryInput);
        let spec = ToolSpec::new(
            "market_history",
            "Fetch historical OHLCV bars for an instrument, capped at 500 bars with downsampling marker.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for MarketHistoryTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let inst = input
            .get("instrument")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(ToolPlan::new(format!("Fetch historical bars for `{inst}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: MarketHistoryInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "market_history".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let inst = parse_instrument(&args.instrument)?;
        let start_dt = args.start.as_deref().and_then(parse_timestamp);
        let end_dt = args.end.as_deref().and_then(parse_timestamp);
        let range = DateRange::new(start_dt, end_dt);

        let capped_limit = match args.limit {
            Some(l) => Some(l.min(MAX_BARS_LIMIT)),
            None => Some(MAX_BARS_LIMIT),
        };

        let mut history = self
            .reader
            .history(&inst, range, capped_limit)
            .await
            .map_err(|e| e.into_tool_error("market_history"))?;

        if history.bars.len() > MAX_BARS_LIMIT {
            history.bars.truncate(MAX_BARS_LIMIT);
            history.downsampled = true;
        }

        let bars_json: Vec<Value> = history
            .bars
            .into_iter()
            .map(|b| {
                serde_json::json!({
                    "timestamp": b.timestamp,
                    "open": b.open.0,
                    "high": b.high.0,
                    "low": b.low.0,
                    "close": b.close.0,
                    "volume": b.volume.0
                })
            })
            .collect();

        Ok(ToolOutput::ok(
            "market_history",
            serde_json::json!({
                "instrument": history.instrument.to_string(),
                "bars": bars_json,
                "count": bars_json.len(),
                "downsampled": history.downsampled,
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 3. market_indicators
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `market_indicators`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarketIndicatorsInput {
    /// Instrument symbol.
    pub instrument: String,
    /// List of indicator names to compute (e.g. ["rsi_14", "sma_20", "ema_50"]).
    pub indicators: Vec<String>,
}

/// Chat tool providing technical indicator values (§6.5).
pub struct MarketIndicatorsTool {
    reader: Arc<dyn MarketReader>,
    spec: ToolSpec,
}

impl MarketIndicatorsTool {
    /// Create new market indicators tool.
    pub fn new(reader: Arc<dyn MarketReader>) -> Self {
        let schema = schemars::schema_for!(MarketIndicatorsInput);
        let spec = ToolSpec::new(
            "market_indicators",
            "Fetch computed technical indicators for an instrument (e.g. RSI, SMA, EMA, MACD).",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for MarketIndicatorsTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let inst = input
            .get("instrument")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(ToolPlan::new(format!(
            "Compute technical indicators for `{inst}`"
        )))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: MarketIndicatorsInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "market_indicators".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        if args.indicators.is_empty() {
            return Err(ToolError::InvalidArguments {
                tool: "market_indicators".to_string(),
                message: "`indicators` array must not be empty".to_string(),
            });
        }

        let inst = parse_instrument(&args.instrument)?;
        let indicators = self
            .reader
            .indicators(&inst, &args.indicators)
            .await
            .map_err(|e| e.into_tool_error("market_indicators"))?;

        Ok(ToolOutput::ok(
            "market_indicators",
            serde_json::json!({
                "instrument": indicators.instrument.to_string(),
                "indicators": indicators.values,
                "as_of": indicators.as_of,
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 4. market_search
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `market_search`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarketSearchInput {
    /// Search query string (company name, symbol fragment, or ticker).
    pub query: String,
}

/// Chat tool searching and discovering instruments (§6.5).
pub struct MarketSearchTool {
    reader: Arc<dyn MarketReader>,
    spec: ToolSpec,
}

impl MarketSearchTool {
    /// Create new market search tool.
    pub fn new(reader: Arc<dyn MarketReader>) -> Self {
        let schema = schemars::schema_for!(MarketSearchInput);
        let spec = ToolSpec::new(
            "market_search",
            "Search securities master and instrument master by symbol or company name.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for MarketSearchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let q = input.get("query").and_then(Value::as_str).unwrap_or("");
        Ok(ToolPlan::new(format!("Search instruments for `{q}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: MarketSearchInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "market_search".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let results = self
            .reader
            .search_instruments(&args.query)
            .await
            .map_err(|e| e.into_tool_error("market_search"))?;

        let hits: Vec<Value> = results
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "instrument": r.instrument.to_string(),
                    "name": r.name,
                    "exchange": r.exchange,
                    "lot_size": r.lot_size,
                })
            })
            .collect();

        Ok(ToolOutput::ok(
            "market_search",
            serde_json::json!({
                "query": args.query,
                "results": hits,
                "count": hits.len()
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 5. macro_series
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `macro_series`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MacroSeriesInput {
    /// Macroeconomic series key (e.g. "cpi", "rbi_repo_rate", "iip", "gdp").
    pub series: String,
}

/// Chat tool providing macroeconomic time-series data (§6.5).
pub struct MacroSeriesTool {
    reader: Arc<dyn MarketReader>,
    spec: ToolSpec,
}

impl MacroSeriesTool {
    /// Create new macro series tool.
    pub fn new(reader: Arc<dyn MarketReader>) -> Self {
        let schema = schemars::schema_for!(MacroSeriesInput);
        let spec = ToolSpec::new(
            "macro_series",
            "Fetch official macroeconomic time series (CPI inflation, RBI repo rate, IIP).",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { reader, spec }
    }
}

#[async_trait]
impl ChatTool for MacroSeriesTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let s = input.get("series").and_then(Value::as_str).unwrap_or("");
        Ok(ToolPlan::new(format!("Fetch macro series `{s}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: MacroSeriesInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "macro_series".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let series = self
            .reader
            .macro_series(&args.series)
            .await
            .map_err(|e| e.into_tool_error("macro_series"))?;

        let points_json: Vec<Value> = series
            .points
            .into_iter()
            .map(|(ts, val)| {
                serde_json::json!({
                    "date": ts,
                    "value": val
                })
            })
            .collect();

        Ok(ToolOutput::ok(
            "macro_series",
            serde_json::json!({
                "series": series.series,
                "data": points_json,
                "count": points_json.len()
            }),
        ))
    }
}
