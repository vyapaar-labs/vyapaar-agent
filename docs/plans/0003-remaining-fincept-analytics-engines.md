# 0003 — Remaining Fincept Analytics Engines Not Yet Scoped

> **Status**: Proposed — not yet implemented.
> **Target Crates**: new `vyapaar-agent-panel`, new `vyapaar-agent-forecast`, extend existing `vyapaar-agent-analytics`.
> **Depends on reading**:
> - `vyapaar-agent/docs/plans/0001-mcp-gateway-discovery-and-quantitative-engines.md`
> - `vyapaar-market/docs/plans/0001-technicals-breadth-and-derivatives-pricing.md`
> - `vyapaar-ledger/docs/plans/0001-portfolio-allocation-and-optimization.md`

---

## 0. Executive Summary & Objective

`0001`'s 15-engine list (market, technicals, scanner, analytics, validation, allocation, risk,
regimes, change_points, volatility, copula, ledger, calc, research, backtest) does not cover three
Fincept engines: **Panel** (fixed/random-effects econometrics, Fama-MacBeth, IV/GMM, factor
asset-pricing tests), **Forecast + TS Forecast** (ML and classical statistical time-series
forecasting of raw series), and the **cross-sectional factor-attribution half** of Fincept's
Portfolio Lab specifically. This document scopes exactly those three gaps, draws the boundary
against two things they are routinely confused with (hierarchical allocation and strategy
parameter optimization), and orders them by how directly useful each is to a live-trading
platform rather than a research desk.

**Explicitly out of scope for this doc**: Fincept's derivatives/options/bonds Pricing engine
(Black-Scholes Greeks). `vyapaar-market/docs/plans/0001-technicals-breadth-and-derivatives-pricing.md`
already proposes a `vyapaar-market-pricing` crate for this — that remains the single home for
Pricing. This doc does not propose a second one, and says so here explicitly so a reader who
notices Pricing is missing from the three engines below knows where it actually lives rather than
assuming it was dropped.

---

## 1. Context

Fincept's toolset includes several analytics engines that `0001`'s 15-engine partition left out
because they don't map cleanly onto Vyapaar's existing bounded contexts. Two in particular are
easy to conflate with work this workspace already covers elsewhere, so the boundary is worth
stating precisely before scoping new crates:

- **Not hierarchical/Black-Litterman allocation.** Fincept's Portfolio Lab bundles
  factor-attribution together with hierarchical risk-parity-style allocation (HRP/HERC) and
  Black-Litterman views. `vyapaar-ledger/docs/plans/0001-portfolio-allocation-and-optimization.md`
  already proposes a new `vyapaar-ledger-allocation` crate with a phased `AllocationObjective`
  covering HRP/HERC (its Phase 2) and Black-Litterman (its Phase 3). This doc does not propose a
  second home for that half of Portfolio Lab — only the Barra-style cross-sectional
  factor-model/attribution piece is a true gap, and it is scoped below as an extension to
  `vyapaar-agent-analytics`, not a new allocation engine.
- **Not strategy-parameter optimization or strategy-rule synthesis.** Forecast + TS Forecast is
  about forecasting *raw series* — prices, volumes, macro indicators — using ML (gradient-boosted
  trees) or classical statistical models (ARIMA/ETS/Theta/TBATS). This is distinct from
  `vyapaar-agent-genetic`'s strategy-rule synthesis (which searches a space of trading rules) and
  from `vyapaar-runtime-enhancer`'s strategy-parameter optimization (GA/Bayesian/WFO/DSR tuning of
  a strategy's own parameters). Both of those already forecast/optimize *strategies*; the new
  `vyapaar-agent-forecast` crate forecasts *series* that feed into strategies and signals, and
  does not touch either existing crate's responsibilities.

With those two distinctions drawn, the three genuine gaps are:

1. **Panel** — fixed/random-effects regression, Fama-MacBeth two-pass regression, instrumental
   variables / GMM estimation, and factor asset-pricing tests (e.g. GRS test). This is
   research-grade cross-sectional/panel econometrics over security-return panels, not anything a
   live-trading loop calls on a hot path.
2. **Forecast + TS Forecast** — ML-based (LightGBM/XGBoost/CatBoost over lag/rolling features) and
   classical statistical (ARIMA/ETS/Theta/TBATS) forecasting of a single series or a panel of
   series.
