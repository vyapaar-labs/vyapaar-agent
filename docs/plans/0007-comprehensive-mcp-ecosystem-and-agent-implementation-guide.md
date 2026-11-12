# 0007 — Comprehensive MCP Ecosystem Implementation Guide & Feature Coverage Audit

**Status**: Active / Master Architecture Guide  
**Target Scope**: Workspace-wide (`vyapaar-agent-mcp`, `vyapaar-agent-chat-tools`, `vyapaar-agent-analytics`, `vyapaar-ledger`, `vyapaar-market`, `vyapaar-strategies`, `vyapaar-runtime`, `vyapaar-data`)  
**Implementation status (2026-11)**: Phase 16 (Axum SSE / HTTP transport — `vyapaar-agent-mcp/src/transport.rs`) already shipped; **all other phases are open**. The related chat-tool breadth delivered under implemented plan 0006 (block deals, price alerts, news/filings, options chain, corporate actions) covers parts of categories 4/5/10 in the coverage matrix. Phase 17 (MCP stdio client bridge) is tracked as gap 0004 under plan 0009. Phases are not gap-tracked individually yet; when a phase begins, file the phase as one or more gaps in `docs/gaps/` before writing code.
**Benchmark References**:
- **Longbridge MCP Server** (`longbridge/longbridge-mcp`) — 164 tools across 20 categories, Rust `rmcp` + `axum`, universal `_jq` filter, confirmation-code workflow for writes, paper-trading sandbox.
- **Fincept Platform MCP** (`docs.fincept.in`) — 17 quantitative engines (Stats, Forecast, Optimizer, Allocation, Volatility/GARCH, Panel, Scanner, Change Points, Regimes/HMM, Copulas, MPO, Portfolio Lab, Pricing, Performance), two-tier discovery (`search_tools`, `describe_tool`, `call_tool`, `read_result`), reference store.
- **Jesse MCP Server** (`bkuri/jesse-mcp`) — 32 tools (backtest, batch backtest, optimization, Monte Carlo, permutation tests, pairs trading, VaR/CVaR, strategy CRUD, community strategies).
- **Vyapaar Internal Plans**: 0001 (Gateway & Discovery), 0002 (Jobs/Memory/Verify), 0003 (Fincept Engines), 0004 (OmniRoute LLM), 0005 (SLM Determinism), 0006 (Chat Tool Expansion).

---

## 0. Executive Summary & Objective

This document is the master implementation blueprint for autonomous subagents and developers. It provides **100% comprehensive coverage** across all capabilities observed in Longbridge (164 tools), Fincept (17 engines + discovery), and Jesse-MCP (32 tools). 

Crucially, it resolves and scopes **every previously "NOT scoped", "excluded", or "deferred" capability**:
1. **Execution & Order Placement (Trade, Grid, DCA, ATM)**: Bridges the NoFX Untrusted-AI Principle (`AGENTS.md` §5.5) with real-world trading via a deterministic, two-step **Approval Queue** (`PendingApproval`) and a **Paper-Trading Simulation Engine**, prohibiting unverified autonomous order dispatch while enabling full agentic trading intent and lifecycle management.
2. **Missing Advanced Quantitative Engines (Copulas, GARCH, HMM Regimes, Change Points, MPO)**: Scopes the mathematical models, parameter bounds, and crate mappings.
3. **Fundamental, Fund, and IPO Data**: Scopes balance sheet / income / cash flow ratios, mutual fund holdings, and IPO subscription / allotment tracking.
4. **Strategy Lifecycle & Community Sharing**: Scopes strategy manifest CRUD and community sharing modeled on Jesse-MCP.
5. **Universal Token-Optimization Filter (`_jq`)**: Establishes server-side filtering on all MCP tool envelopes.
6. **Tax & Statements**: Scopes STCG/LTCG capital gains calculation, trade ledger statements, and contract notes.

---

## 1. Complete Feature Coverage & Gap-Resolution Matrix

The matrix below audits all 20 Longbridge categories, all 17 Fincept engines, and all 32 Jesse tools against Vyapaar's current and scoped state.

