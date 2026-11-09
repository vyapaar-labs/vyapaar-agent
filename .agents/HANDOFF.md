# HANDOFF — vyapaar-agent

**Status**: Plan 0002 complete; plan 0004 Phase 4 complete; plan 0005 finalized. Ready to merge to main.  
**Date**: 2026-11-09 · Repo: `vyapaar-agent` · Branch: `feat/agent-0004-phase4-local-slm-determinism`

## Phase / Slice Status (COMPLETE — this branch)

- **Plan 0002** (job scheduler, memory engine, strategy verification, session checkpoints): commits `d27da44` + `ec54849` + `58255fb`.
- **Plan 0004 Phase 4** (local SLM determinism hardening, plan 0005 §4): commit `b703a59`.
  - `vyapaar-agent-replay`: `LlmAuditRecord` fingerprint fields (`engine`, `engine_version`, `model_file_sha256`, `quantization`, `thread_count`, `hardware`) + `with_*` builders; `hash_file()` utility; `compute_record_hash` chains on the fingerprint.
  - `vyapaar-agent-llm`: `CandleGgufProvider::fingerprint()`; fixed pre-existing `clippy::collapsible_match` in `generate()`.
  - Tests: `tests/audit_fingerprint_tests.rs` (5 tests, pass); live candle_gguf tests run against downloaded `models/*.gguf` and pass.
- **Plan 0005** (reference material): finalized — moved to Implemented Plans in `docs/plans/README.md` citing commit `b703a59`; plan doc removed per cleanup rule. Its actionable output (determinism fingerprint) is implemented in `b703a59`; licensing/quantization/determinism findings were captured in the plan document.

## Decisions & Discoveries

- Tier-1 local-SLM provider role filled by existing `CandleGgufProvider` (pure-Rust Candle CPU, tested live) rather than new `LlamaCppProvider`/`VllmProvider` impls named in 0005 — same purpose, no new engine.
- Two parallel `LlmAuditRecord` types exist (llm crate + replay crate); replay crate's record is the canonical hash-chained audited record and is the one fingerprinted.
- Models already downloaded at `/home/pmallapp/Devel/Finance/vyapaar-labs/models/`: `Llama-3.2-3B-Instruct-Q4_K_M.gguf`, `zenfinance-3b-Instruct.Q4_K_M.gguf`, `SmolLM2-360M-Instruct.Q4_K_M.gguf`.

## Remaining (explicit deferral per plan 0004 design)

- QLoRA fine-tuning on Phase-3 fixtures — needs GPU + training pipeline; not stubbed, deferred.
- BLAS/OMP thread pinning for full CPU determinism — deployment ops; thread count is now auditable via the fingerprint.

## Immediate Next Step

Merge `feat/agent-0004-phase4-local-slm-determinism` into `main`.

## Operating Invariants

- Canonical profile `prod`: `cargo build/check/test --profile prod`.
- Tests in `tests/` parallel to `src/`; no in-file `#[test]`.
- Zero floats in financial paths; `rust_decimal::Decimal` for temperature.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --profile prod` green on touched crates; workspace `cargo check --profile prod` green.
