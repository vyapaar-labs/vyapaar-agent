# Plan 0002 — TDD Cleanup & `vyapaar-agent-chat-tools` Crate

## Objective

Close gaps 0001 and 0002: eliminate the inline `#[cfg(test)]` block in `vyapaar-agent-core/src/ref_store.rs` (violating the no-inline-tests TDD rule) and create the missing `vyapaar-agent-chat-tools` crate with read-only tools + `PureToolAdapter`.

## Gaps Tracked by This Plan

| Gap | Title | Severity |
|-----|-------|----------|
| [0001](../gaps/0001-inline-tests-in-ref-store.md) | `ref_store.rs` contains inline `#[cfg(test)]` block | High |
| [0002](../gaps/0002-chat-tools-crate-missing.md) | `vyapaar-agent-chat-tools` crate not yet created | High |

## Implementation Phases

### Phase A — TDD cleanup (gap 0001)

1. Move the four tests from `vyapaar-agent-core/src/ref_store.rs` (`export_and_resolve_round_trip`, `unknown_reference_resolves_to_none`, `export_overwrites_same_id`, `remove_and_uris`) into `crates/vyapaar-agent-core/tests/ref_store_tests.rs`.
2. Delete the `#[cfg(test)] mod tests` block from `ref_store.rs`.
3. Verify `grep -rn "#\[cfg(test)\]" crates/*/src/` returns no matches.
4. Run `cargo test -p vyapaar-agent-core --profile prod` and `cargo fmt --check` + `cargo clippy --profile prod -p vyapaar-agent-core -- -D warnings`.

### Phase B — `vyapaar-agent-chat-tools` crate (gap 0002)

1. Add `crates/vyapaar-agent-chat-tools/` to workspace members in `Cargo.toml`.
2. Create crate with:
   - `PureToolAdapter` lifting pure `AgentTool`s (`vyapaar-agent-tools`) into `ChatTool`s (`vyapaar-agent-chat`) without duplicating schemas or execution logic.
   - Consumer-owned reader ports: `MarketReader`, `PortfolioReader`, `WatchlistStore` — reading through `vyapaar-agent-core::ports` traits only.
   - Pure `research_calc` tool (side-effect-free, deterministic).
3. **Dependency-rule compliance**: `vyapaar-agent-chat-tools` must NOT depend on `vyapaar-runtime-*`, `vyapaar-exec-*`, or `vyapaar-market-*` (architecture R1–R3). Market reads are inverted through reader traits.
4. Add unit tests in `crates/vyapaar-agent-chat-tools/tests/`.
5. Update `docs/design/architecture.md` sub-crate table (14 → 15 crates; remove the "will expand to 15" note).
6. Update roadmap Phase B checkboxes to complete.

## Exit Criteria

- `grep -rn "#\[cfg(test)\]" crates/*/src/` returns nothing.
- `cargo test --workspace --profile prod` passes.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets --profile prod -- -D warnings` pass.
- `vyapaar-agent-chat-tools` builds and its tests pass.
- `docs/design/architecture.md` lists 15 crates with no "planned" note for chat-tools.

## Related

- `vyapaar-agent-core::ports` — storage/reader traits (`SessionStore`, `MemoryStore`).
- `vyapaar-agent-tools` — pure `AgentTool` trait (`ToolSpec`, `ToolCall`, `ToolOutput`).
- `vyapaar-agent-chat` — async `ChatTool`, `PureToolAdapter` lift target.