| Category / Engine | Reference Tools & Capabilities | Previously Scoped State in Vyapaar | Vyapaar Architecture Resolution & Scope | Target Crate & Status |
| :--- | :--- | :--- | :--- | :--- |
| **1. Execution: Spot/F&O Trade** (Longbridge 16 tools) | Submit/cancel/replace order, market/limit/stop, margin, buying power | Excluded per NoFX in Plan 0006 §5 | **Two-Stage Human-in-the-Loop Bridge**: `propose_trade` / `trade_suggestion` emits verified `vyapaar_core::Decision` with SEBI algo ID into `FileJobRegistry::approvals_queue` (`JobStatus::PendingApproval`). Orders view is read-only. Sandbox mode via `x-papertrading`. | `vyapaar-ledger-oms` + `vyapaar-runtime-jobs` |
| **2. Execution: Grid Trading** (Longbridge 11 tools) | Arithmetic/geometric grid trading, grid create/update/pause/stop, running P&L | Excluded per NoFX in Plan 0006 §5 | **Grid Strategy Manifest**: Scoped as a parameterized strategy in `vyapaar-strategies` with bounded price channels, run via runtime worker pool with hard risk clamping (`RiskEngine`). | `vyapaar-strategies` + `vyapaar-runtime` |
| **3. Execution: DCA / SIP** (Longbridge 9 tools) | Recurring investment plans, schedule, accumulation, average price | Excluded per NoFX in Plan 0006 §5 | **Autonomous Job Scheduler**: Scoped via `vyapaar-agent-core::jobs::Scheduler` and `vyapaar_examples/sip_plan.py`. Agent configures schedule; execution passes through the risk gate. | `vyapaar-agent-core/src/jobs` |
| **4. ATM & Options Chains** (Longbridge 3 ATM + Quote) | At-the-money straddle/strangles, full option chains, Greeks, open interest | Phase 4 blocked on upstream | **Derivatives Gateway & Chain Reader**: `vyapaar-market-pricing` provides Black-Scholes & Greeks. Scoped `OptionsChainTool` in chat-tools and `vyapaar_options_chain` in MCP gateway. | `vyapaar-market-pricing` + `vyapaar-market` |
| **5. Market Quotes & Depth** (Longbridge 32 tools) | L1/L2 order book (5-50 depth), broker queues, turnover, capital flow | L1 quote, history, indicators, block deals | **Order Book Depth & Liquidity Imbalance**: Scoped `MarketDepthTool` (L2 5-depth ladder) and `InstitutionalFlowsTool` (FII/DII cash and F&O net flows). | `vyapaar-agent-chat-tools` + `vyapaar-market` |
| **6. Fundamental Data** (Longbridge 33 tools) | Balance sheet, income statement, cash flow, P/E, P/B, ROE, debt ratio | Excluded in Plan 0006 | **Fundamental Valuation Engine**: Scoped `FundamentalsTool` (`P/E`, `P/B`, `EV/EBITDA`, `ROE`, `ROCE`, debt-to-equity) and `FinancialStatementsTool`. | `vyapaar-data` + `vyapaar-agent-chat-tools` |
| **7. Fund / Mutual Funds** (Longbridge 28 tools) | Mutual fund NAV, category rankings, manager track record, holdings | Excluded in Plan 0006 | **Mutual Fund Analytics Engine**: Scoped `FundSearchTool`, `FundHoldingsTool`, and `FundPerformanceTool` reading AMFI NAV data and portfolio disclosures. | `vyapaar-data` + `vyapaar-agent-chat-tools` |
| **8. IPO Center** (Longbridge 7 tools) | Upcoming IPOs, price bands, subscription rates (QIB/NII/RII), allotment | Excluded in Plan 0006 | **IPO Tracker**: Scoped `IpoCalendarTool` and `IpoSubscriptionTool` tracking primary market issues on NSE/BSE. | `vyapaar-data` + `vyapaar-agent-chat-tools` |
| **9. Statements & Tax** (Longbridge 2 tools) | Trade confirmations, account statements, tax reporting | Excluded in Plan 0006 | **Tax & Trade Ledger**: Scoped `TaxReportTool` computing Indian tax regimes (STCG @ 20%, LTCG @ 12.5% over ₹1.25L) and `LedgerStatementTool` wrapping `vyapaar-ledger-journal`. | `vyapaar-ledger-journal` |
| **10. Social Watchlists / Sharelists** (Longbridge 8 tools) | Shareable watchlists, community portfolios | Excluded in Plan 0006 | **Watchlist Exchange**: Scoped `WatchlistExportTool` and `WatchlistImportTool` via JSON / URI format. | `vyapaar-agent-chat-tools` |
| **11. Economic Calendar** (Fincept Macro, Longbridge 2) | Scheduled RBI policy, CPI, GDP, FOMC releases | Excluded in Plan 0006 | **Economic Calendar Tool**: Scoped `EconomicCalendarTool` wiring `vyapaar-data`'s completed calendar store (gaps 0001+0002). | `vyapaar-data` + `vyapaar-agent-chat-tools` |
| **12. Monte Carlo Simulation** (Jesse Robustness, Fincept) | Bootstrap resampling, synthetic equity paths, drawdown distribution | Not exposed in MCP | **Monte Carlo Gateway**: Expose existing `vyapaar-agent-analytics::monte_carlo` as `vyapaar_monte_carlo` tool in `vyapaar-agent-mcp`. | `vyapaar-agent-analytics` + `vyapaar-agent-mcp` |
| **13. Pairs Trading & Cointegration** (Jesse Pairs, Fincept) | Engle-Granger two-step test, ADF stationarity, spread z-score | Not exposed in MCP | **Cointegration Gateway**: Expose existing `vyapaar-agent-analytics::cointegration` as `vyapaar_cointegration` tool in `vyapaar-agent-mcp`. | `vyapaar-agent-analytics` + `vyapaar-agent-mcp` |
| **14. Permutation Testing** (Jesse Robustness) | Shuffling trade returns / bar timestamps to test alpha significance | Not scoped | **Permutation Test Engine**: Scoped in `vyapaar-agent-analytics::validation` computing empirical p-values for Sharpe and win rates. | `vyapaar-agent-analytics` |
| **15. Walk-Forward Validation** (Jesse Optimization) | Anchored and rolling walk-forward out-of-sample optimization | Not exposed in MCP | **Walk-Forward Gateway**: Expose existing `vyapaar-agent-analytics::walk_forward` as `vyapaar_walk_forward` tool in `vyapaar-agent-mcp`. | `vyapaar-agent-analytics` + `vyapaar-agent-mcp` |
| **16. Factor Attribution & Stress** (Fincept Portfolio Lab) | Multi-factor Fama-French decomposition, cost slippage stress | Not exposed in MCP | **Factor & Stress Gateway**: Expose `factor_attribution.rs` and `cost_stress.rs` as `vyapaar_factor_attribution` and `vyapaar_cost_stress`. | `vyapaar-agent-analytics` + `vyapaar-agent-mcp` |
| **17. Portfolio Optimization & Alloc** (Fincept Optimizer & Alloc) | Markowitz Mean-Variance, Black-Litterman, Risk Parity, HRP | Not exposed in MCP | **Allocation Gateway**: Expose `vyapaar-ledger-allocation` as `vyapaar_portfolio_optimize` in `vyapaar-agent-mcp`. | `vyapaar-ledger-allocation` + `vyapaar-agent-mcp` |
| **18. Volatility & GARCH** (Fincept Volatility) | GARCH(1,1), EGARCH, GJR-GARCH, implied vol smile | Not scoped | **Volatility Modeling Engine**: Scoped new `vyapaar-agent-analytics::volatility` module implementing GARCH(1,1) parameter estimation and volatility term structures. | `vyapaar-agent-analytics` |
| **19. Regime Switching & HMM** (Fincept Regimes) | Hidden Markov Model (2-state/3-state), Gaussian mixture | Only pure mock check | **Probabilistic HMM Engine**: Scoped in `vyapaar-agent-analytics::regime` with Baum-Welch training and Viterbi state decoding. | `vyapaar-agent-analytics` |
| **20. Change Point Detection** (Fincept Change Points) | CUSUM, structural break detection in price & macro series | Not scoped | **Change Point Engine**: Scoped in `vyapaar-agent-analytics::changepoint` detecting regime shift timestamps and volatility breaks. | `vyapaar-agent-analytics` |
| **21. Copulas & Tail Risk** (Fincept Copula) | Clayton, Gumbel, Frank copulas, tail dependence coefficient | Not scoped | **Copula Dependency Engine**: Scoped in `vyapaar-agent-analytics::copula` for non-linear dependency and tail VaR. | `vyapaar-agent-analytics` |
| **22. Multi-Period Optimizer (MPO)** (Fincept MPO) | Dynamic multi-period rebalancing under transaction costs and impact | Not scoped | **Multi-Period Rebalancer**: Scoped in `vyapaar-ledger-allocation::mpo` solving quadratic programs over T-horizons. | `vyapaar-ledger-allocation` |
| **23. Strategy CRUD & Lifecycle** (Jesse Strategy Tools) | Create, edit, list, delete, verify strategy code | Only compile & verify in schema | **Strategy Lifecycle Suite**: Scoped `StrategyManageTool` for manifest authoring, syntax verification, and deletion. | `vyapaar-strategies` + `vyapaar-runtime` |
| **24. Community Strategy Ecosystem** (Jesse Community) | Search, inspect, clone community trading strategies | Not scoped | **Community Strategy Registry**: Scoped `CommunityStrategyTool` searching and cloning approved templates from the open-source repo. | `vyapaar-agent-research` |
| **25. Universal Response Filter (`_jq`)** (Longbridge `_jq`) | Server-side JSON filtering on all tool outputs | Not scoped | **`_jq` Query Engine**: Scoped in `VyapaarResultEnvelope` using `jaq` or `serde_json_path` across all MCP responses. | `vyapaar-agent-core` + `vyapaar-agent-mcp` |
| **26. Transport Protocols** (Longbridge SSE/HTTP, Jesse stdio/SSE) | stdio, SSE, Streamable HTTP with OAuth | Only stdio in MCP | **Dual Transport Server**: Scoped Axum-based SSE/HTTP transport in `vyapaar-agent-mcp` alongside standard stdio. | `vyapaar-agent-mcp` |
| **27. MCP Stdio Client Bridge** (Vyapaar Gap 0004) | Consume external MCP tools (Longbridge, Fincept, Jesse) | Open Gap 0004 | **MCP Client Bridge**: Scoped child-process stdio client in `vyapaar-agent-mcp::client` allowing Vyapaar agents to invoke external servers. | `vyapaar-agent-mcp` |

