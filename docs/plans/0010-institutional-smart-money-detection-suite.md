# 0010 — Institutional / Smart-Money Flow Detection Suite

**Status**: Proposed
**Target Scope**: `vyapaar-agent-chat-tools`, `vyapaar-runtime-api`, `vyapaar-runtime-api-rest`, `vyapaar-strategies`
**Upstream Dependencies**: `vyapaar-data` (gaps [0005](../../../vyapaar-data/docs/gaps/0005-block-bulk-deal-disclosure-ingestion.md), [0007](../../../vyapaar-data/docs/gaps/0007-filings-rust-bridge.md), [0013](../../../vyapaar-data/docs/gaps/0013-fii-dii-institutional-flow-ingestion.md) *(new)*, [0014](../../../vyapaar-data/docs/gaps/0014-delivery-percentage-ingestion.md) *(new)*), `vyapaar-market` (gaps [0001](../../../vyapaar-market/docs/gaps/0001-options-chain-reader.md), [0002](../../../vyapaar-market/docs/gaps/0002-open-interest-history-and-buildup-tracking.md) *(new)*)

---

## 0. Objective

Detect institutional ("smart money" / "operator" / "market maker") accumulation and
distribution — the kind of signal a volume-anomaly "elephant"-style script tries to surface —
with a **composite, cross-validated** signal rather than a single volume/candle heuristic. A
pure volume spike can't distinguish real institutional accumulation from an index rebalance or
a block unwind; this plan combines four independently-sourced legs so each confirms or
contradicts the others:

1. **Delivery-adjusted volume** (cash market: are shares actually changing hands into demat,
   or is it same-day churn?)
2. **Shareholding-pattern / insider-trade filings** (disclosed ownership: did promoter/FII/DII
   stake or insider positions actually move?)
3. **Block/bulk deal prints** (size: was there a large negotiated trade, and who was on it?)
4. **Open-interest buildup direction** (derivatives: is positioning in F&O consistent with the
   cash-market read?)

Plus one **auxiliary, market-wide signal**: FII/DII daily net cash/F&O flows. Unlike the four
legs above (all per-instrument), this is an index-level regime filter — it doesn't confirm or
deny an individual stock's accumulation, but it tells the strategy whether the broader tape is
in an institutional buying or selling regime, which the per-stock composite score can be gated
against (e.g. discount long-buildup signals on days DIIs are net heavy sellers).

Research done before writing this plan (fork, 2026-11-12) confirmed: **none of the four legs'
underlying data exists in this workspace today.** Legs 2 and 3 already have chat tools wired
(`filings_search`, `block_deals`) but both answer `PortError::unavailable` because their
upstream data gaps ([0007](../../../vyapaar-data/docs/gaps/0007-filings-rust-bridge.md),
[0005](../../../vyapaar-data/docs/gaps/0005-block-bulk-deal-disclosure-ingestion.md)) are still
open. Legs 1 and 4 have no tool, no port method, and no upstream data source at all — this plan
scopes both halves (upstream gap + tool/port code) for them.

## 1. Verified Current State

Checked directly against source before scoping (no guessing):

- `MarketQuote` (`crates/vyapaar-agent-chat-tools/src/ports/market.rs:16-28`): `instrument,
  last_price, bid, ask, volume: Quantity, as_of`. No delivery field.
- `MarketBar` (`crates/vyapaar-agent-chat-tools/src/ports/market.rs:31-43`): `timestamp, open,
  high, low, close, volume: Quantity`. No delivery field.
- `FilingRecord` (`crates/vyapaar-agent-chat-tools/src/ports/content.rs:44`): `filing_type` is a
  free-form string; the canonical set already includes `"shareholding"` and `"insider_trade"`
  (mirrors the Python `FilingType` enum in `vyapaar-data/vyapaar_data/domain/filings.py:19-20`
  1:1) — **leg 2's tool surface already exists**, `filings_search` just needs
  `vyapaar-data/docs/gaps/0007` closed to return real rows.
- `OptionContract` (`crates/vyapaar-agent-chat-tools/src/ports/market.rs`, landed this session
  for plan 0006 Phase 4): carries `open_interest: Option<Quantity>` — a **single current-moment
  value**, not a time series. Not sufficient for buildup classification (needs OI *delta* over
  a window), which is why Phase 3 below needs its own upstream gap rather than reusing this
  field.
- `block_deals` tool exists (plan 0006 Phase 1) — **leg 3's tool surface already exists**, it
  just needs `vyapaar-data/docs/gaps/0005` closed.
