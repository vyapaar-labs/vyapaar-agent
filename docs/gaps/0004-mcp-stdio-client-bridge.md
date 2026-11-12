# Gap 0004 — MCP Stdio Client Bridge Not Implemented

## What's Missing

`vyapaar-agent-mcp` implements an MCP **server** — JSON-RPC 2.0 over Stdio and SSE
transports exposing `get_indicator_catalog`, `check_spec`, `run_backtest`,
`evaluate_metrics`, `optimize_strategy`, `explain_decision`, `vyapaar_forecast_series`,
and gateway discovery (`vyapaar_search_tools`, `vyapaar_describe_tool`,
`vyapaar_read_result`). What does **not** exist is the client side:

There is no client-side bridge that lets Vyapaar's own agent act as an MCP client
connecting to external MCP servers (e.g. a remote market-data MCP server).

## Why It Matters

Without an MCP client bridge, Vyapaar agents can only *serve* tools to external clients;
they cannot *consume* tools exposed by external MCP servers. This limits interoperability
with the broader MCP ecosystem and blocks Design Doc 0001's plugin model (plugin chat tools
running as out-of-process MCP stdio servers).

## How to Close

1. Add an MCP stdio client module in `vyapaar-agent-mcp` (or a new `vyapaar-agent-mcp-client`
   crate) connecting to external servers over stdio.
2. Implement JSON-RPC 2.0 request/response handling mirroring the server's
   `RpcMessage`/`RpcError`.
3. Expose discovered external tools through the `AgentTool`/`ChatTool` registry so the
   trader loop and chat can invoke them.
4. Add unit tests with a mock stdio MCP server.

See `docs/plans/0009-persistent-memory-mcp-client-and-phase-e.md` for the tracking plan
and `docs/plans/0007-comprehensive-mcp-ecosystem-and-agent-implementation-guide.md`
(Phase 17) for the detailed specification.