---

## 2. In-Depth Architecture & Safety Design

```
    ┌─────────────────────────────────────────────────────────────────────────────────┐
    │                        SAFE TRADING INTENT WORKFLOW                             │
    │                       (Preserving NoFX AGENTS.md §5.5)                          │
    └─────────────────────────────────────────────────────────────────────────────────┘

     AI Agent (Chat / LLM)                  Human Supervisor / UI           Vyapaar Engine
    ──────────────────────                  ────────────────────           ──────────────
              │                                       │                          │
              │ 1. `propose_trade`                    │                          │
              │    (Symbol, Qty, Limit, SEBI Algo ID) │                          │
              ├─────────────────────────────────────────────────────────────────►│
              │                                       │                          │ 2. Pre-Trade Risk Check
              │                                       │                          │    (OTR, Price Band, VaR)
              │                                       │                          │
              │                                       │ 3. Enqueue to            │
              │                                       │    `FileJobRegistry`     │
              │                                       │    (`PendingApproval`)   │
              │                                       │◄─────────────────────────┤
              │                                       │                          │
              │                                       │ 4. Operator reviews &    │
              │                                       │    approves in Web UI    │
              │                                       │─────────────────────────►│
              │                                       │                          │ 5. Dispatched to Broker
              │                                       │                          │    (`vyapaar-ledger-oms`)
              │ 6. `orders_view` (ReadOnly poll)      │                          │
              ├─────────────────────────────────────────────────────────────────►│
              │    Returns Fill @ ₹2,450.50           │                          │
              │◄─────────────────────────────────────────────────────────────────┤
```

