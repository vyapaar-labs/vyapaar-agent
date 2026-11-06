# 0002 — Autonomous Job Scheduler, Local Memory Engine, and Strategy Verification Backend

> **Status**: Ready for implementation.
> **Target Crates**: `vyapaar-agent-core`, `vyapaar-agent-chat`, `vyapaar-agent-research`, `vyapaar-agent-plugins`.
> **Depends on reading**:
> - `vyapaar-agent/docs/plans/0001-mcp-gateway-discovery-and-quantitative-engines.md`
> - `vyapaar-ui/docs/plans/0004-cli-command-reference-and-tui-slash-commands.md`
> - Quantcept Reference Specifications (`https://quantcept.io/docs/reference/cli`)

---

## 0. Executive Summary & Objective

The CLI commands specified in `vyapaar-ui/docs/plans/0004` (`jobs`, `memory`, `verify`, `doctor`, `purge`) and in-app slash commands (`/remember`, `/undo`, `/checkpoints`, `/export`) require industrial-grade backend engines inside `vyapaar-agent`.

This plan details the architecture and implementation of:
1. **Autonomous Scheduled Job Engine (`vyapaar-agent-core::jobs`)**: Cron-based task runner with multi-dimensional budget enforcement (max turns, max INR statutory cost, max tokens) and an explicit human approval gate.
2. **Local Financial Memory Engine (`vyapaar-agent-core::memory`)**: Persistent local knowledge base (`~/.vyapaar/memory.db`) storing trader preferences, instrument nuances, and historical learnings with periodic distillation.
3. **Static Strategy Verification Backend (`vyapaar-agent-research::verify`)**: Static AST analyzer that detects lookahead bias, future timestamp access, and in-sample data leakage before any backtest runs.
4. **Session State Checkpointing & Time-Travel (`vyapaar-agent-chat::session`)**: Turn-by-turn checkpointing enabling `/undo`, `/redo`, and transcript exports.

---

## 1. System Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│               Caller: vyapaar-cli / vyapaar-tui                        │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
┌───────────────────────────────────▼────────────────────────────────────┐
│                    vyapaar-agent Backend Subsystems                    │
├───────────────────┬───────────────────┬────────────────────────────────┤
│    Job Engine     │   Memory Engine   │      Verification Engine       │
│                   │                   │                                │
│ • Cron Evaluator  │ • SQLite Vector   │ • AST Lookahead Bias Scanner   │
│ • Budget Guard    │ • BM25 Search     │ • Data Leakage / Scaler Check  │
│ • Approval Gate   │ • Auto-Distill    │ • NoFX Execution Invariant     │
└─────────┬─────────┴─────────┬─────────┴────────────────┬───────────────┘
          │                   │                          │
┌─────────▼───────────────────▼──────────────────────────▼───────────────┐
│                          vyapaar-agent-chat                            │
│           • Checkpoint & Turn History Store (Undo / Redo)              │
│           • Session Transcript Exporters (JSON / Markdown)             │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │
┌───────────────────────────────────▼────────────────────────────────────┐
│                        Deterministic Safety Core                       │
│              vyapaar-risk (Pre-Trade Margin & Drawdown Gates)          │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Component Specifications

### 2.1 Autonomous Scheduled Job Engine (`vyapaar-agent-core::jobs`)

