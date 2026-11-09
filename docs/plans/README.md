# Plans Index — `vyapaar-agent`

Per workspace rule `.agents/rules/gap-tracking.md`:
- Gaps are individual features (`docs/gaps/XXXX-*.md`).
- Plans are collections of gaps (`docs/plans/XXXX-*.md`).
- When implemented, plan documents are cleaned up from `docs/plans/` and recorded in this index.

## Active Plans

| # | Title | Target Scope | Status |
|---|-------|--------------|--------|
| [0002](0002-autonomous-jobs-memory-and-strategy-verification-engine.md) | Autonomous Job Scheduler, Local Memory Engine, and Strategy Verification Backend | `vyapaar-agent-jobs`, `vyapaar-agent-memory` | Ready for implementation |
| [0004](0004-free-tier-llm-backend-and-deferred-local-slm.md) | Free-Tier Multi-Provider LLM Backend (OmniRoute), Local SLM Deferred to Last | `vyapaar-agent-llm`, `vyapaar-agent-core` | Implemented through Phase 3 (commits `51b024d` + `236f8eb` + `5470e7b`); Phase 4 (local SLM) deferred |
| [0005](0005-local-slm-model-selection-and-reliability-reference.md) | Local SLM Model Selection, Licensing, Quantization, and Determinism Reference | `vyapaar-agent-llm` | Proposed reference material for Phase 4 |

## Implemented Plans

| # | Title | Target Scope | Implemented In |
|---|-------|--------------|----------------|
| 0001 | MCP Gateway Discovery, Universal Result Protocol, and 15 Quantitative Engines | `vyapaar-agent-core`, `vyapaar-agent-mcp` | `VyapaarResultEnvelope` in `vyapaar-agent-core/src/result.rs` (pre-existing); `DataReferenceStore` in `vyapaar-agent-core/src/ref_store.rs` with `export`/`resolve`; gateway discovery methods `vyapaar_search_tools`, `vyapaar_describe_tool`, `vyapaar_read_result` in `vyapaar-agent-mcp/src/server.rs`; all MCP tool results wrapped in `VyapaarResultEnvelope` (`vyapaar/result/v1`); 10 new tests (4 ref_store + 6 gateway). |
| 0003 | Remaining Fincept Analytics Engines Not Yet Scoped | `vyapaar-agent-forecast`, `vyapaar-agent-panel`, `vyapaar-agent-reports` | Implemented in commits `51b024d` + `236f8eb` (Phases 1–3: Theta forecasting, Fama-MacBeth 2-pass regression, deterministic financial report narratives) |