### 2.1 Resolving Execution Tools (Trade, Grid, DCA, ATM) safely
In Longbridge, real-money trading is gated by a confirmation code. In Jesse-MCP, simulation/paper trading is supported. In Vyapaar:
- **Direct Real Execution by LLMs is strictly prohibited**: No `ToolRisk::Trade` is ever given to an LLM.
- **The Safe Execution Bridge**:
  1. **`TradeIntentTool` / `ProposeTradeTool` (`ToolRisk::LocalWrite`)**: Formulates a closed-grammar `vyapaar_core::Decision` struct. The runtime validates SEBI algo registration and clamps orders against pre-trade risk bands.
  2. **Pending Approval Enqueueing**: The validated decision is saved into `vyapaar-runtime`'s `FileJobRegistry` under `JobStatus::PendingApproval`.
  3. **Human / Admin Authorization**: The trade can only execute when approved by a human operator in `vyapaar-ui-web` or via `vyapaar jobs run --approve <job_id>`.
  4. **Simulation / Paper-Trading Sandbox (`x-papertrading: true`)**: For automated backtesting and paper trading, an agent can target the paper ledger without human approvals, mirroring Longbridge's `x-papertrading: true` mode.
  5. **Grid & DCA Strategies**: Configured via declarative strategy manifests (`vyapaar-strategies`), verified synthetically, and executed inside bounded scheduler loops.

