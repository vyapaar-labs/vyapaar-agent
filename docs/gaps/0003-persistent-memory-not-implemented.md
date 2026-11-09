# Gap 0003 — Persistent Memory (`UserMemory`) Not Implemented

## What's Missing

Roadmap Phase C (Persistent Memory) lists:
- `UserMemory` implementation with profile budgeters and recall section renderers.
- Cache-hit regression tests for prompt caching.

The `vyapaar-agent-core` crate already defines the persistence ports (`SessionStore`, `MemoryStore`, `MemoryEngine`, `MemoryFact`, `ProfileBudget`, etc.) and `vyapaar-agent-core/src/memory/` holds the in-memory `MemoryEngine`. However, there is no higher-level `UserMemory` abstraction that:
- Maintains per-user profile budgeters.
- Renders a recall section into assembled prompts.
- Persists across sessions beyond the in-memory `MemoryStore`.

## Why It Matters

Without persistent user memory, each chat session starts cold — no user preferences, no recalled context, no profile budget enforcement. This limits the assistant's usefulness across multi-session conversations.

## How to Close

1. Define a `UserMemory` struct layering over `MemoryStore`/`MemoryEngine` with per-user profile budgeters.
2. Add recall-section renderers in `vyapaar-agent-prompt` (prompt section assembly already supports `PromptSection`).
3. Add cache-hit regression tests for prompt caching (`vyapaar-agent-core::prompt` hash functions already exist).
4. Add unit tests in `crates/vyapaar-agent-core/tests/` and `crates/vyapaar-agent-prompt/tests/`.

See `docs/plans/0003-persistent-memory-and-mcp-client.md` for the tracking plan.
