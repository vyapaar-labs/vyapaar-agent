# Gap 0003 — Persistent Memory (`UserMemory`) Not Implemented

## What's Missing

Roadmap Phase C (Persistent Memory) listed:
- `UserMemory` implementation with per-user profile budgeters and recall section renderers.
- Cache-hit regression tests for prompt caching (now tracked separately as gap 0007).

**Implemented so far**: `vyapaar-agent-core/src/ports/` defines the storage ports
(`SessionStore`, `MemoryStore`, `MemoryId`, `MemoryKind`, `MemoryOrigin`, `MemoryQuery`,
`NewMemory`, `SaveOutcome`, `ScoredMemory`, `StoreError`, `ProfileBudget`) and
`vyapaar-agent-core/src/memory/` implements the in-memory `MemoryEngine`, `MemoryFact`,
`MemoryCategory`, and `MemoryOrigin` (`remember`, `search`, `distill_tick`, `forget`).

**Missing**: the higher-level `UserMemory` abstraction that:
- Maintains per-user profile budgeters.
- Renders a recall section into assembled prompts (stable-prefix / per-turn split per Design
  Doc 0001 §7).
- Persists across sessions via a real `MemoryStore` implementation (SQLite in
  `vyapaar-runtime-persistence`).

## Why It Matters

Without persistent user memory, each chat session starts cold — no user preferences, no
recalled context, no profile budget enforcement. This limits the assistant's usefulness
across multi-session conversations.

## How to Close

1. Define a `UserMemory` struct layering over `MemoryStore`/`MemoryEngine` with per-user
   profile budgeters.
2. Add recall-section renderers in `vyapaar-agent-prompt` (prompt section assembly already
   supports `PromptSection` placement).
3. Add unit tests in `crates/vyapaar-agent-core/tests/` and `crates/vyapaar-agent-prompt/tests/`.

See `docs/plans/0009-persistent-memory-mcp-client-and-phase-e.md` for the tracking plan.