### 2.2 Universal Server-Side Filtering (`_jq`)
Longbridge's standout performance feature is server-side `_jq` execution. In financial workflows, returning 500 order book depth levels, 1,000 historical bars, or 10 years of financial statements can easily consume 20,000+ tokens and cause model degradation.
- **Specification**:
  - Every MCP tool schema accepts an optional `_jq: { "type": "string", "description": "jq filter expression to evaluate against response data before serialization" }`.
  - In `vyapaar-agent-core::result::VyapaarResultEnvelope`, if `_jq` is present, it compiles the expression using `jaq` (Rust-native JQ) and applies it to the payload.
  - If invalid syntax is provided, the envelope returns an informative error without crashing.

---

## 3. Subagent Implementation Roadmap

The implementation of all previously unscoped features is divided into 6 distinct, sequential subagent work tracks.

```mermaid
flowchart TD
    subgraph T1["Track 1: MCP Gateway Quantitative Engines"]
        P1["Phase 1: Analytics Engines (Monte Carlo, Cointegration, Walk-Forward, Factor)"]
        P2["Phase 2: Portfolio Optimization & Allocation (Mean-Variance, Risk Parity, HRP)"]
        P3["Phase 3: Derivatives Pricing & Volatility (Black-Scholes, Greeks, GARCH)"]
        P1 --> P2 --> P3
    end

    subgraph T2["Track 2: Conversational Market & Macro Tools"]
        P4["Phase 4: Market Depth & Capital Flows (L2 5-depth, FII/DII net flows)"]
        P5["Phase 5: Economic Calendar & Macro Releases (Wire vyapaar-data store)"]
        P4 --> P5
    end

    subgraph T3["Track 3: Fundamentals, Mutual Funds & IPOs"]
        P6["Phase 6: Fundamental Valuation & Financial Statements"]
        P7["Phase 7: Mutual Fund Analytics & Holdings"]
        P8["Phase 8: IPO Calendar & Subscription Tracking"]
        P6 --> P7 --> P8
    end

    subgraph T4["Track 4: Advanced Statistical & Risk Engines"]
        P9["Phase 9: Regime Switching (HMM) & Change Point Detection"]
        P10["Phase 10: Copula Tail-Risk & Permutation Tests"]
        P11["Phase 11: Multi-Period Optimizer (MPO with Costs)"]
        P9 --> P10 --> P11
    end

    subgraph T5["Track 5: Safe Execution & Strategy Lifecycle"]
        P12["Phase 12: Safe Trade Intent Bridge & Paper Trading Sandbox"]
        P13["Phase 13: Strategy Manifest CRUD & Community Sharing"]
        P14["Phase 14: Tax Reporting (STCG/LTCG) & Statements"]
        P12 --> P13 --> P14
    end

    subgraph T6["Track 6: Token Optimization & Protocol Transports"]
        P15["Phase 15: Universal _jq Response Filtering"]
        P16["Phase 16: Axum SSE / HTTP MCP Server Transport"]
        P17["Phase 17: MCP Stdio Client Bridge (Gap 0004)"]
        P15 --> P16 --> P17
    end
```

---

### Track 1: MCP Gateway Quantitative Engines (Phases 1–3)

