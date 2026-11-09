# `vyapaar-agent` — Chat Assistant Reference

Documents `vyapaar-agent-chat/src/`: conversational assistant engine, session persistence,
turn state machine, permission gating, and event streaming (Design Doc 0001, Phase A —
COMPLETED).

## Modules

| Module | Purpose |
|--------|---------|
| `agent` | `ChatAgent`, `ChatAgentBuilder`, `ChatConfig` |
| `cancel` | `CancellationToken` |
| `error` | `ChatError`, `ChatErrorView` |
| `event` | `SessionEvent`, `ToolCallView`, `ToolOutcome`, `TurnOutcome`, `TurnId`, `CallId`, `Notice` |
| `headless` | `run_turn_headless` (unattended CLI execution) |
| `permission` | `ToolRisk`, `PermissionPolicy`, `PermissionPrompt`, `PermissionResponder`, `ConfirmOverride`, `PermissionDecision` |
| `session` | `SessionActor`, `SessionHandle`, `SessionEvents`, `SessionSelector`, `checkpoint` (`Checkpoint`, `ExportFormat`, `SessionCheckpointer`) |
| `testing` | `FakeChatTool`, `InMemoryMemoryStore`, `InMemorySessionStore` |
| `tool` | `ChatTool`, `PlanContext`, `ToolContext`, `ToolPlan`, `ToolRegistry`, `ToolSet` |
| `turn` | `TurnConfig`, `run_turn`, `CancelPoint` |

## Turn State Machine

- Max rounds, 64 KiB tool outcome truncation, write-ahead persistence, repeat-error nudge,
  synthetic cancellation repair.
- Channel-based `SessionActor` / `SessionHandle` dispatching typed `SessionEvent` streams.

## Permissions Engine

`ToolRisk` levels (`ReadOnly`, `LocalWrite`, `External`, `Execute`) gated by
`PermissionPolicy`, with interactive prompt/responder channels for user confirmation.

## Headless Runner

`run_turn_headless` enables unattended CLI execution (used by `vyapaar-cli ask`).

## Dependency Rules (R1–R5)

`vyapaar-agent-chat` must NOT depend on `vyapaar-runtime-*`, `vyapaar-exec-*`, or
`vyapaar-market-*` (architecture R1–R3). External reads are inverted through ports in
`vyapaar-agent-core::ports`. Regression-tested via `tests/dependency_rules.rs` inspecting
`cargo metadata`.

## Untrusted AI Principle

Chat turns never produce a trade or bypass the `DecisionValidator → PriceBand → RiskEngine`
pipeline.

## Related

- `vyapaar-agent-tools` — pure `AgentTool` registry (lifted into chat via `PureToolAdapter`;
  the lift crate `vyapaar-agent-chat-tools` is planned — gap 0002).
- `vyapaar-agent-core::prompt` — chat base prompts & section renderers.
