# HANDOFF — vyapaar-agent

**Date**: 2026-11-11 · Repo: `vyapaar-agent` · Branch: `main`

## Phase / Slice Status

- **Plan 0006 Phase 2 — `PriceAlertTool` CRUD (implemented, all gates green)**: new
  `AlertReader`/`AlertWriter` port (`src/ports/alert.rs`: `PriceAlert` DTO mirroring ui-web's
  `AlertMeta`, `AlertStatus` active/triggered/paused); tools in `src/alert.rs` —
  `price_alert_list` (ReadOnly) + `price_alert` (LocalWrite; ops add/delete/enable/disable);
  registered via new `create_alert_tools`/`register_alert_tools` catalog functions, so the
  read-only catalogue count stays pinned at 15. Tests: `MockAlertStore` +
  `test_alert_tools` (CRUD, filters, triggered no-op, validation, risk tiers, bundle count = 2)
  in `tests/unit_tests.rs` (12 total). Runtime side (in `vyapaar-runtime`):
  `chat_alerts.rs::ProfileAlertStore` over the existing `Profile.alerts` document — see that
  repo's HANDOFF for the storage decision.
- **Plan 0006 Phase 1 — `BlockDealsTool` (implemented 2026-11-11, all gates green)**: one line,
  see `docs/plans/README.md`-style record below and Phase 1 of plan 0006.
- **Plans 0001–0005**: complete and merged — see `docs/plans/README.md` Implemented table
  (0004's Phase 4 local-SLM deferral and plan 0005 QLoRA/BLAS notes live there, not here).

## Immediate Next Step

Plan 0006 Phase 3 — `NewsSearchTool`/`FilingsSearchTool`: first verify against
`vyapaar-data/docs/gaps/README.md` + `vyapaar_data/service/filings_reader.py` (closed gap 0003)
whether filings ingestion already exists — if so it's pure tool-layer wiring like Phase 1;
otherwise file the missing-reader gap in `vyapaar-data` first. Phase 4 stays blocked until its
two upstream gaps are filed (`vyapaar-market` options-chain, corporate actions).

## Decisions & Discoveries

- Chat catalogue: **15 read-only tools** (`test_total_read_only_catalogue_count` pins the count)
  **plus** the 2-tool alert bundle (`create_alert_tools`: 1 ReadOnly list + 1 LocalWrite CRUD —
  LocalWrite deliberately outside the all-ReadOnly bundle so its invariant and count pin are
  untouched). Plan 0006 §2's flat-registry decision stands (revisit at ~40 tools).
- Alert storage is the user's `Profile.alerts` document in `vyapaar-runtime` (second writer
  beside `vyapaar-ui-web`'s alerts-store): DTO is camelCase `AlertMeta`-shaped with decimal-string
  `price`, so both writers round-trip the same array. Port contract: caller mints ids
  (`al_<unix_ms>_<counter>`), duplicates rejected; `Triggered` alerts are never flipped by
  enable/disable (ui-web toggle parity); unknown id → `Ok(None)` from the port, surfaced by the
  tool as a not-found `ToolError::Execution`.
- `ToolRisk::LocalWrite` on the chat agent triggers the existing Interactive permission prompt
  per mutating call (`ToolTraits::new` also forces serial execution for non-ReadOnly) — no
  engine change was needed.
- `MarketReader::block_deals` returns `PortError::unavailable` from `FeedMarketReader` until
  `vyapaar-data` gap 0005 closes — never an empty success (chat agent honestly reports the gap).
- `MarketReader` has exactly two implementors: `FeedMarketReader` (runtime-api-rest) and
  `RuntimeMarketReader` (runtime-api, seedable test adapter). Any new MarketReader method must
  touch both (not needed for the alert port: it has the one profile-backed implementor).
- Toolchain's clippy (rust-1.97) flags `map_or(true, ..)` as `unnecessary_map_or`; use
  `is_none_or` in new code.

## Operating Invariants

- Canonical profile `prod`: `cargo build/check/test --profile prod`; `make prod` end-of-turn (§11).
- Tests in `tests/` parallel to `src/`; no in-file `#[test]`.
- Zero floats in financial paths; `rust_decimal::Decimal`.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets --profile prod -- -D warnings`,
  `cargo test --workspace --profile prod` all green (verified 2026-11-11).
- Gap/plan numbering per-repo, sequential, never reused (`.agents/rules/gap-tracking.md`).
