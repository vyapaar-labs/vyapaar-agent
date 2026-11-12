# Plan 0008 — TDD Cleanup: Remove Inline Tests from `src/`

> Renumbered from the original "plan 0002 (TDD Cleanup & `vyapaar-agent-chat-tools`)".
> The chat-tools half of that plan is **implemented** (gap 0002 closed); only the TDD
> cleanup half remains, tracking gap 0001.

## Objective

Close gap 0001: eliminate all inline `#[cfg(test)]` blocks from production `src/` files.
Two locations remain:

- `crates/vyapaar-agent-core/src/ref_store.rs` (inline `mod tests`, 4 tests).
- `crates/vyapaar-agent-research/src/verify/mod.rs` (inline `mod verify_unit_tests`).

## Gaps Tracked by This Plan

| Gap | Title | Severity |
|-----|-------|----------|
| [0001](../gaps/0001-inline-tests-in-ref-store.md) | Inline `#[cfg(test)]` blocks in `ref_store.rs` and `research/verify/mod.rs` | High |

## Implementation Phases

1. Move the four `ref_store.rs` tests into `crates/vyapaar-agent-core/tests/ref_store_tests.rs`.
2. Move the `verify` unit tests into `crates/vyapaar-agent-research/tests/verify_tests.rs`.
3. Delete both `#[cfg(test)]` blocks from `src/`.
4. Verify `grep -rn "#\[cfg(test)\]" crates/*/src/` returns no matches.
5. Run `cargo test -p vyapaar-agent-core -p vyapaar-agent-research --profile prod`, then
   `cargo fmt --check` and `cargo clippy --all-targets --profile prod -- -D warnings`.

## Exit Criteria

- `grep -rn "#\[cfg(test)\]" crates/*/src/` returns nothing.
- `cargo test --workspace --profile prod` passes.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets --profile prod -- -D warnings` pass.
- `docs/design/code-guardrails.md` and `docs/design/code-migration-map.md` no longer list
  inline-test violations.

## Related

- `docs/design/code-refstore.md` — the `DataReferenceStore` module whose tests move.
- `docs/design/code-trader-research.md` — the research `verify/` module whose tests move.
