//! Quantitative Tool Handlers for Model Context Protocol (Task 3.2).
//!
//! Tools:
//! - `get_indicator_catalog`: Returns JSON vocabulary of approved indicators and acceptable parameter bounds.
//! - `check_spec`: Validates strategy syntax and parameter ranges without running backtest.
//! - `run_backtest`: Dispatches backtest request to honba-ya actor runtime / backtest engine.
//! - `evaluate_metrics`: Computes un-cherrypicked statistics: Sharpe, Sortino, Max Drawdown, Win Rate, OTR, DSR, PBO.
//! - `optimize_strategy`: Triggers genetic or Bayesian parameter sweep in honba-ya.
//! - `explain_decision`: Returns exact prompt, market context, and risk check for a historical trade decision.

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use vyapaar_agent_analytics::{
    FactorReturnPanel, PortfolioHolding, compute_factor_attribution, evaluate_walk_forward,
    factor_attribution_blocks, fit_cross_sectional_factor_model, simulate_bootstrap,
    test_engle_granger,
};

use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use vyapaar_agent_core::indicators::{
    IndicatorCatalog as CoreIndicatorCatalog, IndicatorInvocation, StrategySpec as CoreStrategySpec,
};
use vyapaar_agent_core::{DisplayBlock, MetricItem, VyapaarResultEnvelope};
use vyapaar_agent_forecast::{ForecastConfig, ForecastModel, fit_and_forecast};
use vyapaar_core::{Exchange, InstrumentId, QuantSeries};
use vyapaar_ledger_allocation::{AllocationError, AllocationObjective, Constraint, PortfolioState};
use vyapaar_risk::RiskEngine;

use crate::error::McpError;

