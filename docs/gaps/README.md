# Gap Index — `vyapaar-agent`

Per workspace rule `.agents/rules/gap-tracking.md`:
- **Gaps** (`docs/gaps/XXXX-*.md`): individual missing features, stubs, or rule violations.
- **Plans** (`docs/plans/XXXX-*.md`): collections of gaps.
- Numbering is per-repo, sequential, never reused.

## Open Gaps

| # | Title | Severity | Status | Plan |
|---|-------|----------|--------|------|
| [0001](0001-inline-tests-in-ref-store.md) | `vyapaar-agent-core/src/ref_store.rs` contains inline `#[cfg(test)]` block | High | Open | [0002](../plans/0002-tdd-cleanup-and-chat-tools.md) |
| [0002](0002-chat-tools-crate-missing.md) | `vyapaar-agent-chat-tools` crate (read-only tools + `PureToolAdapter`) not yet created | High | Open | [0002](../plans/0002-tdd-cleanup-and-chat-tools.md) |
| [0003](0003-persistent-memory-not-implemented.md) | `UserMemory` persistent memory with profile budgeters not implemented | Medium | Open | [0003](../plans/0003-persistent-memory-and-mcp-client.md) |
| [0004](0004-mcp-stdio-client-bridge.md) | MCP stdio client bridge not implemented in `vyapaar-agent-mcp` | Medium | Open | [0003](../plans/0003-persistent-memory-and-mcp-client.md) |
| [0005](0005-research-note-not-implemented.md) | `ResearchNote` in `vyapaar-agent-report` not implemented | Low | Open | [0003](../plans/0003-persistent-memory-and-mcp-client.md) |
| [0006](0006-monitor-scheduler-bridge.md) | Runtime-agent monitor scheduler bridge not implemented | Low | Open | [0003](../plans/0003-persistent-memory-and-mcp-client.md) |
| [0007](0007-prompt-cache-regression-tests.md) | Cache-hit regression tests for prompt caching not implemented | Low | Open | [0003](../plans/0003-persistent-memory-and-mcp-client.md) |

## Closed Gaps

| # | Title | Closed In |
|---|-------|-----------|
| — | — | (none yet) |