3. **Factor attribution** (the non-allocation half of Portfolio Lab) — Barra-style cross-sectional
   factor model fitting and performance/risk attribution of a portfolio's returns to a set of
   factor exposures.

### Priority ordering and why

Of the three, Forecast is the most directly useful to a live-trading platform: forecasting
price/macro series feeds the research loop and downstream strategy signals today. Factor
attribution is a natural, relatively small extension of the tearsheet/analytics surface the
platform already has. Panel econometrics is the lowest priority — it is useful for a research
desk validating factor models, but nothing in the live pipeline consumes it, and it can slip
without blocking anything else in this document set.

---

## 2. Component Specifications

### 2.1 `vyapaar-agent-forecast` (new crate, Phase 1)

Depends on `vyapaar-core` for canonical value objects and reuses the `vyapaar/result/v1` envelope
(`vyapaar-agent-core::result`) from `0001` for its MCP-facing outputs.

- **ML forecasters**: gradient-boosted-tree models (LightGBM/XGBoost/CatBoost equivalents) over
  lag and rolling-window features of the input series. Input is a single `InstrumentId`-keyed
  series or a panel of series; output is a point forecast plus prediction interval.
- **Statistical forecasters**: ARIMA, ETS (exponential smoothing), Theta, and TBATS, for series
  where a classical model is preferable to a learned one (shorter histories, interpretability
  requirements).
- **Scope boundary** (restated from §1): this crate forecasts raw series data — prices, volumes,
  macro indicators — not strategy rules (`vyapaar-agent-genetic`) and not strategy parameters
  (`vyapaar-runtime-enhancer`). A forecast produced here may feed either of those as an input
  signal, but this crate has no dependency on either.
- Exposed as a new `forecast` engine namespace discoverable via `vyapaar_search_tools` /
  `vyapaar_describe_tool` per `0001`'s two-tier gateway protocol.

### 2.2 `factor_attribution.rs` extension to `vyapaar-agent-analytics` (Phase 2)

`vyapaar-agent-analytics` already contains `tearsheet.rs`, `dsr.rs`, `pbo.rs`, `equity_stats.rs`,
and `cointegration.rs`. Add a new `factor_attribution.rs` module alongside these:

- Fits a cross-sectional factor model (Barra-style: style/sector/market factors) against a
  portfolio's holdings and a factor-return panel.
- Produces attribution of portfolio return and risk to factor exposures, in the same
  `DisplayBlock`/`MetricGrid` shapes `0001` §2.2 defines, so it renders identically to the
  existing tearsheet output in any MCP client.
- Depends only on existing `vyapaar-agent-analytics` dependencies plus whatever factor-return
  panel source `vyapaar-data` exposes (no new crate dependency).

### 2.3 `vyapaar-agent-panel` (new crate, Phase 3)

Depends on `vyapaar-core` only.

- Fixed-effects and random-effects panel regression over security-return panels.
- Fama-MacBeth two-pass cross-sectional regression.
- Instrumental-variables / GMM estimation for endogeneity-corrected factor-return estimates.
- Factor asset-pricing tests (e.g. a GRS-style joint test of pricing-error significance).
- Lowest implementation priority of the three engines in this document — research-grade
  econometrics with no live-pipeline consumer today.

---

## 3. Implementation Phases

### Phase 1: `vyapaar-agent-forecast`
- New crate scaffold, `vyapaar-core` dependency only.
- ML forecaster backends (gradient-boosted trees over lag/rolling features).
- Classical statistical forecaster backends (ARIMA/ETS/Theta/TBATS).
- MCP discovery wiring via `vyapaar-agent-mcp`'s two-tier gateway (`0001` §2.1), new `forecast`
  category added to `vyapaar_search_tools`'s category enum.
- Rationale: most directly useful of the three — forecasting price/macro series feeds the
  research loop and strategy signals immediately.

### Phase 2: `factor_attribution.rs` extension to `vyapaar-agent-analytics`
- New module alongside `tearsheet.rs`, `dsr.rs`, `pbo.rs`, `equity_stats.rs`, `cointegration.rs`.
- Cross-sectional factor model fit and attribution output in existing `DisplayBlock` shapes.

### Phase 3: `vyapaar-agent-panel`
- New crate scaffold, `vyapaar-core` dependency only.
- Fixed/random-effects regression, Fama-MacBeth, IV/GMM, factor asset-pricing tests.
- Lowest priority — can slip past Phases 1 and 2 without blocking either.
