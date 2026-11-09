# `vyapaar-agent` — TDD & Engineering Roadmap

## Milestone 1: Core AI Contracts, Prompting & SHA-256 Replay (P0) — [COMPLETED]
- [x] Port `vyapaar-agent-core`, `vyapaar-agent-prompt`, and `vyapaar-agent-replay` from `honba-nou`.
- [x] Verify SHA-256 deterministic replay and `Decision` grounding tests.

## Milestone 2: LLM Providers, Agent, Trader Loop & Genetic Engine (P0) — [COMPLETED]
- [x] Port `vyapaar-agent-llm`, `vyapaar-agent`, `vyapaar-agent-trader-loop`, `vyapaar-agent-research`, `vyapaar-agent-analytics`, `vyapaar-agent-genetic` (from `honba-nou-genetic-engine`), and `vyapaar-agent-mcp`.

## Milestone 3: Agent Tools, Plugins & Typst Report Compiler (P1) — [COMPLETED]
- [x] Implement `vyapaar-agent-tools` (`AgentTool` trait + built-in market/portfolio/screener query tools).
- [x] Implement `vyapaar-agent-plugins` (`PluginManifest`, `PluginRegistry` with deterministic validation).
- [x] Implement `vyapaar-agent-report` (`ResearchReport`, Typst/Markdown tearsheet renderer with `Decimal` metrics).

## Milestone 4: Conversational Assistant & Superset Capabilities (Design Doc 0001)

### Phase A: Conversation Core & Actor Session Loop — [COMPLETED across all repos]
- [x] Storage ports in `vyapaar-agent-core::ports`: `SessionStore`, `MemoryStore`, `StoreError`, `SessionScope`, `RuntimeMode`.
- [x] Shared tool contracts in `vyapaar-agent-core::tool`: `ToolSpec`, `ToolCall`, `ToolOutput`, `ToolError`.
- [x] Prompt assembly in `vyapaar-agent-core::prompt`: `PromptAssembly` (`cached_prefix` vs `turn_context`), `Placement`, `PromptSection`, `default_assemble`.
- [x] Request hashing in `vyapaar-agent-replay`: canonical `hash_request` for chat audit integrity.
- [x] Streaming LLM interface in `vyapaar-agent-llm`: `ChatMessage`, `ChatStreamEvent`, `ChatRequest`, `EventStream`, `LlmProvider::chat_stream`, `ScriptedFakeLlm`.
- [x] Chat base prompt and section renderers in `vyapaar-agent-prompt`.
- [x] Conversation loop crate `vyapaar-agent-chat`:
  - [x] Turn state machine with max rounds, 64 KiB tool outcome truncation, write-ahead persistence, repeat error nudge, synthetic cancellation repair.
  - [x] Permissions engine (`ToolRisk`, `PermissionPolicy`, interactive prompt/responder channels).
  - [x] Channel-based `SessionActor` and `SessionHandle` dispatching typed `SessionEvent` streams.
  - [x] Headless runner (`run_turn_headless`) for unattended CLI execution.
  - [x] In-memory testing stores and unit tests.
  - [x] R1–R3 dependency rules integration test enforcing decoupling via `cargo metadata`.
- [x] Cross-repo Phase A:
  - [x] `vyapaar-runtime`: SQLite & Postgres migrations (`agent_chat_session`, `agent_chat_message`) and `SqlxSessionStore` implementation in `vyapaar-runtime-persistence`.
  - [x] `vyapaar-ui`: `vyapaar-highlight` lexer, `vyapaar-tui` AgentChat tab state machine, and `vyapaar-cli ask` headless runner.

### Phase B: Read-Only Tools (Planned — tracked by plan 0002)
- [ ] Create `vyapaar-agent-chat-tools` crate (gap 0002)
- [ ] Implement `PureToolAdapter` lifting pure `AgentTool`s into chat (gap 0002)
- [ ] Consumer-owned reader ports (`MarketReader`, `PortfolioReader`, `WatchlistStore`) and pure `research_calc` (gap 0002)

### Phase C: Persistent Memory (Planned — tracked by plan 0003)
- [ ] `UserMemory` implementation with profile budgeters and recall section renderers (gap 0003)
- [ ] Cache-hit regression tests for prompt caching (gap 0007)

### Phase D: Monitors & Background Tasks (Planned — tracked by plan 0003)
- [ ] Runtime-agent monitor scheduler bridge (gap 0006)

### Phase E: Reports, Plugins & MCP Client (Planned — tracked by plan 0003)
- [ ] `ResearchNote` in `vyapaar-agent-report` (gap 0005)
- [ ] MCP stdio client bridge in `vyapaar-agent-mcp` (gap 0004)

## Cross-Cutting Cleanup (tracked by plan 0002)
- [ ] Remove inline `#[cfg(test)]` block from `vyapaar-agent-core/src/ref_store.rs` (gap 0001); move tests to `crates/vyapaar-agent-core/tests/ref_store_tests.rs`.
