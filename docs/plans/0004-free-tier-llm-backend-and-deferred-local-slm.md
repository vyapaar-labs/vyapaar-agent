# 0004 — Free-Tier Multi-Provider LLM Backend (OmniRoute), Local SLM Deferred to Last

> **Status**: Implemented through Phase 3 — trait duplication resolved,
> `OpenAiCompatibleProvider` + env config + `FreeTierPolicy` guardrail,
> `TraderLoop::run()` wired through `DecisionParser`, all four workloads
> injected (chat example + research/report `llm` features), golden fixtures
> with risk-gate acceptance oracle. **Phase 4 (local SLM) remains deferred by
> design** (strict internal order; needs the Phase-3 fixture dataset and model
> licensing/fine-tuning per `0005`).
> **Target Crates**: `vyapaar-agent-llm`, `vyapaar-agent-trader-loop`, `vyapaar-agent-chat`, `vyapaar-agent-research`, `vyapaar-agent-report`.
> **Depends on reading**:
> - `vyapaar-agent/docs/plans/0005-local-slm-model-selection-and-reliability-reference.md` (the
>   detailed model licensing, fine-tuning, quantization, and determinism findings behind Phase 4
>   below — originally a deep-research report and its source notes, migrated into that doc and
>   removed from the repo as scratch material)

---

## 0. Executive Summary & Objective

`vyapaar-agent` has four LLM-dependent workloads (closed-grammar decision parser, tool-calling
chat assistant, autonomous research loop, report generation) and no real HTTP LLM provider
implementation anywhere in the workspace today, despite design docs describing one as already
built. This plan gets all four workloads working end-to-end now, at zero cost, by routing through
a local multi-provider free-tier router ("OmniRoute"), while deliberately deferring the
previously-researched locally-hosted fine-tuned SLM stack to a final phase — both so there is
something usable immediately, and so the free-tier runs produce the real fixtures that stack's
fine-tuning will eventually need.

---

## 1. Context

The deep-research behind this plan (now captured in
`vyapaar-agent/docs/plans/0005-local-slm-model-selection-and-reliability-reference.md`) recommends a
tiered locally-hosted fine-tuned SLM stack for `vyapaar-agent`'s four LLM workloads (closed-grammar
decision parser, tool-calling chat assistant, autonomous research loop, report generation). That
stack needs fine-tuning data, GPU time, and per-checkpoint quantization validation — none of which
exist yet.

The project owner wants the four workloads working end-to-end *now*, at zero cost, using free-tier
cloud AI through a multi-provider router ("omniroute kind"), with the local embedded SLM work
explicitly pushed to the final phase — both so there's something usable today, and so the
free-tier runs produce the real (prompt → decision) / (prompt → tool-call) fixtures that Phase 4's
fine-tuning will need.

Grounding (confirmed by direct reads + one Explore pass in the session that produced this plan,
not assumed):

- `vyapaar-agent-llm/src/lib.rs` defines the `LlmProvider` trait, `ChatRequest`/`ChatStreamEvent`/
  `LlmAuditRecord` types, and `ScriptedFakeLlm` (a test double). **There is no real HTTP provider
  implementation anywhere in the workspace** — despite `vyapaar-agent/docs/design/architecture.md`
  describing Anthropic/OpenAI/Gemini/Ollama adapters as already built. This is the actual gap to
  fill.
- There are **two separate `LlmProvider` trait definitions**: `vyapaar-agent-core::traits::LlmProvider`
  (`generate` only, no `chat_stream`, `vyapaar-agent-core/src/traits.rs:9-26`) and
  `vyapaar-agent-llm::LlmProvider` (`generate` + `chat_stream`, the fuller one, referenced by
  `ChatRequest.tools: Vec<ToolSpec>` at `vyapaar-agent-llm/src/lib.rs:268`). This duplication needs
  resolving, not perpetuating.
- `vyapaar-agent-trader-loop::TraderLoop::run()` (`vyapaar-agent-trader-loop/src/lib.rs:46-49`) is
  a **stub** — it never calls `generate`. The only caller of `generate` anywhere is a test mock
  (`tests/unit_tests.rs:16-33`), and even that test bypasses `generate` and hand-builds `Decision`
  values directly. Workload 1's decision loop needs real wiring, not just a backend swap.
- The real closed-grammar validation logic already exists as plain methods on `DecisionParser`
  (`vyapaar-agent-core/src/decision.rs`): `parse_str`, `validate_algo_id`, `validate_decision`,
  `parse_and_validate`, `evaluate_with_risk` (→ calls `risk_engine.check_decision`,
  decision.rs:252-261). There is no separate `DecisionValidator` type — `architecture.md`'s
  reference to one doesn't match the code. Reuse `DecisionParser` as-is.