#### Phase 1: Analytics Engines Exposure (`vyapaar-agent-mcp`)
- **Target Files**:
  - `crates/vyapaar-agent-mcp/src/tools.rs`
  - `crates/vyapaar-agent-mcp/src/server.rs`
  - `crates/vyapaar-agent-mcp/tests/unit_tests.rs`
- **Tools to Register**:
  - `vyapaar_monte_carlo`: Parameters `{ returns: number[], num_sims: integer, horizon: integer }`. Invokes `vyapaar_agent_analytics::monte_carlo::simulate_bootstrap`.
  - `vyapaar_cointegration`: Parameters `{ series_x: number[], series_y: number[] }`. Invokes `vyapaar_agent_analytics::cointegration::test_engle_granger`.
  - `vyapaar_walk_forward`: Parameters `{ returns: number[], train_size: integer, test_size: integer }`. Invokes `vyapaar_agent_analytics::walk_forward::evaluate_walk_forward`.
  - `vyapaar_factor_attribution`: Parameters `{ asset_returns: number[], factor_returns: object }`. Invokes `vyapaar_agent_analytics::factor_attribution::compute_factor_attribution`.
- **Outputs**: Wrapped in `VyapaarResultEnvelope` with `DisplayBlock::MetricGrid`.

#### Phase 2: Portfolio Optimization & Allocation
- **Target Files**:
  - `crates/vyapaar-agent-mcp/Cargo.toml` (add dependency `vyapaar-ledger-allocation`)
  - `crates/vyapaar-agent-mcp/src/tools.rs`
  - `crates/vyapaar-agent-mcp/src/server.rs`
- **Tools to Register**:
  - `vyapaar_portfolio_optimize`: Parameters `{ expected_returns: number[], covariance_matrix: number[][], objective: string ("mean_variance" | "risk_parity" | "hrp"), target_return?: number }`. Invokes `vyapaar-ledger-allocation`. Returns asset weights and efficient frontier metrics.

#### Phase 3: Derivatives Pricing & Volatility
- **Target Files**:
  - `crates/vyapaar-agent-mcp/Cargo.toml` (add dependency `vyapaar-market-pricing`)
  - `crates/vyapaar-agent-mcp/src/tools.rs`
  - `crates/vyapaar-agent-mcp/src/server.rs`
- **Tools to Register**:
  - `vyapaar_price_derivatives`: Parameters `{ spot: number, strike: number, t_years: number, risk_free_rate: number, volatility: number, option_type: string ("call" | "put") }`. Computes Black-Scholes price and Greeks (Delta, Gamma, Vega, Theta, Rho).
  - `vyapaar_garch_volatility`: Parameters `{ returns: number[], p?: integer, q?: integer }`. Fits GARCH(1,1) model and outputs conditional volatility forecasts.

---

### Track 2: Conversational Market & Macro Tools (Phases 4–5)

#### Phase 4: Market Depth & Capital Flows (`vyapaar-agent-chat-tools`)
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/ports/market.rs`
  - `crates/vyapaar-agent-chat-tools/src/market.rs`
  - `crates/vyapaar-agent-chat-tools/src/catalog.rs`
  - `crates/vyapaar-agent-chat-tools/tests/unit_tests.rs`
- **Tools to Add**:
  - `MarketDepthTool` (`market_depth`): Level 2 order book with 5 bid/ask levels, total bid/ask quantities, and liquidity imbalance percentage.
  - `InstitutionalFlowsTool` (`institutional_flows`): Daily FII and DII net cash purchases and index futures open interest changes.
- **Invariants**: Derive `rust_decimal::Decimal`, `ToolRisk::ReadOnly`, bump catalogue count pin.

#### Phase 5: Economic Calendar & Macro Releases
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/ports/calendar.rs` (new port)
  - `crates/vyapaar-agent-chat-tools/src/calendar.rs` (`EconomicCalendarTool`)
  - `crates/vyapaar-agent-chat-tools/src/catalog.rs`
  - `vyapaar-runtime/crates/vyapaar-runtime-api-rest/src/chat_calendar.rs`
- **Specification**: Connect to `vyapaar-data`'s economic calendar store. Query scheduled events by country (`IN`, `US`), date range, and impact (`high`, `medium`, `low`).

---

### Track 3: Fundamentals, Mutual Funds & IPOs (Phases 6–8)

