# 0001 — MCP Gateway Discovery, Universal Result Protocol, and 15 Quantitative Engines

> **Status**: Ready for implementation.
> **Target Crates**: `vyapaar-agent-mcp`, `vyapaar-agent-chat-tools`, `vyapaar-agent-core`, `vyapaar-agent-analytics`, `vyapaar-agent-research`.
> **Depends on reading**:
> - `vyapaar-agent/docs/design/0001-chat-assistant-and-superset-capabilities.md`
> - `vyapaar-agent/docs/design/agents/0001-guide.md`
> - `vyapaar-ui/docs/plans/0001-agent-chat-terminal-from-artha.md`
> - Fincept MCP Architectural Reference (`https://docs.fincept.in/`)

---

## 0. Executive Summary & Objective

Fincept MCP demonstrates how an institutional trading terminal can be exposed as an **AI-first quantitative toolset** for external coding agents (Claude Code, Cursor, Copilot, Codex) and internal conversational loops. 

Instead of dumping hundreds of static tool schemas into an LLM context window—which wastes tokens, degrades reasoning, and causes hallucinated invocations—Fincept implements:
1. **A Two-Tier Gateway Discovery Protocol** (`search_tools` + `describe_tool` + `call_tool`).
2. **A Uniform Result Envelope (`result/v1`)** with structured visual presentation blocks.
3. **In-Memory Chained Data References (`$ref/...`)** to pass heavy historical time-series between analytical engines without token roundtrips.
4. **15 Modular Quantitative Engines** spanning markets, technicals, econometrics, portfolio optimization, volatility, and regimes.

This document defines the architecture and implementation roadmap for incorporating these design patterns natively into `vyapaar-agent`, with **strict adherence to Vyapaar core invariants**:
- **Zero Floats Rule**: All currency, quantities, ratios, and margins use `rust_decimal::Decimal`.
- **NoFX Untrusted AI Principle**: All AI tools are strictly read-only and advisory; trade actions are proposed signals that must be deterministically validated by `vyapaar-risk`.
- **Indian Market Specialization**: Native support for NSE/BSE holidays, IST timestamps, statutory charges (STT, SEBI turnover fees, GST, Stamp Duty), and derivative lot/strike rules.

---

## 1. Architectural Components

```
┌────────────────────────────────────────────────────────────────────────┐
│                      Client Layer (Claude / Cursor / CLI)              │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ JSON-RPC 2.0 (Stdio / SSE)
┌───────────────────────────────────▼────────────────────────────────────┐
│                  vyapaar-agent-mcp Gateway Router                      │
│                                                                        │
│  Default Eager Tools:                                                  │
│   • vyapaar_search_tools(query, category)                              │
│   • vyapaar_describe_tool(tool_name)                                   │
│   • vyapaar_call_tool(tool_name, arguments)                            │
│   • vyapaar_read_result(ref_id, offset, limit)                         │
└───────────┬────────────────────────────────────────────────┬───────────┘
            │ Resolves & Dispatches                          │ Stores & Emits
┌───────────▼──────────────────────────┐    ┌────────────────▼───────────┐
│       Tool Registry & Engines        │    │    Data Reference Store    │
│  (15 Categorized Domain Toolsets)    │    │   ($vyapaar/ref/<uuid>)    │
│   • Market & Candles   • Technicals  │    │                            │
│   • Scanner & Screens  • Validation  │    │  In-memory Arrow / SQLite  │
│   • Portfolio / Alloc  • Regimes     │    │  ephemeral cache for       │
│   • Volatility         • Tax/Ledger  │    │  large time-series & ticks │
└───────────┬──────────────────────────┘    └────────────────┬───────────┘
            │                                                │
┌───────────▼────────────────────────────────────────────────▼───────────┐
│                 Universal Result Protocol (vyapaar/result/v1)          │
│  { schema, status, meta, summary, data, blocks: [table|chart], refs }  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Component Specifications

### 2.1 The Two-Tier Gateway Discovery Protocol

Instead of advertising 100+ schemas in `tools/list` on session startup, `vyapaar-agent-mcp` exposes a compact discovery gateway:

#### `vyapaar_search_tools`
- **Purpose**: Search tools by natural language query, asset class, or analytical category.
- **Input Schema**:
  ```json
  {
    "type": "object",
    "properties": {
      "query": { "type": "string", "description": "Search keyword or intent (e.g. 'GARCH', 'RSI', 'portfolio risk')" },
      "category": { 
        "type": "string", 
        "enum": ["market", "technicals", "scanner", "analytics", "allocation", "regimes", "volatility", "research", "ledger", "all"],
        "default": "all"
      }
    },
    "required": ["query"]
  }
  ```
- **Output**: Ranked list of tool names, categories, and concise 1-line taglines.

#### `vyapaar_describe_tool`
- **Purpose**: Return the full JSON-Schema, parameter constraints, mathematical definitions, and examples for a chosen tool.
- **Input Schema**:
  ```json
  {
    "type": "object",
    "properties": {
      "tool_name": { "type": "string", "description": "The exact tool name (e.g. 'vyapaar_technicals_calculate')" }
    },
    "required": ["tool_name"]
  }
  ```
- **Output**: JSON schema matching MCP Tool Definition format with detailed field descriptions and validation bounds.

#### `vyapaar_call_tool`
- **Purpose**: Dynamic tool invocation wrapper when dynamic client execution is preferred.

---

### 2.2 Universal Result Protocol (`vyapaar/result/v1`)

Every tool in `vyapaar-agent-mcp` and `vyapaar-agent-chat-tools` returns a structured envelope:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VyapaarResultEnvelope<T> {
    pub schema: String, // Always "vyapaar/result/v1"
    pub status: ResultStatus, // Ok | Error
    pub meta: ExecutionMeta,
    pub summary: String, // Natural language 1-line summary
    pub data: T,         // Exact typed machine-readable JSON
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<DisplayBlock>, // Typed visual presentation blocks
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub refs: HashMap<String, String>, // Reference URIs for large datasets
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMeta {
    pub elapsed_ms: u64,
    pub timestamp_ist: String, // RFC3339 in Asia/Kolkata
    pub version: String,
    pub precision: String, // "exact_decimal_zero_floats"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DisplayBlock {
    Table {
        title: String,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    MetricGrid {
        items: Vec<MetricItem>,
    },
    Chart {
        title: String,
        chart_type: String, // "candlestick", "line", "sparkline"
        series_ref: String,
    },
    Alert {
        level: String, // "info", "warning", "critical"
        message: String,
    },
}
```

