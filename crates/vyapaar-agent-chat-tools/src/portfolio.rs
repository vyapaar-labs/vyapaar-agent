//! Portfolio and ledger inspection chat tools (§6.2, §6.5).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use vyapaar_agent_chat::permission::{ToolRisk, ToolTraits};
use vyapaar_agent_chat::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};
use vyapaar_agent_core::{ToolError, ToolOutput, ToolSpec};
use vyapaar_core::{AccountId, Money};

use crate::market::parse_instrument;
use crate::ports::market::MarketReader;
use crate::ports::portfolio::{DateRange, OrderFilter, PortfolioReader};

/// Helper to parse optional RFC3339 timestamp.
fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.to_utc())
}

// ——————————————————————————————————————————————————————————————————————————
// 1. portfolio_view
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `portfolio_view`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortfolioViewInput {
    /// Account identifier (e.g. "ACC-LIVE-01"). If omitted, lists all accounts.
    pub account: Option<String>,
}

/// Chat tool providing high-level portfolio overview (§6.5).
pub struct PortfolioViewTool {
    portfolio: Arc<dyn PortfolioReader>,
    market: Option<Arc<dyn MarketReader>>,
    spec: ToolSpec,
}

impl PortfolioViewTool {
    /// Create new portfolio view tool.
    pub fn new(portfolio: Arc<dyn PortfolioReader>, market: Option<Arc<dyn MarketReader>>) -> Self {
        let schema = schemars::schema_for!(PortfolioViewInput);
        let spec = ToolSpec::new(
            "portfolio_view",
            "View portfolio accounts, cash balances, position count, and valuation.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self {
            portfolio,
            market,
            spec,
        }
    }
}

