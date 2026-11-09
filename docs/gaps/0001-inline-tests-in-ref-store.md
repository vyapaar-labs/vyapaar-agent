# Gap 0001 — Inline `#[cfg(test)]` Block in `vyapaar-agent-core/src/ref_store.rs`

## What Violates the Rule

`vyapaar-agent-core/src/ref_store.rs` contains an inline `#[cfg(test)] mod tests { ... }` block (lines 107–159) with four unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_and_resolve_round_trip() { ... }
    #[test]
    fn unknown_reference_resolves_to_none() { ... }
    #[test]
    fn export_overwrites_same_id() { ... }
    #[test]
    fn remove_and_uris() { ... }
}
```

## Why It Matters

The workspace TDD rule (`.agents/AGENTS.md §3.2`) strictly forbids in-file `#[test]` blocks inside production `src/` files. Production source files must remain focused on business logic; tests must live in `crates/vyapaar-agent-core/tests/`. Leaving inline tests in production source:
- Clutters the production module with test scaffolding.
- Prevents clean separation of concerns.
- Fails the migration verification step in `docs/roadmap/migration-verification.md` (`grep -rn "#\[cfg(test)\]" crates/*/src/` must return nothing).

## How to Close

1. Move the four tests from `ref_store.rs` into a new `crates/vyapaar-agent-core/tests/ref_store_tests.rs`.
2. Delete the `#[cfg(test)] mod tests` block from `ref_store.rs`.
3. Verify `grep -rn "#\[cfg(test)\]" crates/*/src/` returns no matches.
4. Verify `cargo test -p vyapaar-agent-core --profile prod` still passes.

See `docs/plans/0002-tdd-cleanup-and-chat-tools.md` for the tracking plan.
