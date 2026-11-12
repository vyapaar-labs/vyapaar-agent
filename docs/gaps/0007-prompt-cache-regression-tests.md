# Gap 0007 — Prompt Cache-Hit Regression Tests Not Implemented

## What's Missing

Roadmap Phase C lists "Cache-hit regression tests for prompt caching" as planned, but they
do not exist.

`vyapaar-agent-core::prompt` already defines `PromptAssembly`, `PromptHash`,
`compute_prompt_hash`, `hash_prompt_hex`, `sha256_hex`, and `default_assemble` (with
`cached_prefix` vs `turn_context` placement). However, there are no tests asserting that
reusing a `cached_prefix` produces a cache hit (stable hash) while varying `turn_context`
produces a cache miss (different hash).

## Why It Matters

Prompt caching is the mechanism that keeps long conversations cheap. Without regression
tests asserting cache-hit/cache-miss behavior, refactors to prompt assembly could silently
break caching and increase token costs.

## How to Close

1. Add `crates/vyapaar-agent-core/tests/prompt_cache_tests.rs` asserting:
   - Same `cached_prefix` + same `turn_context` → same hash (cache hit).
   - Same `cached_prefix` + different `turn_context` → different hash (cache miss).
   - Different `cached_prefix` → different hash.
2. Add `crates/vyapaar-agent-prompt/tests/` for section renderer cache behavior.

See `docs/plans/0009-persistent-memory-mcp-client-and-phase-e.md` for the tracking plan.