#[async_trait]
impl ChatTool for PortfolioViewTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let acc = input
            .get("account")
            .and_then(Value::as_str)
            .unwrap_or("all accounts");
        Ok(ToolPlan::new(format!("View portfolio for `{acc}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: PortfolioViewInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "portfolio_view".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let accounts = match args.account {
            Some(ref acc) => {
                let id = AccountId::new(acc);
                let all = self
                    .portfolio
                    .accounts()
                    .await
                    .map_err(|e| e.into_tool_error("portfolio_view"))?;
                all.into_iter().filter(|a| a.account_id == id).collect()
            }
            None => self
                .portfolio
                .accounts()
                .await
                .map_err(|e| e.into_tool_error("portfolio_view"))?,
        };

        let mut account_views = Vec::new();

        for acc in accounts {
            let cash = self
                .portfolio
                .cash(&acc.account_id)
                .await
                .unwrap_or_default();
            let positions = self
                .portfolio
                .positions(&acc.account_id)
                .await
                .unwrap_or_default();
            let realized = self
                .portfolio
                .realized_pnl(&acc.account_id, DateRange::default())
                .await
                .unwrap_or_else(|_| Money::zero_inr());

            let mut total_cost_basis = Decimal::ZERO;
            let mut total_market_val = Decimal::ZERO;
            let mut valuation_available = self.market.is_some();

            for pos in &positions {
                let cost = pos.quantity.0 * pos.avg_price.0;
                total_cost_basis += cost;

                if let Some(ref mkt) = self.market {
                    match mkt.quote(&pos.instrument).await {
                        Ok(q) => {
                            total_market_val += pos.quantity.0 * q.last_price.0;
                        }
                        Err(_) => {
                            valuation_available = false;
                        }
                    }
                }
            }

            let valuation_payload = if valuation_available && self.market.is_some() {
                serde_json::json!({
                    "market_value": total_market_val,
                    "unrealized_pnl": total_market_val - total_cost_basis,
                })
            } else {
                serde_json::json!("unavailable")
            };

            account_views.push(serde_json::json!({
                "account_id": acc.account_id.to_string(),
                "algo_id": acc.algo_id.map(|a| a.to_string()),
                "status": acc.status,
                "cash": cash.iter().map(|m| serde_json::json!({ "amount": m.amount, "currency": m.currency.code() })).collect::<Vec<_>>(),
                "open_positions_count": positions.len(),
                "cost_basis": total_cost_basis,
                "valuation": valuation_payload,
                "realized_pnl": { "amount": realized.amount, "currency": realized.currency.code() },
                "updated_at": acc.updated_at,
            }));
        }

        Ok(ToolOutput::ok(
            "portfolio_view",
            serde_json::json!({
                "accounts": account_views,
                "total_accounts": account_views.len(),
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 2. positions_view
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `positions_view`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PositionsViewInput {
    /// Account identifier.
    pub account: String,
    /// Optional instrument filter.
    pub instrument: Option<String>,
}

/// Chat tool providing detailed position breakdown (§6.5).
pub struct PositionsViewTool {
    portfolio: Arc<dyn PortfolioReader>,
    market: Option<Arc<dyn MarketReader>>,
    spec: ToolSpec,
}

impl PositionsViewTool {
    /// Create new positions view tool.
    pub fn new(portfolio: Arc<dyn PortfolioReader>, market: Option<Arc<dyn MarketReader>>) -> Self {
        let schema = schemars::schema_for!(PositionsViewInput);
        let spec = ToolSpec::new(
            "positions_view",
            "View open positions, quantities, average cost prices, and mark-to-market valuations.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self {
            portfolio,
            market,
            spec,
        }
    }
}

#[async_trait]
impl ChatTool for PositionsViewTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let acc = input
            .get("account")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(ToolPlan::new(format!("Inspect open positions for `{acc}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: PositionsViewInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "positions_view".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let account_id = AccountId::new(&args.account);
        let filter_inst = args
            .instrument
            .as_deref()
            .map(parse_instrument)
            .transpose()?;

        let mut positions = self
            .portfolio
            .positions(&account_id)
            .await
            .map_err(|e| e.into_tool_error("positions_view"))?;

        if let Some(ref fi) = filter_inst {
            positions.retain(|p| &p.instrument == fi);
        }

        let mut pos_records = Vec::new();

        for pos in positions {
            let cost_basis = pos.quantity.0 * pos.avg_price.0;

            let valuation = if let Some(ref mkt) = self.market {
                match mkt.quote(&pos.instrument).await {
                    Ok(q) => {
                        let mkt_val = pos.quantity.0 * q.last_price.0;
                        let unrl_pnl = mkt_val - cost_basis;
                        serde_json::json!({
                            "last_price": q.last_price.0,
                            "market_value": mkt_val,
                            "unrealized_pnl": unrl_pnl,
                        })
                    }
                    Err(_) => serde_json::json!("unavailable"),
                }
            } else {
                serde_json::json!("unavailable")
            };

            pos_records.push(serde_json::json!({
                "instrument": pos.instrument.to_string(),
                "side": match pos.side {
                    vyapaar_core::PositionSide::Long => "LONG",
                    vyapaar_core::PositionSide::Short => "SHORT",
                },
                "quantity": pos.quantity.0,
                "avg_price": pos.avg_price.0,
                "cost_basis": cost_basis,
                "valuation": valuation,
                "realized_pnl": pos.realized_pnl.amount,
                "tax_lots_count": pos.lots.len(),
                "updated_at": pos.updated_at,
            }));
        }

        Ok(ToolOutput::ok(
            "positions_view",
            serde_json::json!({
                "account": args.account,
                "positions": pos_records,
                "count": pos_records.len()
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 3. orders_view
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `orders_view`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OrdersViewInput {
    /// Account identifier.
    pub account: String,
    /// Restrict only to active orders (New, PartiallyFilled).
    pub active_only: Option<bool>,
    /// Filter by instrument.
    pub instrument: Option<String>,
    /// Limit number of records returned.
    pub limit: Option<usize>,
}

/// Chat tool querying order book and status (§6.5).
pub struct OrdersViewTool {
    portfolio: Arc<dyn PortfolioReader>,
    spec: ToolSpec,
}

impl OrdersViewTool {
    /// Create new orders view tool.
    pub fn new(portfolio: Arc<dyn PortfolioReader>) -> Self {
        let schema = schemars::schema_for!(OrdersViewInput);
        let spec = ToolSpec::new(
            "orders_view",
            "View active and historical order submissions, execution status, and fill details.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { portfolio, spec }
    }
}

#[async_trait]
impl ChatTool for OrdersViewTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let acc = input
            .get("account")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(ToolPlan::new(format!("Query orders for `{acc}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: OrdersViewInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "orders_view".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let account_id = AccountId::new(&args.account);
        let instrument = args
            .instrument
            .as_deref()
            .map(parse_instrument)
            .transpose()?;

        let filter = OrderFilter {
            instrument,
            status: None,
            active_only: args.active_only.unwrap_or(false),
            limit: args.limit,
        };

        let orders = self
            .portfolio
            .orders(&account_id, filter)
            .await
            .map_err(|e| e.into_tool_error("orders_view"))?;

        let order_records: Vec<Value> = orders
            .into_iter()
            .map(|o| {
                serde_json::json!({
                    "client_order_id": o.client_order_id.to_string(),
                    "broker_order_id": o.broker_order_id,
                    "instrument": o.instrument.to_string(),
                    "side": match o.side {
                        vyapaar_core::OrderSide::Buy => "BUY",
                        vyapaar_core::OrderSide::Sell => "SELL",
                    },
                    "order_type": format!("{:?}", o.order_type),
                    "price": o.price.map(|p| p.0),
                    "quantity": o.quantity.0,
                    "filled_quantity": o.filled_quantity.0,
                    "avg_fill_price": o.avg_fill_price.map(|p| p.0),
                    "status": format!("{:?}", o.status),
                    "created_at": o.created_at,
                    "updated_at": o.updated_at,
                })
            })
            .collect();

        Ok(ToolOutput::ok(
            "orders_view",
            serde_json::json!({
                "account": args.account,
                "orders": order_records,
                "count": order_records.len()
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 4. taxlots_view
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `taxlots_view`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaxLotsViewInput {
    /// Account identifier.
    pub account: String,
    /// Instrument symbol.
    pub instrument: String,
}

/// Chat tool querying FIFO tax lots (§6.5).
pub struct TaxLotsViewTool {
    portfolio: Arc<dyn PortfolioReader>,
    spec: ToolSpec,
}

impl TaxLotsViewTool {
    /// Create new tax lots view tool.
    pub fn new(portfolio: Arc<dyn PortfolioReader>) -> Self {
        let schema = schemars::schema_for!(TaxLotsViewInput);
        let spec = ToolSpec::new(
            "taxlots_view",
            "View open FIFO cost-basis tax lots and remaining quantities for an instrument.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { portfolio, spec }
    }
}

#[async_trait]
impl ChatTool for TaxLotsViewTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let acc = input
            .get("account")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let inst = input
            .get("instrument")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(ToolPlan::new(format!(
            "Inspect tax lots for `{inst}` on account `{acc}`"
        )))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: TaxLotsViewInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "taxlots_view".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let account_id = AccountId::new(&args.account);
        let inst = parse_instrument(&args.instrument)?;

        let lots = self
            .portfolio
            .tax_lots(&account_id, &inst)
            .await
            .map_err(|e| e.into_tool_error("taxlots_view"))?;

        let lot_records: Vec<Value> = lots
            .into_iter()
            .map(|l| {
                serde_json::json!({
                    "lot_id": l.lot_id,
                    "side": match l.side {
                        vyapaar_core::PositionSide::Long => "LONG",
                        vyapaar_core::PositionSide::Short => "SHORT",
                    },
                    "entry_price": l.entry_price.0,
                    "original_quantity": l.quantity.0,
                    "remaining_quantity": l.remaining_quantity.0,
                    "entry_date": l.entry_date,
                    "created_at": l.created_at,
                })
            })
            .collect();

        Ok(ToolOutput::ok(
            "taxlots_view",
            serde_json::json!({
                "account": args.account,
                "instrument": inst.to_string(),
                "lots": lot_records,
                "count": lot_records.len()
            }),
        ))
    }
}

// ——————————————————————————————————————————————————————————————————————————
// 5. pnl_view
// ——————————————————————————————————————————————————————————————————————————

/// Input arguments for `pnl_view`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PnlViewInput {
    /// Account identifier.
    pub account: String,
    /// Inclusive start timestamp (RFC3339).
    pub start: Option<String>,
    /// Exclusive end timestamp (RFC3339).
    pub end: Option<String>,
}

/// Chat tool querying realized profit and loss (§6.5).
pub struct PnlViewTool {
    portfolio: Arc<dyn PortfolioReader>,
    spec: ToolSpec,
}

impl PnlViewTool {
    /// Create new PnL view tool.
    pub fn new(portfolio: Arc<dyn PortfolioReader>) -> Self {
        let schema = schemars::schema_for!(PnlViewInput);
        let spec = ToolSpec::new(
            "pnl_view",
            "Query realized profit and loss for an account across an optional time window.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { portfolio, spec }
    }
}

#[async_trait]
impl ChatTool for PnlViewTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let acc = input
            .get("account")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Ok(ToolPlan::new(format!("Query realized PnL for `{acc}`")))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: PnlViewInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "pnl_view".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        let account_id = AccountId::new(&args.account);
        let start_dt = args.start.as_deref().and_then(parse_timestamp);
        let end_dt = args.end.as_deref().and_then(parse_timestamp);
        let range = DateRange::new(start_dt, end_dt);

        let realized = self
            .portfolio
            .realized_pnl(&account_id, range)
            .await
            .map_err(|e| e.into_tool_error("pnl_view"))?;

        Ok(ToolOutput::ok(
            "pnl_view",
            serde_json::json!({
                "account": args.account,
                "realized_pnl": realized.amount,
                "currency": realized.currency.code(),
                "start": start_dt,
                "end": end_dt,
            }),
        ))
    }
}
