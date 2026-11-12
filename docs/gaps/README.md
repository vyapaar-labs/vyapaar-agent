# Gap Index — `vyapaar-agent`

Per workspace rule `.agents/rules/gap-tracking.md`:
- **Gaps** (`docs/gaps/XXXX-*.md`): individual missing features, stubs, or rule violations.
- **Plans** (`docs/plans/XXXX-*.md`): collections of gaps.
- Numbering is per-repo, sequential, never reused.

## Open Gaps

| # | Title | Severity | Status | Plan |
|---|-------|----------|--------|------|
| [0001](0001-inline-tests-in-ref-store.md) | Inline `#[cfg(test)]` blocks in `vyapaar-agent-core/src/ref_store.rs` **and** `vyapaar-agent-research/src/verify/mod.rs` | High | Open | [0008](../plans/0008-tdd-cleanup-inline-tests.md) |
| [0003](0003-persistent-memory-not-implemented.md) | `UserMemory` persistent memory with profile budgeters not implemented | Medium | Open | [0009](../plans/0009-persistent-memory-mcp-client-and-phase-e.md) |
| [0004](0004-mcp-stdio-client-bridge.md) | MCP stdio client bridge not implemented in `vyapaar-agent-mcp` | Medium | Open | [0009](../plans/0009-persistent-memory-mcp-client-and-phase-e.md), [0007 Phase 17](../plans/0007-comprehensive-mcp-ecosystem-and-agent-implementation-guide.md) |
| [0005](0005-research-note-not-implemented.md) | `ResearchNote` in `vyapaar-agent-report` not implemented | Low | Open | [0009](../plans/0009-persistent-memory-mcp-client-and-phase-e.md) |
| [0006](0006-monitor-scheduler-bridge.md) | Runtime-agent monitor scheduler bridge not implemented (agent-side job scheduler exists) | Low | Open | [0009](../plans/0009-persistent-memory-mcp-client-and-phase-e.md) |
| [0007](0007-prompt-cache-regression-tests.md) | Cache-hit regression tests for prompt caching not implemented | Low | Open | [0009](../plans/0009-persistent-memory-mcp-client-and-phase-e.md) |

## Closed Gaps

| # | Title | Closed In |
|---|-------|-----------|
| 0002 | `vyapaar-agent-chat-tools` crate (read-only tools + `PureToolAdapter`) | Implemented (Phase B) in commit `f3f1cdc` (read-only tools & ports) + `85d3c07` (strategy advisory tools), expanded by plan 0006 Phases 1–4 (`a72689c`, `98c66c2`, `127597d`). See `docs/design/code-chat-tools.md`. |