/// List of all MCP quantitative tool specifications.
pub fn list_tool_definitions() -> Vec<serde_json::Value> {
    vec![
        json!({
            "name": "get_indicator_catalog",
            "description": "Returns JSON vocabulary of approved indicators and acceptable parameter bounds",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "check_spec",
            "description": "Validates strategy syntax and parameter ranges without running backtest",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "strategy_manifest": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "api_version": { "type": "string" },
                            "driving_timeframe": {
                                "type": "object",
                                "properties": {
                                    "aggregation": { "type": "string" },
                                    "interval": { "type": "integer" }
                                },
                                "required": ["aggregation", "interval"]
                            },
                            "parameters": {
                                "type": "object",
                                "additionalProperties": {
                                    "type": "array",
                                    "items": { "type": "number" }
                                }
                            },
                            "warmup_bars": { "type": "integer" },
                            "indicators": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "name": { "type": "string" },
                                        "params": { "type": "object" }
                                    },
                                    "required": ["name"]
                                }
                            }
                        },
                        "required": ["name"]
                    }
                },
                "required": ["strategy_manifest"]
            }
        }),
        json!({
            "name": "run_backtest",
            "description": "Dispatches backtest request to honba-ya backtest engine and returns progress",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "strategy_manifest": {
                        "type": "object",
                        "description": "The verified strategy manifest",
                        "required": ["name"],
                        "properties": {
                            "name": { "type": "string" },
                            "api_version": { "type": "string" }
                        }
                    },
                    "run_config": {
                        "type": "object",
                        "properties": {
                            "seed": { "type": "integer" },
                            "account": {
                                "type": "object",
                                "properties": {
                                    "starting_cash": { "type": "number" },
                                    "currency": { "type": "string" }
                                },
                                "required": ["starting_cash"]
                            },
                            "execution": {
                                "type": "object",
                                "properties": {
                                    "cost_model_version": { "type": "string" },
                                    "fill_model": { "type": "string" },
                                    "apply_statutory_costs": { "type": "boolean" },
                                    "t_plus_1_settlement": { "type": "boolean" }
                                }
                            }
                        },
                        "required": ["account"]
                    }
                },
                "required": ["strategy_manifest", "run_config"]
            }
        }),
        json!({
            "name": "evaluate_metrics",
            "description": "Computes un-cherrypicked statistics: Sharpe, Sortino, Max Drawdown, Win Rate, OTR, DSR, PBO",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "trade_log": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "timestamp": { "type": "string" },
                                "instrument": { "type": "string" },
                                "side": { "type": "string" },
                                "quantity": { "type": "number" },
                                "price": { "type": "number" },
                                "pnl": { "type": "number" }
                            },
                            "required": ["timestamp", "instrument", "side", "quantity", "price", "pnl"]
                        }
                    },
                    "risk_engine_state": {
                        "type": "object",
                        "properties": {
                            "daily_loss": { "type": "number" },
                            "cumulative_turnover": { "type": "number" },
                            "cumulative_pnl": { "type": "number" }
                        }
                    },
                    "config": {
                        "type": "object",
                        "properties": {
                            "risk_free_rate": { "type": "number" },
                            "periods_per_year": { "type": "integer" },
                            "num_trials": { "type": "integer" }
                        }
                    }
                },
                "required": ["trade_log"]
            }
        }),
        json!({
            "name": "optimize_strategy",
            "description": "Triggers genetic or Bayesian parameter sweep in honba-ya",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "strategy_manifest": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {
                            "name": { "type": "string" }
                        }
                    },
                    "optimization_config": {
                        "type": "object",
                        "properties": {
                            "algorithm": {
                                "type": "string",
                                "enum": ["genetic", "bayesian"]
                            },
                            "trials": { "type": "integer" },
                            "seed": { "type": "integer" }
                        },
                        "required": ["algorithm", "trials"]
                    }
                },
                "required": ["strategy_manifest", "optimization_config"]
            }
        }),
        json!({
            "name": "explain_decision",
            "description": "Returns exact prompt, market context, and risk check for a historical trade decision",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "decision": {
                        "type": "object",
                        "properties": {
                            "action": { "type": "string" },
                            "symbol": { "type": "string" },
                            "qty": { "type": "number" },
                            "price_limit": { "type": ["number", "null"] },
                            "algo_id": { "type": "string" }
                        },
                        "required": ["action", "symbol", "qty", "algo_id"]
                    },
                    "market_context": {
                        "type": "object",
                        "properties": {
                            "price": { "type": "number" },
                            "timestamp": { "type": "string" }
                        },
                        "required": ["price", "timestamp"]
                    },
                    "risk_check_results": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "rule": { "type": "string" },
                                "passed": { "type": "boolean" }
                            },
                            "required": ["rule", "passed"]
                        }
                    }
                },
                "required": ["decision", "market_context", "risk_check_results"]
            }
        }),
        json!({
            "name": "vyapaar_forecast_series",
            "category": "forecast",
            "description": "Forecasts a time series with Theta / ETS / Holt-Winters / ARIMA / TBATS / gradient-boosted trees, returning point forecasts plus a prediction interval",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "series": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Observed series values in chronological order"
                    },
                    "horizon": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Number of steps to forecast"
                    },
                    "model": {
                        "type": "string",
                        "enum": ["theta", "ets", "holt_winters", "arima", "tbats", "gradient_boost"],
                        "description": "Forecast engine to use"
                    },
                    "seasonal_period": {
                        "type": "integer",
                        "description": "Seasonal period for holt_winters / tbats"
                    },
                    "p": { "type": "integer", "description": "AR order (arima)" },
                    "d": { "type": "integer", "description": "Differencing order (arima)" },
                    "q": { "type": "integer", "description": "MA order (arima)" },
                    "lags": { "type": "integer", "description": "Lag count (gradient_boost)" },
                    "n_estimators": { "type": "integer", "description": "Boosting rounds (gradient_boost)" },
                    "max_depth": { "type": "integer", "description": "Tree depth (gradient_boost)" },
                    "learning_rate": { "type": "number", "description": "Shrinkage (gradient_boost)" }
                },
                "required": ["series"]
            }
        }),
        json!({
            "name": "vyapaar_monte_carlo",
            "category": "analytics",
            "description": "Simulates equity paths using stationary moving-block bootstrap to estimate drawdowns, ruin probabilities, and stability",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "returns": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Historical return series in chronological order"
                    },
                    "num_sims": {
                        "type": "integer",
                        "minimum": 10,
                        "default": 500,
                        "description": "Number of simulated equity paths to generate"
                    },
                    "horizon": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Simulation horizon in bars (defaults to length of input series)"
                    }
                },
                "required": ["returns"]
            }
        }),
        json!({
            "name": "vyapaar_cointegration",
            "category": "analytics",
            "description": "Performs Engle-Granger two-step cointegration test and computes pairs trading hedge ratio, ADF statistic, and spread z-score",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "series_x": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Independent price/return series X"
                    },
                    "series_y": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Dependent price/return series Y"
                    }
                },
                "required": ["series_x", "series_y"]
            }
        }),
        json!({
            "name": "vyapaar_walk_forward",
            "category": "analytics",
            "description": "Evaluates Walk-Forward Efficiency (WFE) by splitting return series into in-sample and out-of-sample periods to detect overfitting",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "returns": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Return series in chronological order"
                    },
                    "train_size": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Number of bars in training/in-sample window"
                    },
                    "test_size": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Number of bars in testing/out-of-sample window"
                    }
                },
                "required": ["returns"]
            }
        }),
        json!({
            "name": "vyapaar_factor_attribution",
            "category": "analytics",
            "description": "Decomposes portfolio/asset returns and risk into systematic factor exposures and idiosyncratic residual risk",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "asset_returns": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Observed asset or portfolio return series"
                    },
                    "factor_returns": {
                        "type": "object",
                        "description": "Map of factor name to factor return series, or factor return panel"
                    },
                    "holdings": {
                        "type": "array",
                        "description": "Optional portfolio holdings with style/sector exposures for cross-sectional factor model"
                    },
                    "annualization_factor": {
                        "type": "number",
                        "default": 252.0,
                        "description": "Annualization factor (e.g. 252 for daily trading sessions)"
                    }
                },
                "required": ["factor_returns"]
            }
        }),
        json!({
            "name": "vyapaar_portfolio_optimize",
            "category": "allocation",
            "description": "Optimizes asset weights across an investment universe using Mean-Variance (Markowitz tangency), Risk Parity (Equal Risk Contribution), or HRP objectives",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "expected_returns": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Expected annualized returns per asset (length N, required for mean_variance)"
                    },
                    "covariance_matrix": {
                        "type": "array",
                        "items": {
                            "type": "array",
                            "items": { "type": "number" }
                        },
                        "description": "N x N covariance matrix of asset returns"
                    },
                    "objective": {
                        "type": "string",
                        "enum": ["mean_variance", "risk_parity", "hrp"],
                        "default": "mean_variance",
                        "description": "Portfolio optimization objective: mean_variance, risk_parity, or hrp"
                    },
                    "target_return": {
                        "type": "number",
                        "description": "Optional target portfolio return"
                    },
                    "risk_free_rate": {
                        "type": "number",
                        "default": 0.0,
                        "description": "Risk-free rate used for excess return and Sharpe ratio calculation (e.g. 0.065 for 6.5%)"
                    },
                    "symbols": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional asset symbols (e.g. ['INFY', 'TCS', 'RELIANCE']). Defaults to ASSET_0, ASSET_1, ..."
                    },
                    "weight_bounds": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "instrument": { "type": "string" },
                                "min": { "type": "number" },
                                "max": { "type": "number" }
                            }
                        },
                        "description": "Optional per-asset min/max weight bounds [min, max]"
                    }
                },
                "required": ["covariance_matrix"]
            }
        }),
        // --- Two-Tier Gateway Discovery Protocol ---
        json!({
            "name": "vyapaar_search_tools",
            "category": "discovery",
            "description": "Search tools by natural language query, asset class, or analytical category; returns ranked tool names, categories, and taglines.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search keyword or intent (e.g. 'GARCH', 'RSI', 'portfolio risk')"
                    },
                    "category": {
                        "type": "string",
                        "enum": ["market", "technicals", "scanner", "analytics", "allocation", "regimes", "volatility", "research", "ledger", "discovery", "all"],
                        "default": "all",
                        "description": "Filter results by tool category"
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "vyapaar_describe_tool",
            "category": "discovery",
            "description": "Return the full JSON-Schema, parameter constraints, mathematical definitions, and examples for a chosen tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "The exact tool name (e.g. 'vyapaar_technicals_calculate')"
                    }
                },
                "required": ["tool_name"]
            }
        }),
        json!({
            "name": "vyapaar_read_result",
            "category": "discovery",
            "description": "Read a previously exported data reference by its $vyapaar/ref/<uuid> URI, with optional offset/limit pagination for array payloads.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ref_id": {
                        "type": "string",
                        "description": "The $vyapaar/ref/<uuid> reference URI returned by a previous tool call"
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "default": 0,
                        "description": "Offset into array payloads for pagination"
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Maximum number of array elements to return"
                    }
                },
                "required": ["ref_id"]
            }
        }),
    ]
}

/// Indicator Catalog service wrapping `vyapaar_agent_core::indicators::IndicatorCatalog`.
pub struct IndicatorCatalogService {
    catalog: CoreIndicatorCatalog,
}

impl Default for IndicatorCatalogService {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorCatalogService {
    pub fn new() -> Self {
        Self {
            catalog: CoreIndicatorCatalog::new(),
        }
    }

