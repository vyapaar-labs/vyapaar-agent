# `vyapaar-agent` — DDD Architecture & AI Agent Bounded Context

## 1. Bounded Context & Safety Guardrails
`vyapaar-agent` (Tier 3A, evolved from `honba-nou`) is the **AI Reasoning & Autonomous Research Bounded Context**. Per the NoFX Untrusted AI Principle:
* LLMs are treated strictly as untrusted signal/decision proposers.
* Every LLM completion is parsed into `vyapaar_core::Decision` (`#[serde(deny_unknown_fields)]`), grounded against real-time `PriceBand`s, and clamped by `vyapaar_risk::RiskEngine`.
* Every prompt, seed, temperature, and completion is SHA-256 hashed (`vyapaar-agent-replay`) for deterministic backtest replay.

## 2. Sub-Crate Topology (13 Crates)

| Sub-Crate | Responsibility |
| :--- | :--- |
| `vyapaar-agent-core` | Shared AI traits (`LlmProvider`, `PromptBuilder`, `DecisionValidator`), audit envelope, and grounding checks. |
| `vyapaar-agent-prompt` | Versioned prompt templates, market/portfolio context serializers, token budgeters. |
| `vyapaar-agent-replay` | Content-addressed SHA-256 prompt/completion cassette archive and deterministic offline replayer. |
| `vyapaar-agent-llm` | Cloud & local LLM adapters (Anthropic Claude, OpenAI, Gemini, Ollama/vLLM, `ScriptedFakeLlm`). |
| `vyapaar-agent` | Multi-step reasoning agent state machine and memory context. |
| `vyapaar-agent-trader-loop` | Event-driven AI trader loop connecting `LlmProvider` → `DecisionValidator` → `vyapaar_risk::RiskEngine`. |
| `vyapaar-agent-research` | Autonomous hypothesis generation, factor discovery, and walk-forward experiment loop. |
| `vyapaar-agent-analytics` | Performance attribution, Deflated Sharpe Ratio (DSR), regime classification, and drawdown analytics. |
| `vyapaar-agent-genetic` | StockSharp Odysseus-style genetic programming & rule-tree strategy synthesizer. |
| `vyapaar-agent-mcp` | Model Context Protocol (`rmcp`) server exposing market, portfolio, and backtest tools to external AI clients. |
| `vyapaar-agent-tools` **[NEW]** | Typed `AgentTool` registry for screener queries, regime checks, and risk inspection. |
| `vyapaar-agent-plugins` **[NEW]**| Extensible plugin manifest (`PluginManifest`) and capability loader for custom research skills. |
| `vyapaar-agent-report` **[NEW]** | Embedded Typst / Markdown quantitative research tearsheet & PDF report generator. |
