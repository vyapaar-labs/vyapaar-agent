# HANDOFF — vyapaar-agent

**Date**: 2026-11-12 · Repo: `vyapaar-agent` · Branch: `main`

## Phase / Slice Status

- **Docs overhaul (2026-11-12)**: the historical `docs/roadmap/` was dismantled into
  `docs/design/`, `docs/gaps/`, `docs/plans/` and the whole set rewritten to match the source.
  - Design: `architecture.md` now 17 crates (chat-tools, forecast, panel real); new
    `code-chat-tools.md`; migration map all-complete; guardrails note the 2 remaining inline
    `#[cfg(test)]` blocks; `code-engines.md` corrected (analytics `cointegration.rs` +
    `tearsheet.rs` are orphaned files not declared in `lib.rs`).
  - Gaps: **0002 closed** (chat-tools crate, commit `f3f1cdc`); 0001 widened to two files
    (`ref_store.rs` + `research/verify/mod.rs`); 0003–0007 still open with corrected scopes
    (jobs scheduler exists — gap 0006 is now only the runtime-side bridge).
  - Plans: active plans renumbered to fix a numbering collision — **0008** (TDD cleanup, gap
    0001) and **0009** (persistent memory/MCP client/Phase E, gaps 0003–0007) replace the old
    `0002`/`0003` files; **0007** (MCP master guide) unchanged except status header (Phase 16
    SSE transport noted shipped).
- **Plan 0006 (MCP/chat tool catalog expansion) — complete**: Phases 1–4 implemented
  (2026-11-11/12) and recorded in `docs/plans/README.md` Implemented table. Full detail in the
  README + git history; catalogue is 19 read-only + 2 alert + 3 strategy tools.
- **Plans 0001–0005**: complete and merged — see `docs/plans/README.md` Implemented table.

## Immediate Next Step

- **Gap 0001** (inline `#[cfg(test)]` in `ref_store.rs` + `research/verify/mod.rs`) → plan
  0008; quickest close: move tests to `tests/`, verify `grep -rn "#\[cfg(test)\]"
  crates/*/src/` is empty, then update `code-guardrails.md`/`code-migration-map.md`.
- Persistent memory & Phase E work continues under plan 0009 (gaps 0003–0007).
- Future chat-tool/MCP breadth continues under active plan 0007; open data blockers are the
  filed upstream gaps — `vyapaar-market/docs/gaps/0001` (chain reader),
  `vyapaar-data/docs/gaps/0008`+`0009` (bridge + ingestion) — alongside the standing
  `vyapaar-data` 0005/0006/0007 ingestion/bridge gaps.

## Decisions & Discoveries

- Plan numbering collision fixed: active plans 0008/0009 replace the reused `0002`/`0003`
  numbers (implemented plans 0001–0006 keep theirs). Gap 0002 closed in the index; gap
  numbers are never reused per `.agents/rules/gap-tracking.md`.
- Chat catalogue: **19 read-only tools** (`test_total_read_only_catalogue_count` pins the count)
  **plus** the 2-tool alert bundle and 3 strategy tools. Content (Phase 3) and
  options-chain/corporate-actions (Phase 4) tools all went into the main all-ReadOnly bundle
  (unlike the LocalWrite alert CRUD, which sits outside so its count-pin invariant stays
  untouched). Plan 0006 §2's flat-registry decision stands (revisit at ~40).
- Phase 4 port semantics: `OptionContract.option_type` is the canonical FNO symbol-grammar
  string ("CE"/"PE", mirroring `vyapaar-market-finance`'s `OptionType`); `OptionsChain` carries
  the underlying + snapshot time, contracts ordered by expiry/strike/type; chain cap 500
  (`MAX_CHAIN_CONTRACTS`, mirroring `MAX_BARS_LIMIT`). `CorporateAction.action_type` is the
  canonical `"dividend"|"split"|"bonus"|"rights"` set mirroring
  `vyapaar_data.domain.corporate_actions`' value objects; ratios are exact `Quantity` pairs
  (`ratio_from`/`ratio_to`), dividends carry `Option<Money>` per-share amounts; list cap 20
  default / 50 (`DEFAULT_ACTIONS_LIMIT`/`MAX_ACTIONS_LIMIT`, content-tools precedent).
- Content port semantics: news `instruments` are free-form mention tags (never parsed as
  canonical `InstrumentId` — news text doesn't carry validated scrip refs); filings instruments
  are canonical. `filing_type` is the canonical `FilingType` string set from Python
  (`corporate_action`, `shareholding`, `insider_trade`, `fund_holding`, `annual_report`,
  `quarterly_result`, `other`). Both tools accept `limit` (default 20, cap 50) enforced at the
  tool layer via `effective_limit`.
- Filings upstream reality: `vyapaar-data` (Python) ingests filings (`LakehouseFilingsReader`,
  closed gap 0003) but no Rust consumer can reach that lakehouse — the standing
  `vyapaar-data` → Rust FFI bridge is now gap `vyapaar-data/docs/gaps/0007-filings-rust-bridge.md`
  (the market-reference sample stores in `vyapaar-runtime` stub against the same missing bridge).
  News has no ingestion anywhere → `vyapaar-data/docs/gaps/0006-news-ingestion.md`.
- `ToolRisk::LocalWrite` on the chat agent triggers the existing Interactive permission prompt
  per mutating call (`ToolTraits::new` also forces serial execution for non-ReadOnly) — no
  engine change was needed.
- `MarketReader` has exactly two implementors: `FeedMarketReader` (runtime-api-rest) and
  `RuntimeMarketReader` (runtime-api, seedable test adapter); the new `ContentReader` port has
  the one `FeedContentReader` implementor (alert-port precedent: no seedable test adapter until
  a consumer needs one).
- Toolchain's clippy (rust-1.97) flags `map_or(true, ..)` as `unnecessary_map_or`; use
  `is_none_or` in new code.
- Live GGUF inference tests (`vyapaar-agent-llm/tests/candle_gguf_tests.rs::test_live_zenfinance_*`)
  are `#[ignore]`d: whenever `models/zenfinance-3b-Instruct.Q4_K_M.gguf` exists (2 GB) they run
  minutes of real 3B inference inside the default gate — machine-dependent and CI-impossible.
  Same policy as 4B's OmniRoute live tests; runnable via `cargo test -p vyapaar-agent-llm
  --test candle_gguf_tests -- --ignored`.

## Operating Invariants

- Canonical profile `prod`: `cargo build/check/test --profile prod`; `make prod` end-of-turn (§11).
- Tests in `tests/` parallel to `src/`; no in-file `#[test]`.
- Zero floats in financial paths; `rust_decimal::Decimal`.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets --profile prod -- -D warnings`,
  `cargo test --workspace --profile prod` all green (verified 2026-11-12).
- Gap/plan numbering per-repo, sequential, never reused (`.agents/rules/gap-tracking.md`).