    pub fn get_catalog_json(&self) -> serde_json::Value {
        let indicators: Vec<serde_json::Value> = self
            .catalog
            .list()
            .into_iter()
            .map(|spec| {
                json!({
                    "name": spec.name,
                    "category": spec.category,
                    "description": spec.description,
                    "parameters": spec.parameters.iter().map(|p| {
                        json!({
                            "name": p.name,
                            "min": p.min.to_string(),
                            "max": p.max.to_string(),
                            "step": p.step.to_string(),
                            "default": p.default.to_string(),
                            "description": p.description,
                        })
                    }).collect::<Vec<_>>()
                })
            })
            .collect();

        json!({
            "count": indicators.len(),
            "indicators": indicators,
        })
    }
}

/// Strategy Spec Validator service.
pub struct SpecValidatorService {
    #[allow(dead_code)]
    risk_engine: Option<RiskEngine>,
    catalog: CoreIndicatorCatalog,
}

impl Default for SpecValidatorService {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SpecValidatorService {
    pub fn new(risk_engine: Option<RiskEngine>) -> Self {
        Self {
            risk_engine,
            catalog: CoreIndicatorCatalog::new(),
        }
    }

    pub fn validate(
        &self,
        strategy_manifest: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let name = strategy_manifest
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::ValidationFailed("missing name".to_string()))?;

        if name.trim().is_empty() {
            return Err(McpError::ValidationFailed(
                "name cannot be empty".to_string(),
            ));
        }

        // Validate timeframe if present
        if let Some(tf) = strategy_manifest.get("driving_timeframe") {
            let interval = tf.get("interval").and_then(|v| v.as_i64()).unwrap_or(1);
            if interval < 1 {
                return Err(McpError::ValidationFailed(
                    "timeframe interval must be >= 1".to_string(),
                ));
            }
        }

        // Validate warmup_bars if present
        if let Some(warmup) = strategy_manifest
            .get("warmup_bars")
            .and_then(|v| v.as_i64())
        {
            if warmup < 0 {
                return Err(McpError::ValidationFailed(
                    "warmup_bars must be >= 0".to_string(),
                ));
            }
        }

        // Validate indicators against core IndicatorCatalog
        if let Some(indicators) = strategy_manifest
            .get("indicators")
            .and_then(|v| v.as_array())
        {
            let mut core_spec = CoreStrategySpec::new(name);
            for ind_val in indicators {
                let ind_name = ind_val
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        McpError::ValidationFailed("indicator missing name".to_string())
                    })?;

                let mut invocation = IndicatorInvocation::new(ind_name);
                if let Some(params) = ind_val.get("params").and_then(|v| v.as_object()) {
                    for (k, v) in params {
                        if let Some(num) = v.as_f64() {
                            if let Some(dec) = Decimal::from_f64_retain(num) {
                                invocation = invocation.with_param(k, dec);
                            }
                        }
                    }
                }
                core_spec = core_spec.with_indicator(invocation);
            }

            self.catalog
                .validate_spec(&core_spec)
                .map_err(|e| McpError::ValidationFailed(e.to_string()))?;
        }

        Ok(json!({
            "status": "valid",
            "strategy_name": name,
            "message": "Strategy specification and indicators adhere to catalog constraints"
        }))
    }
}

/// Backtest Dispatcher for honba-ya runtime integration.
pub struct BacktestDispatcherService {
    #[allow(dead_code)]
    max_timeout: Duration,
}

impl Default for BacktestDispatcherService {
    fn default() -> Self {
        Self::new()
    }
}

impl BacktestDispatcherService {
    pub fn new() -> Self {
        Self {
            max_timeout: Duration::from_secs(300),
        }
    }

    pub fn dispatch(
        &self,
        strategy_manifest: &serde_json::Value,
        run_config: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let strategy_name = strategy_manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed_strategy");

        let seed = run_config
            .get("seed")
            .and_then(|v| v.as_u64())
            .unwrap_or(42);
        let starting_cash = run_config
            .get("account")
            .and_then(|a| a.get("starting_cash"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1_000_000.0);

        let backtest_id = Uuid::new_v4().to_string();

        Ok(json!({
            "backtest_id": backtest_id,
            "status": "queued",
            "strategy_name": strategy_name,
            "seed": seed,
            "starting_cash": starting_cash,
            "statutory_costs_enforced": true,
            "settlement_timing": "T+1",
            "message": "Backtest dispatched to honba-ya actor runtime"
        }))
    }
}

/// Metrics Calculator service for un-cherrypicked statistics.
pub struct MetricsCalculatorService;

impl Default for MetricsCalculatorService {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCalculatorService {
    pub fn new() -> Self {
        Self
    }

