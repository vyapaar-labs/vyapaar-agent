# HANDOFF — vyapaar-agent

**Date**: 2026-11-12 · Repo: `vyapaar-agent` · Branch: `main`

## Phase / Slice Status

- **Plan 0006 Phase 3 — `NewsSearchTool`/`FilingsSearchTool` (implemented 2026-11-12, all gates
  green)**: new `ContentReader` port (`src/ports/content.rs`: `NewsArticle` — free-form
  instrument tags, not canonical `InstrumentId`s — and `FilingRecord` mirroring Python
  `vyapaar_data.domain.filings.Filing` incl. canonical `FilingType` values); tools in
  `src/content.rs` — `news_search` + `filings_search` (both ReadOnly, `limit` default 20 / cap
  50); registered in the main `create_read_only_tools`/`register_read_only_tools` bundle →
  read-only catalogue count **15 → 17** (`test_total_read_only_catalogue_count` bumped). Tests:
  `MockContentReader` (fail-flag style, matching `MockMarketReader`) + `test_content_tools`
  (success, query/instrument/type filters, empty-success ≠ error, invalid instrument, honest
  unavailable, deny-unknown-fields) in `tests/unit_tests.rs` (13 total). Runtime side (in
  `vyapaar-runtime`): `chat_content.rs::FeedContentReader` honest stubs — upstream check verdict:
  filings ingestion exists in Python `vyapaar-data` (closed gap 0003) but has no Rust bridge;
  news has no source anywhere. Gaps filed in the provider repo at implementation time:
  `vyapaar-data/docs/gaps/0006-news-ingestion.md` + `0007-filings-rust-bridge.md`.
- **Plan 0006 Phase 2 — `PriceAlertTool` CRUD (implemented 2026-11-11)**: one line —
  `AlertReader`/`AlertWriter` port + `price_alert_list` (ReadOnly) / `price_alert` (LocalWrite)
  in a separate `create_alert_tools`/`register_alert_tools` bundle so the all-ReadOnly count
  stayed pinned; runtime storage = the existing `Profile.alerts` document (`vyapaar-runtime`'s
  `chat_alerts.rs::ProfileAlertStore`; storage decision in that repo's HANDOFF).
- **Plan 0006 Phase 1 — `BlockDealsTool` (implemented 2026-11-11)**: one line — see Phase 1 of
  plan 0006.
- **Plans 0001–0005**: complete and merged — see `docs/plans/README.md` Implemented table
  (0004's Phase 4 local-SLM deferral and plan 0005 QLoRA/BLAS notes live there, not here).

## Immediate Next Step

Plan 0006 Phase 4 stays blocked: first dispatch a narrow research subagent to re-confirm (not
assume) that `vyapaar-market`/`vyapaar-data` still lack options-chain and corporate-actions
readers, then file the two gaps named in plan 0006 §1.3 (`vyapaar-market/docs/gaps/` needs
creating) **before** any `OptionsChainTool`/`CorporateActionsTool` code. Catalogue now 17
read-only + 2 alert + 3 strategy tools — plan 0006 §2's flat-registry decision stands
(revisit at ~40).

## Decisions & Discoveries

- Chat catalogue: **17 read-only tools** (`test_total_read_only_catalogue_count` pins the count)
  **plus** the 2-tool alert bundle and 3 strategy tools. Content tools went into the main
  all-ReadOnly bundle (unlike the LocalWrite alert CRUD, which sits outside so its count-pin
  invariant stays untouched). Plan 0006 §2's flat-registry decision stands (revisit at ~40).
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
