# `vyapaar-agent` — MCP Server Reference

Documents `vyapaar-agent-mcp/src/`: the Model Context Protocol (`rmcp`) JSON-RPC 2.0 server
exposing quantitative tools to external AI clients over **Stdio and SSE** transports.

## `McpServer`

```rust
pub struct McpServer {
    pub indicator_catalog: IndicatorCatalogService,
    pub spec_validator: SpecValidatorService,
    pub backtest_dispatcher: BacktestDispatcherService,
    pub metrics_calculator: MetricsCalculatorService,
    pub strategy_optimizer: StrategyOptimizerService,
    pub decision_explainor: DecisionExplainorService,
    pub forecast: ForecastService,
    pub data_store: DataReferenceStore,  // $vyapaar/ref/<uuid> ephemeral store
}
```

## Quantitative Tools (JSON-RPC 2.0)

| Tool | Purpose |
|------|---------|
| `get_indicator_catalog` | Returns JSON vocabulary of approved indicators + parameter bounds |
| `check_spec` | Validates strategy syntax & parameter ranges without running backtest |
| `run_backtest` | Dispatches backtest to runtime/backtest engine, returns progress |
| `evaluate_metrics` | Sharpe, Sortino, Max Drawdown, Win Rate, OTR, DSR, PBO |
| `optimize_strategy` | Genetic or Bayesian parameter sweep |
| `explain_decision` | Exact prompt, market context, risk check for a historical trade decision |
| `vyapaar_forecast_series` | Time-series forecast (Theta, ETS, ARIMA, TBATS, gradient boost) via `vyapaar-agent-forecast` |
| `vyapaar_search_tools` | Gateway discovery: search tool catalogue by category/keyword |
| `vyapaar_describe_tool` | Gateway discovery: describe one tool's schema & parameters |
| `vyapaar_read_result` | Gateway discovery: resolve a `$vyapaar/ref/<uuid>` result |

All tool results are wrapped in `VyapaarResultEnvelope` (`vyapaar/result/v1`) from
`vyapaar-agent-core::result`.

## JSON-RPC Layer

`server.rs` implements JSON-RPC 2.0 message handling: `RpcMessage` (Request/Response/
Notification, `serde(untagged)`) and `RpcError` with standard codes
(-32700 parse, -32600 invalid request, -32601 method not found, -32602 invalid params,
-32603 internal error). Standard MCP methods `initialize`, `tools/list`, `tools/call`, and
`ping` are supported.

## Transports (`transport.rs`)

- **Stdio**: `run_stdio_server` / `run_stdio_transport` — line-delimited JSON-RPC over any
  async reader/writer.
- **SSE**: `create_router` (Axum) exposing `GET /sse` and `POST /message` with
  `SseSessionManager` session handling.

## Note (gap 0004)

No MCP **stdio client bridge** exists yet — Vyapaar can serve tools to external clients but
cannot consume tools exposed by external MCP servers. Planned as Phase 17 of plan 0007 and
tracked by gap 0004 (`docs/gaps/0004-mcp-stdio-client-bridge.md`).

## Related

- `vyapaar-agent-core::result` — `VyapaarResultEnvelope`, `DisplayBlock`.
- `vyapaar-agent-core::ref_store` — `DataReferenceStore` for large series.
- `vyapaar-agent-core::indicators` — `IndicatorCatalog` (approved indicators).
- `vyapaar-agent-forecast` — forecast models backing the `vyapaar_forecast_series` tool.
- `docs/plans/0007-comprehensive-mcp-ecosystem-and-agent-implementation-guide.md` — master
  MCP ecosystem expansion guide.