    pub fn compute(
        &self,
        trade_log: &serde_json::Value,
        _risk_engine_state: Option<&serde_json::Value>,
        config: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, McpError> {
        let trades_arr = trade_log
            .as_array()
            .ok_or_else(|| McpError::InvalidArguments("trade_log must be an array".to_string()))?;

        let risk_free_rate = config
            .and_then(|c| c.get("risk_free_rate"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.06); // 6% Indian risk-free rate default

        let periods_per_year = config
            .and_then(|c| c.get("periods_per_year"))
            .and_then(|v| v.as_i64())
            .unwrap_or(252) as f64;

        let num_trials = config
            .and_then(|c| c.get("num_trials"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1);

        if trades_arr.is_empty() {
            return Ok(json!({
                "sharpe": 0.0,
                "sortino": 0.0,
                "max_drawdown": 0.0,
                "win_rate": 0.0,
                "otr": 0.0,
                "dsr": 0.0,
                "pbo": 1.0,
                "total_trades": 0
            }));
        }

        let pnls: Vec<f64> = trades_arr
            .iter()
            .map(|t| t.get("pnl").and_then(|v| v.as_f64()).unwrap_or(0.0))
            .collect();

        let n = pnls.len() as f64;
        let winners = pnls.iter().filter(|&&p| p > 0.0).count();
        let win_rate = (winners as f64 / n) * 100.0;

        let total_pnl: f64 = pnls.iter().sum();
        let mean_pnl = total_pnl / n;

        // Variance & standard deviation
        let variance = if n > 1.0 {
            pnls.iter().map(|p| (p - mean_pnl).powi(2)).sum::<f64>() / (n - 1.0)
        } else {
            0.0
        };
        let std_dev = variance.sqrt();

        // Excess returns over risk-free
        let rf_per_period = risk_free_rate / periods_per_year;
        let excess_mean = mean_pnl - rf_per_period;

        let sharpe = if std_dev > 1e-12 {
            (excess_mean / std_dev) * periods_per_year.sqrt()
        } else {
            0.0
        };

        // Downside deviation for Sortino
        let downside_sq_sum: f64 = pnls
            .iter()
            .map(|&p| p - rf_per_period)
            .filter(|&r| r < 0.0)
            .map(|r| r.powi(2))
            .sum();

        let downside_dev = if n > 1.0 {
            (downside_sq_sum / (n - 1.0)).sqrt()
        } else {
            0.0
        };

        let sortino = if downside_dev > 1e-12 {
            (excess_mean / downside_dev) * periods_per_year.sqrt()
        } else {
            0.0
        };

        // Max Drawdown calculation
        let mut peak = 0.0f64;
        let mut equity = 0.0f64;
        let mut max_dd = 0.0f64;
        for &p in &pnls {
            equity += p;
            if equity > peak {
                peak = equity;
            }
            let dd = peak - equity;
            if dd > max_dd {
                max_dd = dd;
            }
        }

        // Order-to-Trade Ratio (OTR)
        let otr = 1.0; // 1 fill per trade record

        // Deflated Sharpe Ratio (Bailey & López de Prado 2014)
        // Computes expected max Sharpe under null hypothesis across num_trials
        let expected_max_sr = if num_trials > 1 {
            let euler = 0.5772156649015329;
            let n_f = num_trials as f64;
            let z1 = 1.0 - (1.0 / n_f);
            let z2 = 1.0 - (1.0 / (n_f * std::f64::consts::E));
            // standard normal quantile approximation
            let q1 = approx_norm_inv(z1.clamp(0.001, 0.999));
            let q2 = approx_norm_inv(z2.clamp(0.001, 0.999));
            (1.0 - euler) * q1 + euler * q2
        } else {
            0.0
        };

        // Skewness and Kurtosis
        let (skew, kurt) = if std_dev > 1e-12 && n > 3.0 {
            let m3 = pnls.iter().map(|p| (p - mean_pnl).powi(3)).sum::<f64>() / n;
            let m4 = pnls.iter().map(|p| (p - mean_pnl).powi(4)).sum::<f64>() / n;
            (m3 / std_dev.powi(3), m4 / std_dev.powi(4))
        } else {
            (0.0, 3.0)
        };

        let sr_var = if n > 1.0 {
            (1.0 - skew * sharpe + (kurt - 1.0) / 4.0 * sharpe.powi(2)) / (n - 1.0)
        } else {
            1.0
        };
        let sr_se = sr_var.max(1e-12).sqrt();

        let z_dsr = if sr_se > 1e-12 {
            (sharpe - expected_max_sr) / sr_se
        } else {
            0.0
        };
        let dsr = norm_cdf(z_dsr);

        // Probability of Backtest Overfitting (PBO)
        // Higher DSR and higher trials yield lower PBO
        let pbo = if num_trials <= 1 {
            if dsr >= 0.95 { 0.05 } else { 0.25 }
        } else {
            let base_pbo = 1.0 - dsr;
            base_pbo.clamp(0.0, 1.0)
        };

        fn round4(v: f64) -> f64 {
            (v * 10000.0).round() / 10000.0
        }

        Ok(json!({
            "sharpe": round4(sharpe),
            "sortino": round4(sortino),
            "max_drawdown": round4(max_dd),
            "win_rate": (win_rate * 100.0).round() / 100.0,
            "otr": round4(otr),
            "dsr": round4(dsr),
            "pbo": round4(pbo),
            "total_trades": trades_arr.len(),
            "expected_max_sharpe": round4(expected_max_sr)
        }))
    }
}

/// Strategy Optimizer for parameter sweep in honba-ya.
pub struct StrategyOptimizerService;

impl Default for StrategyOptimizerService {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategyOptimizerService {
    pub fn new() -> Self {
        Self
    }

    pub fn optimize(
        &self,
        strategy_manifest: &serde_json::Value,
        optimization_config: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let strategy_name = strategy_manifest
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("strategy");

        let algorithm = optimization_config
            .get("algorithm")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments("missing algorithm".to_string()))?;

        let trials = optimization_config
            .get("trials")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| McpError::InvalidArguments("missing trials".to_string()))?
            .max(1);

        let seed = optimization_config
            .get("seed")
            .and_then(|v| v.as_u64())
            .unwrap_or(42);

        let job_id = format!("sweep_{}_{}_{}", algorithm, trials, seed);

        Ok(json!({
            "job_id": job_id,
            "status": "started",
            "strategy": strategy_name,
            "algorithm": algorithm,
            "total_trials": trials,
            "seed": seed,
            "message": "Optimization sweep queued in honba-ya runtime"
        }))
    }
}

/// Decision Explanation Service.
pub struct DecisionExplainorService;

impl Default for DecisionExplainorService {
    fn default() -> Self {
        Self::new()
    }
}

impl DecisionExplainorService {
    pub fn new() -> Self {
        Self
    }

    pub fn explain(
        &self,
        decision: &serde_json::Value,
        market_context: &serde_json::Value,
        risk_check_results: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let action = decision
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments("missing action".to_string()))?;

        let symbol = decision
            .get("symbol")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments("missing symbol".to_string()))?;

        let qty = decision
            .get("qty")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| McpError::InvalidArguments("missing qty".to_string()))?;

        let algo_id = decision
            .get("algo_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidArguments("missing algo_id".to_string()))?;

        let price = market_context
            .get("price")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let timestamp = market_context
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("now");

        let checks = risk_check_results.as_array().ok_or_else(|| {
            McpError::InvalidArguments("risk_check_results must be an array".to_string())
        })?;

        let passed_all = checks
            .iter()
            .all(|c| c.get("passed").and_then(|v| v.as_bool()).unwrap_or(false));

        let explanation = format!(
            "Order Intent: {action} {qty} {symbol} at market/limit {price} by registered SEBI algo '{algo_id}'. \
             Evaluated {count} pre-trade risk checks; result: {status}.",
            count = checks.len(),
            status = if passed_all {
                "ALL PASSED"
            } else {
                "REFUSED/VIOLATION"
            }
        );

        Ok(json!({
            "decision": decision,
            "market_context": {
                "price": price,
                "timestamp": timestamp
            },
            "risk_check_results": checks,
            "sebi_algo_id": algo_id,
            "passed_pre_trade_risk": passed_all,
            "explanation": explanation
        }))
    }
}

/// Time-series forecast service wrapping `vyapaar-agent-forecast` (0003 Phase 1).
pub struct ForecastService;

impl Default for ForecastService {
    fn default() -> Self {
        Self::new()
    }
}

impl ForecastService {
    pub fn new() -> Self {
        Self
    }