The Job Engine executes long-running quantitative research, morning briefs, and scanner watches on a clock.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub id: String,                 // 8-character hex ID (e.g. "a3f81c9b")
    pub goal: String,               // Natural language goal
    pub schedule: Option<String>,   // Standard 5-field cron (e.g. "0 9 * * 1-5")
    pub budget: JobBudget,
    pub read_only: bool,            // Defaults to true (NoFX advisory)
    pub status: JobStatus,          // Active, Paused, PendingApproval, Completed
    pub created_at: String,         // RFC3339 IST
    pub next_run: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobBudget {
    pub max_turns: usize,                  // Default: 20
    pub max_inr: Option<Decimal>,          // Hard limit on statutory / data cost
    pub max_tokens: Option<usize>,         // Hard token cap
    pub max_tool_calls: Option<usize>,     // Prevent tool call loops
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobStatus {
    Active,
    Paused,
    PendingApproval { action_id: String, details: String },
    Completed,
    Failed(String),
}
```

#### Execution & Approval Invariants
- **`vyapaar jobs tick`**: Evaluates cron expressions against current IST time. If a job is due, it triggers a headless execution session using `run_turn_headless`.
- **Budget Exhaustion**: If a job reaches `max_turns`, `max_inr`, or `max_tokens`, it terminates immediately with a budget error to prevent runaway cloud or statutory expenditure.
- **Human Approval Gate**: Any tool call proposing a portfolio rebalance or touching orders switches job status to `PendingApproval`. Execution halts until confirmed via `vyapaar jobs approve <id>`.

---

### 2.2 Local Financial Memory Engine (`vyapaar-agent-core::memory`)

Quantcept maintains a local memory engine that retains trader notes, risk tolerance, and learned patterns across sessions. Vyapaar stores memory in local SQLite at `~/.vyapaar/memory.db`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFact {
    pub id: i64,
    pub content: String,            // e.g. "Trader avoids PSU banks due to policy volatility"
    pub category: MemoryCategory,   // Preference, RiskRule, InstrumentFact, General
    pub confidence: Decimal,        // 0.0 to 1.0
    pub created_at: String,         // RFC3339 IST
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryCategory {
    Preference,
    RiskRule,
    InstrumentFact,
    General,
}
```

#### Memory Operations
- **`remember(fact)`**: Immediately inserts a validated user fact.
- **`search(query)`**: Combines BM25 keyword matching with cosine similarity on local embeddings to find relevant context.
- **`distill_tick()`**: Background task scanning recent session transcripts, prompting a summarizer model to extract new permanent facts while deduplicating existing ones.

---

### 2.3 Static Strategy Verification Engine (`vyapaar-agent-research::verify`)

Backtests frequently look profitable due to subtle programming errors. `vyapaar-agent-research` implements a static AST and execution verifier:

```rust
pub struct StrategyVerifier;

impl StrategyVerifier {
    pub fn verify_manifest(manifest: &StrategyManifest, strict: bool) -> Result<VerificationReport, VerificationError> {
        let mut violations = Vec::new();

        // 1. Lookahead Bias Checks:
        // - Indicator computation accessing bar[i + k] where k >= 0 during bar i processing
        // - Use of daily close price in an intraday strategy before 15:30 IST
        
        // 2. Data Leakage Checks:
        // - Machine learning feature scalers fitted on entire dataset prior to walk-forward train split
        
        // 3. NoFX Safety Violations:
        // - Direct un-sandboxed network or socket calls attempting order placement
        
        if violations.is_empty() {
            Ok(VerificationReport::clean())
        } else {
            Err(VerificationError::ViolationsFound(violations))
        }
    }
}
```

---

### 2.4 Session State Checkpointing & Time-Travel (`vyapaar-agent-chat::session`)

To support `/undo`, `/redo`, `/checkpoints`, and `/export`:
- Every turn in `ChatAgent` writes a snapshot of `(TurnId, ConversationState, ContextSummary)`.
- **`undo()`**: Decrements the session turn pointer and restores prior message state.
- **`redo()`**: Advances pointer if no intervening user message has branched the tree.
- **`export(format)`**: Formats session history into JSON or human-readable Markdown with timestamps, tool traces, and performance summaries.

---

## 3. Implementation Phases

### Phase 1: Job Spec & Scheduler (`vyapaar-agent-core/src/jobs/`)
- Define `JobSpec`, `JobBudget`, and `JobStore` (SQLite backed).
- Implement `tick()` runner with cron parsing and budget enforcement.
- Implement `ApprovalGate` preventing unapproved execution.

### Phase 2: Memory Store & Retrieval (`vyapaar-agent-core/src/memory/`)
- Implement SQLite tables for `facts` and `learnings`.
- Wire retrieval into `PromptSection::Context` in `vyapaar-agent-prompt`.

### Phase 3: Strategy AST Verifier (`vyapaar-agent-research/src/verify/`)
- Implement syntax and logic rules detecting lookahead bias and fit-before-split data leakage.

### Phase 4: Session Checkpoints & Undo (`vyapaar-agent-chat/src/session/`)
- Implement turn checkpoints, rollback, and JSON/TXT transcript export.
