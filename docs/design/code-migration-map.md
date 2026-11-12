# `vyapaar-agent` — Historical Migration Map (honba-nou → vyapaar-agent)

> **Tier**: Tier 3A (Depends on `vyapaar-core` and `vyapaar-risk`)
> **Source**: `honba-workspace/honba-nou`
> **Target**: `vyapaar-agent`
> **Status**: Migration complete & expanded (Phase A conversational assistant implemented).

## Sub-Crate Mapping Table

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

## Verification Commands

```bash
# Run from the repository root
grep -rn "#\[cfg(test)\]" crates/*/src/ && exit 1 || true
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```