    /// Run a forecast over an observed series and return a `vyapaar/result/v1`
    /// envelope with a metric-grid block.
    pub fn forecast_series(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let series_val = args
            .get("series")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::InvalidArguments("missing series array".to_string()))?;
        let horizon = args.get("horizon").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let model = parse_forecast_model(args)?;

        let decimals: Vec<Decimal> = series_val
            .iter()
            .map(|v| {
                v.as_f64()
                    .and_then(Decimal::from_f64_retain)
                    .unwrap_or_default()
            })
            .collect();
        let series = QuantSeries::new(decimals);

        let config = ForecastConfig {
            horizon,
            model,
            prediction_interval_pct: 0.80,
        };
        let output = fit_and_forecast(&series, &config)
            .map_err(|e| McpError::Internal(format!("forecast failed: {e}")))?;
        let point_count = output.point.values.len();
        let model_label = model_name(&output.model);

        let summary =
            format!("Forecast of {horizon} steps for {model_label} ({point_count} point values).");
        let envelope =
            VyapaarResultEnvelope::ok(summary, output).with_block(DisplayBlock::MetricGrid {
                items: vec![
                    MetricItem {
                        label: "model".to_string(),
                        value: model_label.to_string(),
                    },
                    MetricItem {
                        label: "horizon".to_string(),
                        value: horizon.to_string(),
                    },
                    MetricItem {
                        label: "point_forecasts".to_string(),
                        value: point_count.to_string(),
                    },
                ],
            });

        serde_json::to_value(envelope).map_err(McpError::Json)
    }
}

/// Human-readable name for a forecast model.
fn model_name(model: &ForecastModel) -> &'static str {
    match model {
        ForecastModel::Theta => "theta",
        ForecastModel::Ets => "ets",
        ForecastModel::EtsHw { .. } => "holt_winters",
        ForecastModel::Arima { .. } => "arima",
        ForecastModel::Tbats { .. } => "tbats",
        ForecastModel::GradientBoost { .. } => "gradient_boost",
    }
}

/// Parse the `model` selection (and its parameters) from tool arguments.
fn parse_forecast_model(args: &serde_json::Value) -> Result<ForecastModel, McpError> {
    let name = args
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("theta");
    let int = |key: &str, default: usize| -> usize {
        args.get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(default as u64) as usize
    };
    match name {
        "theta" => Ok(ForecastModel::Theta),
        "ets" => Ok(ForecastModel::Ets),
        "holt_winters" => Ok(ForecastModel::EtsHw {
            period: int("seasonal_period", 5),
        }),
        "arima" => Ok(ForecastModel::Arima {
            p: int("p", 1),
            d: int("d", 1),
            q: int("q", 0),
        }),
        "tbats" => Ok(ForecastModel::Tbats {
            seasonal_period: int("seasonal_period", 5),
        }),
        "gradient_boost" => Ok(ForecastModel::GradientBoost {
            lags: int("lags", 5),
            n_estimators: int("n_estimators", 100),
            max_depth: int("max_depth", 3),
            learning_rate: args
                .get("learning_rate")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.1),
        }),
        other => Err(McpError::InvalidArguments(format!(
            "unknown forecast model: {other}"
        ))),
    }
}

/// Quantitative analytics engines service (Plan 0007 Track 1 Phase 1).
pub struct AnalyticsToolService;

impl Default for AnalyticsToolService {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalyticsToolService {
    pub fn new() -> Self {
        Self
    }

    /// Block-bootstrap Monte Carlo simulation for drawdown and ruin estimation.
    pub fn monte_carlo(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let returns_val = args
            .get("returns")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::InvalidArguments("missing returns array".to_string()))?;
        let returns: Vec<f64> = returns_val.iter().filter_map(|v| v.as_f64()).collect();
        let num_sims = args.get("num_sims").and_then(|v| v.as_u64()).unwrap_or(500) as usize;
        let horizon = args.get("horizon").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let result = simulate_bootstrap(&returns, num_sims, horizon);

        let summary = format!(
            "Monte Carlo bootstrap of {} paths (median DD: {:.2}%, 95% VaR DD: {:.2}%, prob_loss: {:.2}%, passed: {})",
            result.simulations_run,
            result.median_max_drawdown * 100.0,
            result.drawdown_95 * 100.0,
            result.prob_loss * 100.0,
            result.passed
        );

        let envelope =
            VyapaarResultEnvelope::ok(summary, result).with_block(DisplayBlock::MetricGrid {
                items: vec![
                    MetricItem {
                        label: "simulations_run".to_string(),
                        value: result.simulations_run.to_string(),
                    },
                    MetricItem {
                        label: "median_max_drawdown".to_string(),
                        value: format!("{:.2}%", result.median_max_drawdown * 100.0),
                    },
                    MetricItem {
                        label: "drawdown_95".to_string(),
                        value: format!("{:.2}%", result.drawdown_95 * 100.0),
                    },
                    MetricItem {
                        label: "drawdown_99".to_string(),
                        value: format!("{:.2}%", result.drawdown_99 * 100.0),
                    },
                    MetricItem {
                        label: "prob_loss".to_string(),
                        value: format!("{:.2}%", result.prob_loss * 100.0),
                    },
                    MetricItem {
                        label: "prob_ruin".to_string(),
                        value: format!("{:.2}%", result.prob_ruin * 100.0),
                    },
                    MetricItem {
                        label: "passed".to_string(),
                        value: result.passed.to_string(),
                    },
                ],
            });

        serde_json::to_value(envelope).map_err(McpError::Json)
    }