- `vyapaar-agent-chat`'s turn loop is **already fully wired**: `turn.rs:241` calls
  `provider.chat_stream(&req)` on an `Arc<dyn LlmProvider>`; `ToolCallReady` events
  (turn.rs:290-295/303) feed a real `ToolRegistry`/`ChatTool` dispatch (`tool.rs:93-180`, call site
  `turn.rs:512-678`) that actually executes tools. Workload 2 needs nothing but a real provider
  instance — no new dispatch code.
- Workspace HTTP client convention: **`reqwest = "0.12"`, `default-features = false`,
  `features = ["json", "rustls-tls"]`**, used identically in `vyapaar-exec-zerodha/src/lib.rs:83`,
  `vyapaar-exec-upstox/src/lib.rs:76`, `vyapaar-exec-auth/src/login.rs:82,89`.
  `vyapaar-agent-mcp/Cargo.toml:29` already declares `reqwest.workspace = true` but nothing in its
  `src/` calls it yet — vestigial, not a precedent to build on.
- Config/secrets convention lives in `vyapaar-exec-auth/src/credentials.rs`, not
  `vyapaar-runtime-config` (that crate is backtest-run config only, no provider/API-key concept at
  all). The exec-auth pattern: env vars named `HONBA_{BROKER}_{FIELD}` (`credentials.rs:34-37`), a
  `BrokerCredentials` struct, a `CredentialStore` trait. Per dependency rules R2/R4
  (`vyapaar-agent/docs/design/architecture.md` §2), no `vyapaar-agent-*` crate may depend on
  `vyapaar-exec-*`, so this must be a **new, small, same-shaped** convention inside
  `vyapaar-agent-llm` itself, not a reuse of exec-auth's types.
- `~/tmp/omniroute-app/node_modules/omniroute` is a real, MIT-licensed npm package ("Unified AI
  router with 375 providers... auto fallback... OpenAI-compatible APIs", `homepage: omniroute.online`,
  `bin: omniroute`) — this is the "omniroute kind" the project owner means: a local process
  exposing an OpenAI-compatible endpoint that itself fans out across many providers (including free
  tiers) with fallback. It is a standalone tool the project owner runs (`npx omniroute`), not
  something to vendor into vyapaar-labs.

---

## 2. Component Specifications

### 2.1 `OpenAiCompatibleProvider` (new module `vyapaar-agent-llm/src/openai_compat.rs`)

```rust
pub struct OpenAiCompatibleProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
    http: reqwest::Client,
}
```

Uses the workspace's existing `reqwest` 0.12 / `rustls-tls` convention (feature-identical to
`vyapaar-exec-zerodha`, `vyapaar-exec-upstox`, `vyapaar-exec-auth`). Implements `LlmProvider`:

