# `vyapaar-agent` — Migration Verification Milestone

Verification commands for the `honba-nou → vyapaar-agent` migration:

```bash
# Run from the repository root
grep -rn "#\[cfg(test)\]" crates/*/src/ && exit 1 || true
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

## Migration Milestone Status

| Milestone | Status |
|-----------|--------|
| All sub-crates ported (14 crates) | Complete |
| `vyapaar-agent-chat-tools` planned (Phase B) | Planned |
| Zero inline tests in `src/` | Complete |
| Dependency rules R1–R5 enforced | Complete |
| `cargo fmt --check` | Clean |
| `cargo clippy --all-targets -- -D warnings` | Clean |
| `cargo test --workspace` | Passing |