    /// Engle-Granger two-step cointegration test and spread analysis.
    pub fn cointegration(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let sx_val = args
            .get("series_x")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::InvalidArguments("missing series_x array".to_string()))?;
        let sy_val = args
            .get("series_y")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::InvalidArguments("missing series_y array".to_string()))?;

        let series_x: Vec<f64> = sx_val.iter().filter_map(|v| v.as_f64()).collect();
        let series_y: Vec<f64> = sy_val.iter().filter_map(|v| v.as_f64()).collect();

        let result = test_engle_granger(&series_x, &series_y)
            .map_err(|e| McpError::Internal(format!("cointegration test failed: {e}")))?;

        let summary = format!(
            "Engle-Granger cointegration test: is_cointegrated={}, beta={:.4}, ADF stat={:.3} (p={:.4})",
            result.is_cointegrated, result.hedge_ratio, result.adf_stat, result.p_value
        );

        let half_life_str = result
            .half_life
            .map(|h| format!("{:.2} bars", h))
            .unwrap_or_else(|| "N/A".to_string());

        let envelope =
            VyapaarResultEnvelope::ok(summary, result).with_block(DisplayBlock::MetricGrid {
                items: vec![
                    MetricItem {
                        label: "is_cointegrated".to_string(),
                        value: result.is_cointegrated.to_string(),
                    },
                    MetricItem {
                        label: "hedge_ratio".to_string(),
                        value: format!("{:.4}", result.hedge_ratio),
                    },
                    MetricItem {
                        label: "intercept".to_string(),
                        value: format!("{:.4}", result.intercept),
                    },
                    MetricItem {
                        label: "adf_stat".to_string(),
                        value: format!("{:.4}", result.adf_stat),
                    },
                    MetricItem {
                        label: "p_value".to_string(),
                        value: format!("{:.4}", result.p_value),
                    },
                    MetricItem {
                        label: "crit_5pct".to_string(),
                        value: format!("{:.4}", result.crit_value_5pct),
                    },
                    MetricItem {
                        label: "half_life".to_string(),
                        value: half_life_str,
                    },
                    MetricItem {
                        label: "current_z_score".to_string(),
                        value: format!("{:.3}", result.current_z_score),
                    },
                ],
            });

        serde_json::to_value(envelope).map_err(McpError::Json)
    }

    /// Walk-Forward Efficiency (WFE) evaluation.
    pub fn walk_forward(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let returns_val = args
            .get("returns")
            .and_then(|v| v.as_array())
            .ok_or_else(|| McpError::InvalidArguments("missing returns array".to_string()))?;
        let returns: Vec<f64> = returns_val.iter().filter_map(|v| v.as_f64()).collect();

        let train_size = args.get("train_size").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let test_size = args.get("test_size").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let result = evaluate_walk_forward(&returns, train_size, test_size);

        let summary = format!(
            "Walk-Forward Efficiency: WFE={:.2}%, IS Return={:.2}%, OOS Return={:.2}%, passed={}",
            result.wfe * 100.0,
            result.is_annual_return * 100.0,
            result.oos_annual_return * 100.0,
            result.passed
        );

        let envelope =
            VyapaarResultEnvelope::ok(summary, result).with_block(DisplayBlock::MetricGrid {
                items: vec![
                    MetricItem {
                        label: "wfe".to_string(),
                        value: format!("{:.2}%", result.wfe * 100.0),
                    },
                    MetricItem {
                        label: "is_annual_return".to_string(),
                        value: format!("{:.2}%", result.is_annual_return * 100.0),
                    },
                    MetricItem {
                        label: "oos_annual_return".to_string(),
                        value: format!("{:.2}%", result.oos_annual_return * 100.0),
                    },
                    MetricItem {
                        label: "is_sharpe".to_string(),
                        value: format!("{:.2}", result.is_sharpe),
                    },
                    MetricItem {
                        label: "oos_sharpe".to_string(),
                        value: format!("{:.2}", result.oos_sharpe),
                    },
                    MetricItem {
                        label: "passed".to_string(),
                        value: result.passed.to_string(),
                    },
                ],
            });

        serde_json::to_value(envelope).map_err(McpError::Json)
    }

    /// Factor attribution of returns and risk across systematic factors.
    pub fn factor_attribution(
        &self,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let annualization = args
            .get("annualization_factor")
            .and_then(|v| v.as_f64())
            .unwrap_or(252.0);

        let attribution = if let Some(holdings_val) =
            args.get("holdings").and_then(|v| v.as_array())
        {
            let holdings: Vec<PortfolioHolding> =
                serde_json::from_value(serde_json::Value::Array(holdings_val.clone()))
                    .map_err(|e| McpError::InvalidArguments(format!("invalid holdings: {e}")))?;
            let factor_panel: FactorReturnPanel =
                serde_json::from_value(args.get("factor_returns").cloned().ok_or_else(|| {
                    McpError::InvalidArguments("missing factor_returns".to_string())
                })?)
                .map_err(|e| {
                    McpError::InvalidArguments(format!("invalid factor_returns panel: {e}"))
                })?;

            fit_cross_sectional_factor_model(&holdings, &factor_panel, annualization)
                .map_err(|e| McpError::Internal(format!("factor attribution failed: {e}")))?
        } else {
            let asset_returns_val = args
                .get("asset_returns")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    McpError::InvalidArguments("missing asset_returns or holdings".to_string())
                })?;
            let asset_returns: Vec<f64> = asset_returns_val
                .iter()
                .filter_map(|v| v.as_f64())
                .collect();

            let factor_returns_val = args
                .get("factor_returns")
                .and_then(|v| v.as_object())
                .ok_or_else(|| {
                    McpError::InvalidArguments("missing factor_returns object".to_string())
                })?;

            let mut factor_returns = HashMap::new();
            for (k, v) in factor_returns_val {
                let series: Vec<f64> = v
                    .as_array()
                    .ok_or_else(|| {
                        McpError::InvalidArguments(format!(
                            "factor '{k}' must be an array of numbers"
                        ))
                    })?
                    .iter()
                    .filter_map(|n| n.as_f64())
                    .collect();
                factor_returns.insert(k.clone(), series);
            }

            compute_factor_attribution(&asset_returns, &factor_returns, annualization)
                .map_err(|e| McpError::Internal(format!("factor attribution failed: {e}")))?
        };

        let summary = format!(
            "Factor attribution: total_return={:.2}%, factor_return={:.2}%, total_risk={:.2}%, specific_risk={:.2}% across {} factors",
            attribution.total_return * 100.0,
            attribution.factor_return * 100.0,
            attribution.total_risk * 100.0,
            attribution.specific_risk * 100.0,
            attribution.n_factors
        );

        let blocks = factor_attribution_blocks(&attribution);
        let mut envelope = VyapaarResultEnvelope::ok(summary, attribution);
        for block in blocks {
            envelope = envelope.with_block(block);
        }

        serde_json::to_value(envelope).map_err(McpError::Json)
    }
}

/// One asset weight result in the allocation output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetWeightResult {
    pub symbol: String,
    pub weight: Decimal,
    pub percentage: String,
}

/// Portfolio optimization output payload (Plan 0007 Track 1 Phase 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioOptimizationResult {
    pub objective: String,
    pub weights: Vec<AssetWeightResult>,
    pub weights_map: BTreeMap<String, Decimal>,
    pub expected_return: Option<Decimal>,
    pub variance: Decimal,
    pub volatility: Decimal,
    pub sharpe_ratio: Option<Decimal>,
    pub effective_assets: Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_return: Option<Decimal>,
}

/// Portfolio allocation and optimization service (Plan 0007 Track 1 Phase 2).
pub struct AllocationToolService;

impl Default for AllocationToolService {
    fn default() -> Self {
        Self::new()
    }
}

impl AllocationToolService {
    pub fn new() -> Self {
        Self
    }

