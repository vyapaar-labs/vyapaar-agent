# 0001 — Conversational Assistant & Superset Capabilities for `vyapaar-agent`

> **Status**: Proposed (design only — nothing in this document is implemented).
> **Owner repo**: `vyapaar-agent`. **Also touches**: `vyapaar-runtime` (recommendations only, §9 and §15.3), `vyapaar-ledger` and `vyapaar-core` (boundary statements only, no changes requested), `vyapaar-ui` (consumer of the API defined here).
> **Supersedes**: the open questions in `vyapaar-ui/docs/plans/0001-agent-chat-terminal-from-artha.md` §6, and the "`AgentTool` purity" caveat in §2/§3 of the same doc.
> **Authoritative context**: `vyapaar-agent/docs/design/architecture.md` (13-crate topology, NoFX principle), `vyapaar-agent/docs/migration-guide.md`, `vyapaar-runtime/docs/migration-guide.md`, `vyapaar-runtime/docs/design/architecture.md`, `docs/roadmap/roadmap.md`.

---

> ## ⛔ Implementation gate — read before writing any code
>
> **This design MUST NOT be implemented until both of the following roadmap milestones are complete and verified** (all boxes checked in `docs/roadmap/roadmap.md`, and each repo's `migration-guide.md` §3 verification commands passing on `main`):
>
> 1. **Milestone 3.1** — `vyapaar-agent` migrated from `honba-nou` (10 ported crates) plus the three originally-scoped new crates (`vyapaar-agent-tools`, `vyapaar-agent-plugins`, `vyapaar-agent-report`) in their **original, narrow** scope.
> 2. **Milestone 3.2** — `vyapaar-runtime` migrated from `honba-ya` (9 ported crates) plus `vyapaar-runtime-jobs` in its **original, narrow** scope.
>
> Everything below describes what to build **after** the ported foundation has landed and stabilised — not concurrently with it. The port is a moving target: trait signatures in `vyapaar-agent-core`, the `honba-nou-agent` state machine, `vyapaar-runtime-persistence`'s repository implementations and `vyapaar-runtime-jobs`' `JobSpec` will all be in flux during 3.1/3.2. Building on them mid-port guarantees merge conflicts and rework. Where this document names a ported type (`PromptBuilder`, `LlmProvider`, `JobSpec`, `OrderRepository`, …) it describes the **intended extension**, and §14 lists the facts that must be re-verified against the landed code before implementation starts.
>
> Milestone 1.3 (`vyapaar-ledger`) is a transitive prerequisite (3.2 depends on it) and is therefore implied.

---

## 0. Thesis

**`vyapaar-agent` must become a strict superset of `honba-nou`'s capabilities, not merely a renamed port of it.**

`honba-nou`, and therefore `vyapaar-agent` as Milestone 3.1 delivers it, is a **narrow, safety-constrained autonomous decision engine**: an LLM proposes a `vyapaar_core::Decision`, it is parsed with `#[serde(deny_unknown_fields)]`, grounded against live `PriceBand`s, clamped by `vyapaar_risk::RiskEngine`, and its prompt/seed/temperature/completion are SHA-256 cassette-hashed by `vyapaar-agent-replay`. Nothing else.

The product (README invariant 4: "`vyapaar ask`, `vyapaar tui`"; the `AgentChat` tab in `vyapaar-tui`) additionally needs a **general-purpose conversational finance assistant**: free-form Q&A, portfolio and watchlist reporting, market/macro lookups, research, durable memory of user preferences, monitors and background tasks, plugins, and generated reports.

The design problem is therefore not "port a chat app". It is: **add a second agent surface beside the decision engine, sharing its LLM, prompt, replay and analytics foundations, without ever giving the new surface a path to a trade that bypasses the existing `Decision → DecisionValidator → RiskEngine` pipeline.** Every decision in this document is evaluated against that sentence.

Concretely, after this design `vyapaar-agent` has two surfaces:

| Surface | Loop | Tools | Output | Determinism |
| :--- | :--- | :--- | :--- | :--- |
| **Decision surface** (ported, unchanged) | `vyapaar-agent` state machine + `vyapaar-agent-trader-loop` | pure, synchronous `AgentTool` (`vyapaar-agent-tools`) | `vyapaar_core::Decision` only | Fully replayable from cassettes |
| **Conversation surface** (new) | `vyapaar-agent-chat` session/turn loop | async, I/O-capable `ChatTool` (`vyapaar-agent-chat-tools`) + every `AgentTool`, lifted | Text, structured tool results, reports, monitors — **never a `Decision`, never an order** | Audited (transcript + request hashes), not replayed |

---

## 1. Summary of decisions

| # | Question | Decision |
| :--- | :--- | :--- |
| 1 | `AgentTool` purity | **Split.** `vyapaar-agent-tools` stays pure and synchronous (`AgentTool`). A **new** crate `vyapaar-agent-chat-tools` holds async I/O tools implementing a **new** `ChatTool` trait (defined in `vyapaar-agent-chat`). Pure `AgentTool`s are lifted into the chat registry through an adapter. Chat is strictly advisory: no `ChatTool` produces a `Decision` or reaches `vyapaar-exec`, enforced by the dependency graph. |
| 2 | Chat loop home | **New crate `vyapaar-agent-chat`**: session actor, turn state machine, `SessionEvent` stream, permission seam, background notices, session persistence through a port implemented by `vyapaar-runtime-persistence`. The base `vyapaar-agent` crate keeps the ported decision state machine untouched. |
| 3 | Memory | **Extend the base `vyapaar-agent` crate's memory context** with a long-term `UserMemory` module. Storage through a `MemoryStore` port in `vyapaar-agent-core::ports`, implemented in `vyapaar-runtime-persistence`. The stable-prefix / per-turn split is carried over by **extending `PromptBuilder`'s output** with a placement-typed `PromptAssembly` (no new contributor trait); memory I/O happens before prompt assembly, so `PromptBuilder` stays pure. |
| 4 | Portfolio/watchlist from chat | Chat tools call a **read-only `PortfolioReader` port owned by `vyapaar-agent-chat-tools`**, using `vyapaar-ledger` domain types. Its implementation is an adapter over `vyapaar-ledger`'s repository traits, living in `vyapaar-runtime-api` (Tier 3B → 3A, downward). No reverse dependency, no new data layer. |
| 5 | Monitors / background research | **Recommendation to `vyapaar-runtime`**: extend `JobSpec` with `JobSpec::Monitor(MonitorSpec)` and `JobSpec::AgentTask(AgentTaskSpec)`. Monitor evaluation is deterministic (no LLM). Chat reaches the scheduler through a `MonitorScheduler` port. Not decided here. |
| 6 | Plugins | **Broaden** `PluginCapability` to a superset (`ResearchSkill` + `ChatTool` + `PromptPack`). Plugin chat tools run **out of process as MCP stdio servers**. **No marketplace / remote distribution** in this design; local, manually installed, pinned-by-hash plugins only. |
| 7 | MCP direction | **Yes, add client capability**, as a `client` feature of the existing `vyapaar-agent-mcp` (bidirectional), not a new crate. It is the transport for plugin chat tools (Q6). Bridged tools are always `ToolRisk::External` or `ReadOnly` (hint-gated), never unattended-writable by default. Lands last (phase E). |
| 8 | Reports from chat | **Extend** `vyapaar-agent-report` with one additional, typed report kind — `ResearchNote` — whose numeric content is pulled by **reference to tool results** (`DataRef`), not retyped by the model. The model never writes raw Typst. Tearsheet-on-demand alone does not cover "summarise what we just discussed". |
| 9 | Checkpoint/undo of agent-edited files | **Out of scope.** No file-editing chat tool exists or is proposed. Nothing is built. §12 records the narrow conditions and home (strategy files only; plumbing in `vyapaar-runtime`'s existing git2 snapshot code) if that ever changes. |

New crates: **2** (`vyapaar-agent-chat`, `vyapaar-agent-chat-tools`), both in the `vyapaar-agent` repo, bringing it from 13 to 15. Justification in §16.2.

---

## 2. Constraints this design inherits (not re-litigated)

1. **NoFX Untrusted AI Principle** (`architecture.md` §1). Applies in full to every path that can produce a trade. This design adds no such path.
2. **Tier ordering.** `vyapaar-agent` is Tier 3A and "only depends downward on `vyapaar-core` and `vyapaar-risk`" (agent migration guide §2.1). `vyapaar-runtime` is Tier 3B and depends on `vyapaar-agent`. Therefore **no `vyapaar-agent-*` crate may depend on any `vyapaar-runtime-*` crate.** All I/O the conversation surface needs is expressed as **ports (traits) owned on the agent side and implemented on the runtime side.**
3. **Zero-I/O domain purity** of `vyapaar-ledger` and `vyapaar-risk` (README invariant 1; `vyapaar-ledger/Cargo.toml`: no tokio, no tracing, no async runtime, explicit `now`). Ledger types may be used by agent crates as plain data; ledger never gains I/O.
4. **Strict zero-floats.** Every price, quantity, money, P&L and percentage in tool inputs/outputs is `rust_decimal::Decimal`, serialised to the model as a JSON **string** (`"2843.15"`) so precision survives the wire.
5. **Tests in `tests/`**, zero `#[cfg(test)]` in `src/` (all migration guides).
6. **Persistence belongs to `vyapaar-runtime-persistence`** (SQLite and PostgreSQL via `sqlx`). No agent crate opens a database file.

---

## 3. Target topology

### 3.1 Crates after this design

```
vyapaar-agent (repo)                                           Tier 3A
├── vyapaar-agent-core        EXTENDED: ports module; PromptAssembly
├── vyapaar-agent-prompt      EXTENDED: chat base prompt, memory section renderers
├── vyapaar-agent-replay      EXTENDED: request-hash audit helper for chat (no replay)
├── vyapaar-agent-llm         unchanged contract; post-port gap check (§10)
├── vyapaar-agent             EXTENDED: memory::UserMemory beside ported MemoryContext
├── vyapaar-agent-trader-loop unchanged (the only Decision producer)
├── vyapaar-agent-research    unchanged; optionally callable as background AgentTask
├── vyapaar-agent-analytics   unchanged; consumed by tearsheet/report tools
├── vyapaar-agent-genetic     unchanged
├── vyapaar-agent-mcp         EXTENDED: `client` feature (bidirectional)
├── vyapaar-agent-tools       unchanged scope: pure AgentTool (stays pure)
├── vyapaar-agent-plugins     EXTENDED: superset PluginCapability
├── vyapaar-agent-report      EXTENDED: ResearchNote report kind
├── vyapaar-agent-chat        NEW: session/turn loop, ChatTool trait, events, permissions
└── vyapaar-agent-chat-tools  NEW: built-in ChatTools + their ports
```

### 3.2 Dependency graph (new and changed edges in bold)

```
           vyapaar-ui: vyapaar-cli (composition root) ─────┬──────────────┐
                         │                                 │              │
                         ▼                                 ▼              ▼
                    vyapaar-tui (AgentChat tab)   vyapaar-runtime-api   vyapaar-runtime-jobs
                         │                          (port adapters)       (Monitor/AgentTask)
                         │                                 │      │            │
                         ▼                                 ▼      │            │
              ┌── vyapaar-agent-chat ◀── vyapaar-agent-chat-tools │            │
              │     │   ▲        ▲               │    │           ▼            │
              │     │   │        └── vyapaar-agent-mcp (client)   vyapaar-runtime-persistence
              │     │   └── vyapaar-agent-plugins                 (impl SessionStore, MemoryStore,
              │     ▼                            │    │            WatchlistStore; ledger repos)
              │  vyapaar-agent (UserMemory) ◀────┘    │                    │
              │     │                                 ▼                    │
              │     ▼                        vyapaar-agent-tools (pure)    │
              └▶ vyapaar-agent-core (ports) ◀───────────────────────────────┘
                 vyapaar-agent-llm, -prompt, -replay, -report
                         │
                         ▼
          vyapaar-core, vyapaar-risk, **vyapaar-ledger (types only, chat-tools only)**
```

Dependency rules (enforced by a `cargo metadata`-based test in `vyapaar-agent/tests/` or `cargo-deny` `[bans]`, added with phase A):

- **R1.** No `vyapaar-agent-*` crate depends on any `vyapaar-runtime-*`, `vyapaar-exec-*`, or `vyapaar-market-*` crate. (Market data reaches chat tools through a port, §6.4.)
- **R2.** `vyapaar-agent-chat` and `vyapaar-agent-chat-tools` do not depend on `vyapaar-agent-trader-loop` and are not depended on by it. The two surfaces share foundations, not loops.
- **R3.** `vyapaar-agent-tools` has no `tokio`, no `async-trait`, no `reqwest`/`sqlx` — it stays pure (§4.1).
- **R4.** The only new downward edge out of Tier 3A is `vyapaar-agent-chat-tools → vyapaar-ledger` (pure, Tier 1B, no cycle possible; ledger depends only on `vyapaar-core`). This is an amendment to the agent migration guide's "core and risk only" sentence, scoped to one crate and to types.
- **R5.** `vyapaar-runtime-persistence` depends on `vyapaar-agent-core` only (for the `SessionStore`/`MemoryStore` ports), keeping the 3B→3A edge to the lightest agent crate.

---

## 4. Question 1 — `AgentTool` purity: split into pure `AgentTool` and async `ChatTool`

### 4.1 Decision

`vyapaar-agent-tools` is **kept pure**, exactly as its migration-guide row says ("Implement pure `AgentTool` trait, `ToolCall`, `ToolOutput`, `ToolRegistry`"). The broader, I/O-capable conversational tool set lives in the **new** crate `vyapaar-agent-chat-tools`, implementing a **new**, async `ChatTool` trait. `architecture.md`'s framing of `vyapaar-agent-tools` stays correct and does not need rewriting; it gains one sentence (§15.1) noting that its tools are also exposed to chat through an adapter.

The pure trait, as intended for Milestone 3.1 (shape to be confirmed against what 3.1 actually lands):

```rust
// vyapaar-agent-tools — pure, synchronous, deterministic.
pub trait AgentTool: Send + Sync + 'static {
    fn spec(&self) -> &ToolSpec;                          // name, description, JSON Schema
    /// Pure function of (call, snapshot). No I/O, no clock, no randomness.
    fn call(&self, call: &ToolCall, snapshot: &AgentSnapshot) -> Result<ToolOutput, ToolError>;
}

/// Everything a pure tool may read, fetched *before* the call by its host.
pub struct AgentSnapshot<'a> {
    pub as_of: DateTime<Utc>,                 // explicit time, ledger-style
    pub bands: &'a PriceBandTable,            // vyapaar-risk
    pub risk: &'a RiskStateView,              // OTR, kill-switch, freeze limits (read-only view)
    pub universe: &'a ScreenerUniverse,       // pre-fetched bars/indicators for screener queries
    pub regime: Option<&'a RegimeInputs>,     // inputs for vyapaar-agent-analytics regime classifier
    pub portfolio: Option<&'a PortfolioView>, // pre-fetched positions (README: "portfolio queries")
}
```

> Note: `architecture.md` lists the tool set as "screener queries, regime checks, risk inspection"; `vyapaar-agent/README.md` says "screener, depth, and portfolio queries". Both are consistent with purity as long as depth and portfolio arrive in the snapshot. Reconcile the two texts during 3.1; this design assumes the union.

The conversational trait:

```rust
// vyapaar-agent-chat — async, may perform I/O through injected ports.
#[async_trait::async_trait]
pub trait ChatTool: Send + Sync + 'static {
    fn spec(&self) -> &ToolSpec;                                   // same ToolSpec type as AgentTool
    fn traits(&self) -> ToolTraits { ToolTraits::default() }       // risk, concurrency, timeout, confirm
    /// Cheap, synchronous, no I/O. Validates input, writes the permission-prompt summary.
    fn plan(&self, input: &serde_json::Value, ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let _ = (input, ctx); Ok(ToolPlan::default())
    }
    /// Must be cancel-safe at every .await; must not block the reactor.
    async fn execute(&self, input: serde_json::Value, ctx: ToolContext) -> Result<ToolOutput, ToolError>;
}
```

`ToolSpec`, `ToolOutput`, `ToolError` are shared types defined once in `vyapaar-agent-core` (so both traits, the MCP server and the LLM adapters agree on one spec type). `ToolTraits`, `ToolPlan`, `ToolContext` are chat-only and live in `vyapaar-agent-chat`.

### 4.2 Why split rather than make `vyapaar-agent-tools` async

1. **Replay determinism is the decision surface's core property.** `vyapaar-agent-replay` reproduces a backtest by replaying cassettes. If a decision-surface tool can hit a database or HTTP endpoint, the replayed run's tool outputs depend on the world at replay time and the cassette hash no longer pins the decision. A pure `AgentTool` over an `AgentSnapshot` is replayable for free: the snapshot is the recorded input, the tool is a function. Making the trait async and I/O-capable would silently void that guarantee for every future tool author.
2. **Different risk models.** Pure tools have one risk level (none). Chat tools need a permission policy (`ToolRisk`, user prompts, session grants, unattended allow-lists). Putting both behind one trait forces either meaningless permission metadata on pure tools or missing metadata on I/O tools.
3. **Different dependency fan-out.** Chat tools need ports to ledger reads, market reads, watchlists, memory, the job scheduler, and the report engine. The trader loop needs none of that; pulling them into `vyapaar-agent-tools` bloats the decision surface's compile graph and attack surface.
4. **The superset still holds.** Every pure `AgentTool` is usable from chat via one adapter (§4.4), so the chat surface strictly contains the decision surface's tool vocabulary.

### 4.3 NoFX: is chat ever a `Decision` source? No.

The deciding question is whether any chat-facing tool needs to produce something that flows into a `Decision`. **It does not, and this design forbids it.**

- **Chat is strictly advisory/informational.** Its outputs are text, structured tool results, saved memories, watchlist edits, monitor definitions, and report files. None is an order or a `Decision`.
- **There is no `ToolRisk::Trade`.** The `ToolRisk` enum (§5.5) deliberately has no variant for order placement; the permission system cannot be configured into allowing one.
- **Structural enforcement (R1).** `vyapaar-agent-chat-tools` cannot name `vyapaar-exec` or `vyapaar-runtime-actors` types. A chat tool literally cannot submit an order without a dependency edge that the R1 test rejects.
- **If a "trade from chat" feature is ever wanted**, it is not a new code path. It must be a `propose_decision` tool whose only effect is to hand a candidate to the **existing** decision pipeline (trader-loop's `DecisionValidator` → `PriceBand` grounding → `RiskEngine`, with the proposal and its prompt cassette-hashed), plus explicit human confirmation. That would be its own design doc; this one only reserves the rule: **one choke point for trades, regardless of which agent surface triggered them.**
- **Monitors' unattended actions** (§9) inherit the same rule: a monitor can notify or run a read-only headless chat turn, never trade.

### 4.4 Lifting pure tools into chat: `PureToolAdapter`

```rust
// vyapaar-agent-chat-tools
pub struct PureToolAdapter<T: AgentTool> {
    inner: Arc<T>,
    snapshots: Arc<dyn SnapshotSource>,   // port: builds an AgentSnapshot from live state
}

#[async_trait::async_trait]
impl<T: AgentTool> ChatTool for PureToolAdapter<T> {
    fn spec(&self) -> &ToolSpec { self.inner.spec() }
    fn traits(&self) -> ToolTraits { ToolTraits::new(ToolRisk::ReadOnly) } // eager-start eligible
    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let call = ToolCall::from_json(self.inner.spec(), input)?;
        let snap = self.snapshots.snapshot(call.snapshot_needs(), &ctx.cancel).await?; // I/O here
        let tool = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || tool.call(&call, &snap.as_view()))       // pure here
            .await.map_err(ToolError::from_join)?
    }
}
```

The I/O happens in the adapter, outside the pure tool; the tool itself is identical to what the trader loop calls. `SnapshotSource` is a port in `vyapaar-agent-chat-tools::ports`, implemented in `vyapaar-runtime-api`.

---

## 5. Question 2 — Where the conversation loop lives: new crate `vyapaar-agent-chat`

### 5.1 Decision

A **new crate, `vyapaar-agent-chat`**, owns the free-form conversation loop. The base `vyapaar-agent` crate keeps the ported `honba-nou-agent` multi-step reasoning state machine **unchanged** and gains only the memory extension of §7.

### 5.2 Why not extend the base `vyapaar-agent` crate

- **Different state machines with different invariants.** The ported machine is a bounded reasoning loop whose terminal output is a `Decision` and whose every step is replay-hashed. The chat machine is open-ended, streams deltas to a UI, pauses for permission prompts, survives process restarts through session persistence, and terminates with a `TurnOutcome`, never a `Decision`. Merging them makes it possible for a future change to route a chat turn's output into the decision path by accident — the exact failure NoFX exists to prevent.
- **Port stability.** The gate (top of document) requires the ported base crate to be stable before extension. Keeping chat in a separate crate means the 3.1 port remains a diffable, straight port that can be compared line-by-line against `honba-nou-agent`.
- **Direction of reuse.** Chat reuses the base crate's memory and the shared `vyapaar-agent-core`/`-llm`/`-prompt` foundations (chat → agent). The base crate never needs chat.

`vyapaar-agent-chat` owns the `ChatTool` contract (the crate that *calls* tools owns their interface), so `vyapaar-agent-chat-tools`, `vyapaar-agent-mcp` (client bridge) and `vyapaar-agent-plugins` all plug into one seam.

### 5.3 Public API (what `vyapaar-tui`'s `AgentChat` tab and `vyapaar-cli ask` consume)

| Item | Kind | Used by |
| :--- | :--- | :--- |
| `ChatAgent`, `ChatAgentBuilder` | shared engine: `Arc<dyn LlmProvider>`, `ToolRegistry`, `Arc<dyn SessionStore>`, `UserMemory`, `ChatConfig`; cheap `Clone` | `vyapaar-cli`, `vyapaar-runtime-jobs` (via adapter) |
| `SessionSelector` (`New`, `Continue`, `Resume(SessionId)`) | open a session | `vyapaar-cli` |
| `SessionHandle` | `submit`, `cancel`, `retry`, `notify`, `rename`, `set_provider`, `history`, `list_background`, `cancel_background`, `close` | `vyapaar-tui`, `ask`, jobs |
| `SessionEvents: Stream<Item = SessionEvent>` | single ordered outward stream | `vyapaar-tui`, `ask --json` |
| `PermissionResponder`, `PermissionDecision` | answer a prompt | UIs |
| `run_turn_headless(agent, selector, input, policy)` | open, submit, drain, close | `ask`, jobs `AgentTask` |
| `ChatTool`, `ToolTraits`, `ToolRisk`, `ToolPlan`, `ToolContext`, `ToolRegistry`, `ToolSet`, `TypedChatTool` | tool contract | `-chat-tools`, `-mcp`, `-plugins` |
| `ChatError` | `thiserror`, `#[non_exhaustive]` | all of the above |
| `testing::{FakeChatTool, ScriptedSession, EventLog}` (feature `test-util`) | test harness over `ScriptedFakeLlm` | own tests, `vyapaar-tui` tests |

The event stream type is named **`SessionEvent`** (not `ChatEvent`) to avoid colliding with whatever streaming-event type `vyapaar-agent-llm` exposes after the port.

### 5.4 Turn state machine

Vocabulary: a **turn** begins with one user submission and ends when the model stops without requesting tools (or on cancel/failure/limit); a **round** is one `LlmProvider` call plus the dispatch of the tools it requested; a **call** is one tool-use block, keyed by the model's tool-use id.

```
submit(user) ─▶ persist user msg ─▶ fold turn context (§7.4) ─▶ snapshot ToolSet
   ┌────────────────────────────────────────────────────────────────────┐
   ▼                                                                    │
 Requesting ──chat()──▶ Streaming ──MessageEnd──▶ Committing            │
   │ context-length: compact once, retry        │ persist assistant msg │
   │                                            ▼                       │
   │                  stop = ToolUse ───────▶ Dispatching ──────────────┤
   │                  PauseTurn ────────────────────────────────────────┘
   │                  EndTurn / StopSequence ─▶ Finished(Completed)
   │                  MaxTokens (no calls) ───▶ Finished(MaxTokens)
   │                  Refusal / ContentFilter ▶ Finished(Refused)
   └─ any state ──cancel──▶ Cancelling ─▶ repair + persist ─▶ Finished(Cancelled)
```

```rust
pub(crate) enum TurnState {
    Preparing,
    Requesting  { round: u32, attempt: u32, compacted: bool },
    Streaming   { round: u32, attempt: u32, stream: llm::EventStream, acc: RoundConsumer },
    Committing  { round: u32, message: llm::Message, stop: llm::StopReason, usage: llm::Usage, eager: EagerSet },
    Dispatching { round: u32, seq: Option<MessageSeq>, calls: Vec<llm::ToolCall>, eager: EagerSet },
    Cancelling  { at: CancelPoint },
    Finished(TurnOutcome),
}
pub(crate) enum CancelPoint {
    BeforeRequest,
    Streaming   { partial_text: String },
    Dispatching { seq: Option<MessageSeq>, calls: Vec<llm::ToolCall>, done: Vec<(CallId, llm::ToolResult)> },
}
#[non_exhaustive]
pub enum TurnOutcome { Completed, MaxTokens, Refused { reason: RefusalKind }, RoundLimit { limit: u32 },
                       Stopped, Cancelled, Failed { error: ChatErrorView } }
```

The driver is `loop { state = match state { … } }`; each long `.await` is wrapped in `select! { biased; _ = cancel.cancelled() => Cancelling{..}, r = fut => … }`. Key policies:

- **Write-ahead persistence at message granularity.** User message on submit; assistant message at `MessageEnd` *before* any tool runs; one tool-results message after all calls of the round finish. A crash loses at most the in-flight round.
- **Repair on load.** An assistant message whose tool-use ids have no results in the next message (crash or cancel mid-dispatch) gets a persisted, `synthetic`-flagged results message ("not executed: the process exited before this call completed"). A message that fails to decode is a hard `ChatError::CorruptHistory { seq }`, never silently skipped. A property test asserts every reachable crash point yields a history that passes tool-use/tool-result pairing validation.
- **Stop-reason policy.** `PauseTurn` re-requests and counts as a round. `MaxTokens` with tool calls dispatches them (truncated calls come back as `is_error` results so the model can retry with shorter arguments); without calls it ends the turn so the UI can offer "continue".
- **Bounded autonomy.** `max_rounds` (default 24); on the limit, the turn ends with `RoundLimit` after persisting the last results, so the user can say "keep going".
- **Stable request prefix.** System prompt and `ToolSet` are fixed for the whole turn (and the system prompt for the whole session, §7.4), so provider prompt-cache breakpoints keep hitting across tool round trips. Tools registered mid-turn (an MCP server finishing its handshake) appear next turn.
- **Retries.** `vyapaar-agent-llm` retries before the first byte. Mid-stream retryable errors are retried by the chat loop up to `stream_retries` (2) after emitting `RoundRetrying { discard_partial: true }`. `ContextLengthExceeded` is compacted once (old tool-result bodies elided in the *request view only*, never in storage).

### 5.5 Tool dispatch and permissions

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolRisk {
    ReadOnly,    // quotes, positions, screener, memory_search, risk inspection
    LocalWrite,  // platform-owned state: memory, watchlists, monitors, report files in the reports dir
    Execute,     // CPU-heavy work in the runtime: backtests, research jobs
    External,    // anything crossing a trust boundary: MCP/plugin tools
    // Deliberately absent: Trade (§4.3) and FileWrite (§12).
}
#[non_exhaustive]
pub struct ToolTraits { pub risk: ToolRisk, pub concurrency_safe: bool, pub timeout: Option<Duration>, pub confirm: ConfirmOverride }
impl ToolTraits { pub fn new(risk: ToolRisk) -> Self; pub fn serial(self) -> Self; pub fn timeout(self, d: Option<Duration>) -> Self; pub fn confirm(self, c: ConfirmOverride) -> Self; }
```

(`ToolTraits` is `#[non_exhaustive]`, so the builder methods are part of the contract — sibling crates cannot use struct-update syntax on it.)

- **Policy in `Ask` mode:** `ReadOnly` runs without a prompt; everything else prompts unless the tool sets `ConfirmOverride::Never` (e.g. `memory_save`, `watchlist_edit`). `ConfirmOverride::Always` forces a prompt even for read-only tools.
- **Prompts are events.** `SessionEvent::PermissionRequested(PermissionPrompt { tool, risk, summary, input, responder })`; the UI calls `responder.respond(AllowOnce | AllowForSession | Deny { feedback } | DenyAndStop)`. A dropped responder, or a cancel while a prompt is open, resolves to `Deny`. Prompts are asked one at a time, in call order.
- **Denied calls** still produce an `is_error` tool result carrying the user's feedback, and the loop continues so the model can adapt.
- **Unattended** (`PermissionPolicy::NonInteractive { allow }`, used by jobs): no prompts; only listed tools may write; default allow list = the `ReadOnly` set, exported by `vyapaar_agent_chat_tools::names::read_only()`.
- **Isolation.** Every `execute` runs in its own spawned task (panic → `is_error` result), under `tokio::time::timeout(traits.timeout)`, with a child `CancellationToken` of the turn. Auto-allowed, read-only, concurrency-safe calls run in parallel (≤ 4) and may **start eagerly** as soon as their arguments are complete while the model is still streaming; results are discarded if the round is retried.
- **Result serialisation.** Results are capped at 64 KiB per call (UTF-8-safe truncation with an explicit `[truncated: N bytes omitted]` note); structured data is returned as JSON with `Decimal`s as strings and every market datum carrying `as_of` and `source`. Rich `ToolDisplay` (tables) goes only to the UI via `ToolFinished`, never to the model.
- **Repeat-error nudge.** The same tool failing identically 3 times in a turn gets "This has failed repeatedly; stop retrying and tell the user" appended.

### 5.6 `SessionEvent`

One ordered, owned, `serde`-serialisable stream per session over a bounded `tokio::sync::mpsc` (1024). Bounded on purpose: a stalled UI backpressures into the LLM stream rather than buffering without limit. Not a broadcast channel (it drops text for slow receivers); not a per-turn stream (cannot carry between-turn notices).

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SessionEvent {
    // session
    SessionOpened { session: Option<SessionId>, resumed: bool, history: usize, profile: SessionScope, model: Option<String> },
    SessionPersisted { session: SessionId },
    TitleSet { title: String },
    ProviderChanged { provider: Option<String>, model: Option<String> },
    Notice(Notice),                          // Info | Warning | BackgroundFinished | BackgroundFailed | MonitorFired
    // turn lifecycle
    TurnStarted { turn: TurnId, queued_behind: usize },
    RoundStarted { turn: TurnId, round: u32 },
    RoundRetrying { turn: TurnId, round: u32, attempt: u32, reason: String, discard_partial: bool },
    TurnEnded { turn: TurnId, outcome: TurnOutcome, usage: llm::Usage, rounds: u32, persisted: bool },
    // model output
    TextDelta { turn: TurnId, round: u32, text: String },
    ThinkingDelta { turn: TurnId, round: u32, text: String },
    ToolCallStreaming { turn: TurnId, round: u32, call_id: CallId, name: String },
    ToolCallReady { turn: TurnId, round: u32, call: ToolCallView },
    AssistantCommitted { turn: TurnId, round: u32, seq: Option<MessageSeq>, stop_reason: llm::StopReason },
    Usage { turn: TurnId, round: u32, usage: llm::Usage },
    // tools
    PermissionRequested(PermissionPrompt),   // responder is #[serde(skip)]
    ToolStarted { turn: TurnId, call_id: CallId, name: String, summary: String, eager: bool },
    ToolProgress { turn: TurnId, call_id: CallId, message: String, fraction: Option<Decimal> },
    ToolFinished { turn: TurnId, call_id: CallId, name: String, outcome: ToolOutcome, elapsed_ms: u64, display: Option<ToolDisplay> },
}
```

Every event carries its `TurnId` so UIs can ignore stale events after a cancel. `vyapaar ask --json` prints one event per line. (Note `fraction: Option<Decimal>` — zero-floats holds even for progress bars.) There is no `CheckpointRecorded` variant (§12).

### 5.7 Session model and persistence

One `SessionActor` task per open session owns the `Session` (history as `Vec<Message>` plus parallel `Vec<EntryMeta>`, in-memory permission grants, pending notices, usage totals). `SessionHandle` is a cheap `Clone` command sender; a `Submit` arriving mid-turn is queued FIFO. No lock around history — exactly one task mutates it.

Sessions are scoped by a **`SessionScope { profile: String, mode: RuntimeMode }`** (`RuntimeMode` = `Live | Paper | Backtest` from `vyapaar-runtime-config`, mirrored as a plain enum in `vyapaar-agent-core` to respect R1), not by working directory: a trading workstation's natural partition is "which account profile and which mode", and `--continue` means "the most recent session for this profile and mode".

Persistence goes through a port, implemented by `vyapaar-runtime-persistence` (R5):

```rust
// vyapaar-agent-core::ports
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync + 'static {
    async fn create(&self, s: NewSession) -> Result<SessionId, StoreError>;          // lazy: on first submit
    async fn append(&self, id: SessionId, msgs: &[PersistedMessage]) -> Result<Vec<MessageSeq>, StoreError>;
    async fn messages(&self, id: SessionId) -> Result<Vec<(MessageSeq, PersistedMessage)>, StoreError>;
    async fn most_recent(&self, scope: &SessionScope) -> Result<Option<SessionInfo>, StoreError>;
    async fn list(&self, filter: SessionFilter) -> Result<Vec<SessionInfo>, StoreError>;
    async fn rename(&self, id: SessionId, title: &str) -> Result<(), StoreError>;
    async fn update_provider(&self, id: SessionId, provider: &str, model: &str) -> Result<(), StoreError>;
    async fn truncate_after(&self, id: SessionId, seq: MessageSeq) -> Result<(), StoreError>;
}
pub struct StoreError { pub kind: StoreErrorKind /* Busy | NotFound | Conflict | Corrupt | Unavailable */, pub message: String }
```

The trait is async because `vyapaar-runtime-persistence` is `sqlx`-based (async) — this differs from a synchronous store, and removes any need for `spawn_blocking` in the chat loop. Recommended schema (SQLite DDL; the PostgreSQL migration uses `JSONB`/`TIMESTAMPTZ` equivalents):

```sql
CREATE TABLE agent_chat_session (
    id          TEXT PRIMARY KEY NOT NULL,          -- UUIDv7
    profile     TEXT NOT NULL,
    mode        TEXT NOT NULL CHECK (mode IN ('live','paper','backtest')),
    title       TEXT,
    provider    TEXT NOT NULL,
    model       TEXT NOT NULL,
    archived    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX agent_chat_session_recent ON agent_chat_session (profile, mode, archived, updated_at DESC);

CREATE TABLE agent_chat_message (
    session_id    TEXT    NOT NULL REFERENCES agent_chat_session(id) ON DELETE CASCADE,
    seq           INTEGER NOT NULL,
    role          TEXT    NOT NULL CHECK (role IN ('user','assistant','tool')),
    content       TEXT    NOT NULL CHECK (json_valid(content)),  -- provider-neutral message + "vyapaar" meta key
    request_hash  TEXT,                                          -- §5.8, assistant rows only
    input_tokens  INTEGER, output_tokens INTEGER,
    created_at    INTEGER NOT NULL,
    PRIMARY KEY (session_id, seq)
) STRICT;
```

`UNIQUE(session_id, seq)` (the primary key) makes two processes appending to one session safe; a session lease is deferred.

### 5.8 Audit without replay

NoFX requires every *decision* prompt to be cassette-hashed for replay. Chat turns are not decisions and their tools are I/O-bound, so deterministic replay of a chat session is neither possible nor needed. For audit, each assistant row stores `request_hash` = SHA-256 over (system prompt, tool specs, message history, generation options), computed with `vyapaar-agent-replay`'s existing canonical hasher (exposed as a small `hash_request` helper — the only change to that crate). This gives "exactly what was sent" provenance without pretending chat is replayable.

---

## 6. Question 4 — Portfolio, watchlist and market reads from chat

### 6.1 The reverse-dependency problem

`vyapaar-runtime-persistence` implements `vyapaar-ledger`'s repository traits (`OrderRepository`, `AccountRepository`, `PositionRepository`, `LedgerRepository`, per `vyapaar-runtime/docs/design/architecture.md`). A chat tool calling it directly would be a Tier 3A → 3B edge, while 3B already depends on 3A: a cycle.

### 6.2 Decision: consumer-owned read port, ledger-owned types, runtime-side adapter

- **Trait definition owner: `vyapaar-agent-chat-tools`** (the caller owns the port). It is **read-only** and narrower than the ledger repositories.
- **Data types owner: `vyapaar-ledger`** (`Position`, tax lots, `Money`, etc.) and `vyapaar-core`. The port uses them directly — no parallel DTO copies. This is the one new downward edge, R4.
- **Implementation owner: `vyapaar-runtime-api`** (application layer, Tier 3B). It implements the port generically over the ledger repository traits that `vyapaar-runtime-persistence` already implements. Edge 3B → 3A, downward, no cycle.
- **Wiring: `vyapaar-cli`** (composition root) constructs the adapter and injects it as `Arc<dyn PortfolioReader>`.

```rust
// vyapaar-agent-chat-tools::ports
#[async_trait::async_trait]
pub trait PortfolioReader: Send + Sync + 'static {
    async fn accounts(&self) -> Result<Vec<AccountSummary>, PortError>;
    async fn positions(&self, account: &AccountId) -> Result<Vec<vyapaar_ledger_portfolio::Position>, PortError>;
    async fn tax_lots(&self, account: &AccountId, instrument: &InstrumentId) -> Result<Vec<TaxLot>, PortError>;
    async fn cash(&self, account: &AccountId) -> Result<Vec<Money>, PortError>;
    async fn orders(&self, account: &AccountId, filter: OrderFilter) -> Result<Vec<OrderView>, PortError>; // read-only view
    async fn realized_pnl(&self, account: &AccountId, range: DateRange) -> Result<Money, PortError>;
}

// vyapaar-runtime-api (recommendation, §15.3)
pub struct LedgerPortfolioReader<R> { repos: R, now: Arc<dyn Clock> }
impl<R> PortfolioReader for LedgerPortfolioReader<R>
where R: AccountRepository + PositionRepository + OrderRepository + LedgerRepository + Send + Sync + 'static { … }
```

Why a new narrow port rather than handing chat tools the ledger repository traits themselves:

1. **Interface segregation enforces read-only at compile time.** The ledger repositories include write methods used by the OMS. A chat tool holding `Arc<dyn PortfolioReader>` cannot mutate the ledger, by construction — a second structural NoFX guard.
2. **Async shape.** `vyapaar-ledger` has no async runtime; whether its repository traits are synchronous (with persistence bridging) or `async-trait` is a Milestone 1.3/3.2 detail. The port is async regardless, and the adapter absorbs whatever shape lands.
3. **Paper vs live.** The adapter is constructed per `RuntimeMode`, so a paper-mode chat session reads the paper ledger. The tool never decides which ledger it reads.

### 6.3 Watchlists

Watchlists are user state, not ledger state (ledger stays an accounting domain). Port `WatchlistStore` in `vyapaar-agent-chat-tools::ports` (`list`, `get`, `add`, `remove`, `create`), implemented by `vyapaar-runtime-persistence` behind a small adapter in `vyapaar-runtime-api` (to keep R5: persistence itself depends only on `vyapaar-agent-core`; the adapter that names the chat-tools trait sits in runtime-api). Recommended tables `agent_watchlist (id, name UNIQUE, created_at)` and `agent_watchlist_item (watchlist_id, instrument_id, added_at, PRIMARY KEY(watchlist_id, instrument_id))`. `add` is idempotent; the `default` watchlist is created lazily.

### 6.4 Market and macro reads

Port `MarketReader` in `vyapaar-agent-chat-tools::ports` (`quote`, `history` with bar caps, `indicators`, `search_instruments`, `macro_series`, `price_band`), using `vyapaar-core` types (`InstrumentId`, `Tick`, `Price`). Implemented in `vyapaar-runtime-api` over `vyapaar-market` (which runtime already depends on). R1 keeps `vyapaar-market` out of the agent repo; the trader loop's own market inputs (pushed by runtime actors) are untouched.

All market tools are registered whether or not the feed is available (so the tool list and the prompt-cache prefix don't change with feed state); an unavailable feed is a fast, model-readable `ToolError::Failed { retryable: false }`. Caching and rate limiting belong to the adapter, which sees every caller — not to tools.

### 6.5 Built-in chat tool catalogue (`vyapaar-agent-chat-tools`)

Model-facing names are `<category>_<verb>`, stable API (an `insta` snapshot pins every `ToolSpec`). One risk level per tool — read and write actions never share a name, so a user can grant "always allow `watchlist_edit`" without granting anything else. Every input struct derives `Deserialize + JsonSchema` with `#[serde(deny_unknown_fields)]`.

| Tool | Port | Risk | Confirm | Notes |
| :--- | :--- | :--- | :--- | :--- |
| `market_quote`, `market_history`, `market_indicators`, `market_search` | `MarketReader` | ReadOnly | default | NSE/BSE instruments; bars capped at 500, downsampled with a `"downsampled"` marker |
| `macro_series` | `MarketReader` | ReadOnly | default | RBI/CPI/IIP series from `vyapaar-market`'s finance series |
| `screener_scan`, `regime_check`, `risk_inspect` | `PureToolAdapter` + `SnapshotSource` | ReadOnly | default | the lifted `vyapaar-agent-tools` set; `risk_inspect` shows price bands, freeze limits, OTR status, kill-switch — read-only |
| `portfolio_view`, `positions_view`, `orders_view`, `taxlots_view`, `pnl_view` | `PortfolioReader` (+ `MarketReader` for valuation) | ReadOnly | default | valuation degrades gracefully: positions/cost basis still returned with `valuation: "unavailable"` if quotes fail |
| `watchlist_show` / `watchlist_edit` | `WatchlistStore` | ReadOnly / LocalWrite | default / Never | |
| `memory_search` / `memory_save` / `memory_forget` | `UserMemory` (§7) | ReadOnly / LocalWrite / LocalWrite | default / Never / default | |
| `monitor_list` / `monitor_create` / `monitor_manage` | `MonitorScheduler` (§9) | ReadOnly / LocalWrite / LocalWrite | default | |
| `backtest_run` | `BacktestRunner` → `JobSpec::AgentTask` or runtime backtester | Execute | default | returns a background ticket immediately (§6.6) |
| `tearsheet_render`, `report_render` | `vyapaar-agent-report` + `ReportSink` | LocalWrite | default | §11 |
| `research_calc` | none (pure Rust, `Decimal`) | ReadOnly | default | CAGR, XIRR, position sizing, Indian statutory cost estimate — so "run the numbers" never depends on model arithmetic |

Explicitly **not** in the catalogue: any order placement, paper trade, or broker action (§4.3); any file editing (§12); generic `web_fetch` (deferred — SSRF surface on a machine holding broker credentials; can arrive later as a plugin via MCP with `External` risk).

### 6.6 Long-running work

A tool that starts something slow (`backtest_run`, a research job) returns promptly with a `BackgroundTicket`. In-process, session-scoped work uses `ctx.background().spawn(label, fut)`; durable work goes through the job scheduler (§9). Completion surfaces as `SessionEvent::Notice(BackgroundFinished)` immediately and is folded into the **next** user message as a short `[Background task …]` block (summary capped at 4 KiB) — the same folding path as per-turn memory recall (§7.4). An opt-in `BackgroundFollowup::AutoTurn` starts a turn on completion instead.

---

## 7. Question 3 — Memory

### 7.1 Decision

- **Home: extend the base `vyapaar-agent` crate.** Its existing "memory context" (ported working memory for the reasoning state machine) gains a sibling module, `memory::user::UserMemory`, for durable, cross-session user memory (facts, preferences, standing instructions). Same crate, distinct types: the ported `MemoryContext` is not modified.
  - Why not a new crate: memory is already this crate's stated responsibility in `architecture.md`; a 16th crate for one module would be unjustified. Why not inside `vyapaar-agent-chat`: the decision surface may, under strict rules (§7.5), consume user-authored standing instructions too, so memory must sit below both surfaces.
- **Storage: `MemoryStore` port in `vyapaar-agent-core::ports`, implemented by `vyapaar-runtime-persistence`.**

### 7.2 Model and port

```rust
// vyapaar-agent::memory::user
pub enum MemoryKind { Fact, Preference, Instruction }
pub enum MemoryOrigin { Model, User, Import }
pub struct MemoryEntry {
    pub id: MemoryId, pub kind: MemoryKind, pub content: String /* ≤ 2000 chars */, pub tags: Vec<String>,
    pub importance: u8 /* 0..=3 */, pub pinned: bool, pub origin: MemoryOrigin,
    pub source_session: Option<SessionId>, pub created_at: DateTime<Utc>, pub expires_at: Option<DateTime<Utc>>,
}

// vyapaar-agent-core::ports
#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync + 'static {
    async fn save(&self, e: NewMemory) -> Result<SaveOutcome /* Inserted(id) | Duplicate(id) */, StoreError>;
    async fn profile(&self, budget: ProfileBudget) -> Result<MemoryProfile, StoreError>;   // stable set
    async fn search(&self, q: &MemoryQuery) -> Result<Vec<ScoredMemory>, StoreError>;      // relevance set
    async fn forget(&self, id: MemoryId) -> Result<bool, StoreError>;                        // hard delete
    async fn touch(&self, ids: &[MemoryId]) -> Result<(), StoreError>;                       // usage bump
    async fn version(&self) -> Result<u64, StoreError>;                                      // bumps on any content change
}
```

Recommended SQLite schema for `vyapaar-runtime-persistence` (forward-only migration):

```sql
CREATE TABLE agent_memory_entry (
    id             INTEGER PRIMARY KEY,
    kind           TEXT    NOT NULL CHECK (kind IN ('fact','preference','instruction')),
    content        TEXT    NOT NULL CHECK (length(content) BETWEEN 1 AND 2000),
    content_hash   TEXT    NOT NULL UNIQUE,              -- sha256(normalize(content)): dedupe
    tags           TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(tags) AND json_type(tags) = 'array'),
    importance     INTEGER NOT NULL DEFAULT 1 CHECK (importance BETWEEN 0 AND 3),
    pinned         INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0,1)),
    origin         TEXT    NOT NULL DEFAULT 'model' CHECK (origin IN ('model','user','import')),
    source_session TEXT    REFERENCES agent_chat_session(id) ON DELETE SET NULL,
    created_at     INTEGER NOT NULL, updated_at INTEGER NOT NULL,
    last_used_at   INTEGER, use_count INTEGER NOT NULL DEFAULT 0, expires_at INTEGER
) STRICT;
CREATE INDEX agent_memory_profile ON agent_memory_entry (pinned DESC, kind, importance DESC, id);
CREATE VIRTUAL TABLE agent_memory_fts USING fts5(content, tags, content='agent_memory_entry', content_rowid='id',
                                                 tokenize='porter unicode61 remove_diacritics 2');
-- AFTER INSERT / AFTER DELETE / AFTER UPDATE OF content, tags triggers keep the FTS index in sync;
-- usage bumps (last_used_at, use_count) deliberately do not touch the index.
```

PostgreSQL implementation: a generated `tsvector` column with a GIN index replaces FTS5. Search ranks BM25 (or `ts_rank`) with recency, importance and pinned boosts. If trigger-maintained FTS5 is rejected under the persistence crate's SQLite pragmas, FTS maintenance moves into the same write transaction in Rust; decide before the migration ships (released migrations are checksummed).

### 7.3 The prompt-cache problem this must solve

Provider prompt caching (e.g. Anthropic `cache_control` breakpoints on last tool, system prompt, last user message) keys on an exact prefix. If per-turn, relevance-dependent memory recall is written into the **system prompt**, the head of the prefix changes every turn and every turn becomes a full-price cache miss on the entire conversation. The fix is a **placement split**:

- **Stable prefix** — computed once per session open (and on explicit refresh after a memory write), never dependent on the current user text: pinned entries, all `instruction`/`preference` entries, top facts by importance, ordered by id, capped at a token budget, plus one footer line ("N other memories exist; call `memory_search`"). It changes only when memory content changes.
- **Per-turn context** — relevance-selected entries for *this* turn's user text, **folded into the next user message** as a `[memory] …` block ahead of the user's own text (alongside background notices), never into the system prompt. It varies turn to turn without busting the cached prefix.

### 7.4 Carrying the split over by extending `PromptBuilder` (no new contributor trait)

`vyapaar-agent-core` already defines `PromptBuilder`. Rather than adding a parallel async "prompt contributor" trait beside it, this design **extends `PromptBuilder`'s output** so that placement is a property of the type it returns:

```rust
// vyapaar-agent-core (additive; existing ported methods unchanged)
pub enum Placement { CachedPrefix, TurnContext }

pub struct PromptSection { pub id: &'static str, pub placement: Placement, pub text: String }

/// What a PromptBuilder produces. The chat loop sends `cached_prefix` as the system prompt
/// (frozen for the session) and prepends `turn_context` blocks to the next user message.
pub struct PromptAssembly { pub cached_prefix: String, pub turn_context: Vec<PromptSection> }

pub trait PromptBuilder {
    // … ported decision-prompt methods, unchanged …

    /// New, defaulted: assemble placement-typed sections. Pure and synchronous.
    fn assemble(&self, base: &str, sections: &[PromptSection]) -> PromptAssembly {
        default_assemble(base, sections) // sorts CachedPrefix sections by id; TurnContext kept in order
    }
}
```

Why extend rather than add alongside:

1. **`PromptBuilder` is already the single place prompts are assembled** for the decision surface, and its output is what `vyapaar-agent-replay` hashes. Keeping chat prompt assembly in the same trait means one hashing point and one set of token-budget rules (`vyapaar-agent-prompt`'s budgeters).
2. **Purity is preserved.** Memory recall is I/O; `PromptBuilder` must not be. So the I/O happens *before* assembly: the chat loop calls `UserMemory::profile()` once per session and `UserMemory::recall(user_text)` once per turn (each under a 500 ms timeout; a timeout or error drops that section with a `warn`, never fails the turn), turns the results into `PromptSection`s via renderers in `vyapaar-agent-prompt`, and passes them to the pure `assemble`. A separate async contributor trait would have pushed I/O into the prompt path.
3. **The cache rule becomes a type rule.** A contributor-style trait relies on implementors honouring a doc comment ("don't depend on user text in `contribute`"). Here, anything tagged `Placement::TurnContext` physically cannot land in the system prompt, and `cached_prefix` is computed once per session by the chat loop and frozen on the `Session`; a `debug_assert` and a test pin that it does not change between turns unless `UserMemory::version()` changed.

The date in the prefix is rendered at day granularity so the prefix is stable within a trading day.

### 7.5 Memory hygiene, and the decision surface

- Memory is framed as **data about the user**: instruction entries appear under "User-stated standing instructions", facts are quoted. `memory_save` refuses tool-output boilerplate and over-length content. Writes are denied in unattended runs unless allow-listed. `memory_forget` is a hard delete.
- **The trader loop does not read chat-authored memory.** An LLM-authored memory (`origin = model`) influencing a later decision prompt would be a laundering channel from free-form chat into trading. Default: decision prompts include **no** user memory. Opt-in only: `origin = user`, `kind = instruction`, `pinned` entries (e.g. "never trade F&O", "max 5% per position") may be included in trader-loop prompts, where — like every decision-prompt input — they are cassette-hashed, and they remain advisory: `RiskEngine` limits are the enforcement, not the prompt.
- Deterministic maintenance (`expire`, optional importance decay, FTS optimize) is a pure `UserMemory::tick(now)` plan executed through the store, schedulable as a job (§9). No LLM in maintenance.

---

## 8. Question 2/4 composition: how the pieces wire together

```rust
// vyapaar-cli (composition root), illustrative
let ports = vyapaar_runtime_api::agent_ports(&runtime, mode)?;      // PortfolioReader, MarketReader, WatchlistStore,
                                                                    // SnapshotSource, MonitorScheduler, BacktestRunner
let stores = vyapaar_runtime_persistence::agent_stores(&pool);      // SessionStore, MemoryStore
let memory = vyapaar_agent::memory::UserMemory::new(stores.memory.clone());
let mut registry = vyapaar_agent_chat::ToolRegistry::new();
registry.register_all(vyapaar_agent_chat_tools::builtin(&ports, &memory, &report_engine)?)?;
// later phases: registry.register_all(vyapaar_agent_mcp::client::bridge(&plugin_servers).await?)?;
let agent = vyapaar_agent_chat::ChatAgent::builder(provider, registry, stores.sessions, memory, config).build()?;
let (session, events) = agent.open(SessionSelector::Continue(scope)).await?;   // → vyapaar-tui AgentChat tab
```

---

## 9. Question 5 — Monitors and background research (recommendation to `vyapaar-runtime`)

> **This section is a recommendation, not a decision.** It lives in another repo. Whoever writes `vyapaar-runtime/docs/design/000N-jobs-monitors-and-agent-tasks.md` (after Milestone 3.2) owns the final call. This document only fixes the **agent-side port** it needs.

### 9.1 Recommendation: extend `JobSpec`, do not add a second scheduler

`vyapaar-runtime-jobs` (new in 3.2) is a deterministic `JobScheduler` + `CronSchedule` + `JobSpec` for fixed operational jobs (08:45 IST TOTP refresh, EOD bhavcopy sync, T+1 settlement sweep, snapshots). Monitors and background research are additional *job kinds*, not a different scheduling concept. Recommended additive variants:

```rust
// vyapaar-runtime-jobs (recommended)
pub enum JobSpec {
    // … 3.2 variants: TotpRefresh, EodSync, SettlementSweep, Snapshot …
    Monitor(MonitorSpec),
    AgentTask(AgentTaskSpec),
    MemoryMaintenance,                      // runs UserMemory::tick (§7.5)
}

pub struct MonitorSpec {
    pub id: MonitorId, pub name: String,
    pub condition: MonitorCondition,        // PriceAbove/Below{instrument, price: Price}, PercentMove{..},
                                            // IndicatorCross{..}, VolumeSpike{..}, PortfolioDrawdown{account, pct: Decimal},
                                            // CircuitApproach{instrument, within_pct}  (Indian price-band specific)
    pub schedule: CronSchedule,             // or Every{secs ≥ 60}
    pub market_hours_only: bool,            // NSE/BSE session calendar, IST
    pub action: MonitorAction,              // Notify{message} | RunAgentTurn{goal, allow_tools}
    pub policy: FiringPolicy,               // cooldown, max_fires, expires_at
}

pub struct AgentTaskSpec { pub goal: String, pub allow_tools: Vec<String>, pub scope: SessionScope, pub summary_cap: u32 }
```

Recommended semantics:

- **Evaluation is deterministic and never uses an LLM.** At fire time the scheduler fetches the observation (market data / ledger read — the runtime has both), applies **edge-triggered** semantics (fire on not-satisfied → satisfied; re-arm only once false again), and honours cooldown/max_fires/expiry. State (`armed`, `last_observed`, `last_fired_at`, `consecutive_errors`) is persisted so firing is correct across restarts; the injected clock keeps `JobScheduler` deterministic in tests.
- **Actions are data; the scheduler executes them.** `Notify` → `/ws/live` broadcast + `SessionHandle::notify(Notice::MonitorFired)` for any in-process session. `RunAgentTurn` / `AgentTask` → `run_turn_headless` with `PermissionPolicy::NonInteractive { allow }`, observation prepended to the goal. Runtime may depend on `vyapaar-agent-chat` directly (3B → 3A), but a small `AgentTaskRunner` trait owned by `vyapaar-runtime-jobs` keeps the jobs crate light and testable.
- **Unattended tool allow-lists may contain only `ReadOnly` tools plus `memory_save`/`report_render`.** No unattended action can trade, by the same construction as §4.3 (there is no trade tool to allow).
- **Schema** (recommended): `jobs_monitor_definition`, `jobs_monitor_state` (one row per monitor, written only by the evaluator), `jobs_monitor_event` (fired events with `dispatch_status`), all owned by `vyapaar-runtime-persistence` migrations, `Decimal`s as `TEXT`.

### 9.2 What this repo fixes: the agent-side port

```rust
// vyapaar-agent-chat-tools::ports
#[async_trait::async_trait]
pub trait MonitorScheduler: Send + Sync + 'static {
    async fn create(&self, draft: MonitorDraft) -> Result<MonitorRef, PortError>;
    async fn list(&self) -> Result<Vec<MonitorView>, PortError>;
    async fn pause(&self, id: &MonitorId) -> Result<(), PortError>;
    async fn resume(&self, id: &MonitorId) -> Result<(), PortError>;
    async fn cancel(&self, id: &MonitorId) -> Result<(), PortError>;
    async fn events(&self, id: &MonitorId, limit: u32) -> Result<Vec<MonitorEventView>, PortError>;
    async fn submit_task(&self, task: AgentTaskDraft) -> Result<BackgroundTicket, PortError>;
}
```

`MonitorDraft` / `AgentTaskDraft` are the **model-facing input schemas** (`JsonSchema`, `deny_unknown_fields`), deliberately separate from runtime's `MonitorSpec`: the adapter in `vyapaar-runtime-api` validates and translates (an anti-corruption layer), so neither the shared kernel (`vyapaar-core`) nor the agent repo has to know the scheduler's internal representation. `monitor_create.plan()` validates cron syntax, instrument ids and `allow_tools` (showing the list in the permission prompt) with no I/O. If the scheduler is not wired (e.g. a headless `ask` without runtime), monitor tools stay registered and fail fast with a model-readable "monitors are not available in this session".

---

## 10. LLM adapter cross-check (`vyapaar-agent-llm`, post-port)

`vyapaar-agent-llm` is a straight port of `honba-nou-llm` and is not redesigned here. Before phase A starts, check the ported code against what the conversation loop requires; each gap becomes an **additive** change in `vyapaar-agent-llm`:

| Requirement | Why the chat loop needs it |
| :--- | :--- |
| Object-safe `LlmProvider` (`async fn chat(&self, &ChatRequest<'_>) -> Result<EventStream>` boxed; `Arc<dyn LlmProvider>`) | provider chosen at runtime from config |
| Streaming events: text/thinking deltas, tool-call start/delta/**end with fully assembled input**, usage, `MessageEnd { stop_reason, message, usage }` | turn state machine rows; eager start on tool-call end |
| Per-call argument buffers (Anthropic `input_json_delta` until `content_block_stop`; OpenAI `delta.tool_calls[i].function.arguments` until `finish_reason`) | both wire formats differ; core must see whole calls |
| **Malformed tool-argument JSON delivered as data** (call with an input-error flag + raw text), not a stream error | small local models; lets the model self-correct |
| Retries only before the first byte: backoff with jitter on 408/409/425/429/5xx/529, honour `Retry-After`, give up with `RateLimited { retry_after }` beyond ~60 s | chat loop owns mid-stream retry; never sleeps minutes silently |
| Phase-specific timeouts (connect, first byte, inter-event idle) — not a whole-request timeout | a valid stream can last minutes |
| Prompt caching on by default for Anthropic: breakpoints on last tool, system prompt, last user message; disable switch for compatible local servers | the whole point of §7.3/§7.4 |
| History validation rejecting unpaired tool-use/tool-result | the repair-on-load invariant (§5.4) |
| Usage (incl. cache read/write tokens) reported in-band | session usage totals; cache-hit regression test |
| `ScriptedFakeLlm` able to script streamed tool calls | chat-loop tests |

The decision surface's needs (seed/temperature control, cassette hooks) are unaffected.

---

## 11. Question 8 — Reports from chat

### 11.1 Decision

Tearsheet-on-demand (`tearsheet_render`: strategy/backtest/portfolio performance via `vyapaar-agent-analytics` metrics) covers "show me the performance report". It does **not** cover the realistic request "turn what we just worked out into a note I can keep" (a sector comparison, a position review, a pre-earnings checklist). So `vyapaar-agent-report` gains **one** additional, typed report kind, `ResearchNote`. No free-form document engine.

```rust
// vyapaar-agent-report (additive)
pub struct ResearchNote {
    pub title: String, pub as_of: DateTime<Utc>, pub disclaimer: Disclaimer, // fixed "not investment advice" block
    pub sections: Vec<NoteSection>,
}
pub enum NoteBlock {
    Markdown(String),                       // restricted Markdown subset → escaped into Typst
    Table { columns: Vec<String>, source: DataRef },
    Metric { label: String, source: DataRef },
    Tearsheet(TearsheetRef),                // embed an existing TearsheetReport
}
/// A pointer into a tool result of this session: numbers are copied from what a tool returned,
/// not retyped by the model.
pub struct DataRef { pub call_id: CallId, pub pointer: String /* RFC 6901 JSON Pointer */ }
```

Rules:

- **The model never writes Typst.** Typst is a programming language with file access; model text enters only through `Markdown` blocks that the renderer escapes. Templates are compiled in.
- **Numbers come by reference.** `report_render` resolves each `DataRef` against the session's persisted tool results; an unresolvable reference is an `InvalidInput` error the model can fix. This makes a rendered note auditable back to the tool call that produced each figure, and keeps hallucinated arithmetic out of tables.
- **Typst is embedded** as a library (it is a Rust crate), not shelled out to a binary.
- **Output location** is a platform-owned reports directory (`<data_dir>/reports/<slug>-<date>.pdf`, plus the `.md` source), injected via a `ReportSink` port. Because it writes only platform-owned artifacts, `report_render` is `LocalWrite`, not a user-file write, and needs no checkpointing (§12).

---

## 12. Question 9 — Checkpoint/undo for agent-edited files: out of scope

**Decision: not in scope, nothing built.** No chat tool in this design edits a user file, and none is proposed. The `ToolRisk` enum deliberately omits `FileWrite`, `SessionEvent` has no checkpoint variant, and `ToolContext` exposes no edit recorder. Building snapshot/rewind machinery speculatively would add a persistence surface, a restore algorithm with known sharp edges (restoring a cumulative snapshot can delete files first touched later; restores must be per-path from each path's own first pre-edit snapshot), and UI (`/undo`, `/rewind`) for a feature with no consumer.

If a future design adds a **strategy-file editing** chat tool (e.g. "tighten the stop in my Turtle strategy"), the constraints are recorded here so it starts in the right place:

- Scope **only** to files under the user's `vyapaar-strategies` workspace; never general-purpose file editing, never shell.
- Introduce `ToolRisk::FileWrite` then, with declared writes in `plan()`, fail-closed pre-edit snapshot (no snapshot → no edit), explicit user-triggered restore only.
- **Policy** (when to snapshot, what to restore) would belong in `vyapaar-agent-chat`; **plumbing** belongs in `vyapaar-runtime`, which already owns "SQLite & git2 snapshots" (README) — exposed to the agent through a `CheckpointStore` port in `vyapaar-agent-core::ports`. No new crate.
- An edited strategy must still go through the normal backtest/enhancer path before it can run live; editing a file never deploys it.

---

## 13. Questions 6 and 7 — Plugins and MCP

### 13.1 Plugins: superset capability model, no marketplace

`vyapaar-agent-plugins` (new in 3.1) is scoped to "custom research skills" with `PluginManifest`, `PluginCapability`, `PluginRegistry`. **Broaden it** — the superset framing applied to plugins:

```rust
#[non_exhaustive]
pub enum PluginCapability {
    ResearchSkill(ResearchSkillDecl),       // original 3.1 scope, unchanged
    ChatTool(McpServerDecl),                // new: tools served by an out-of-process MCP stdio server
    PromptPack(PromptPackDecl),             // new: extra CachedPrefix sections (e.g. a sector glossary), static text only
}
pub struct McpServerDecl {
    pub command: PathBuf, pub args: Vec<String>, pub env_allow: Vec<String>,  // explicit env allow-list
    pub max_risk: ToolRisk,                 // ReadOnly or External; anything else rejected at load
    pub timeout: Duration,
}
pub struct PluginManifest { pub id: String, pub version: semver::Version, pub sha256: String, pub capabilities: Vec<PluginCapability>, … }
```

- **Execution model:** plugin chat tools run **out of process** as MCP stdio servers bridged by `vyapaar-agent-mcp`'s client (§13.2). No dynamic Rust loading (unsound across compiler versions) and no embedded WASM runtime (a large new dependency for no capability MCP doesn't already give). Process isolation also keeps broker credentials out of plugin address space: the environment is cleared except for `env_allow`.
- **Hard limits in the manifest validator:** no capability may declare `Execute`, `LocalWrite` on platform state, or anything that touches the ledger, the risk engine or execution. A plugin cannot add a tool to the trader loop's pure `AgentTool` registry (that registry stays compile-time only, so replay stays deterministic).
- **Distribution: explicitly out of scope.** The README's "plugin marketplace" is aspirational. This design supports only **local, manually installed plugins** under `<config_dir>/plugins/<id>/`, enabled per profile, pinned by `sha256` of the manifest and server binary (a changed binary is disabled until re-approved). Rationale: a remote index means signing keys, revocation, update policy and review — real supply-chain infrastructure on a machine that holds broker API keys and TOTP seeds. That deserves its own design once there are third-party plugins to distribute; there are none today.

### 13.2 MCP: make `vyapaar-agent-mcp` bidirectional (feature `client`)

**Decision: yes, client capability is needed, and it lives in the existing crate** behind a cargo feature `client` (server remains the default feature).

- **Needed** because it is the plugin transport (§13.1) and the clean way to add data sources `vyapaar-market` doesn't cover (filings, news, alternative data) without growing the agent repo's dependency tree.
- **Same crate** because it already owns the `rmcp` dependency, transport configuration, and the JSON-Schema ↔ `ToolSpec` mapping; `rmcp` implements both roles, so a separate crate would duplicate the mapping and version-pin `rmcp` twice.
- **Bridge semantics:** each remote tool becomes a `ChatTool` named `mcp__<server>__<tool>` (sanitised to the provider's name rules, < 64 chars). `annotations.readOnlyHint = true` → `ReadOnly` (concurrency-safe); `destructiveHint = true` → `External` with `ConfirmOverride::Always`; no annotations → `External`. The plugin manifest's `max_risk` caps whatever the server claims. MCP tools are **never** in default unattended allow-lists; results are untrusted data (64 KiB cap, no instruction-following from tool output in the base prompt).
- **Server side stays as ported.** Optionally, after phase B, the outward MCP server can serve the read-only subset of the `ChatTool` registry so external clients and `vyapaar ask` see one tool vocabulary; that is a convergence nicety, not a requirement.
- **Sequencing:** last (phase E). Until then the chat assistant is fully functional with built-in tools.

---

## 14. Facts to re-verify when the gate opens

The ported code is the source of truth; before phase A, confirm:

1. `PromptBuilder`'s actual signature in `vyapaar-agent-core` (sync? generic over context?) — adjust §7.4's additive `assemble` accordingly.
2. What the ported base crate's "memory context" actually is (working memory vs. something persistent), and that `UserMemory` does not overlap it.
3. Whether `vyapaar-agent-core` already defines a `ToolSpec`-like type (from `honba-nou-mcp`), to reuse rather than add.
4. `vyapaar-agent-llm`'s gaps against §10.
5. Sync vs async shape of `vyapaar-ledger`'s repository traits and the exact names implemented by `vyapaar-runtime-persistence` (§6.2 adapter bounds).
6. `JobSpec`'s landed shape in `vyapaar-runtime-jobs` (enum? trait objects?) — §9 is phrased for an enum.
7. That `vyapaar-agent-tools` landed as specified (pure `AgentTool`, `ToolCall`, `ToolOutput`, `ToolRegistry`), and reconcile the README vs `architecture.md` tool lists.

---

## 15. Phasing

| Phase | Content | Repos |
| :--- | :--- | :--- |
| **A — Conversation core** | `vyapaar-agent-core::ports` (`SessionStore`, `MemoryStore` traits, `StoreError`, shared `ToolSpec`/`ToolOutput`/`ToolError`), `PromptAssembly`; `vyapaar-agent-chat` loop with `ScriptedFakeLlm` tests; R1–R5 dependency test; `vyapaar-runtime-persistence` session tables + `SessionStore` impl; `vyapaar-cli ask` + `vyapaar-tui` AgentChat wired *(Complete across agent, runtime, ui)* | agent, runtime, ui |
| **B — Read-only tools** | `vyapaar-agent-chat-tools` with market, portfolio, lifted pure tools, `research_calc`; `vyapaar-runtime-api` port adapters | agent, runtime |
| **C — Memory** | `UserMemory`, memory tables + FTS, profile/recall placement, cache-hit regression test (assert cache-read tokens > 0 from turn 2 with `MemoryProfile` unchanged) | agent, runtime |
| **D — Monitors & background tasks** | runtime's own jobs design doc; `JobSpec::{Monitor, AgentTask, MemoryMaintenance}`; `MonitorScheduler` adapter; monitor/backtest tools | runtime (lead), agent |
| **E — Reports, plugins, MCP client** | `ResearchNote`; `PluginCapability` superset; `vyapaar-agent-mcp` `client` feature | agent |

Each phase ends with a commit per logical step and an independent review/validation pass (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, the dependency-rule test).

---

## 16. Crate / repo touch-list

### 16.1 Existing crates extended (repo `vyapaar-agent`)

| Crate | Change |
| :--- | :--- |
| `vyapaar-agent-core` | Add `ports` module: `SessionStore`, `MemoryStore`, `StoreError`, `SessionScope`, `RuntimeMode` mirror (later: `CheckpointStore` only if §12 ever activates). Add shared `ToolSpec`/`ToolOutput`/`ToolError` if not already present. Extend `PromptBuilder` with defaulted, pure `assemble` → `PromptAssembly { cached_prefix, turn_context }`, `Placement`, `PromptSection`. |
| `vyapaar-agent` | Add `memory::user::UserMemory` (profile, recall, save, forget, tick) beside the unchanged ported `MemoryContext`. Ported decision state machine untouched. |
| `vyapaar-agent-prompt` | Chat base prompt (assistant role, tool etiquette, "not investment advice", tool outputs are data); memory profile/recall section renderers using existing token budgeters; day-granular date. |
| `vyapaar-agent-replay` | Expose canonical `hash_request` for chat audit hashes. No change to cassette replay. |
| `vyapaar-agent-llm` | Additive fixes for any gaps found against §10. |
| `vyapaar-agent-tools` | **No scope change** — stays pure. One-line doc note that tools are lifted into chat via `PureToolAdapter`. |
| `vyapaar-agent-plugins` | Broaden `PluginCapability` to `ResearchSkill` + `ChatTool(McpServerDecl)` + `PromptPack`; manifest validation limits; hash pinning; local install only. |
| `vyapaar-agent-mcp` | Add `client` feature: MCP stdio client bridging remote tools to `ChatTool`. Server side as ported. |
| `vyapaar-agent-report` | Add `ResearchNote` with `DataRef`-sourced tables/metrics, restricted Markdown, embedded Typst, `ReportSink` port. |
| `vyapaar-agent-trader-loop`, `-research`, `-analytics`, `-genetic` | No changes. (Trader loop: optional, opt-in inclusion of user-authored pinned instructions, cassette-hashed — §7.5.) |

### 16.2 New crates (repo `vyapaar-agent`, workspace grows 13 → 15)

| New crate | Why it cannot be one of the existing 13 |
| :--- | :--- |
| `vyapaar-agent-chat` | The conversation loop is a different state machine from the ported decision machine in `vyapaar-agent` (open-ended, streaming, permission-gated, session-persistent, non-`Decision` output). Housing it in the base crate would mix the two surfaces NoFX needs kept apart and would modify a crate that must stay a diffable port. It also owns the `ChatTool` contract so tools, MCP bridges and plugins share one seam. |
| `vyapaar-agent-chat-tools` | I/O-capable, async tools cannot live in `vyapaar-agent-tools` without voiding its purity and replay determinism (§4.2); they cannot live in `vyapaar-agent-chat` without dragging the ledger dependency and all ports into the loop crate. This crate is the single place where the conversation surface's dependency fan-out (ledger types, ports) happens, and the R1 test guards it. |

### 16.3 Recommendations for other repos' own future design docs (not finalised here)

| Repo / crate | Recommendation | To be decided by |
| :--- | :--- | :--- |
| `vyapaar-runtime/crates/vyapaar-runtime-jobs` | Extend `JobSpec` with `Monitor(MonitorSpec)`, `AgentTask(AgentTaskSpec)`, `MemoryMaintenance`; deterministic edge-triggered monitor evaluation; `AgentTaskRunner` trait; unattended read-only allow-lists (§9.1). | A future `vyapaar-runtime/docs/design/` doc, after Milestone 3.2. |
| `vyapaar-runtime/crates/vyapaar-runtime-persistence` | Implement `SessionStore` and `MemoryStore` (agent-core ports) with migrations for `agent_chat_*`, `agent_memory_*` (FTS5 / `tsvector`), `agent_watchlist*`, and (with jobs) `jobs_monitor_*`. Depend only on `vyapaar-agent-core` (R5). | Same runtime design doc. |
| `vyapaar-runtime/crates/vyapaar-runtime-api` | Host the adapters implementing `vyapaar-agent-chat-tools::ports` (`PortfolioReader` over ledger repositories, `MarketReader` over `vyapaar-market`, `WatchlistStore`, `SnapshotSource`, `MonitorScheduler`, `BacktestRunner`), constructed per `RuntimeMode`. | Same runtime design doc. |
| `vyapaar-ui/crates/vyapaar-tui`, `vyapaar-cli` | Consume `SessionHandle` / `SessionEvent` / `run_turn_headless` as specified in §5.3/§5.6 instead of the placeholder contract in `vyapaar-ui/docs/plans/0001-…` §4.2–4.3. | `vyapaar-ui` follow-up plan. |
| `vyapaar-agent/docs/migration-guide.md`, `architecture.md` | Once this design is accepted: amend the tier sentence for R4 (`vyapaar-agent-chat-tools → vyapaar-ledger`, types only), add the two new crates to the topology table, and add the "`AgentTool`s are lifted into chat" note. Not edited by this document. | `vyapaar-agent` maintainers at acceptance. |
| `vyapaar-ledger`, `vyapaar-core`, `vyapaar-risk` | **No changes requested.** Ledger types are consumed read-only; no I/O is added; `Decision` grammar is untouched. | — |
