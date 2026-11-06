//! Deterministic quantitative financial calculation tool (`research_calc`) (§4.3, §6.5).

use async_trait::async_trait;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use vyapaar_agent_chat::permission::{ToolRisk, ToolTraits};
use vyapaar_agent_chat::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};
use vyapaar_agent_core::{ToolError, ToolOutput, ToolSpec};

/// Supported calculation operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalcOperation {
    /// Compound Annual Growth Rate.
    Cagr,
    /// Risk-based position sizing and share quantity.
    PositionSizing,
    /// Indian statutory market charges breakdown.
    StatutoryCosts,
    /// Holding period returns and metrics.
    ReturnMetrics,
}

/// Market segment for statutory cost calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarketSegment {
    /// Equity delivery (CNC).
    Delivery,
    /// Equity intraday (MIS).
    Intraday,
    /// Equity futures.
    Futures,
    /// Equity options.
    Options,
}

/// Order trade side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TradeSide {
    /// Buy side.
    Buy,
    /// Sell side.
    Sell,
}

/// Inputs for CAGR calculation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CagrArgs {
    /// Starting capital or investment amount.
    pub start_value: Decimal,
    /// Ending or current portfolio value.
    pub end_value: Decimal,
    /// Time period in years.
    pub years: Decimal,
}

/// Inputs for risk-based position sizing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PositionSizingArgs {
    /// Total trading account capital.
    pub capital: Decimal,
    /// Percentage of account capital willing to risk on trade (e.g. 1.0 for 1%).
    pub risk_percent: Decimal,
    /// Intended entry price per share.
    pub entry_price: Decimal,
    /// Stop loss exit price per share.
    pub stop_loss: Decimal,
    /// Minimum lot size (default 1).
    pub lot_size: Option<u64>,
}

/// Inputs for statutory transaction costs in Indian equities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatutoryCostsArgs {
    /// Market segment (delivery, intraday, futures, options).
    pub segment: MarketSegment,
    /// Trade side (buy or sell).
    pub side: TradeSide,
    /// Total trade notional value in INR.
    pub notional: Decimal,
    /// Broker commission in INR (if omitted, uses standard flat/capped model).
    pub brokerage: Option<Decimal>,
}

/// Inputs for basic return metrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReturnMetricsArgs {
    /// Cost basis / invested amount.
    pub cost_basis: Decimal,
    /// Current market value.
    pub current_value: Decimal,
    /// Cumulative dividends or cash flows.
    pub cash_flow: Option<Decimal>,
    /// Days held.
    pub days_held: Option<u64>,
}

/// Input parameters for `research_calc`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResearchCalcInput {
    /// Operation to perform.
    pub operation: CalcOperation,
    /// Arguments for CAGR.
    pub cagr: Option<CagrArgs>,
    /// Arguments for position sizing.
    pub position_sizing: Option<PositionSizingArgs>,
    /// Arguments for statutory transaction costs.
    pub statutory_costs: Option<StatutoryCostsArgs>,
    /// Arguments for return metrics.
    pub return_metrics: Option<ReturnMetricsArgs>,
}

/// Pure quantitative research calculator tool (`research_calc`) (§6.5).
pub struct ResearchCalcTool {
    spec: ToolSpec,
}

impl ResearchCalcTool {
    /// Construct a new calculator tool.
    pub fn new() -> Self {
        let schema = schemars::schema_for!(ResearchCalcInput);
        let spec = ToolSpec::new(
            "research_calc",
            "Deterministic financial calculations using exact Decimal arithmetic: CAGR, position sizing based on risk, Indian statutory costs (STT, exchange, SEBI, GST), and return metrics.",
            serde_json::to_value(schema).unwrap_or_default(),
        );
        Self { spec }
    }
}