---

### 2.3 Ephemeral Data Reference Store (`$vyapaar/ref/...`)

To prevent huge token costs when chaining market quotes into technicals and portfolio optimization:
1. `MarketHistoryTool` queries 5,000 candles for `NSE:RELIANCE`.
2. Instead of returning a 1 MB JSON array to the model, it caches the series in `DataReferenceStore` with key `$vyapaar/ref/ohlcv_reliance_5m_20261106`.
3. The result envelope returns the reference in `refs` and a compact summary (latest price, range, volume).
4. When calling `vyapaar_technicals_calculate`, the agent passes:
   ```json
   {
     "series_ref": "$vyapaar/ref/ohlcv_reliance_5m_20261106",
     "indicators": ["rsi(14)", "macd(12,26,9)"]
   }
   ```
5. The technicals tool directly dereferences the in-memory series via zero-copy borrow.

---

### 2.4 The 15 Modular Quantitative Engines

The full quantitative surface is partitioned into 15 engine namespaces:

1. **`market`**: Real-time quotes, level-2 depth, OHLCV bars, option chains, holiday calendars.
2. **`technicals`**: 200+ indicators (TA-Lib parity), 61 candlestick pattern recognizers.
3. **`scanner`**: Universe screening across NSE/BSE stocks, ETFs, F&O underlyings; custom rule screening.
4. **`analytics`**: Performance tearsheets, Sharpe, Sortino, Calmar, Max Drawdown, rolling betas.
5. **`validation`**: Deflated Sharpe Ratio (DSR), Probability of Backtest Overfitting (PBO), white reality check.
6. **`allocation`**: Mean-Variance Optimization (MVO), Hierarchical Risk Parity (HRP), Black-Litterman.
7. **`risk`**: Pre-trade SEBI margin checks, VaR (Parametric, Historical, Monte Carlo), Expected Shortfall (CVaR).
8. **`regimes`**: Hidden Markov Models (2-state and 3-state calm/turbulent regime detection), transition probabilities.
9. **`change_points`**: Structural break detection (PELT, binary segmentation) on return distributions and volatility.
10. **`volatility`**: GARCH(1,1), EGARCH, GJR-GARCH, realized volatility, volatility cone.
11. **`copula`**: Bivariate and vine copulas for non-linear tail dependence modeling across Indian sectors.
12. **`ledger`**: Tax lot matching (FIFO, LIFO), Capital Gains calculation (LTCG Section 112A, STCG Section 111A).
13. **`calc`**: Indian statutory cost calculator (STT, GST 18%, Stamp Duty, Exchange Charges, SEBI fees).
14. **`research`**: Genetic hypothesis generation, grammar rules, closed-loop strategy mutation.
15. **`backtest`**: Isolated parity execution of strategy manifests with market impact and slippage.

---

## 3. Implementation Phases

### Phase 1: Gateway & Discovery Primitives (`vyapaar-agent-mcp`)
- Implement `vyapaar_search_tools` and `vyapaar_describe_tool`.
- Refactor `crates/vyapaar-agent-mcp/src/tools.rs` to support dynamic registration and discovery.
- Implement `VyapaarResultEnvelope` in `crates/vyapaar-agent-core/src/result.rs`.

### Phase 2: In-Memory Reference Cache
- Build `DataReferenceStore` in `crates/vyapaar-agent-core/src/ref_store.rs`.
- Wire reference resolution into `crates/vyapaar-agent-chat-tools`.

### Phase 3: Engine Expansion & Toolset Wiring
- Lift remaining quantitative capabilities from `vyapaar-agent-analytics`, `vyapaar-agent-research`, and `vyapaar-strategies` into discoverable MCP tools.
- Wire Indian statutory calculation into universal result blocks.

### Phase 4: Integration & Conformance Testing
- Add regression unit tests verifying that all 15 engine tools conform to `vyapaar/result/v1`.
- Verify zero float violations with static checks and clippy.
