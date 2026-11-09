# `vyapaar-agent` — MCP Server Reference

Documents `vyapaar-agent-mcp/src/`: the Model Context Protocol (`rmcp`) server exposing
quantitative tools to external AI clients.

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
| `forecast` | Time-series forecast (Theta, ETS, ARIMA, TBATS, gradient boost) |

All tool results are wrapped in `VyapaarResultEnvelope` (`vyapaar/result/v1`) from
`vyapaar-agent-core::result`.

## JSON-RPC Layer

`server.rs` implements JSON-RPC 2.0 message handling: `RpcMessage` (Request/Response/
Notification, `serde(untagged)`) and `RpcError` with standard codes
(-32700 parse, -32600 invalid request, -32601 method not found, -32602 invalid params,
-32603 internal error).

## Note (gap 0004)

No MCP **stdio client bridge** exists yet — Vyapaar can serve tools to external clients but
cannot consume tools exposed by external MCP servers. Planned
(`docs/plans/0003-persistent-memory-and-mcp-client.md`, gap 0004).

## Related

- `vyapaar-agent-core::result` — `VyapaarResultEnvelope`, `DisplayBlock`.
- `vyapaar-agent-core::ref_store` — `DataReferenceStore` for large series.
- `vyapaar-agent-core::indicators` — `IndicatorCatalog` (approved indicators).
- `vyapaar-agent-forecast` — forecast models backing the `forecast` tool.
