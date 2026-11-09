# Plan 0003 — Persistent Memory, MCP Client Bridge & Phase E

## Objective

Close gaps 0003–0007: implement persistent `UserMemory` with profile budgeters, an MCP stdio client bridge, `ResearchNote` in `vyapaar-agent-report`, the runtime-agent monitor scheduler bridge, and prompt cache-hit regression tests. This plan also covers roadmap Phases C, D, and E.

## Gaps Tracked by This Plan

| Gap | Title | Severity |
|-----|-------|----------|
| [0003](../gaps/0003-persistent-memory-not-implemented.md) | `UserMemory` persistent memory with profile budgeters not implemented | Medium |
| [0004](../gaps/0004-mcp-stdio-client-bridge.md) | MCP stdio client bridge not implemented | Medium |
| [0005](../gaps/0005-research-note-not-implemented.md) | `ResearchNote` in `vyapaar-agent-report` not implemented | Low |
| [0006](../gaps/0006-monitor-scheduler-bridge.md) | Runtime-agent monitor scheduler bridge not implemented | Low |
| [0007](../gaps/0007-prompt-cache-regression-tests.md) | Prompt cache-hit regression tests not implemented | Low |

## Implementation Phases

### Phase C — Persistent memory + cache-hit tests (gaps 0003, 0007)

1. Define a `UserMemory` struct layering over `MemoryStore`/`MemoryEngine` with per-user profile budgeters (`ProfileBudget`).
2. Add recall-section renderers in `vyapaar-agent-prompt` (prompt section assembly already supports `PromptSection`).
3. Add `crates/vyapaar-agent-core/tests/prompt_cache_tests.rs` asserting cache-hit (same hash) / cache-miss (different hash) behavior of `compute_prompt_hash` / `hash_prompt_hex`.
4. Add `crates/vyapaar-agent-prompt/tests/` for section renderer cache behavior.
5. Add unit tests in `crates/vyapaar-agent-core/tests/` and `crates/vyapaar-agent-prompt/tests/`.

### Phase D — Monitor scheduler bridge (gap 0006)

1. Define monitor/scheduler ports in `vyapaar-agent-core::jobs` or `vyapaar-agent-core::ports` (invert through traits — do NOT depend on `vyapaar-runtime-*` per rule R1).
2. Implement the bridge in `vyapaar-runtime` (runtime may depend on agent per R1 inversion).
3. Add unit tests.

### Phase E — MCP client bridge + ResearchNote (gaps 0004, 0005)

1. Add an MCP stdio client module in `vyapaar-agent-mcp` (or new `vyapaar-agent-mcp-client` crate) connecting to external MCP servers over stdio, mirroring the server's `RpcMessage`/`RpcError`.
2. Expose discovered external tools through the `AgentTool`/`ChatTool` registry.
3. Define `ResearchNote` in `vyapaar-agent-report` (hypothesis, evidence refs, verification outcome, metrics).
4. Integrate `vyapaar-agent-research` output so verified hypotheses produce `ResearchNote`s.
5. Add Typst/Markdown rendering for research notes.
6. Add unit tests in `crates/vyapaar-agent-mcp/tests/` and `crates/vyapaar-agent-report/tests/`.

## Exit Criteria

- `cargo test --workspace --profile prod` passes.
- `cargo fmt --check` and `cargo clippy --workspace --all-targets --profile prod -- -D warnings` pass.
- `UserMemory` compiles with profile budgeters and recall rendering.
- Prompt cache-hit/cache-miss regression tests pass.
- MCP stdio client connects to a mock server and exposes tools.
- `ResearchNote` renders via Typst/Markdown.
- Roadmap Phases C, D, E checkboxes marked complete.

## Related

- `vyapaar-agent-core::prompt` — `PromptAssembly`, `PromptHash`, `compute_prompt_hash`, `hash_prompt_hex`.
- `vyapaar-agent-core::memory` — `MemoryEngine`, `MemoryFact`, `ProfileBudget`.
- `vyapaar-agent-mcp` — existing MCP server (`RpcMessage`, `RpcError`, tool handlers).
- `vyapaar-agent-research` — autonomous research loop output source for `ResearchNote`.
