# `vyapaar-agent` — Subagent Migration & DDD Refactoring Guide

> **Tier**: Tier 3A (Depends on `vyapaar-core` and `vyapaar-risk`)
> **Source**: `/home/pmallapp/Devel/Finance/honba-workspace/honba-nou`
> **Target**: `/home/pmallapp/Devel/Finance/vyapaar-labs/vyapaar-agent`
> **Status**: **MIGRATION COMPLETE & EXPANDED (Phase A conversational assistant implemented)**

---

## 1. Sub-Crate Mapping Table

| Source Crate (`honba-nou/crates/`) | Target Crate (`vyapaar-agent/crates/`) | Status | Notes |
| :--- | :--- | :--- | :--- |
| `honba-nou-core` | `vyapaar-agent-core` | Complete | Core AI traits, grounding validators, storage ports (`SessionStore`, `MemoryStore`), shared tool types. |
| `honba-nou-prompt` | `vyapaar-agent-prompt` | Complete | Prompt templates, context builders, chat base prompts, section renderers. |
| `honba-nou-replay` | `vyapaar-agent-replay` | Complete | SHA-256 cassette store, replayer, and canonical `hash_request`. |
| `honba-nou-llm` | `vyapaar-agent-llm` | Complete | LLM client adapters, streaming `chat_stream`, `ScriptedFakeLlm`. |
| `honba-nou-agent` | `vyapaar-agent` | Complete | Agent state machine and memory context. |
| `honba-nou-trader-loop` | `vyapaar-agent-trader-loop` | Complete | Wired to `vyapaar_risk::RiskEngine`. |
| `honba-nou-research` | `vyapaar-agent-research` | Complete | Autonomous research loop. |
| `honba-nou-analytics` | `vyapaar-agent-analytics` | Complete | Attribution and statistical analytics. |
| `honba-nou-genetic-engine` | `vyapaar-agent-genetic` | Complete | Crate renamed: `-genetic-engine` → `-genetic`. |
| `honba-nou-mcp` | `vyapaar-agent-mcp` | Complete | MCP server and JSON-RPC tool handlers. |
| *(New Crate)* | `vyapaar-agent-tools` | Complete | Pure `AgentTool` trait, `ToolCall`, `ToolOutput`, and `ToolRegistry` with unit tests. |
| *(New Crate)* | `vyapaar-agent-plugins` | Complete | `PluginManifest`, `PluginCapability`, and `PluginRegistry` with unit tests. |
| *(New Crate)* | `vyapaar-agent-report` | Complete | `TearsheetReport` & Typst/Markdown source generator consuming `vyapaar-core` `Decimal` metrics. |
| *(New Crate - Design 0001)* | `vyapaar-agent-chat` | Complete (Phase A) | Turn state machine, actor session loop, permissions engine, headless CLI runner. |
| *(New Crate - Design 0001)* | `vyapaar-agent-chat-tools` | Planned (Phase B) | Read-only market & portfolio adapters, `PureToolAdapter` lifting pure tools. |

---

## 2. Mandatory Architectural Guardrails & Invariants
1. **Eliminate Upward Dependency on Runtime & Exec (R1–R3)**:
   - In `honba-nou/Cargo.toml`, `honba-nou-strategy` pointed to `../honba-ya/crates/honba-ya-strategy`. In `vyapaar-agent`, all dependencies strictly flow downward (`vyapaar-core`, `vyapaar-risk`).
   - All external storage and reader capabilities are inverted via traits in `vyapaar-agent-core::ports`.
2. **Zero Floating Point Math**:
   - All financial figures, quantities, prices, metrics, and drawdown numbers strictly use `rust_decimal::Decimal`.
3. **No Inline Tests in Production Source**:
   - Zero `#[cfg(test)]` blocks remain in `crates/*/src/`. All tests reside cleanly under `crates/*/tests/`.
4. **Architectural Dependency Rules**:
   - Enforced by integration test `tests/dependency_rules.rs` inspecting `cargo metadata`.

## 3. Verification Commands
```bash
cd /home/pmallapp/Devel/Finance/vyapaar-labs/vyapaar-agent
grep -rn "#\[cfg(test)\]" crates/*/src/ && exit 1 || true
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```
