# Gap 0001 — Inline `#[cfg(test)]` Blocks in Production `src/`

## What Violates the Rule

Two production source files still contain inline `#[cfg(test)]` blocks, violating the
workspace TDD rule (`.agents/AGENTS.md §3.2`) that forbids in-file `#[test]` in `src/`:

1. **`crates/vyapaar-agent-core/src/ref_store.rs`** — inline `mod tests` with four unit
   tests (`export_and_resolve_round_trip`, `unknown_reference_resolves_to_none`,
   `export_overwrites_same_id`, `remove_and_uris`).
2. **`crates/vyapaar-agent-research/src/verify/mod.rs`** — inline `mod verify_unit_tests`
   (e.g. `test_clean_strategy_passes`).

## Why It Matters

The workspace TDD rule strictly forbids in-file `#[test]` blocks inside production `src/`
files. Production source files must remain focused on business logic; tests must live in
`crates/*/tests/`. Leaving inline tests in production source:
- Clutters the production module with test scaffolding.
- Prevents clean separation of concerns.
- Breaks the `grep -rn "#\[cfg(test)\]" crates/*/src/` migration check.

## How to Close

1. Move the four `ref_store.rs` tests into `crates/vyapaar-agent-core/tests/ref_store_tests.rs`.
2. Move the `verify` module tests into `crates/vyapaar-agent-research/tests/verify_tests.rs`
   (a `verify_tests.rs` already exists for integration-style tests; add the unit cases there).
3. Delete both `#[cfg(test)]` blocks from `src/`.
4. Verify `grep -rn "#\[cfg(test)\]" crates/*/src/` returns no matches.
5. Verify `cargo test -p vyapaar-agent-core -p vyapaar-agent-research --profile prod` passes.

See `docs/plans/0008-tdd-cleanup-inline-tests.md` for the tracking plan.
