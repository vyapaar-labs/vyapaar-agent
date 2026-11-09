# `vyapaar-agent` — DDD Architecture & AI Agent Bounded Context

## 1. Bounded Context & Safety Guardrails
`vyapaar-agent` (Tier 3A, evolved from `honba-nou`) is the **AI Reasoning & Autonomous Research Bounded Context**. Per the NoFX Untrusted AI Principle:
* LLMs are treated strictly as untrusted signal/decision proposers.
* Every LLM completion on the trading decision surface is parsed into `vyapaar_core::Decision` (`#[serde(deny_unknown_fields)]`), grounded against real-time `PriceBand`s, and clamped by `vyapaar_risk::RiskEngine`.
* Every prompt, seed, temperature, and completion is SHA-256 hashed (`vyapaar-agent-replay`) for deterministic backtest replay.
* Zero floating-point math: all financial quantities use `rust_decimal::Decimal`.

## 2. Architectural Dependency Rules (R1–R5)

Per Design Doc 0001, `vyapaar-agent` strictly enforces inverted dependencies:
* **R1**: No `vyapaar-agent-*` crate may depend on `vyapaar-runtime-*`. Runtime depends on agent or implements agent ports.
* **R2**: No `vyapaar-agent-*` crate may depend on `vyapaar-exec-*`. Execution is mediated by runtime and risk engines.
* **R3**: No `vyapaar-agent-*` crate may depend on `vyapaar-market-*`. Market reads are inverted through reader traits.
* **R4**: No `vyapaar-agent-*` crate may depend on `vyapaar-ledger` except `vyapaar-agent-chat-tools` (types only).
* **R5**: External consumers/adapters depend on minimal core traits (`vyapaar-agent-core::ports`).

These rules are enforced at build time via `cargo check` and regression-tested via `tests/dependency_rules.rs` inspecting `cargo metadata`.

## 3. Sub-Crate Topology (14 Crates)

| Sub-Crate | Responsibility |
| :--- | :--- |
| `vyapaar-agent-core` | Shared AI traits (`LlmProvider`, `PromptBuilder`, `DecisionValidator`), audit envelope, storage ports (`SessionStore`, `MemoryStore`), and shared tool contracts (`ToolSpec`, `ToolOutput`). |
| `vyapaar-agent-prompt` | Versioned prompt templates, market/portfolio context serializers, token budgeters, chat base prompts, and memory section renderers. |
| `vyapaar-agent-replay` | Content-addressed SHA-256 prompt/completion cassette archive, deterministic offline replayer, and canonical `hash_request`. |
| `vyapaar-agent-llm` | Cloud & local LLM adapters (Anthropic Claude, OpenAI, Gemini, Ollama/vLLM, `ScriptedFakeLlm`), chat streaming interface (`LlmProvider::chat_stream`). |
| `vyapaar-agent` | Multi-step reasoning agent state machine and decision-surface memory context. |
| `vyapaar-agent-chat` **[NEW]** | Conversational assistant engine: streaming turn state machine, permissions & risk policies, actor-based session concurrency, and headless CLI runner. |
| `vyapaar-agent-trader-loop` | Event-driven AI trader loop connecting `LlmProvider` → `DecisionValidator` → `vyapaar_risk::RiskEngine`. |
| `vyapaar-agent-research` | Autonomous hypothesis generation, factor discovery, and walk-forward experiment loop. |
| `vyapaar-agent-analytics` | Performance attribution, Deflated Sharpe Ratio (DSR), regime classification, and drawdown analytics. |
| `vyapaar-agent-genetic` | StockSharp Odysseus-style genetic programming & rule-tree strategy synthesizer. |
| `vyapaar-agent-mcp` | Model Context Protocol (`rmcp`) server exposing market, portfolio, and backtest tools to external AI clients. |
| `vyapaar-agent-tools` | Typed, pure `AgentTool` registry for screener queries, regime checks, and risk inspection. Lifted into chat via `PureToolAdapter` (lift crate `vyapaar-agent-chat-tools` planned — gap 0002). |
| `vyapaar-agent-plugins` | Extensible plugin manifest (`PluginManifest`) and capability loader for custom research skills. |
| `vyapaar-agent-report` | Embedded Typst / Markdown quantitative research tearsheet & PDF report generator. |

*(Note: `vyapaar-agent-chat-tools` is the 15th crate, planned in Phase B — gap 0002.)*

## 4. Pure vs. Async Tooling Model
- **Pure `AgentTool` (`vyapaar-agent-tools`)**: Synchronous, side-effect-free, deterministic. Used by the trader loop and backtest replay. Replay determinism requires that tools in this registry cannot perform network or database I/O.
- **Async `ChatTool` (`vyapaar-agent-chat`)**: Asynchronous, I/O-capable, gated by `ToolRisk` levels (`ReadOnly`, `LocalWrite`, `External`, `Execute`) and `PermissionPolicy`. Used exclusively by the conversational assistant. Pure `AgentTool`s are lifted into `ChatTool`s via `PureToolAdapter` without duplicating tool schemas or execution logic.
