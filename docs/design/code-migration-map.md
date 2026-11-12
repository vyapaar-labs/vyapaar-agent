# `vyapaar-agent` — Historical Migration Map (honba-nou → vyapaar-agent)

> **Tier**: Tier 3A (Depends on `vyapaar-core` and `vyapaar-risk`; `vyapaar-agent-chat-tools`
> additionally uses `vyapaar-ledger` types only)
> **Source**: `honba-workspace/honba-nou`
> **Target**: `vyapaar-agent`
> **Status**: Migration complete & expanded (17 crates; conversational assistant Phase A
> implemented). The original roadmap has been dismantled into `docs/design/`,
> `docs/gaps/`, and `docs/plans/`.

## Sub-Crate Mapping Table

| Source Crate (`honba-nou/crates/`) | Target Crate (`vyapaar-agent/crates/`) | Status | Notes |
| :--- | :--- | :--- | :--- |
| `honba-nou-core` | `vyapaar-agent-core` | Complete | Core AI traits, grounding validators, storage ports (`SessionStore`, `MemoryStore`), shared tool types. Extended with `result`, `ref_store`, `jobs/`, `memory/`. |
| `honba-nou-prompt` | `vyapaar-agent-prompt` | Complete | Prompt templates, context builders, chat base prompts, section renderers, personas. |
| `honba-nou-replay` | `vyapaar-agent-replay` | Complete | SHA-256 cassette store, replayer, canonical `hash_request`, audit records. |
| `honba-nou-llm` | `vyapaar-agent-llm` | Complete | LLM client adapters, streaming `chat_stream`, `ScriptedFakeLlm`; extended with `OpenAiCompatibleProvider` and `CandleGgufProvider`. |
| `honba-nou-agent` | `vyapaar-agent` | Complete | Agent state machine, decision parser, audit logger, replayer. |
| `honba-nou-trader-loop` | `vyapaar-agent-trader-loop` | Complete | Wired to `vyapaar_risk::RiskEngine`. |
| `honba-nou-research` | `vyapaar-agent-research` | Complete | Autonomous research loop; extended with static strategy verifier (`verify/`). |
| `honba-nou-analytics` | `vyapaar-agent-analytics` | Complete | Attribution and statistical analytics. |
| `honba-nou-genetic-engine` | `vyapaar-agent-genetic` | Complete | Crate renamed: `-genetic-engine` → `-genetic`. |
| `honba-nou-mcp` | `vyapaar-agent-mcp` | Complete | MCP server and JSON-RPC tool handlers; Stdio + SSE transports; gateway discovery. |
| *(New Crate)* | `vyapaar-agent-tools` | Complete | Pure `AgentTool` trait, `ToolCall`, `ToolOutput`, and `ToolRegistry` with unit tests. |
| *(New Crate)* | `vyapaar-agent-plugins` | Complete | `PluginManifest`, `PluginCapability`, and `PluginRegistry` with unit tests. |
| *(New Crate)* | `vyapaar-agent-report` | Complete | `TearsheetReport` & Typst/Markdown source generator consuming `vyapaar-core` `Decimal` metrics. |
| *(New Crate - Design 0001)* | `vyapaar-agent-chat` | Complete (Phase A) | Turn state machine, actor session loop, permissions engine, headless CLI runner, checkpoints. |
| *(New Crate - Design 0001)* | `vyapaar-agent-chat-tools` | Complete | Read-only market/portfolio/content/alert tools + `PureToolAdapter` lifting pure tools (19 read-only + 2 alert + 3 strategy advisory tools). |
| *(New Crate - plan 0003)* | `vyapaar-agent-forecast` | Complete | Theta / ETS / ARIMA / TBATS / gradient-boost forecasting; backs MCP `vyapaar_forecast_series`. |
| *(New Crate - plan 0003)* | `vyapaar-agent-panel` | Complete | Fama-MacBeth, fixed/random effects, GMM/2SLS, GRS factor asset-pricing tests. |

## Verification Commands

```bash
# Run from the repository root
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace --profile prod
```

> Note: two inline `#[cfg(test)]` blocks remain in `src/` (`vyapaar-agent-core/src/ref_store.rs`
> and `vyapaar-agent-research/src/verify/mod.rs`) — tracked as gap 0001, planned in plan 0008.
> All other crates follow the parallel-`tests/` rule.