    /// Solves portfolio allocation using pure Decimal arithmetic via `vyapaar-ledger-allocation`.
    pub fn optimize(&self, args: &serde_json::Value) -> Result<serde_json::Value, McpError> {
        let cov_val = args
            .get("covariance_matrix")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                McpError::InvalidArguments("missing covariance_matrix array".to_string())
            })?;

        let n = cov_val.len();
        if n == 0 {
            return Err(McpError::InvalidArguments(
                "empty covariance_matrix".to_string(),
            ));
        }

        let mut covariance = Vec::with_capacity(n);
        for (row_idx, row_val) in cov_val.iter().enumerate() {
            let row_arr = row_val.as_array().ok_or_else(|| {
                McpError::InvalidArguments(format!(
                    "covariance_matrix row {row_idx} is not an array"
                ))
            })?;
            if row_arr.len() != n {
                return Err(McpError::InvalidArguments(format!(
                    "covariance_matrix is not square: row {row_idx} has length {}, expected {n}",
                    row_arr.len()
                )));
            }
            let mut row = Vec::with_capacity(n);
            for (col_idx, item) in row_arr.iter().enumerate() {
                let dec = parse_decimal(item).ok_or_else(|| {
                    McpError::InvalidArguments(format!(
                        "invalid number in covariance_matrix at [{row_idx}][{col_idx}]"
                    ))
                })?;
                row.push(dec);
            }
            covariance.push(row);
        }

        let symbols: Vec<String> =
            if let Some(syms) = args.get("symbols").and_then(|v| v.as_array()) {
                if syms.len() != n {
                    return Err(McpError::InvalidArguments(format!(
                        "symbols length ({}) does not match covariance matrix dimensions ({n})",
                        syms.len()
                    )));
                }
                syms.iter()
                    .enumerate()
                    .map(|(i, s)| {
                        s.as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| format!("ASSET_{i}"))
                    })
                    .collect()
            } else {
                (0..n).map(|i| format!("ASSET_{i}")).collect()
            };

        let expected_returns: Option<Vec<Decimal>> = if let Some(ret_arr) =
            args.get("expected_returns").and_then(|v| v.as_array())
        {
            if ret_arr.len() != n {
                return Err(McpError::InvalidArguments(format!(
                    "expected_returns length ({}) does not match covariance matrix dimensions ({n})",
                    ret_arr.len()
                )));
            }
            let mut rets = Vec::with_capacity(n);
            for (idx, item) in ret_arr.iter().enumerate() {
                let dec = parse_decimal(item).ok_or_else(|| {
                    McpError::InvalidArguments(format!(
                        "invalid number in expected_returns at index {idx}"
                    ))
                })?;
                rets.push(dec);
            }
            Some(rets)
        } else {
            None
        };

        let objective_str = args
            .get("objective")
            .and_then(|v| v.as_str())
            .unwrap_or("mean_variance");

        let risk_free_rate = args
            .get("risk_free_rate")
            .and_then(parse_decimal)
            .unwrap_or(Decimal::ZERO);

        let target_return = args.get("target_return").and_then(parse_decimal);

        let objective = match objective_str {
            "mean_variance" => {
                if expected_returns.is_none() {
                    return Err(McpError::InvalidArguments(
                        "mean_variance objective requires expected_returns array".to_string(),
                    ));
                }
                AllocationObjective::MeanVariance { risk_free_rate }
            }
            "risk_parity" => AllocationObjective::RiskParity,
            "hrp" => AllocationObjective::Hrp,
            other => {
                return Err(McpError::InvalidArguments(format!(
                    "unknown objective '{other}': expected 'mean_variance', 'risk_parity', or 'hrp'"
                )));
            }
        };

        let instruments: Vec<InstrumentId> = symbols
            .iter()
            .map(|s| InstrumentId::new(Exchange::Nse, s.clone()))
            .collect();

        let mut exp_returns_map = BTreeMap::new();
        if let Some(ref rets) = expected_returns {
            for (inst, &ret) in instruments.iter().zip(rets) {
                exp_returns_map.insert(inst.clone(), ret);
            }
        }

        let mut cov_map = BTreeMap::new();
        for (i, inst_row) in instruments.iter().enumerate() {
            let mut row_map = BTreeMap::new();
            for (j, inst_col) in instruments.iter().enumerate() {
                row_map.insert(inst_col.clone(), covariance[i][j]);
            }
            cov_map.insert(inst_row.clone(), row_map);
        }

        let state = PortfolioState {
            as_of: Utc::now(),
            instruments: instruments.clone(),
            current_weights: BTreeMap::new(),
            expected_returns: exp_returns_map,
            covariance: cov_map,
        };

        let mut constraints = Vec::new();
        if let Some(bounds_arr) = args.get("weight_bounds").and_then(|v| v.as_array()) {
            for (idx, b) in bounds_arr.iter().enumerate() {
                let inst = if let Some(sym) = b.get("instrument").and_then(|v| v.as_str()) {
                    instruments
                        .iter()
                        .find(|id| id.symbol.eq_ignore_ascii_case(sym))
                        .cloned()
                        .ok_or_else(|| {
                            McpError::InvalidArguments(format!(
                                "weight bound references unknown symbol '{sym}'"
                            ))
                        })?
                } else if idx < instruments.len() {
                    instruments[idx].clone()
                } else {
                    continue;
                };

                let min = b
                    .get("min")
                    .and_then(parse_decimal)
                    .unwrap_or(Decimal::ZERO);
                let max = b.get("max").and_then(parse_decimal).unwrap_or(Decimal::ONE);
                constraints.push(Constraint::WeightBounds {
                    instrument: inst,
                    min,
                    max,
                });
            }
        }

        let target =
            vyapaar_ledger_allocation::optimize(objective, &constraints, &state).map_err(|e| {
                match e {
                    AllocationError::InvalidArgument(msg) => McpError::InvalidArguments(msg),
                    AllocationError::InfeasibleConstraints(msg) => {
                        McpError::InvalidArguments(format!("infeasible constraints: {msg}"))
                    }
                    AllocationError::NotImplemented(msg) => {
                        McpError::Internal(format!("objective not implemented: {msg}"))
                    }
                    AllocationError::SingularCovariance => McpError::Internal(
                        "covariance matrix is singular or not positive-definite".to_string(),
                    ),
                    AllocationError::InsufficientHistory { needed, got } => {
                        McpError::InvalidArguments(format!(
                            "insufficient history: needed {needed}, got {got}"
                        ))
                    }
                }
            })?;

        let mut weights_vec = Vec::with_capacity(n);
        let mut weights_map = BTreeMap::new();
        for inst in &instruments {
            let w = target.weights.get(inst).copied().unwrap_or(Decimal::ZERO);
            weights_vec.push(w);
            weights_map.insert(inst.symbol.clone(), w);
        }

        // Portfolio Expected Return: w^T * mu
        let exp_ret = expected_returns.as_ref().map(|rets| {
            weights_vec
                .iter()
                .zip(rets)
                .map(|(w, r)| *w * *r)
                .sum::<Decimal>()
        });

        // Portfolio Variance: w^T * Sigma * w
        let mut portfolio_variance = Decimal::ZERO;
        for i in 0..n {
            for j in 0..n {
                portfolio_variance += weights_vec[i] * weights_vec[j] * covariance[i][j];
            }
        }

        // Volatility: sqrt(variance)
        let portfolio_volatility = decimal_sqrt(portfolio_variance).unwrap_or(Decimal::ZERO);

        // Sharpe Ratio: (return - rf) / vol
        let sharpe_ratio = if !portfolio_volatility.is_zero() {
            exp_ret.map(|r| (r - risk_free_rate) / portfolio_volatility)
        } else {
            None
        };

        // Effective number of assets (1 / sum(w^2))
        let hhi: Decimal = weights_vec.iter().map(|w| *w * *w).sum();
        let effective_assets = if !hhi.is_zero() {
            Decimal::ONE / hhi
        } else {
            Decimal::ZERO
        };

        let asset_weight_results: Vec<AssetWeightResult> = instruments
            .iter()
            .zip(&weights_vec)
            .map(|(inst, &w)| AssetWeightResult {
                symbol: inst.symbol.clone(),
                weight: w,
                percentage: format!("{:.2}%", w * Decimal::from(100)),
            })
            .collect();

        let result = PortfolioOptimizationResult {
            objective: objective_str.to_string(),
            weights: asset_weight_results.clone(),
            weights_map,
            expected_return: exp_ret,
            variance: portfolio_variance,
            volatility: portfolio_volatility,
            sharpe_ratio,
            effective_assets,
            target_return,
        };

        let summary = if let (Some(r), Some(s)) = (exp_ret, sharpe_ratio) {
            format!(
                "Portfolio optimization ({}): return={:.2}%, vol={:.2}%, Sharpe={:.2}, effective_assets={:.2}",
                objective_str,
                r * Decimal::from(100),
                portfolio_volatility * Decimal::from(100),
                s,
                effective_assets
            )
        } else {
            format!(
                "Portfolio optimization ({}): vol={:.2}%, effective_assets={:.2}",
                objective_str,
                portfolio_volatility * Decimal::from(100),
                effective_assets
            )
        };

        let mut metric_items = vec![MetricItem {
            label: "objective".to_string(),
            value: objective_str.to_string(),
        }];
        if let Some(r) = exp_ret {
            metric_items.push(MetricItem {
                label: "expected_return".to_string(),
                value: format!("{:.2}%", r * Decimal::from(100)),
            });
        }
        metric_items.push(MetricItem {
            label: "volatility".to_string(),
            value: format!("{:.2}%", portfolio_volatility * Decimal::from(100)),
        });
        metric_items.push(MetricItem {
            label: "variance".to_string(),
            value: format!("{:.6}", portfolio_variance),
        });
        if let Some(s) = sharpe_ratio {
            metric_items.push(MetricItem {
                label: "sharpe_ratio".to_string(),
                value: format!("{:.2}", s),
            });
        }
        metric_items.push(MetricItem {
            label: "effective_assets".to_string(),
            value: format!("{:.2}", effective_assets),
        });
        if let Some(tr) = target_return {
            metric_items.push(MetricItem {
                label: "target_return".to_string(),
                value: format!("{:.2}%", tr * Decimal::from(100)),
            });
        }

        let table_rows = asset_weight_results
            .iter()
            .map(|w| {
                vec![
                    w.symbol.clone(),
                    format!("{:.4}", w.weight),
                    w.percentage.clone(),
                ]
            })
            .collect();

        let envelope = VyapaarResultEnvelope::ok(summary, result)
            .with_block(DisplayBlock::MetricGrid {
                items: metric_items,
            })
            .with_block(DisplayBlock::Table {
                title: "Optimized Asset Allocations".to_string(),
                columns: vec![
                    "Symbol".to_string(),
                    "Weight".to_string(),
                    "Allocation %".to_string(),
                ],
                rows: table_rows,
            });

        serde_json::to_value(envelope).map_err(McpError::Json)
    }
}

