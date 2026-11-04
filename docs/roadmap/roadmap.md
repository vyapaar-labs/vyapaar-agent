# `vyapaar-agent` — TDD & Engineering Roadmap

## Milestone 1: Core AI Contracts, Prompting & SHA-256 Replay (P0)
- [ ] Port `vyapaar-agent-core`, `vyapaar-agent-prompt`, and `vyapaar-agent-replay` from `honba-nou`.
- [ ] Verify SHA-256 deterministic replay and `Decision` grounding tests.

## Milestone 2: LLM Providers, Agent, Trader Loop & Genetic Engine (P0)
- [ ] Port `vyapaar-agent-llm`, `vyapaar-agent`, `vyapaar-agent-trader-loop`, `vyapaar-agent-research`, `vyapaar-agent-analytics`, `vyapaar-agent-genetic` (from `honba-nou-genetic-engine`), and `vyapaar-agent-mcp`.

## Milestone 3: Agent Tools, Plugins & Typst Report Compiler (P1)
- [ ] Implement `vyapaar-agent-tools` (`AgentTool` trait + built-in market/portfolio/screener query tools).
- [ ] Implement `vyapaar-agent-plugins` (`PluginManifest`, `PluginRegistry` with deterministic validation).
- [ ] Implement `vyapaar-agent-report` (`ResearchReport`, Typst/Markdown tearsheet renderer with `Decimal` metrics).
