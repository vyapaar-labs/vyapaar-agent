# HANDOFF — vyapaar-agent

**Status**: All implemented plans (0001–0005) complete and merged to `main`. New documentation batch added: `docs/gaps/` (7 open gaps), `docs/plans/` (plans 0002 & 0003 active), `docs/design/` (9 code reference docs), `docs/roadmap/` (updated). Batch done.
**Date**: 2026-11-09 · Repo: `vyapaar-agent` · Branch: `main`

## Phase / Slice Status (COMPLETE — merged)

- **Plan 0001** (MCP gateway discovery, universal result protocol, 15 quantitative engines): implemented — `VyapaarResultEnvelope`, `DataReferenceStore`, MCP gateway discovery, 10 tests.
- **Plan 0002** (job scheduler, memory engine, strategy verification, session checkpoints): commits `d27da44` + `ec54849` + `58255fb`. Merged in `5ac0919`.
- **Plan 0004 Phase 4** (local SLM determinism hardening, plan 0005 §4): commit `b703a59`. Merged in `5ac0919`.
- **Plan 0005** (reference material): finalized in `c5557fa`. Merged in `5ac0919`.
- **Plan 0004** (Free-Tier Multi-Provider LLM Backend): fully implemented through Phase 3. Phase 4 (local SLM) deferred by design. Commit `e6b9b76`.

## Active Plans (new — tracking open gaps)

- **Plan 0002** `docs/plans/0002-tdd-cleanup-and-chat-tools.md`: TDD cleanup (gap 0001 — inline `#[cfg(test)]` in `ref_store.rs`) + `vyapaar-agent-chat-tools` crate (gap 0002). Target: `vyapaar-agent-core`, `vyapaar-agent-chat-tools`.
- **Plan 0003** `docs/plans/0003-persistent-memory-and-mcp-client.md`: persistent `UserMemory` (gap 0003), MCP stdio client bridge (gap 0004), `ResearchNote` (gap 0005), monitor scheduler bridge (gap 0006), prompt cache-hit regression tests (gap 0007). Target: `vyapaar-agent-core`, `vyapaar-agent-prompt`, `vyapaar-agent-mcp`, `vyapaar-agent-report`, `vyapaar-agent-research`.

## Immediate Next Step

Execute plan 0002: (1) move inline tests out of `vyapaar-agent-core/src/ref_store.rs` into `crates/vyapaar-agent-core/tests/ref_store_tests.rs`; (2) create `vyapaar-agent-chat-tools` crate with `PureToolAdapter` + reader ports, respecting dependency rules R1–R3.

## Decisions & Discoveries (final)

- Tier-1 local-SLM provider role filled by existing `CandleGgufProvider` (pure-Rust Candle CPU) rather than new `LlamaCppProvider`/`VllmProvider` impls.
- Two parallel `LlmAuditRecord` types exist; replay crate's record is the canonical hash-chained audited record (fingerprinted).
- Models downloaded at `/home/pmallapp/Devel/Finance/vyapaar-labs/models/`: `Llama-3.2-3B-Instruct-Q4_K_M.gguf`, `zenfinance-3b-Instruct.Q4_K_M.gguf`, `SmolLM2-360M-Instruct.Q4_K_M.gguf`.
- Rule violation found: `vyapaar-agent-core/src/ref_store.rs` contains inline `#[cfg(test)]` block (gap 0001) — must be moved to `tests/` per TDD rule.
- `vyapaar-agent-chat-tools` (15th crate) does not yet exist (gap 0002).

## Remaining (explicit deferral per plan 0004 design)

- QLoRA fine-tuning on Phase-3 fixtures — needs GPU + training pipeline; deferred, not stubbed.
- BLAS/OMP thread pinning for full CPU determinism — deployment ops; thread count now auditable via fingerprint.

## Operating Invariants

- Canonical profile `prod`: `cargo build/check/test --profile prod`.
- Tests in `tests/` parallel to `src/`; no in-file `#[test]` (gap 0001 is an active violation to fix).
- Zero floats in financial paths; `rust_decimal::Decimal` for temperature.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --profile prod` green on touched crates; workspace `cargo check --profile prod` green.
- Gap/plan numbering is per-repo, sequential, never reused (`.agents/rules/gap-tracking.md`).