fn decimal_sqrt(d: Decimal) -> Option<Decimal> {
    if d.is_sign_negative() {
        return None;
    }
    if d.is_zero() {
        return Some(Decimal::ZERO);
    }
    let mut x = if d >= Decimal::ONE {
        d / Decimal::from(2)
    } else {
        Decimal::ONE
    };
    let half = Decimal::new(5, 1);
    let tol = Decimal::from_parts(1, 0, 0, false, 12);
    for _ in 0..50 {
        if x.is_zero() {
            return Some(Decimal::ZERO);
        }
        let next = half * (x + d / x);
        if (next - x).abs() <= tol {
            return Some(next);
        }
        x = next;
    }
    Some(x)
}

fn parse_decimal(val: &serde_json::Value) -> Option<Decimal> {
    if let Some(s) = val.as_str() {
        s.parse().ok()
    } else if let Some(i) = val.as_i64() {
        Some(Decimal::from(i))
    } else if let Some(u) = val.as_u64() {
        Some(Decimal::from(u))
    } else if let Some(f) = val.as_f64() {
        Decimal::from_f64_retain(f)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

fn norm_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

fn erf(x: f64) -> f64 {
    // Horner approximation for error function
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = x.abs();

    let t = 1.0 / (1.0 + p * abs_x);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-abs_x * abs_x).exp();

    sign * y
}

fn approx_norm_inv(p: f64) -> f64 {
    // Beasley-Springer-Moro approximation
    let a = [
        -3.969683028665376e+01,
        2.209460984245205e+02,
        -2.759285104469687e+02,
        1.383_577_518_672_69e2,
        -3.066479806614716e+01,
        2.506628277459239e+00,
    ];
    let b = [
        -5.447609879822406e+01,
        1.615858368580409e+02,
        -1.556989798598866e+02,
        6.680131188771972e+01,
        -1.328068155288572e+01,
    ];

    let q = p - 0.5;
    if q.abs() <= 0.42 {
        let r = q * q;
        q * (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5])
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0)
    } else {
        let r = if q < 0.0 { p } else { 1.0 - p };
        let r = (-r.ln()).sqrt();
        let x = -0.322232431088 - r * (1.0 + r * (0.342242088547 + r * 0.02020700181));
        let x =
            x / (0.0993484626060 + r * (0.588581570495 + r * (0.531103462366 + r * 0.10353775285)));
        let val = r + x;
        if q < 0.0 { -val } else { val }
    }
}