impl Default for ResearchCalcTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChatTool for ResearchCalcTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        ToolTraits::new(ToolRisk::ReadOnly)
    }

    fn plan(&self, _input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        Ok(ToolPlan::new("Execute financial calculations"))
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let args: ResearchCalcInput =
            serde_json::from_value(input).map_err(|e| ToolError::InvalidArguments {
                tool: "research_calc".to_string(),
                message: format!("schema validation failure: {e}"),
            })?;

        match args.operation {
            CalcOperation::Cagr => {
                let cagr_args = args.cagr.ok_or_else(|| ToolError::InvalidArguments {
                    tool: "research_calc".to_string(),
                    message: "missing required `cagr` parameters".to_string(),
                })?;

                if cagr_args.start_value <= Decimal::ZERO {
                    return Err(ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "`start_value` must be greater than zero".to_string(),
                    });
                }
                if cagr_args.years <= Decimal::ZERO {
                    return Err(ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "`years` must be greater than zero".to_string(),
                    });
                }

                let ratio = (cagr_args.end_value / cagr_args.start_value)
                    .to_f64()
                    .ok_or_else(|| {
                        ToolError::Execution(
                            "research_calc".to_string(),
                            "overflow in ratio computation".to_string(),
                        )
                    })?;
                let yrs = cagr_args.years.to_f64().unwrap_or(1.0);
                let cagr_f64 = ratio.powf(1.0 / yrs) - 1.0;
                let cagr_pct = Decimal::from_f64_retain(cagr_f64 * 100.0).unwrap_or(Decimal::ZERO);
                let total_return = ((cagr_args.end_value - cagr_args.start_value)
                    / cagr_args.start_value)
                    * dec!(100);

                Ok(ToolOutput::ok(
                    "research_calc",
                    serde_json::json!({
                        "operation": "cagr",
                        "start_value": cagr_args.start_value,
                        "end_value": cagr_args.end_value,
                        "years": cagr_args.years,
                        "cagr_percent": cagr_pct.round_dp(4),
                        "total_return_percent": total_return.round_dp(4),
                        "absolute_gain": cagr_args.end_value - cagr_args.start_value
                    }),
                ))
            }

            CalcOperation::PositionSizing => {
                let ps = args
                    .position_sizing
                    .ok_or_else(|| ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "missing required `position_sizing` parameters".to_string(),
                    })?;

                if ps.capital <= Decimal::ZERO {
                    return Err(ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "`capital` must be positive".to_string(),
                    });
                }
                if ps.entry_price <= Decimal::ZERO || ps.stop_loss <= Decimal::ZERO {
                    return Err(ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "`entry_price` and `stop_loss` must be positive".to_string(),
                    });
                }

                let per_share_risk = (ps.entry_price - ps.stop_loss).abs();
                if per_share_risk == Decimal::ZERO {
                    return Err(ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "`entry_price` cannot equal `stop_loss`".to_string(),
                    });
                }

                let risk_fraction =
                    if ps.risk_percent <= dec!(0.1) && ps.risk_percent > Decimal::ZERO {
                        ps.risk_percent
                    } else {
                        ps.risk_percent / dec!(100.0)
                    };

                let risk_capital = ps.capital * risk_fraction;
                let raw_shares = (risk_capital / per_share_risk).floor();
                let lot_size = Decimal::from(ps.lot_size.unwrap_or(1));
                let shares = (raw_shares / lot_size).floor() * lot_size;
                let position_value = shares * ps.entry_price;
                let actual_risk = shares * per_share_risk;
                let actual_risk_pct = (actual_risk / ps.capital) * dec!(100.0);

                Ok(ToolOutput::ok(
                    "research_calc",
                    serde_json::json!({
                        "operation": "position_sizing",
                        "capital": ps.capital,
                        "risk_capital": risk_capital.round_dp(2),
                        "entry_price": ps.entry_price,
                        "stop_loss": ps.stop_loss,
                        "per_share_risk": per_share_risk,
                        "quantity": shares.to_u64().unwrap_or(0),
                        "position_value": position_value.round_dp(2),
                        "actual_risk_amount": actual_risk.round_dp(2),
                        "actual_risk_percent": actual_risk_pct.round_dp(4),
                    }),
                ))
            }

            CalcOperation::StatutoryCosts => {
                let sc = args
                    .statutory_costs
                    .ok_or_else(|| ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "missing required `statutory_costs` parameters".to_string(),
                    })?;

                if sc.notional <= Decimal::ZERO {
                    return Err(ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "`notional` must be positive".to_string(),
                    });
                }

                let notional = sc.notional;
                let (stt, stamp_duty, exchange, sebi, ipft, brokerage_default) = match sc.segment {
                    MarketSegment::Delivery => {
                        let stt = notional * dec!(0.001); // 0.1% both sides
                        let stamp = match sc.side {
                            TradeSide::Buy => notional * dec!(0.00015), // 0.015%
                            TradeSide::Sell => Decimal::ZERO,
                        };
                        let exchange = notional * dec!(0.0000297);
                        let sebi = notional * dec!(0.000001);
                        let ipft = notional * dec!(0.000001);
                        let brokerage = Decimal::ZERO;
                        (stt, stamp, exchange, sebi, ipft, brokerage)
                    }
                    MarketSegment::Intraday => {
                        let stt = match sc.side {
                            TradeSide::Sell => notional * dec!(0.00025), // 0.025% sell only
                            TradeSide::Buy => Decimal::ZERO,
                        };
                        let stamp = match sc.side {
                            TradeSide::Buy => notional * dec!(0.00003), // 0.003% buy only
                            TradeSide::Sell => Decimal::ZERO,
                        };
                        let exchange = notional * dec!(0.0000297);
                        let sebi = notional * dec!(0.000001);
                        let ipft = notional * dec!(0.000001);
                        let brokerage = (notional * dec!(0.0003)).min(dec!(20.0));
                        (stt, stamp, exchange, sebi, ipft, brokerage)
                    }
                    MarketSegment::Futures => {
                        let stt = match sc.side {
                            TradeSide::Sell => notional * dec!(0.0002), // 0.02% sell side
                            TradeSide::Buy => Decimal::ZERO,
                        };
                        let stamp = match sc.side {
                            TradeSide::Buy => notional * dec!(0.00002), // 0.002%
                            TradeSide::Sell => Decimal::ZERO,
                        };
                        let exchange = notional * dec!(0.0000173);
                        let sebi = notional * dec!(0.000001);
                        let ipft = Decimal::ZERO;
                        let brokerage = dec!(20.0);
                        (stt, stamp, exchange, sebi, ipft, brokerage)
                    }
                    MarketSegment::Options => {
                        let stt = match sc.side {
                            TradeSide::Sell => notional * dec!(0.001), // 0.1% on premium
                            TradeSide::Buy => Decimal::ZERO,
                        };
                        let stamp = match sc.side {
                            TradeSide::Buy => notional * dec!(0.00003),
                            TradeSide::Sell => Decimal::ZERO,
                        };
                        let exchange = notional * dec!(0.0003503);
                        let sebi = notional * dec!(0.000001);
                        let ipft = Decimal::ZERO;
                        let brokerage = dec!(20.0);
                        (stt, stamp, exchange, sebi, ipft, brokerage)
                    }
                };

                let brokerage = sc.brokerage.unwrap_or(brokerage_default);
                let gst = (brokerage + exchange + sebi + ipft) * dec!(0.18);
                let total_charges = stt + stamp_duty + exchange + sebi + ipft + brokerage + gst;
                let cost_bps = (total_charges / notional) * dec!(10000.0);

                Ok(ToolOutput::ok(
                    "research_calc",
                    serde_json::json!({
                        "operation": "statutory_costs",
                        "segment": sc.segment,
                        "side": sc.side,
                        "notional": notional,
                        "charges": {
                            "stt": stt.round_dp(2),
                            "stamp_duty": stamp_duty.round_dp(2),
                            "exchange_charges": exchange.round_dp(2),
                            "sebi_turnover": sebi.round_dp(2),
                            "ipft": ipft.round_dp(2),
                            "brokerage": brokerage.round_dp(2),
                            "gst_18pct": gst.round_dp(2),
                        },
                        "total_cost": total_charges.round_dp(2),
                        "cost_basis_points": cost_bps.round_dp(2),
                    }),
                ))
            }

            CalcOperation::ReturnMetrics => {
                let rm = args
                    .return_metrics
                    .ok_or_else(|| ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "missing required `return_metrics` parameters".to_string(),
                    })?;

                if rm.cost_basis <= Decimal::ZERO {
                    return Err(ToolError::InvalidArguments {
                        tool: "research_calc".to_string(),
                        message: "`cost_basis` must be positive".to_string(),
                    });
                }

                let cash_flow = rm.cash_flow.unwrap_or(Decimal::ZERO);
                let capital_gain = rm.current_value - rm.cost_basis;
                let total_profit = capital_gain + cash_flow;
                let total_return_pct = (total_profit / rm.cost_basis) * dec!(100.0);

                let annualized_pct = rm.days_held.and_then(|days| {
                    if days > 0 {
                        let years = Decimal::from(days) / dec!(365.25);
                        let ratio = ((rm.current_value + cash_flow) / rm.cost_basis).to_f64()?;
                        let yrs = years.to_f64()?;
                        let ann = ratio.powf(1.0 / yrs) - 1.0;
                        Decimal::from_f64_retain(ann * 100.0).map(|d| d.round_dp(4))
                    } else {
                        None
                    }
                });

                Ok(ToolOutput::ok(
                    "research_calc",
                    serde_json::json!({
                        "operation": "return_metrics",
                        "cost_basis": rm.cost_basis,
                        "current_value": rm.current_value,
                        "cash_flow": cash_flow,
                        "capital_gain": capital_gain,
                        "total_profit": total_profit,
                        "return_percent": total_return_pct.round_dp(4),
                        "annualized_return_percent": annualized_pct,
                        "days_held": rm.days_held,
                    }),
                ))
            }
        }
    }
}
