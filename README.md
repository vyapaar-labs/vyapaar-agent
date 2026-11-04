# व्यापार・代理 vyapaar-agent

> *AI Agent & Autonomous Research Engine for the Vyapaar Platform.*

Autonomous quantitative research, LLM decision reasoning, prompt replay cassettes, Model Context Protocol (MCP) server, and genetic strategy synthesis.

## Architecture & Subcrates

- `vyapaar-agent` — Multi-step reasoning agent state machine & facade
- `vyapaar-agent-core` — Shared AI traits (`LlmProvider`, `PromptBuilder`, `DecisionValidator`), grounding validators
- `vyapaar-agent-llm` — Cloud & local LLM client adapters (Anthropic Claude, OpenAI, Gemini, Ollama)
- `vyapaar-agent-prompt` — Versioned prompt templates & context serializers
- `vyapaar-agent-replay` — SHA-256 prompt/completion cassette store for deterministic backtest replay
- `vyapaar-agent-trader-loop` — Event-driven trader loop wiring LLM reasoning to `vyapaar-risk`
- `vyapaar-agent-research` — Autonomous hypothesis generation & walk-forward research loop
- `vyapaar-agent-analytics` — Reasoning attribution, Deflated Sharpe Ratio (DSR), and performance analytics
- `vyapaar-agent-genetic` — StockSharp Odysseus genetic strategy synthesizer & parameter mutator
- `vyapaar-agent-mcp` — Model Context Protocol (`rmcp`) server & tool endpoints
- `vyapaar-agent-tools` — Typed `AgentTool` registry for screener, depth, and portfolio queries
- `vyapaar-agent-plugins` — Extensible plugin manifest & capability discovery
- `vyapaar-agent-report` — Embedded Typst / Markdown tearsheet & research report compiler

## Invariants

- **Untrusted AI Principle**: LLMs are treated strictly as untrusted decision proposers; all completions are validated into `vyapaar_core::Decision` and gated by `vyapaar_risk::RiskEngine`.
- **Deterministic Replay**: Every prompt, completion, seed, and temperature is SHA-256 cassette hashed.
- **Strict Decimals**: Zero floating-point arithmetic across all calculations.

## Build & Test

```bash
make prod
make check
make test
```

License: AGPL-3.0.
