# `vyapaar-agent` — Analytics, Genetic, Forecast, Panel & Report Reference

Documents `vyapaar-agent-analytics/`, `vyapaar-agent-genetic/`, `vyapaar-agent-forecast/`,
`vyapaar-agent-panel/`, and `vyapaar-agent-report/`.

## `vyapaar-agent-analytics`

Performance attribution, Deflated Sharpe Ratio (DSR), regime classification, and drawdown
analytics. Modules: `cointegration`, `cost_stress`, `dsr`, `equity_stats`, `factor_attribution`,
`holdout`, `math`, `metrics`, `monte_carlo`, `pbo`, `regime`, `report`, `round_trip`,
`tearsheet`, `trade_stats`, `validation`, `walk_forward`.

Key engines:
- **DSR** — Deflated Sharpe Ratio (multiple-testing adjustment for Sharpe).
- **PBO** — Probability of Backtest Overfitting.
- **Walk-forward** — out-of-sample validation.
- **Regime classification** — market regime detection.

## `vyapaar-agent-genetic`

StockSharp Odysseus-style genetic programming & rule-tree strategy synthesizer &
parameter mutator. Modules: `ast` (rule-tree AST), `genetic` (GA engine), `lib`,
`regime`, `synthesizer`.

## `vyapaar-agent-forecast`

Time-series forecasting. Modules: `arima`, `ets`, `features`, `gradient_boost`, `linalg`,
`tbats`, `theta`. Backs the MCP `forecast` tool.

## `vyapaar-agent-panel`

Econometric panel models. Modules: `asset_pricing`, `fama_macbeth` (2-pass regression),
`fixed_effects`, `gmm`, `matrix`, `random_effects`.

## `vyapaar-agent-report`

Embedded Typst / Markdown quantitative research tearsheet & PDF report generator consuming
`vyapaar-core` `Decimal` metrics. Modules: `lib`, `narrate`.

### Note (gap 0005)

`ResearchNote` (structured research note: hypothesis, evidence refs, verification outcome,
metrics) is planned but not implemented
(`docs/plans/0003-persistent-memory-and-mcp-client.md`, gap 0005).

## Zero-Floats Policy

All financial figures, quantities, prices, metrics, and drawdown numbers strictly use
`rust_decimal::Decimal` — no `f32`/`f64` in analytics/forecast/panel/report paths.

## Related

- `vyapaar-agent-core::result` — `VyapaarResultEnvelope` wraps tool outputs.
- `vyapaar-agent-core::indicators` — `IndicatorCatalog` (approved indicators vocabulary).