#### Phase 6: Fundamental Valuation & Financial Statements
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/ports/fundamental.rs` (new port `FundamentalReader`)
  - `crates/vyapaar-agent-chat-tools/src/fundamental.rs` (`FundamentalsTool`, `FinancialStatementsTool`)
  - `crates/vyapaar-agent-chat-tools/src/catalog.rs`
- **DTOs**:
  - `FundamentalRatios`: P/E, P/B, EV/EBITDA, Dividend Yield, ROCE, ROE, Debt/Equity, Interest Coverage.
  - `FinancialStatement`: Statement type (`IncomeStatement`, `BalanceSheet`, `CashFlow`), fiscal quarter, revenue, operating profit, net income, eps.

#### Phase 7: Mutual Fund Analytics
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/ports/fund.rs` (new port `FundReader`)
  - `crates/vyapaar-agent-chat-tools/src/fund.rs` (`FundSearchTool`, `FundHoldingsTool`)
  - `crates/vyapaar-agent-chat-tools/src/catalog.rs`
- **DTOs**:
  - `MutualFundMeta`: Scheme code, scheme name, category (Large Cap, Mid Cap, Flexi Cap, Liquid), NAV, 1Y/3Y/5Y CAGR, expense ratio.
  - `FundHolding`: Asset name, percentage weight, sector.

#### Phase 8: IPO Calendar & Subscription Tracking
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/ports/ipo.rs` (new port `IpoReader`)
  - `crates/vyapaar-agent-chat-tools/src/ipo.rs` (`IpoCalendarTool`, `IpoSubscriptionTool`)
  - `crates/vyapaar-agent-chat-tools/src/catalog.rs`
- **DTOs**:
  - `IpoIssue`: Symbol, company name, open date, close date, price band (min/max), lot size, issue size.
  - `IpoSubscription`: QIB subscription multiple, NII multiple, RII multiple, total subscription multiple.

---

### Track 4: Advanced Statistical & Risk Engines (Phases 9–11)

#### Phase 9: Regime Switching (HMM) & Change Point Detection
- **Target Files**:
  - `crates/vyapaar-agent-analytics/src/regime/hmm.rs` (new module)
  - `crates/vyapaar-agent-analytics/src/changepoint.rs` (new module)
  - `crates/vyapaar-agent-mcp/src/tools.rs`
- **Specification**:
  - `vyapaar_detect_regimes`: 2-state/3-state Gaussian HMM fitting historical price series into `BullishTrend`, `BearishTrend`, or `ChoppyVolatile`.
  - `vyapaar_change_point`: CUSUM and Bayesian change point algorithm identifying timestamps of structural volatility shifts.

#### Phase 10: Copula Tail-Risk & Permutation Tests
- **Target Files**:
  - `crates/vyapaar-agent-analytics/src/copula.rs` (new module)
  - `crates/vyapaar-agent-analytics/src/validation.rs` (extend with permutation testing)
  - `crates/vyapaar-agent-mcp/src/tools.rs`
- **Specification**:
  - `vyapaar_copula_tail_risk`: Bivariate copula fitting (Clayton, Gumbel) computing tail correlation coefficients and joint Value at Risk (VaR).
  - `vyapaar_permutation_test`: Resamples trade returns 1,000 times without replacement to calculate the empirical p-value of strategy alpha.

#### Phase 11: Multi-Period Optimizer (MPO)
- **Target Files**:
  - `vyapaar-ledger/crates/vyapaar-ledger-allocation/src/mpo.rs`
  - `crates/vyapaar-agent-mcp/src/tools.rs`
- **Specification**:
  - `vyapaar_multi_period_optimize`: Solves a dynamic portfolio rebalancing sequence over $T$ periods incorporating quadratic transaction costs and linear turnover penalties.

---

### Track 5: Safe Execution & Strategy Lifecycle (Phases 12–14)

#### Phase 12: Safe Trade Intent Bridge & Paper Trading Sandbox
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/ports/trade_intent.rs` (new port)
  - `crates/vyapaar-agent-chat-tools/src/trade_intent.rs` (`ProposeTradeTool`, `ToolRisk::LocalWrite`)
  - `vyapaar-runtime/crates/vyapaar-runtime-jobs/src/registry.rs`