- `generate`: POST `{base_url}/chat/completions`, single user message = prompt, request JSON
  structured output (`response_format: {"type":"json_schema", ...}`) when the backend advertises
  support, otherwise plain JSON-mode prompting; parse the response text through
  `vyapaar_agent_core::decision::DecisionParser::parse_str` (reuse, don't reimplement).
- `chat_stream`: consume the OpenAI-compatible SSE stream, map `delta.content` →
  `ChatStreamEvent::TextDelta`, `delta.tool_calls` chunks → `ToolCallChunk`/`ToolCallReady`; map
  `ToolSpec { name, description, parameters }` directly into the OpenAI
  `{"type":"function","function":{name, description, parameters}}` tool shape — `parameters` is
  already a raw `serde_json::Value`, no translation needed.

### 2.2 `LlmAuditRecord.provider` field addition

Add a `provider: String` field to `LlmAuditRecord` (currently only has `model: String`) so the
replay hash captures *which* provider/endpoint actually served each call — required because a
multi-provider router can route the same logical request to a different underlying model each
time, which the research report's determinism findings say must be part of the audit trail.

### 2.3 Env-var config convention (new, inside `vyapaar-agent-llm`)

A small env-var loader matching the *shape* of `vyapaar-exec-auth/src/credentials.rs`'s
`HONBA_{BROKER}_{FIELD}` convention, without depending on that crate (R2/R4 compliance):

- `VYAPAAR_LLM_BASE_URL`
- `VYAPAAR_LLM_API_KEY`
- `VYAPAAR_LLM_MODEL`

---

## 3. Implementation Phases

### Phase 1 — Unify the provider contract, build the first real LLM provider

1. **Resolve the trait duplication.** Verify (quick grep) that nothing outside
   `vyapaar-agent-core` itself actually depends on `vyapaar-agent-core::traits::LlmProvider`; if
   so, either delete it or re-point it to re-export `vyapaar-agent-llm::LlmProvider` so there's one
   canonical trait (the `agent-llm` one, since it's the one `ChatRequest`/`ToolSpec` and the chat
   loop already depend on).
2. Add `vyapaar-agent-llm/src/openai_compat.rs`'s `OpenAiCompatibleProvider` (§2.1) and the
   `LlmAuditRecord.provider` field (§2.2).
3. Add the env-var config loader (§2.3).
4. **Wire `TraderLoop::run()`** (currently a stub) to: build a prompt via `vyapaar-agent-prompt`,
   call `self.llm.generate(...)`, pass the result through `DecisionParser::validate_decision` /
   `evaluate_with_risk` (not straight to `RiskEngine::check_decision` — the parser's validation
   step exists and isn't being called by this crate today; wire it in).

### Phase 2 — Point it at OmniRoute (the free-tier router)

1. Run OmniRoute locally (`npx omniroute`, the project owner's own tool/process — not vendored
   into this repo) configured to prioritize free-tier models across its provider list and reject
   paid ones by default.
2. Point `OpenAiCompatibleProvider` (Phase 1) at OmniRoute's local `base_url` — this is the only
   change needed to make free-tier routing the default backend; no new vyapaar-side code.
3. Inject this provider into the four workloads:
   - Trader-loop (wired in Phase 1).
   - `vyapaar-agent-chat` — identify the actual session-construction call site (bootstrap/CLI entry
     point) and inject the provider there; the turn loop and tool dispatch need no changes.
   - `vyapaar-agent-research` — plug into its existing optional `llm` feature
     (`vyapaar-agent-research/Cargo.toml`: `llm = ["dep:vyapaar-agent-llm"]`).
   - `vyapaar-agent-report` — same provider, for tearsheet narrative text.
4. Add an integration test per repo in the parallel `tests/` dir (per `AGENTS.md` TDD rule) that
   exercises each workload once against a live OmniRoute endpoint — `#[ignore]`d by default (needs
   network + a running local router), runnable manually to confirm end-to-end wiring.

### Phase 3 — Harden before touching any local model

1. Verify the `provider`-augmented `LlmAuditRecord` round-trips through `AuditRecorder`.
2. Capture a golden-fixture set from live OmniRoute runs: real (prompt → `Decision`) and
   (prompt → tool-call) pairs across the 4 workloads. Accept a fixture only after it passes
   `RiskEngine::check_decision` (for workload 1) — this is the "execute-and-verify" oracle pattern
   the research report recommends, and this project already owns the oracle. These fixtures double
   as Phase 4's fine-tuning/eval dataset.
3. Add a guardrail that logs/rejects if the configured endpoint ever reports a non-free-tier model,
   so "free-tier only" is enforced, not just configured.

### Phase 4 (LAST) — Tiered local embedded SLM

Only after Phases 1-3 are solid, introduce the local model tiers (licensing, fine-tuning,
quantization, and determinism detail in
`vyapaar-agent/docs/plans/0005-local-slm-model-selection-and-reliability-reference.md`), each as a
new `LlmProvider` impl behind the same trait (no interface changes needed):

1. Tier 1: Qwen3-4B/SmolLM3-3B, QLoRA-tuned on the Phase 3 fixtures, served via llama.cpp with GBNF
   grammar constraints — new `LlamaCppProvider`, swapped in for the trader-loop only.
2. Tier 2: Qwen3-8B/14B for `vyapaar-agent-chat`, served via vLLM with Hermes-style tool-call
   parsing.
3. Tier 3: largest-affordable model for `vyapaar-agent-research`.
4. Tier 4: reuse Tier 2/3 for `vyapaar-agent-report`.
5. Apply the determinism hardening (pin engine version/model file hash/thread config into the
   audit record) at this stage — it wasn't meaningful while routing through OmniRoute, which never
   claimed determinism, but matters once a single fixed local engine is the backend of record.

### Verification (per phase)

- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all` in
  `vyapaar-agent/` after each phase (per `vyapaar-agent/AGENTS.md` CI workflow).
- Phase 1: unit tests for `OpenAiCompatibleProvider` using a mocked HTTP server (e.g. `wiremock` or
  a local `axum` stub) — not live network — to verify request/response shape and `DecisionParser`
  integration.
- Phase 2: manual run — start OmniRoute locally, run each workload's `#[ignore]`d integration test
  by hand, confirm a real free-tier model produces a parseable `Decision` and at least one real
  tool-call round-trip through the chat assistant.
- Phase 3: confirm fixtures accumulate and that a deliberately-invalid `Decision` is rejected by
  `RiskEngine::check_decision` before being accepted into the fixture set.
- Phase 4: per-checkpoint structured-output/tool-calling reliability check against the Phase 3
  fixtures before any local model replaces the free-tier router for a given workload.

---

No implementation has been started under this doc; a prior attempt was deliberately reverted
mid-way per the project owner's direction to plan first.
