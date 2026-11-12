# `vyapaar-agent` — Migration Guardrails & Invariants

Mandatory architectural guardrails for the `honba-nou → vyapaar-agent` migration and for all
subsequent work in this repository.

## 1. Eliminate Upward Dependency on Runtime & Exec (R1–R3)

- In `honba-nou/Cargo.toml`, `honba-nou-strategy` pointed to `../honba-ya/crates/honba-ya-strategy`. In `vyapaar-agent`, all dependencies strictly flow downward (`vyapaar-core`, `vyapaar-risk`).
- All external storage and reader capabilities are inverted via traits in `vyapaar-agent-core::ports` and `vyapaar-agent-chat-tools::ports`.

## 2. Zero Floating Point Math

- All financial figures, quantities, prices, metrics, and drawdown numbers strictly use `rust_decimal::Decimal`.
- Internal return/statistics kernels in `vyapaar-agent-analytics`, `-forecast`, and `-panel` compute in `f64` without crossing the crate boundary (documented per-crate); no money/price/quantity arithmetic ever crosses as a float.

## 3. No Inline Tests in Production Source

- The rule: zero `#[cfg(test)]` blocks in `crates/*/src/`; all tests reside in `crates/*/tests/`.
- **Current state**: two violations remain —
  - `crates/vyapaar-agent-core/src/ref_store.rs` (inline `mod tests`)
  - `crates/vyapaar-agent-research/src/verify/mod.rs` (inline `mod verify_unit_tests`)
- Both are tracked as **gap 0001** and planned in **plan 0008**. Until closed, the grep below
  is expected to report those two files; the target is zero matches.

```bash
grep -rn "#\[cfg(test)\]" crates/*/src/ || true
```

## 4. Architectural Dependency Rules (R1–R5)

Enforced by integration test `crates/vyapaar-agent-chat/tests/dependency_rules.rs` inspecting `cargo metadata`:

| Rule | Statement |
|------|-----------|
| R1 | No `vyapaar-agent-*` crate may depend on `vyapaar-runtime-*`. Runtime depends on agent or implements agent ports. |
| R2 | No `vyapaar-agent-*` crate may depend on `vyapaar-exec-*`. Execution is mediated by runtime and risk engines. |
| R3 | No `vyapaar-agent-*` crate may depend on `vyapaar-market-*`. Market reads are inverted through reader traits. |
| R4 | No `vyapaar-agent-*` crate may depend on `vyapaar-ledger` except `vyapaar-agent-chat-tools` (types only). |
| R5 | External consumers/adapters depend on minimal core traits (`vyapaar-agent-core::ports`). |

## 5. Untrusted AI Principle (NoFX)

- LLMs are untrusted signal proposers. Chat turns never produce a trade or bypass the `DecisionParser → PriceBand → RiskEngine` pipeline.
- Every decision is parsed into `vyapaar_core::Decision` (`#[serde(deny_unknown_fields)]`) and clamped before reaching execution buffers.

## Verification Commands

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace --profile prod
```