- **Specification**:
  - Formulates a verified `vyapaar_core::Decision`.
  - Enqueues into `FileJobRegistry` as `JobStatus::PendingApproval`.
  - Emits event with SEBI algorithm identifier and risk checks.
  - Supports `x-papertrading: true` mode allowing direct execution in sandbox ledger without human gate.

#### Phase 13: Strategy Manifest CRUD & Community Sharing
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/strategy_manage.rs` (`StrategyManageTool`)
  - `crates/vyapaar-agent-research/src/community.rs` (`CommunityStrategyTool`)
  - `crates/vyapaar-agent-mcp/src/tools.rs`
- **Specification**:
  - CRUD operations on `StrategySpec` manifests (`create_strategy`, `inspect_strategy`, `delete_strategy`).
  - Search and clone verified open-source templates (e.g. Dual Moving Average, RSI Mean Reversion, Supertrend Intraday) matching Jesse-MCP.

#### Phase 14: Tax Reporting & Statements
- **Target Files**:
  - `crates/vyapaar-agent-chat-tools/src/tax.rs` (`TaxReportTool`)
  - `crates/vyapaar-agent-chat-tools/src/statement.rs` (`LedgerStatementTool`)
- **Specification**:
  - Inspects closed tax lots from `vyapaar-ledger-portfolio`.
  - Calculates Indian Income Tax capital gains: Short-Term Capital Gains (STCG @ 20% flat for equity) and Long-Term Capital Gains (LTCG @ 12.5% over ₹1.25L exemption threshold).
  - Emits trade ledger statements and contract notes formatted in markdown tables.

---

### Track 6: Token Optimization & Protocol Transports (Phases 15–17)

#### Phase 15: Universal `_jq` Response Filtering
- **Target Files**:
  - `crates/vyapaar-agent-core/Cargo.toml` (add `jaq-core` and `jaq-json`)
  - `crates/vyapaar-agent-core/src/result.rs`
  - `crates/vyapaar-agent-mcp/src/server.rs`
- **Specification**:
  - Add optional `_jq` argument parsing across all JSON-RPC dispatch endpoints.
  - Filter response data using `jaq` before wrapping into `VyapaarResultEnvelope`, slashing token usage.

#### Phase 16: Axum SSE / Streamable HTTP Server Transport
- **Target Files**:
  - `crates/vyapaar-agent-mcp/Cargo.toml` (enable axum features)
  - `crates/vyapaar-agent-mcp/src/transport/sse.rs` (new module)
  - `crates/vyapaar-agent-mcp/src/server.rs`
- **Specification**:
  - Expose an MCP SSE endpoint (`/sse` and `/message`) via Axum, allowing remote web-based LLM clients (such as ChatGPT Connectors or remote Cursor instances) to connect over HTTPS.

#### Phase 17: MCP Stdio Client Bridge (Gap 0004)
- **Target Files**:
  - `crates/vyapaar-agent-mcp/src/client.rs` (new client module)
  - `crates/vyapaar-agent-mcp/tests/client_tests.rs`
- **Specification**:
  - Implement an MCP Client that spawns external server processes (e.g., Longbridge or Fincept MCP servers) over stdio.
  - Enables Vyapaar agents to discover, inspect, and invoke third-party MCP tools directly.

---

## 4. Subagent Execution Invariants

Subagents implementing any phase of this guide MUST strictly observe:
1. **Zero Inline Tests**: Never place `#[test]` or `#[cfg(test)]` in `src/**/*.rs`. All tests must reside in `tests/`.
2. **Zero Floats Policy**: All prices, quantities, currency balances, and ratios must use `rust_decimal::Decimal`. Never use `f32` or `f64`.
3. **Honest Degradation**: When an upstream store is unavailable, return `PortError::unavailable("...")` citing the specific gap file. Never fabricate synthetic or zero data.
4. **Catalogue Count Pin**: When updating `create_read_only_tools`, update the count doc comment in `catalog.rs` AND update the assertion in `crates/vyapaar-agent-chat-tools/tests/unit_tests.rs::test_total_read_only_catalogue_count`.
5. **Quality Gates**: Every slice must pass formatting, clippy without warnings, and tests before completion:
   ```bash
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test --all
   ```
