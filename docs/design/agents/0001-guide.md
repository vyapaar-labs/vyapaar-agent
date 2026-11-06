# Vyapaar AI Capabilities & Quantitative Research Desk Playbook

Welcome to **Vyapaar**, an institutional-grade quantitative finance platform and AI research desk. Vyapaar bridges generative AI reasoning with deterministic quantitative execution, high-performance backtesting, real-time risk control, and multi-asset portfolio accounting.

---

## 1. Operating Principles & Safety Architecture

1. **Advisory Only (NoFX Untrusted AI Principle)**:
   Vyapaar agents function strictly as research analysts and signal proposers. No AI model directly executes orders with a broker or bypasses pre-trade risk checks. Every trading proposal must pass through the typed decision boundary (`vyapaar_core::Decision`), validated against circuit bands and portfolio limits by `vyapaar-risk`.
2. **Grounded Data**:
   All prices, returns, indicators, fundamentals, and portfolio positions must be fetched via live tool execution (`market_quote`, `market_history`, `market_indicators`, `screener_query`, `macro_series`, `portfolio_view`). Vyapaar agents never hallucinate financial figures.
3. **Exact Decimal Arithmetic (Zero Floats)**:
   Financial calculations use exact 128-bit decimal arithmetic (`rust_decimal::Decimal`). Derived metrics such as CAGR, risk-based position sizing, and statutory market charges (STT, GST, stamp duty) must be computed using `research_calc`.
4. **Indian Market Grounding**:
   Native support for Indian capital market structures: National Stock Exchange (NSE), Bombay Stock Exchange (BSE), Multi Commodity Exchange (MCX), SEBI regulations, circuit filters, F&O lot sizes, and IST trading hours (09:15 – 15:30 IST).

---

## 2. Capabilities by Financial Domain

### 2.1 Markets, Equities & Live Prices
Real-time quotes, multi-timeframe OHLCV history, and cross-asset comparison across Indian equities, indices, ETFs, currencies, and global ADRs.
- **Tools**: `market_quote`, `market_history`, `market_search`
- **What You Can Do**:
  - Live Last Traded Price (LTP), bid/ask spread, day high/low, circuit bands, and volume.
  - Multi-timeframe historical bars (1m, 5m, 15m, 1h, 1d) with split/dividend adjustments.
  - Multi-asset comparative returns and drawdowns side-by-side.
- **Example Prompts**:
  - `Compare TCS, Infosys, and Wipro over the last 3 years: total returns, annualized volatility, and maximum drawdown.`
  - `Give me a technical and price volume read on NIFTY 50 today with key intraday levels.`

### 2.2 Technical Analysis & Indicators
Quantitative technical indicators computed deterministically using standard libraries (StockSharp indicator suite).
- **Tools**: `market_indicators`, `regime_check`
- **What You Can Do**:
  - Moving averages (SMA, EMA, VWAP, Hull MA), momentum oscillators (RSI, MACD, Stochastic), volatility bands (ATR, Bollinger Bands, Keltner Channels), and trend filters (Supertrend, ADX).
  - Market regime classification: determine if an instrument is trending, mean-reverting, or volatile.
- **Example Prompts**:
  - `Calculate the 14-day RSI, 20-day ATR, and 50/200 EMA crossover status for RELIANCE.`
  - `Classify the current market regime of BANKNIFTY based on 30-day realized volatility and trend strength.`

### 2.3 Fundamental Screening & Company Research
Multi-factor equity screening and forensic evaluation of balance sheets, earnings quality, and cash flows.
- **Tools**: `screener_query`, `market_search`, `web_search`
- **What You Can Do**:
  - Screen stocks across market cap, valuation multiples (P/E, P/B, EV/EBITDA), return metrics (ROE, ROCE, ROIC), and debt ratios (Debt/Equity, Interest Coverage).
  - Corporate filings inspection: annual reports, quarterly results, auditor remarks, and related-party transactions.
- **Example Prompts**:
  - `Screen Nifty 500 stocks for ROE > 20%, Debt-to-Equity < 0.5, and 3-year sales growth > 15%.`
  - `Perform a forensic earnings quality analysis on HDFC Bank over the last 5 financial years.`

### 2.4 Macroeconomics & Central Bank Policy
Sovereign bond yield curves, macroeconomic time series, inflation indicators, and monetary policy tracking.
- **Tools**: `macro_series`, `web_search`
- **What You Can Do**:
  - Central bank policy rates: Reserve Bank of India (RBI) repo rate, US Federal Reserve funds rate, liquidity adjustment facility (LAF) balances.
  - Inflation metrics (CPI, WPI), GDP growth trends, foreign exchange reserves, and currency pairs (USD/INR, EUR/INR).
