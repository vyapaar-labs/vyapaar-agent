# `vyapaar-agent` — Chat Tools Reference

Documents `vyapaar-agent-chat-tools/`: the conversational read-only tool catalogue, the
consumer-owned read ports, the pure-tool lift adapter, and the strategy advisory tools
(Design Doc 0001 §6, plan 0006 expansions).

## Architectural Invariants

- **Pure advisory (NoFX)**: chat tools never place orders or produce `Decision`s.
- **Dependency rules (R1, R2, R4)**: depends on `vyapaar-ledger` for types only; zero
  dependency on `vyapaar-runtime`, `vyapaar-exec`, `vyapaar-market`, or
  `vyapaar-agent-trader-loop`. Market/portfolio reads are inverted through reader traits.

## Read Ports (`src/ports/`)

| Port | Purpose |
|------|---------|
| `MarketReader` | Quotes, bars, indicators, instrument search, macro series, block deals, options chains, corporate actions |
| `PortfolioReader` | Portfolio, positions, orders, tax lots, P&L views |
| `ContentReader` | News & filing search |
| `AlertReader` / `AlertWriter` | Price-alert CRUD (list / create / update / delete) |
| `SnapshotSource` | Pre-fetched `AgentSnapshot` for lifted pure tools |
| `StrategyRunner` | Strategy run/tune execution backend |

All ports return `Decimal`-typed DTOs (no floats) and degrade gracefully when the backing
store is unavailable (`PortError::unavailable`).

## `PureToolAdapter` (`src/lift.rs`)

Lifts pure, synchronous `AgentTool`s (`vyapaar-agent-tools`) into async conversational
`ChatTool`s without duplicating tool schemas or execution logic:

- `lift_screener` → `ScreenerQueryTool`
- `lift_regime` → `RegimeCheckTool`
- `lift_risk` → `RiskInspectionTool`

## Tool Catalogue (`src/catalog.rs`)

### Read-only tools (19) — `create_read_only_tools` / `register_read_only_tools`

| # | Category | Tools |
|---|----------|-------|
| 1 | Market (8) | `market_quote`, `market_history`, `market_indicators`, `market_search`, `macro_series`, `block_deals`, `options_chain`, `corporate_actions` |
| 2 | Portfolio (5) | `portfolio_view`, `positions_view`, `orders_view`, `taxlots_view`, `pnl_view` |
| 3 | Lifted pure (3) | `screener_scan`, `regime_check`, `risk_inspect` |
| 4 | Research calc (1) | `research_calc` (CAGR, XIRR, position sizing, statutory cost estimate — pure `Decimal`) |
| 5 | Content (2, plan 0006 Phase 3) | `news_search`, `filings_search` |

### Alert tools (2) — `create_alert_tools` / `register_alert_tools`

`price_alert_list` (ReadOnly) and `price_alert` (LocalWrite CRUD, plan 0006 Phase 2). The
CRUD tool is platform-managed state and lives outside the all-`ReadOnly` catalogue-count
pin.

### Strategy advisory tools (3) — `create_strategy_tools` / `register_strategy_tools`

`strategy_run`, `strategy_tune`, `trade_suggestion` — advisory only; no order placement.

## Tests

- `crates/vyapaar-agent-chat-tools/tests/unit_tests.rs` pins the read-only catalogue count
  (19) — bump the count doc comment and the assertion together when adding tools.

## Related

- `vyapaar-agent-chat` — `ChatTool` trait, `ToolRisk`, `ToolRegistry`.
- `vyapaar-agent-tools` — the pure `AgentTool` registry being lifted.
- `vyapaar-agent-core::result` — `VyapaarResultEnvelope` used for tool outputs.
- `docs/gaps/` — upstream data stores are owned by `vyapaar-market`/`vyapaar-data`; runtime
  provides `FeedMarketReader` / `RuntimeMarketReader` adapters implementing these ports.
