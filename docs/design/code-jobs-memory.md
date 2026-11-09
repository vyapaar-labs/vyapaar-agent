# `vyapaar-agent` — Job Scheduler, Memory Engine & Ports Reference

Documents `vyapaar-agent-core/src/jobs/` and `vyapaar-agent-core/src/memory/` plus the
storage ports in `vyapaar-agent-core/src/ports/`.

## Persistence Ports (`vyapaar-agent-core/src/ports/`)

| Port | Purpose |
|------|---------|
| `SessionStore` | Persist/load chat sessions |
| `MemoryStore` | Persist/load memory facts |
| `SessionScope`, `RuntimeMode` | Session scoping & runtime mode |
| `StoreError`, `StoreErrorKind` | Storage error taxonomy |

Ports are intentionally decoupled from concrete runtimes (`vyapaar-runtime-*` implements
them; agent never depends on runtime per rule R1).

## Job Scheduler (`vyapaar-agent-core/src/jobs/`)

Autonomous scheduled job engine with:
- Cron evaluation (`cron.rs`).
- Multi-dimensional budget enforcement (`spec.rs`).
- Explicit human approval gate.
- SQLite-backed store (`sqlite_store.rs`) + trait store (`store.rs`).
- In-memory store (`memory_store.rs`).

Modules: `cron`, `jobs/mod`, `jobs/scheduler`, `jobs/spec`, `jobs/store`, `jobs/sqlite_store`,
`jobs/memory_store`.

## Memory Engine (`vyapaar-agent-core/src/memory/`)

Local financial memory engine layered over the `MemoryStore` port:

| Type | Purpose |
|------|---------|
| `MemoryEngine` | Core memory read/write engine |
| `MemoryFact` | A single remembered fact |
| `MemoryCategory` | Fact classification |
| `MemoryOrigin` | Where the fact came from |

Modules: `memory/mod.rs`.

## Note (gap 0003)

No higher-level `UserMemory` abstraction with per-user profile budgeters and recall-section
renderers exists yet. Ports and in-memory engine are present; `UserMemory` is planned
(`docs/plans/0003-persistent-memory-and-mcp-client.md`, gap 0003).

## Related

- `vyapaar-agent-core::tool` — `ToolSpec`, `ToolCall`, `ToolOutput`, `ToolError` (shared tool
  contracts).
- `vyapaar-agent-prompt` — prompt templates & section renderers (recall rendering planned).