- No `fii`/`dii`/`delivery`/`open_interest history` hits anywhere in `vyapaar-data` or
  `vyapaar-market` outside the gap documents filed alongside this plan. No existing "elephant"
  or volume-anomaly strategy anywhere in `vyapaar-strategies`.

**Consequence for sequencing**: legs 2 and 3 need *no new agent-side code* — they only need
their already-open upstream gaps closed. This plan's new tool/port work (Phases 1–3 below) is
entirely for legs 1 and 4.

## 2. Phases

### Phase 1: `InstitutionalFlowsTool` (auxiliary market-wide signal)

- **Target files**:
  - `crates/vyapaar-agent-chat-tools/src/ports/market.rs` (new DTO + port method)
  - `crates/vyapaar-agent-chat-tools/src/market.rs` (new tool)
  - `crates/vyapaar-agent-chat-tools/src/catalog.rs`
  - `crates/vyapaar-agent-chat-tools/tests/unit_tests.rs`
- **DTO**: `InstitutionalFlow { date: DateTime<Utc>, participant: String ("FII"|"DII"), segment:
  String ("cash"|"index_futures"|"stock_futures"|"index_options"|"stock_options"), buy_value:
  Money, sell_value: Money, net_value: Money }` — exact `Decimal`-backed `Money`, mirroring the
  `OptionContract`/`CorporateAction` precedent's exact-decimal fields.
- **Port method**: `MarketReader::institutional_flows(&self, participant: Option<&str>,
  segment: Option<&str>, range: Option<DateRange>, limit: Option<usize>) ->
  Result<Vec<InstitutionalFlow>, PortError>`.
- **Tool**: `InstitutionalFlowsTool` (`institutional_flows`), `ToolRisk::ReadOnly`, registered
  in the main all-ReadOnly bundle (catalogue **19 → 20**).
- **Upstream dependency**: `vyapaar-data/docs/gaps/0013-fii-dii-institutional-flow-ingestion.md`
  (filed alongside this plan). Until closed, implementations answer `PortError::unavailable`
  citing that gap — same honest-degradation pattern as `options_chain`/`corporate_actions`.

### Phase 2: `DeliveryAnalysisTool` (leg 1 — delivery-adjusted volume)

- **Target files**: same four as Phase 1.
- **DTO**: `DeliveryBar { instrument: InstrumentId, date: DateTime<Utc>, traded_qty: Quantity,
  delivered_qty: Quantity, delivery_pct: Decimal }` — `delivery_pct` computed and stored as
  exact `Decimal` (`deliverable_qty / traded_qty * 100`), never a float ratio.
- **Port method**: `MarketReader::delivery_stats(&self, instrument: &InstrumentId, range:
  Option<DateRange>, limit: Option<usize>) -> Result<Vec<DeliveryBar>, PortError>`.
- **Tool**: `DeliveryAnalysisTool` (`delivery_analysis`), `ToolRisk::ReadOnly`, main bundle
  (catalogue **20 → 21**).
- **Upstream dependency**: `vyapaar-data/docs/gaps/0014-delivery-percentage-ingestion.md`
  (filed alongside this plan).

### Phase 3: `OiBuildupTool` (leg 4 — open-interest buildup direction)

- **Target files**: same four as Phase 1.
- **DTO**: `OiBuildupSnapshot { instrument: InstrumentId, as_of: DateTime<Utc>, open_interest:
  Quantity, oi_change_pct: Option<Decimal>, price_change_pct: Option<Decimal>, classification:
  String }`. `classification` is one of `"long_buildup"` (price↑, OI↑) | `"short_buildup"`
  (price↓, OI↑) | `"short_covering"` (price↑, OI↓) | `"long_unwinding"` (price↓, OI↓) —
  computed from the sign of `oi_change_pct` × sign of `price_change_pct` (plain `Decimal`
  comparison, no float, no external crate needed).
- **Port method**: `MarketReader::oi_buildup(&self, instrument: &InstrumentId, lookback_days:
  Option<u32>) -> Result<OiBuildupSnapshot, PortError>`.
- **Tool**: `OiBuildupTool` (`oi_buildup`), `ToolRisk::ReadOnly`, main bundle (catalogue
  **21 → 22**).
- **Upstream dependency**: `vyapaar-market/docs/gaps/0002-open-interest-history-and-buildup-tracking.md`
  (filed alongside this plan; itself sequenced after `vyapaar-market/docs/gaps/0001`).

### Phase 4: Runtime wiring (`vyapaar-runtime`)

