# 0006 — MCP/chat tool catalog expansion guide

**Status**: Proposed

---

## 0. Executive Summary & Objective

Three external references were researched to benchmark this repo's tool catalog against mature
MCP implementations:

1. **`longbridge/longbridge-mcp`** (Rust, `rmcp` + `axum`) — a production brokerage MCP server,
   **164 tools in 20 categories**, hosted at one streamable-HTTP endpoint behind OAuth 2.1, with a
   universal `_jq` response-filter parameter on every tool and typed output schemas exposed as MCP
   resources rather than inline in every tool spec.
2. **`docs.fincept.in`** — Fincept's platform docs: **17 analytics engines** (stats, forecasting,
   portfolio allocation/optimization, volatility/GARCH, panel regressions, technicals, screener,
   performance, regime detection, copulas, multi-period backtesting, fixed-income/derivatives
   pricing) plus 5 toolset categories (Markets, Data & filings, Analytics & backtests, Portfolios
   & paper trading, News & alt-data), explicitly built around a **tool-discovery pattern** ("core
   set listed, the rest searchable by name") rather than a flat `tools/list`.
3. **`bkuri/jesse-mcp`** (Python, FastMCP) — wraps the Jesse backtesting framework: **32 tools**
   (17 core across backtest/optimize/risk/pairs-trading, 15 agent-specific) plus 5 community
   tools, bridging a Python trading framework into MCP the same way this repo's own
   `vyapaar-agent-llm`/`vyapaar-agent-chat-tools` split bridges Rust ports to a chat engine — and
   notably falls back to **mock implementations** when its backend (Jesse) is unreachable, the
   same honest-degradation shape this repo already uses (`PortError::unavailable`), just less
   disciplined (jesse-mcp's mocks return *synthetic* data rather than an explicit unavailable
   error).

**This repo already has two separate tool surfaces, at two very different scales**, and the
benchmark lands differently on each:

- `vyapaar-agent-mcp`'s JSON-RPC gateway (`crates/vyapaar-agent-mcp/src/server.rs`) exposes **15
  quantitative engines** (`list_tool_definitions()`, `crates/vyapaar-agent-mcp/src/tools.rs`)
  through the `vyapaar_search_tools`/`vyapaar_describe_tool`/`vyapaar_read_result` two-tier
  discovery protocol (implemented plan 0001) — already the same pattern Fincept uses, and already
  built explicitly "instead of advertising 100+ static schemas in `tools/list`" per that file's
  own doc comment. Plan 0003 (implemented) already closed most of the Fincept-engine gap here
  (Theta forecasting, Fama-MacBeth panel regression, deterministic report narratives). **This
  surface is not this plan's target** — it's already at the right scale and pattern.
- `vyapaar-agent-chat-tools`'s `ChatTool`/`ToolRegistry` catalog (`crates/vyapaar-agent-chat-tools/src/catalog.rs`)
  — the one wired into `vyapaar-runtime`'s `/ws/chat` local-LLM conversational agent — has **17
  tools total**: 14 read-only (`create_read_only_tools`, asserted at exactly 14 in
  `tests/unit_tests.rs::test_total_read_only_catalogue_count`) plus 3 strategy-advisory tools
  (`create_strategy_tools`). This is the surface a user actually talks to (see the "any notable
  block deals today?" gap this session already filed as `vyapaar-runtime/docs/plans/0004`), and
  it is this plan's target: **14+3 tools vs. longbridge's 164 or jesse-mcp's 32** is a real,
  user-visible gap in conversational breadth, not just a count to chase for its own sake.

## 1. Context & Gaps Covered

### 1.1 Verified current state (`vyapaar-agent-chat-tools`)

- Market tools (5, `crates/vyapaar-agent-chat-tools/src/market.rs`): `MarketQuoteTool`,
  `MarketHistoryTool`, `MarketIndicatorsTool`, `MarketSearchTool`, `MacroSeriesTool`, backed by
  the `MarketReader` port (`src/ports/market.rs`).
- Portfolio tools (5, `src/portfolio.rs`): `PortfolioViewTool`, `PositionsViewTool`,
  `OrdersViewTool`, `TaxLotsViewTool`, `PnlViewTool`, backed by `PortfolioReader` (`src/ports/portfolio.rs`).
- Lifted pure tools (3, `src/lift.rs` wrapping `vyapaar-agent-tools`): `ScreenerQueryTool`,
  `RegimeCheckTool`, `RiskInspectionTool`, needing a `SnapshotSource` (`src/ports/snapshot.rs`).
- Research calculator (1, `src/calc.rs`): `ResearchCalcTool`.
- Strategy advisory (3, `src/strategy.rs`): `StrategyRunTool`, `StrategyTuneTool`,
  `TradeSuggestionTool`, backed by `StrategyRunner` (`src/ports/strategy.rs`).
- Risk classification (`crates/vyapaar-agent-chat/src/permission.rs`): exactly four `ToolRisk`
  tiers — `ReadOnly`, `LocalWrite` ("writes to platform-managed state: memory, watchlists,
  monitors, generated reports"), `Execute` (CPU-heavy runs), `External` (boundary crossings). The
  enum's own doc comment is explicit: **`Trade` and `FileWrite` are deliberately absent per the
  NoFX untrusted-AI principle** (`AGENTS.md` §5.5) — the chat agent must never be given a tool
  that places, modifies, or cancels a real order, or writes to the filesystem/ledger directly.

### 1.2 Architectural constraint this changes about the benchmark

Longbridge's 164 tools include three large **execution/write** categories that are categorically
out of scope here, not just unbuilt: **Trade (15)** — order submit/cancel/replace; **Grid (11)**
— grid-trading order submit/replace/cancel/suspend; **DCA (9)** — plan create/update/pause/stop;
and **ATM (3)** — bank withdrawals/deposits. That's 38 of the 164 tools (~23%) this repo must
**not** replicate as chat tools under the current `ToolRisk` design, regardless of how much value
they'd add — any real order placement must go through the existing non-LLM-exposed path
(`vyapaar-exec`/`vyapaar-ledger`), never a `ChatTool`. **Alert (5)** (price-alert CRUD) is the one
borderline longbridge category that *is* in scope: it's local, platform-managed state
("watchlists, monitors" — the exact phrase in `ToolRisk::LocalWrite`'s own doc comment), not a
real-money action, so it fits `LocalWrite` cleanly.

### 1.3 Gaps covered / to be filed

This plan's Phase 1–3 (below) need no new upstream gap — they're pure tool-layer wiring over data
`vyapaar-runtime`'s `AppState` or `vyapaar-ledger` already serves (confirmed: `MarketReader` and
`PortfolioReader`'s existing concrete implementations in `vyapaar-runtime` already have the
underlying reads). Two categories are genuinely blocked on upstream data and need their own gap,
filed per `.agents/rules/gap-tracking.md` §5 in the provider repo **at implementation time, not
by this planning pass** — this plan only flags them:

- **Options chain / contract data** — no options chain, Greeks, or contract reference data exists
  anywhere in `vyapaar-market` or `vyapaar-data` today (verified: no file in either repo mentions
  option chains or contract Greeks). A future Phase 2b implementer must file a gap in
  `vyapaar-market/docs/gaps/` (create the `docs/gaps/` directory there if it doesn't exist yet —
  confirmed absent) before building `OptionsChainTool`.
- **Corporate actions / dividends-splits-bonus disclosure feed** — `vyapaar-data` has
  `docs/gaps/0001-economic-calendar-ingestion.md` covering the *calendar* (scheduled, forward
  dividends/splits/IPO events) but no *historical corporate-actions* reader exists for a
  `CorporateActionsTool` to read from. Check `vyapaar-data/docs/gaps/README.md`'s current Open
  gaps table before filing a new one here — this may already be partially covered by gap 0001 by
  the time Phase 2b starts.

### 1.4 NoFX `mcp/` directory cross-check

`AGENTS.md` §5.5 attributes this repo's `ToolRisk` design (and its untrusted-AI/decision-grammar/
risk-clamping/replayability philosophy) to NoFX. The `mcp/` directory on `NoFxAiOS/nofx`'s `dev`
branch was fetched and read directly (via `gh api repos/NoFxAiOS/nofx/contents/mcp?ref=dev` and the
raw file contents) to check whether that philosophy has a concrete counterpart there worth
adopting. It does not — and the reason is that `mcp/` is not what §5.5 is actually describing:

- **What `mcp/` actually is**: a Go SDK for calling multiple LLM providers through one client
  interface (`AIClient`/`ClientHooks` in `interface.go`/`hooks.go`), with a provider registry
  (`registry.go`, `providers.go` — OpenAI, Claude, Gemini, DeepSeek, Qwen, Grok, Kimi, MiniMax, each
  a file under `mcp/provider/`), context-window token-budget truncation (`context_guard.go`), and an
  x402/Claw402 on-chain micropayment layer for metering API calls (`mcp/payment/`: `x402.go`,
  `claw402.go`, `gasless.go`, `receipt.go`). This is the same role `vyapaar-agent-llm`'s OmniRoute
  (implemented plan 0004: `OpenAiCompatibleProvider`, `FreeTierPolicy`, provider injection) already
  plays in this repo — a multi-provider LLM client layer, not a tool-catalog/risk-tiering system.
- **No tool catalog, no risk tiers, no decision-grammar validation, no tool-call audit** exist
  anywhere in `mcp/`. There is no list of callable trading/account tools (unlike longbridge-mcp or
  jesse-mcp), no permission/risk enum comparable to `ToolRisk`, and no enforcement point analogous
  to `vyapaar_core::Decision`'s `#[serde(deny_unknown_fields)]` closed grammar. Logging is a plain
  `Logger` interface (`logger.go`: `Debugf`/`Infof`/`Warnf`/`Errorf`) — no hash-chained audit record
  comparable to this repo's own `LlmAuditRecord` (implemented plan 0004/0005), let alone a
  tool-*call*-level one.
- **Where the §5.5 philosophy actually lives in NoFX**: the root README describes two execution
  modes — "Frontier" (generate a plan, "review and trade... explicitly approve execution" — a
  human-in-the-loop gate, not an automated clamp) and "Autopilot" (continuous unattended strategy
  execution). Neither is implemented in `mcp/`; the untrusted-AI/risk-clamping language in
  `AGENTS.md` §5.5 is this repo's own paraphrase of NoFX's overall human-approval-gated trading
  model, not a description of anything in the `mcp/` source tree specifically.

**Conclusion: no new pattern to adopt.** This cross-check confirms `vyapaar-agent-chat-tools`'s
existing `ToolRisk` design and `vyapaar_core::Decision` grammar already go further toward
automated, non-bypassable safety than anything in NoFX's `mcp/` code — there is nothing in that
directory to port into the implementation recipe (§3) or the phased rollout (§4). No change to
either section follows from this reference.

## 2. Discoverability pattern decision

**Decision: keep `vyapaar-agent-chat-tools` as a flat `ToolRegistry` through this plan's full
rollout (target ~30 tools); do not extend the `vyapaar_search_tools`/`describe_tool` two-tier
protocol to it yet.**

Reasoning:

- The two systems serve different consumers. `vyapaar-agent-mcp`'s gateway serves *external* LLM
  agents over JSON-RPC, where the discovery protocol's point is to avoid shipping 100+ schemas in
  every `tools/list` response to an arbitrary remote client (per that file's own comment, quoted
  above). `vyapaar-agent-chat-tools`'s `ToolRegistry` instead feeds **one local chat engine's own
  system prompt**, built once at process start from a known, bounded tool set — the cost model is
  completely different: it's a one-time prompt-construction cost, not a per-request wire payload
  to an arbitrary client.
- Fincept's own tool-reference docs structure ("core set listed, rest searchable") is evidence
  this pattern earns its cost only once a catalog is large enough that no single system prompt
  should list it all — Fincept's own count (17 engines × several tools each, well past 100) is a
  different order of magnitude from this plan's ~30-tool target.
- This repo's own prior engineering decision (`vyapaar-agent-mcp`'s gateway, implemented plan
  0001) already drew exactly this line once, between a small flat catalog and a 100+-tool one —
  reuse that precedent rather than re-deriving it: **the line is somewhere between 30 and 100, not
  at 17 or 20.**

Revisit this decision specifically when `create_read_only_tools`'s count would cross **~40**
(longbridge's own per-category counts — Quote 32, Fundamental 33 — show that even one well-scoped
category can itself approach the threshold where flat listing strains a system prompt). At that
point, lift `vyapaar_search_tools`/`describe_tool`'s pattern from `vyapaar-agent-mcp/src/server.rs`
into a sibling two-tier registry for chat tools, rather than inventing a new discovery mechanism.

## 3. Implementation recipe — adding one new chat-tool category

A subagent adding a new read-only category (e.g. "block deals," "corporate actions," "price
alerts") should follow this recipe exactly, without re-deriving it from source:

1. **DTO + port method** — in `crates/vyapaar-agent-chat-tools/src/ports/<category>.rs` (new file
   if the category doesn't share an existing port, e.g. `market.rs`'s `MarketReader`): add a
   `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` DTO struct using `Price`/`Quantity`
   (never `f32`/`f64`, per the Zero Floats Policy), then one `async fn` on the relevant port trait
   returning `Result<YourDto, PortError>` (`PortError::new(PortErrorKind::Unavailable, msg)` or
   `.unavailable(msg)` if the data source doesn't exist yet — never fabricate a plausible-looking
   empty/zero result; see `vyapaar-runtime/docs/gaps/0007-*.md` for why that distinction matters).
2. **Tool struct + `ChatTool` impl** — in the matching `src/<category>.rs` file (e.g. `market.rs`
   for a market-data tool): follow `MarketQuoteTool`'s exact shape (same file, ~line 61–126) —
   an `Input` struct deriving `Deserialize, JsonSchema` with `#[serde(deny_unknown_fields)]`, a
   `ToolSpec::new("snake_case_name", "one-line imperative description", schema_json)`, `traits()`
   returning `ToolTraits::new(ToolRisk::ReadOnly)` (or `LocalWrite` for platform-state writes like
   alerts/watchlists — **never** a risk tier implying trade execution or filesystem writes), a
   `plan()` returning a one-line `ToolPlan::new(...)` human-readable description of what's about to
   happen, and an `execute()` that deserializes input, calls the port method, and maps the result
   into `ToolOutput::ok("tool_name", serde_json::json!({...}))`.
3. **Register it** — in `crates/vyapaar-agent-chat-tools/src/catalog.rs`: add the tool to the
   relevant `use crate::<category>::{...}` import, add one `Arc::new(YourTool::new(Arc::clone(&port)))`
   line to `create_read_only_tools`'s `vec![...]` (comment-numbered groups, e.g. "`// 1. Market
   tools (N)`" — bump `N` in the comment), and update the "Bundle of all N Phase B read-only
   conversational tools" doc comment on **both** `create_read_only_tools` and
   `register_read_only_tools` to the new total.
4. **Wire the concrete reader in `vyapaar-runtime`** — implement the new port method on
   `FeedMarketReader`/`LedgerPortfolioReader`-equivalent in
   `vyapaar-runtime/crates/vyapaar-runtime-api-rest/src/chat_market.rs` (or the portfolio
   equivalent), returning `PortError::unavailable(...)` honestly if the upstream data source isn't
   ready yet (cite the gap in the error message and in a code comment, per
   `.agents/rules/gap-tracking.md` §2's citation requirement).
5. **Tests** — add to `crates/vyapaar-agent-chat-tools/tests/unit_tests.rs` (this repo's
   convention is one flat file, not per-category files — confirmed: the only file in
   `tests/` today). Extend the relevant `Mock*Reader` struct (e.g. `MockMarketReader`, line ~37)
   with your new method, add one `#[tokio::test] async fn test_<category>_tools()` exercising
   success and `unavailable` paths, and **update
   `test_total_read_only_catalogue_count`'s `assert_eq!(specs.len(), N)`** to the new total — this
   is the single easiest step to forget, and the test will fail loudly if skipped, which is the
   point.
6. **No in-file `#[test]`** — all tests go in `tests/`, per this repo's TDD rule; `cargo fmt
   --check && cargo clippy --all-targets -- -D warnings && cargo test --all` before calling the
   slice done.

## 4. Phased rollout (self-contained, no-recon subagent sizing)

Each phase below is sized to fit one subagent's ~10–15 step budget
(`.agents/rules/subagent-discipline.md`, `.agents/rules/no-recon.md`): exact target files named,
no open-ended "explore the codebase" scope. Dispatch sequentially (at most 2 concurrent per the
binding cap), verify each phase's `cargo test` pass before starting the next.

**Phase 1 — `BlockDealsTool`** (already fully scoped): implement exactly
`vyapaar-runtime/docs/plans/0004-block-bulk-deal-chat-tool-and-rest-endpoint.md` Phases 1–3. Files:
`vyapaar-agent/crates/vyapaar-agent-chat-tools/src/ports/market.rs`,
`.../src/market.rs`, `.../src/catalog.rs`, `.../tests/unit_tests.rs`;
`vyapaar-runtime/crates/vyapaar-runtime-api-rest/src/chat_market.rs`.

**Phase 2 — `PriceAlertTool` (CRUD, `ToolRisk::LocalWrite`)**: modeled on longbridge's Alert
category (5 ops: add/delete/enable/disable/list) and this repo's own `ToolRisk::LocalWrite` doc
comment ("watchlists, monitors"). New port `AlertReader`/`AlertWriter` in
`vyapaar-agent-chat-tools/src/ports/alert.rs`; tool(s) in a new `src/alert.rs`; needs a storage
decision in `vyapaar-runtime` (likely alongside `AppState`'s existing `profiles`/`portfolios`
flat-file stores, per `vyapaar-runtime/docs/gaps/0004-*.md`'s profile-schema precedent — check
that file first, it may already define a `Watchlist`/`RiskProfile` shape this should reuse rather
than duplicate).

**Phase 3 — `NewsSearchTool` / `FilingsSearchTool`**: modeled on longbridge's Content/Search (news
list/detail/search) and Fincept's "News and alternative data" toolset. Needs
`vyapaar-data/docs/gaps/0003-calendar-stubbed-data.md` and any filings-reader precedent
(`vyapaar_data/service/filings_reader.py`, closed gap 0003 per `vyapaar-data/docs/gaps/README.md`)
checked first — filings ingestion may already exist; this phase may be pure tool-layer wiring with
no new upstream gap, similar to Phase 1's Phases 1–3.

**Phase 4 (blocked — file gaps first)** — `OptionsChainTool`, `CorporateActionsTool`: do not
start implementation. First dispatch a narrow research subagent to confirm (not assume) that
`vyapaar-market`/`vyapaar-data` still lack options-chain and corporate-actions readers (re-check;
this plan's verification is a point-in-time snapshot), then file the two gaps named in §1.3 above
in the correct provider repo before any tool code is written.

## 5. What this plan does not cover

- Any execution/write category from longbridge (Trade, Grid, DCA, ATM) — excluded by the NoFX
  `ToolRisk` design, not by oversight. Do not propose these as chat tools.
- Extending the `vyapaar-agent-mcp` gateway's 15 quant engines — that surface already tracks
  Fincept's engines reasonably closely (plan 0003, implemented) and is not this plan's target.
- The two-tier search/describe discovery protocol for chat tools — deferred per §2 until the
  catalog nears ~40 tools.
