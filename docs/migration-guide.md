# `vyapaar-agent` — Subagent Migration & DDD Refactoring Guide

> **Tier**: Tier 3A (Depends on `vyapaar-core` and `vyapaar-risk`)
> **Source**: `/home/pmallapp/Devel/Finance/honba-workspace/honba-nou`
> **Target**: `/home/pmallapp/Devel/Finance/vyapaar-labs/vyapaar-agent`

---

## 1. Sub-Crate Mapping Table

| Source Crate (`honba-nou/crates/`) | Target Crate (`vyapaar-agent/crates/`) | Notes |
| :--- | :--- | :--- |
| `honba-nou-core` | `vyapaar-agent-core` | Port core AI traits and grounding validators. |
| `honba-nou-prompt` | `vyapaar-agent-prompt` | Port prompt templates and context builders. |
| `honba-nou-replay` | `vyapaar-agent-replay` | Port SHA-256 cassette store and replayer. |
| `honba-nou-llm` | `vyapaar-agent-llm` | Port LLM client adapters and `ScriptedFakeLlm`. |
| `honba-nou-agent` | `vyapaar-agent` | Port agent state machine. |
| `honba-nou-trader-loop` | `vyapaar-agent-trader-loop` | Wire to `vyapaar_risk::RiskEngine`. |
| `honba-nou-research` | `vyapaar-agent-research` | Port autonomous research loop. |
| `honba-nou-analytics` | `vyapaar-agent-analytics` | Port attribution and statistical analytics. |
| `honba-nou-genetic-engine` | `vyapaar-agent-genetic` | Note crate rename: `-genetic-engine` → `-genetic`. |
| `honba-nou-mcp` | `vyapaar-agent-mcp` | Port MCP server and JSON-RPC tool handlers. |
| *(New Crate)* | `vyapaar-agent-tools` | Implement pure `AgentTool` trait, `ToolCall`, `ToolOutput`, and `ToolRegistry` with unit tests in `tests/`. |
| *(New Crate)* | `vyapaar-agent-plugins` | Implement `PluginManifest`, `PluginCapability`, and `PluginRegistry` with unit tests in `tests/`. |
| *(New Crate)* | `vyapaar-agent-report` | Implement `TearsheetReport` & Typst/Markdown source generator consuming `vyapaar-core` `Decimal` metrics with unit tests in `tests/`. |

---

## 2. Mandatory DDD Fixes During Migration
1. **Eliminate Upward Dependency on Runtime (`honba-ya-strategy`)**:
   - In `honba-nou/Cargo.toml`, `honba-nou-strategy` pointed to `../honba-ya/crates/honba-ya-strategy`. Ensure `vyapaar-agent` only depends downward on `vyapaar-core` and `vyapaar-risk` (or if `vyapaar-runtime-strategy` is needed, keep `vyapaar-agent` independent so `vyapaar-runtime` can depend on `vyapaar-agent` without cycles).
2. **Extract Inline Tests**: Ensure zero `#[cfg(test)]` blocks remain in `crates/*/src/`.

## 3. Verification Commands
```bash
cd /home/pmallapp/Devel/Finance/vyapaar-labs/vyapaar-agent
grep -rn "#\[cfg(test)\]" crates/*/src/ && exit 1 || true
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```