- **Example Prompts**:
  - `Chart India CPI inflation against the RBI Repo Rate over the past 8 years and summarize policy implications.`
  - `What are the key upcoming macroeconomic data releases and RBI MPC meeting dates this month?`

### 2.5 Quantitative Backtesting & Factor Research
Rigorous backtesting with transaction costs, slippage modeling, walk-forward validation, and overfitting diagnostics.
- **Tools**: `backtest_run`, `walk_forward_validate`, `strategy_optimize`, `research_calc`
- **What You Can Do**:
  - Simulate rule-based trading strategies on historical data with statutory Indian transaction costs (STT, GST, stamp duty) and realistic slippage.
  - Statistical verification: Deflated Sharpe Ratio (DSR), Probability of Backtest Overfitting (PBO), and walk-forward out-of-sample testing.
  - Genetic strategy synthesis: optimize parameter sets and discover non-linear indicator combinations.
- **Example Prompts**:
  - `Backtest a 20-day Donchian breakout strategy on Nifty futures since 2018 with full statutory costs and slippage.`
  - `Run a walk-forward optimization on an RSI mean-reversion rule for Tata Motors across 5 rolling splits.`

### 2.6 Portfolio Accounting, Tax Lots & Risk Governance
Double-entry general ledger portfolio management, FIFO tax-lot tracking, risk attribution, and drift monitoring.
- **Tools**: `portfolio_view`, `positions_view`, `tax_lots_view`, `pnl_view`, `orders_view`, `risk_inspect`
- **What You Can Do**:
  - Real-time portfolio Net Asset Value (NAV), open positions, realized and unrealized P&L.
  - FIFO tax-lot tracking for Short-Term (STCG) and Long-Term (LTCG) capital gains reporting.
  - Pre-trade risk rule validation: check margin headroom, single-stock exposure limits, and portfolio concentration.
- **Example Prompts**:
  - `Show my current portfolio allocation, sector weights, and any holding that breaches our 10% concentration limit.`
  - `Calculate the tax-lot breakdown and unrealized capital gains for my equity holdings.`

### 2.7 Exact Calculations & Statutory Charges
High-precision financial calculations using deterministic decimal math.
- **Tools**: `research_calc`
- **What You Can Do**:
  - Compound Annual Growth Rate (CAGR) and holding period returns.
  - Risk-based position sizing from account equity and ATR stop distance.
  - Indian statutory transaction charge breakdown: STT, exchange turnover fees, SEBI turnover fees, stamp duty, and 18% GST across delivery, intraday, futures, and options segments.
- **Example Prompts**:
  - `Calculate the exact statutory charges for buying and selling 500 shares of Infosys at ₹1,850 in equity delivery.`
  - `Size a position in TCS with ₹20,00,000 capital, 1% risk per trade, entry at ₹4,100, and stop-loss at ₹3,980.`

---

## 3. Specialized Multi-Agent Personas

Vyapaar includes 9 specialized agent personas tailored for distinct investment and trading disciplines:

| Persona | Primary Focus | Key Tools |
| :--- | :--- | :--- |
| **VyapaarAI** | Generalist financial analyst & research desk lead | `market_quote`, `market_history`, `screener_query`, `macro_series`, `portfolio_view`, `research_calc` |
| **Analyst** | Fundamental equity research & forensic financial statement reading | `screener_query`, `market_quote`, `market_history`, `research_calc`, `web_search` |
| **Quant** | Statistical modeling, factor research, backtesting & walk-forward validation | `market_history`, `market_indicators`, `backtest_run`, `walk_forward_validate`, `research_calc` |
| **Trader** | Price action, technical levels, tactical setups & risk-reward trade plans | `market_quote`, `market_history`, `market_indicators`, `research_calc` |
| **Portfolio Manager** | Multi-asset allocation, sector concentration, drift & rebalancing | `portfolio_view`, `positions_view`, `tax_lots_view`, `screener_query`, `research_calc` |
| **Risk Manager** | Downside protection, stress testing, Value at Risk & position sizing | `portfolio_view`, `positions_view`, `risk_inspect`, `market_history`, `research_calc` |
| **Macro Strategist** | Global macro regimes, interest rates, currency flows & cross-asset positioning | `macro_series`, `market_history`, `market_quote`, `web_search` |
| **Value Investor** | Competitive moats, owner earnings, capital allocation & margin of safety | `screener_query`, `market_quote`, `market_history`, `research_calc` |
| **Devil's Advocate** | Thesis stress-testing, vulnerability discovery, pre-mortems & bear cases | `market_quote`, `market_indicators`, `screener_query`, `risk_inspect`, `research_calc` |
