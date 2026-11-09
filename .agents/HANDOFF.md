# HANDOFF — vyapaar-agent

**Status**: Plan 0004 Phase 4 determinism hardening implemented, verified, and committed.  
**Date**: 2026-11-09 · Repo: `vyapaar-agent` · Branch: `feat/agent-0004-phase4-local-slm-determinism`

## Phase / Slice Status (COMPLETE — this batch)

Plan 0004 Phase 4 (local SLM / Tier 1 determinism hardening, per plan 0005 §4):

- **Determinism fingerprint on the audited record** (`vyapaar-agent-replay::audit`):
  `LlmAuditRecord` gains `engine`, `engine_version`, `model_file_sha256`, `quantization`,
  `thread_count`, `hardware` fields with `with_*` builders. `compute_record_hash` chains on the
  fingerprint so two records differing only in serving stack hash differently (deterministic
  replay requirement: `prompt + seed + temperature = 0` must pin the exact engine/weights).
- **`hash_file` utility** (`vyapaar-agent-replay::audit`): streams a `.gguf` model file in 64 KiB
  chunks and returns its SHA-256, so the exact weights (not just the model name tag) are pinned.
- **`CandleGgufProvider::fingerprint()`** (`vyapaar-agent-llm/src/candle_gguf.rs`): async method
  computing engine id, candle-core version, model-file hash (offloaded to `spawn_blocking`),
  quantization (from file name), thread count, and hardware identity.
- Fixed pre-existing `clippy::collapsible_match` in `candle_gguf.rs::generate` (line shifted by
  additions) so the touched crate is clippy `-D warnings` clean.
- Tests: `tests/audit_fingerprint_tests.rs` (5 tests) parallel to `src/`; all pass. Live SLM tests
  in `candle_gguf_tests.rs` ran against the downloaded `models/zenfinance-3b-Instruct.Q4_K_M.gguf`
  and pass.

Commits: one batch commit (implementation + tests).

## Decisions & Discoveries

- Models already downloaded at `/home/pmallapp/Devel/Finance/vyapaar-labs/models/`:
  `Llama-3.2-3B-Instruct-Q4_K_M.gguf` (2GB), `zenfinance-3b-Instruct.Q4_K_M.gguf` (270MB),
  `SmolLM2-360M-Instruct.Q4_K_M.gguf` (270MB).
- The Tier-1 local-SLM provider role is filled by the existing `CandleGgufProvider` (pure-Rust
  Candle, CPU, tested live) rather than a `LlamaCppProvider` — same purpose, no new engine.
- Two parallel `LlmAuditRecord` types exist (llm crate + replay crate). The replay crate's record
  is the canonical hash-chained audited record and is the one fingerprinted here.
- `std::arch::arch_info()` is nightly-only; hardware identity uses stable `std::env::consts`.

## Remaining (deferred in plan 0004 by design; not part of this batch)

- QLoRA fine-tuning on Phase-3 fixtures (`tests/golden/omniroute_decision_fixtures.json`) — needs
  GPU + a fine-tuning pipeline; no gap filed because it is an explicit plan deferral, not a stub.
- Trade-off note: `CandleGgufProvider` loads on `Device::Cpu` with fixed seed 42; per plan 0005 §4,
  full CPU determinism also wants thread/BLAS pinning (`OMP_NUM_THREADS=1`) — fingerprint captures
  thread count so it can be audited, but BLAS pinning is deployment ops, not code.

## Immediate Next Step

None — batch complete. Next plan per `docs/plans/README.md` is 0005 (reference material only) or
another bounded context.

## Operating Invariants

- Canonical profile `prod`: `cargo build/check/test --profile prod`.
- Tests in `tests/` parallel to `src/`; no in-file `#[test]`.
- Zero floats in financial paths; `rust_decimal::Decimal` for temperature.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --profile prod`
  green on touched crates; workspace `cargo check --profile prod` green.
