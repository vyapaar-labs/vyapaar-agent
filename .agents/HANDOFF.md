# HANDOFF — vyapaar-agent

**Date**: 2026-11-12 · Repo: `vyapaar-agent` · Branch: `main`

## Phase / Slice Status

- **Plan 0006 (MCP/chat tool catalog expansion) — complete: Phases 1–4 implemented
  (2026-11-11/12)**; recorded in `docs/plans/README.md` Implemented table (doc removed from
  active `docs/plans/` per gap-tracking §4). **Phase 4 (2026-11-12)**: research-verified
  upstream first (plan §4's mandated re-check, 1 explore subagent) — `vyapaar-market` has
  contract-reference/Greeks building blocks but no chain reader, and `vyapaar-data`'s
  corporate-actions reader **does** exist Python-side (the plan's "no reader" assumption was
  stale; verified against `DuckDbCorporateActionsStore` + `CorporateActionsRepository`). Gaps
  filed before any tool code: `vyapaar-market/docs/gaps/0001-options-chain-reader.md` (that
  repo's first), `vyapaar-data/docs/gaps/0008-corporate-actions-rust-bridge.md` +
  `0009-corporate-actions-ingestion.md`. Landed: `OptionContract`/`OptionsChain`/
  `CorporateAction` DTOs + `MarketReader::options_chain`/`corporate_actions` port methods;
  `options_chain` + `corporate_actions` tools in `src/market.rs` (both ReadOnly, main bundle —
  catalogue **17 → 19**, count test bumped); runtime side `FeedMarketReader` honest stubs +
  seedable `RuntimeMarketReader` stores. Boy-scout: `BlockDeal`/`BlockDealsInput`/
  `BlockDealsTool` now re-exported at crate root (Phase 1 omission).
- **Phase 1–3 recap (2026-11-11/12)** — Phase 1 `BlockDealsTool`, Phase 2 `PriceAlertTool` CRUD
  (`LocalWrite`, separate alert bundle), Phase 3 `news_search`/`filings_search` via the
  `ContentReader` port (runtime stubs citing `vyapaar-data` gaps 0006/0007); details in the
  plans README + git history.
- **Plans 0001–0005**: complete and merged — see `docs/plans/README.md` Implemented table
  (0004's Phase 4 local-SLM deferral and plan 0005 QLoRA/BLAS notes live there, not here).

## Immediate Next Step

Plan 0006 is complete. Catalogue is **19 read-only + 2 alert + 3 strategy** tools; plan 0006
§2's flat-registry decision stands (revisit at ~40). Future chat-tool breadth continues under
active plan 0007; the open data blockers for the newest tools are the filed upstream gaps —
`vyapaar-market/docs/gaps/0001` (chain reader), `vyapaar-data/docs/gaps/0008`+`0009`
(bridge + ingestion) — alongside the standing 0005/0006/0007 ingestion/bridge gaps.

## Decisions & Discoveries

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
