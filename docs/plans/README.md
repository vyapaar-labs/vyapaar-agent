# Plans Index — `vyapaar-agent`

Per workspace rule `.agents/rules/gap-tracking.md`:
- **Gaps** (`docs/gaps/XXXX-*.md`): individual missing features, stubs, or rule violations.
- **Plans** (`docs/plans/XXXX-*.md`): collections of gaps.
- When implemented, plan documents are cleaned up from `docs/plans/` and recorded in this index.
- Numbering is per-repo, sequential, never reused.

## Active Plans

| # | Title | Target Scope | Status |
|---|-------|--------------|--------|
| [0007](0007-comprehensive-mcp-ecosystem-and-agent-implementation-guide.md) | Comprehensive MCP Ecosystem Implementation Guide & Feature Coverage Audit | `vyapaar-agent-mcp`, `vyapaar-agent-chat-tools`, `vyapaar-agent-analytics`, `vyapaar-ledger`, `vyapaar-market`, `vyapaar-strategies`, `vyapaar-runtime`, `vyapaar-data` | Active — master guide (Longbridge 164 tools, Fincept 17 engines, jesse-mcp 32 tools); Phases 1–15 and 17 open; Phase 16 (SSE transport) shipped; Phase 17 = gap 0004 |
| [0008](0008-tdd-cleanup-inline-tests.md) | TDD Cleanup: Remove Inline Tests from `src/` | `vyapaar-agent-core`, `vyapaar-agent-research` | Open — tracks gap 0001 (two inline `#[cfg(test)]` blocks remain) |
| [0009](0009-persistent-memory-mcp-client-and-phase-e.md) | Persistent Memory, MCP Client Bridge & Phase E | `vyapaar-agent-core`, `vyapaar-agent-prompt`, `vyapaar-agent-mcp`, `vyapaar-agent-report`, `vyapaar-agent-research` | Open — tracks gaps 0003–0007 |

## Implemented Plans

| # | Title | Target Scope | Implemented In |
|---|-------|--------------|----------------|
| 0001 | MCP Gateway Discovery, Universal Result Protocol, and 15 Quantitative Engines | `vyapaar-agent-core`, `vyapaar-agent-mcp` | `VyapaarResultEnvelope` in `vyapaar-agent-core/src/result.rs`; `DataReferenceStore` in `vyapaar-agent-core/src/ref_store.rs` with `export`/`resolve`; gateway discovery methods `vyapaar_search_tools`, `vyapaar_describe_tool`, `vyapaar_read_result` in `vyapaar-agent-mcp/src/server.rs`; all MCP tool results wrapped in `VyapaarResultEnvelope` (`vyapaar/result/v1`); 10 new tests (4 ref_store + 6 gateway). |
| 0002 | Autonomous Job Scheduler, Local Memory Engine, and Strategy Verification Backend | `vyapaar-agent-core`, `vyapaar-agent-research`, `vyapaar-agent-chat` | Job scheduler (`vyapaar-agent-core/src/jobs/`), memory engine (`vyapaar-agent-core/src/memory/`), static strategy verifier (`vyapaar-agent-research/src/verify/`), session checkpoints (`vyapaar-agent-chat/src/session/checkpoint.rs`); commit `d27da44`. |
| 0003 | Remaining Fincept Analytics Engines Not Yet Scoped | `vyapaar-agent-forecast`, `vyapaar-agent-panel`, `vyapaar-agent-report` | Implemented in commits `51b024d` + `236f8eb` (Phases 1–3: Theta forecasting, Fama-MacBeth 2-pass regression, deterministic financial report narratives). |
| 0004 | Free-Tier Multi-Provider LLM Backend (OmniRoute), Local SLM Deferred to Last | `vyapaar-agent-llm`, `vyapaar-agent-trader-loop`, `vyapaar-agent-chat`, `vyapaar-agent-research`, `vyapaar-agent-report` | `OpenAiCompatibleProvider` (`vyapaar-agent-llm/src/openai_compat.rs`) with mocked-HTTP unit tests; `FreeTierPolicy` guardrail (Off/Warn/Enforce); `LlmAuditRecord.provider` field; env-var config (`VYAPAAR_LLM_*`); `TraderLoop::run()` wired through `DecisionParser` + `RiskEngine`; provider injected into all four workloads (chat example `omniroute_chat`, research `ResearchLlm`, report `narrate_report`, trader-loop); golden fixtures with risk-gate acceptance oracle; `#[ignore]`d per-repo OmniRoute integration tests; commit `b703a59`. Phase 4 (local SLM with `LlamaCppProvider`/`VllmProvider`) deferred by design per plan header. |
| 0005 | Local SLM Model Selection, Licensing, Quantization, and Determinism Reference | `vyapaar-agent-llm` | Reference material for Phase 4 of plan 0004; its actionable output (determinism fingerprint on the hash-chained `LlmAuditRecord` + `CandleGgufProvider::fingerprint()`) implemented in commit `b703a59`; model licensing (Apache-2.0/MIT preferred), Q4_K_M quantization floor, and engine-determinism findings captured in the plan document. |
| 0006 | MCP/chat tool catalog expansion guide (benchmarked against longbridge-mcp, Fincept, jesse-mcp) | `vyapaar-agent-chat-tools`, `vyapaar-runtime-api-rest` | Phases 1–4 implemented (2026-11-11/12). Phase 1 `BlockDealsTool` (runtime plan 0004 Phases 1–3, commit `a72689c`); Phase 2 `PriceAlertTool` CRUD (`AlertReader`/`AlertWriter` port, `price_alert_list`/`price_alert`, LocalWrite bundle outside the all-ReadOnly count pin, commits agent `a72689c` / runtime `d54f140`); Phase 3 `news_search`/`filings_search` via the `ContentReader` port (commit `98c66c2`, runtime `96e878f`, gaps `vyapaar-data` 0006/0007); Phase 4 `options_chain`/`corporate_actions` — research-verified first per §4 (`vyapaar-market` has building blocks, no chain reader; `vyapaar-data`'s corporate-actions reader exists Python-side — the plan's "no reader" assumption was stale), gaps filed before tool code (`vyapaar-market/docs/gaps/0001`, `vyapaar-data/docs/gaps/0008`+`0009`), then `OptionContract`/`OptionsChain`/`CorporateAction` DTOs + 2 `MarketReader` methods + 2 ReadOnly tools (catalogue 15 → 17 → 19, runtime `FeedMarketReader` stubs + seedable `RuntimeMarketReader` stores). Durable guidance retained from the doc: §2 flat-registry decision (revisit two-tier discovery at ~40 read-only tools) and §3's category-addition recipe (DTO+port method → tool struct → catalog registration → runtime wiring → count-test bump) — apply it for future categories under plan 0007. Full plan text in git history. |

## Roadmap Dismantling Note

The historical roadmap (`docs/roadmap/roadmap.md`, `docs/roadmap/migration-verification.md`)
has been dismantled:
- **Completed milestones** are now captured in `docs/design/` (architecture, migration map,
  and the `code-*.md` source references).
- **Remaining work** (Milestone 4 Phases B–E + cross-cutting cleanup) lives here as plans
  0008 and 0009, tracking gaps 0001 and 0003–0007.