- **Target files**:
  - `crates/vyapaar-runtime-api-rest/src/chat_market.rs` (`FeedMarketReader`)
  - `crates/vyapaar-runtime-api/src/chat_adapters.rs` (`RuntimeMarketReader`)
  - `crates/vyapaar-runtime-api/tests/chat_adapters_tests.rs`
  - `docs/gaps/0007-chat-market-reader-approximations-and-missing-sources.md` (append items,
    don't create a new gap file — this doc is the existing index of `MarketReader` stub items)
- **Specification**: implement the three new `MarketReader` methods in both implementors,
  exactly mirroring the `options_chain`/`corporate_actions` precedent from plan 0006 Phase 4:
  - `FeedMarketReader`: always answers `PortError::unavailable` citing the relevant upstream
    gap (data/0013, data/0014, market/0002) until that gap closes.
  - `RuntimeMarketReader`: gains `set_institutional_flows`, `set_delivery_stats`,
    `set_oi_buildup` setters; empty store ⇒ `PortError::unavailable` (same shape as
    `set_options_chain`/`set_corporate_actions`), seeded ⇒ real filtered/capped reads, tested in
    `chat_adapters_tests.rs`.
  - Bump `test_total_read_only_catalogue_count` (19 → 22) and append items 10–12 to
    `docs/gaps/0007-*.md`.

### Phase 5: Composite smart-money strategy (`vyapaar-strategies`)

- **Target files**: new strategy module under `vyapaar_strategies/strategies/` (exact path to
  be chosen when implementing — follow this repo's existing strategy package layout), with
  tests in `tests/` parallel to `src/` per this repo's TDD rule.
- **Specification**: a `vyapaar.Strategy` subclass computing a 4-factor composite score per
  instrument:
  1. Delivery-adjusted volume spike — `delivery_analysis` tool, compares current delivery_pct ×
     volume against a trailing average.
  2. Shareholding/insider filing signal — `filings_search(filing_type="shareholding")` and
     `filing_type="insider_trade"`; already-wired tool, blocked on
     `vyapaar-data/docs/gaps/0007`.
  3. Block/bulk deal prints — `block_deals` tool; already-wired tool, blocked on
     `vyapaar-data/docs/gaps/0005`.
  4. OI-buildup direction — `oi_buildup` tool from Phase 3.
  - Optional regime gate: `institutional_flows` (Phase 1) as a market-wide filter, not a
    per-stock leg — e.g. discount long-buildup scores on days DIIs are net heavy sellers.
  - All thresholds declared via `Parameter(default=..., min=..., max=..., step=...)` per this
    repo's convention; all values `decimal.Decimal`, never `float`.
- **TDD note**: the composite scoring/threshold logic must be unit-tested against seeded/mocked
  tool responses (the `RuntimeMarketReader` seedable-adapter pattern used throughout this
  workspace) so the strategy's *logic* is verified independent of when the four upstream data
  gaps actually close. Live signal quality is a separate, later concern from logic correctness.
- **Dependency note**: this phase does **not** need to wait for the upstream data gaps to
  close — it needs Phases 1–4's tool surface to exist (even answering honest
  `PortError::unavailable`), exactly as the agent-side Phase 1–4 tools themselves didn't wait
  for their upstream gaps in plan 0006.

## 3. Subagent Execution Invariants

Subagents implementing any phase of this plan MUST strictly observe:
1. **Research-verify first**: before writing any tool/port code for a phase, re-confirm the
   relevant upstream gap is still open (point-in-time re-check, per plan 0006 §4's precedent) —
   do not assume this document's "Verified Current State" stays true indefinitely.
2. **Zero Inline Tests**: never place `#[test]`/`#[cfg(test)]` in `src/**/*.rs`, nor
   test functions in Python library files; all tests in `tests/`.
3. **Zero Floats Policy**: every price, quantity, percentage, and currency value uses
   `rust_decimal::Decimal` (Rust) or `decimal.Decimal` (Python). Never `f32`/`f64`/`float`.
4. **Honest Degradation**: when an upstream store is unavailable, return
   `PortError::unavailable("...")` citing the specific gap file. Never fabricate synthetic or
   zero data.
5. **Catalogue Count Pin**: when registering a new tool in `create_read_only_tools`, update the
   count doc comment in `catalog.rs` AND the assertion in
   `crates/vyapaar-agent-chat-tools/tests/unit_tests.rs::test_total_read_only_catalogue_count`.
6. **Quality Gates**: every slice passes formatting, clippy, and tests before completion —
   `cargo fmt --check`, `cargo clippy --workspace --all-targets --profile prod -- -D warnings`,
   `cargo test --workspace --profile prod` (Rust repos); `ruff check . && ruff format --check .`,
   `pytest -q` (`vyapaar-strategies`). Rust repos additionally run `make prod` end-of-turn per
   the workspace build-discipline rule.
