# Gap 0002 — `vyapaar-agent-chat-tools` Crate Not Yet Created

## What's Missing

The roadmap (Phase B: Read-Only Tools) and migration guide both list `vyapaar-agent-chat-tools` as a planned crate, but it does not exist in the workspace (`Cargo.toml` members list has 14 crates; `vyapaar-agent-chat-tools` is absent).

The intended crate provides:
- Read-only market & portfolio adapters (reader ports).
- `PureToolAdapter` lifting pure `AgentTool`s (from `vyapaar-agent-tools`) into `ChatTool`s for the conversational assistant.
- Consumer-owned reader ports: `MarketReader`, `PortfolioReader`, `WatchlistStore`.
- Pure `research_calc` tool.

## Why It Matits

Without `vyapaar-agent-chat-tools`, the conversational assistant (`vyapaar-agent-chat`) cannot lift the pure `AgentTool` registry into chat turns without duplicating tool schemas or execution logic. The Phase B design depends on this crate to bridge pure quantitative tools and the async `ChatTool` surface.

## How to Close

1. Add `crates/vyapaar-agent-chat-tools/` to workspace members in `Cargo.toml`.
2. Implement `PureToolAdapter` lifting pure `AgentTool`s into chat.
3. Implement consumer-owned reader ports (`MarketReader`, `PortfolioReader`, `WatchlistStore`) reading through `vyapaar-agent-core::ports` traits (not concrete `vyapaar-market`/`vyapaar-ledger` types — per dependency rule R3/R4).
4. Implement pure `research_calc` tool.
5. Add unit tests in `crates/vyapaar-agent-chat-tools/tests/`.
6. Update `docs/design/architecture.md` sub-crate table (remove the "15th crate" note).
7. Update roadmap Phase B checkboxes.

See `docs/plans/0002-tdd-cleanup-and-chat-tools.md` for the tracking plan.

## Dependency Rule Notes

Per architecture R1–R3, `vyapaar-agent-chat-tools` must NOT depend on `vyapaar-runtime-*`, `vyapaar-exec-*`, or `vyapaar-market-*`. Market reads are inverted through reader traits in `vyapaar-agent-core::ports`.
