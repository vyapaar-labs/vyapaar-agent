# `vyapaar-agent` — Migration Guardrails & Invariants

Mandatory architectural guardrails for the `honba-nou → vyapaar-agent` migration.

## 1. Eliminate Upward Dependency on Runtime & Exec (R1–R3)

- In `honba-nou/Cargo.toml`, `honba-nou-strategy` pointed to `../honba-ya/crates/honba-ya-strategy`. In `vyapaar-agent`, all dependencies strictly flow downward (`vyapaar-core`, `vyapaar-risk`).
- All external storage and reader capabilities are inverted via traits in `vyapaar-agent-core::ports`.

## 2. Zero Floating Point Math

- All financial figures, quantities, prices, metrics, and drawdown numbers strictly use `rust_decimal::Decimal`.

## 3. No Inline Tests in Production Source

- Zero `#[cfg(test)]` blocks remain in `crates/*/src/`. All tests reside cleanly under `crates/*/tests/`.
- Enforced by: `grep -rn "#\[cfg(test)\]" crates/*/src/` must return 0 matches.

## 4. Architectural Dependency Rules (R1–R5)

Enforced by integration test `tests/dependency_rules.rs` inspecting `cargo metadata`:

| Rule | Statement |
|------|-----------|
| R1 | No `vyapaar-agent-*` crate may depend on `vyapaar-runtime-*`. Runtime depends on agent or implements agent ports. |
| R2 | No `vyapaar-agent-*` crate may depend on `vyapaar-exec-*`. Execution is mediated by runtime and risk engines. |
| R3 | No `vyapaar-agent-*` crate may depend on `vyapaar-market-*`. Market reads are inverted through reader traits. |
| R4 | No `vyapaar-agent-*` crate may depend on `vyapaar-ledger` except `vyapaar-agent-chat-tools` (types only). |
| R5 | External consumers/adapters depend on minimal core traits (`vyapaar-agent-core::ports`). |

## Verification Commands

```bash
cd /home/pmallapp/Devel/Finance/vyapaar-labs/vyapaar-agent
grep -rn "#\[cfg(test)\]" crates/*/src/ && exit 1 || true
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```
