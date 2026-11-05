//! Event streaming definitions for conversation turns and session lifecycle.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use vyapaar_agent_core::{MessageSeq, SessionId, SessionScope};
use vyapaar_agent_llm::{StopReason, Usage};

use crate::error::ChatErrorView;
use crate::permission::PermissionPrompt;

/// Strongly-typed 1-indexed turn identifier within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TurnId(pub u64);

impl std::fmt::Display for TurnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Call identifier assigned by model to a tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CallId(pub String);

impl CallId {
    /// Create a new call ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Access inner string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// System or background notifications delivered to a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Notice {
    /// Informational message.
    Info(String),
    /// Warning message.
    Warning(String),
    /// Background job or backtest finished.
    BackgroundFinished {
        /// Ticket identifier.
        ticket: String,
        /// Execution summary.
        summary: String,
    },
    /// Background job failed.
    BackgroundFailed {
        /// Ticket identifier.
        ticket: String,
        /// Error description.
        error: String,
    },
    /// A registered market monitor condition fired.
    MonitorFired {
        /// Monitor ID.
        monitor_id: String,
        /// Condition notification message.
        message: String,
    },
}

/// Presentation view of a tool call before/during execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallView {
    /// Tool invocation ID.
    pub call_id: CallId,
    /// Canonical tool name.
    pub name: String,
    /// Arguments payload.
    pub arguments: serde_json::Value,
}

/// Execution outcome of a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    /// Tool executed successfully.
    Success,
    /// Tool returned an execution failure.
    Error(String),
    /// Tool execution was denied by user permission policy.
    Denied(String),
}

/// Rich display formatting for UI presentation (never sent to model).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolDisplay {
    /// Formatted text.
    Text(String),
    /// Tabular rows and columns.
    Table {
        /// Header column titles.
        columns: Vec<String>,
        /// Row values.
        rows: Vec<Vec<String>>,
    },
}

/// Final outcome of an assistant conversation turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum TurnOutcome {
    /// Completed normally with assistant message.
    Completed,
    /// Context window or max_tokens reached.
    MaxTokens,
    /// Model refused prompt.
    Refused {
        /// Refusal reason.
        reason: String,
    },
    /// Maximum autonomous round limit reached.
    RoundLimit {
        /// Configured limit reached.
        limit: u32,
    },
    /// Turn was stopped by user or policy.
    Stopped,
    /// Turn was cancelled mid-flight.
    Cancelled,
    /// Turn encountered an unrecoverable error.
    Failed {
        /// Error details.
        error: ChatErrorView,
    },
}

/// Comprehensive, ordered event stream emitted by a chat session actor (§5.6).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SessionEvent {
    // Session lifecycle
    /// Session was opened or resumed.
    SessionOpened {
        /// Session ID.
        session: Option<SessionId>,
        /// Whether session was resumed from storage.
        resumed: bool,
        /// Number of history messages loaded.
        history: usize,
        /// Scope (profile + mode).
        profile: SessionScope,
        /// Model name.
        model: Option<String>,
    },
    /// Session state was committed to persistent storage.
    SessionPersisted {
        /// Session ID.
        session: SessionId,
    },
    /// Session title was updated.
    TitleSet {
        /// New title.
        title: String,
    },
    /// LLM provider or model was changed dynamically.
    ProviderChanged {
        /// Provider name.
        provider: Option<String>,
        /// Model name.
        model: Option<String>,
    },
    /// System or background notice received.
    Notice(Notice),

    // Turn lifecycle
    /// A new turn started with user input.
    TurnStarted {
        /// Turn identifier.
        turn: TurnId,
        /// Number of pending messages queued behind this one.
        queued_behind: usize,
    },
    /// An inference round (LLM request + tool dispatch) started.
    RoundStarted {
        /// Turn identifier.
        turn: TurnId,
        /// Round number (1-indexed).
        round: u32,
    },
    /// An inference round failed with a retryable error and is retrying.
    RoundRetrying {
        /// Turn identifier.
        turn: TurnId,
        /// Round number.
        round: u32,
        /// Attempt number.
        attempt: u32,
        /// Reason for retry.
        reason: String,
        /// Whether partially streamed text was discarded.
        discard_partial: bool,
    },
    /// Turn ended with a terminal outcome.
    TurnEnded {
        /// Turn identifier.
        turn: TurnId,
        /// Terminal outcome.
        outcome: TurnOutcome,
        /// Aggregate token usage.
        usage: Usage,
        /// Total rounds executed.
        rounds: u32,
        /// Whether turn was persisted to storage.
        persisted: bool,
    },

    // Model streaming
    /// Text chunk streamed from model.
    TextDelta {
        /// Turn identifier.
        turn: TurnId,
        /// Round number.
        round: u32,
        /// Streamed text fragment.
        text: String,
    },
    /// Model thinking / reasoning chunk streamed.
    ThinkingDelta {
        /// Turn identifier.
        turn: TurnId,
        /// Round number.
        round: u32,
        /// Thinking text fragment.
        text: String,
    },
    /// Model began requesting a tool invocation.
    ToolCallStreaming {
        /// Turn identifier.
        turn: TurnId,
        /// Round number.
        round: u32,
        /// Tool call identifier.
        call_id: CallId,
        /// Tool name.
        name: String,
    },
    /// Model completed specifying tool arguments; tool is ready for dispatch.
    ToolCallReady {
        /// Turn identifier.
        turn: TurnId,
        /// Round number.
        round: u32,
        /// Ready tool call view.
        call: ToolCallView,
    },
    /// Assistant message committed to session history.
    AssistantCommitted {
        /// Turn identifier.
        turn: TurnId,
        /// Round number.
        round: u32,
        /// Persisted message sequence number.
        seq: Option<MessageSeq>,
        /// Stop reason.
        stop_reason: StopReason,
    },
    /// Token usage metrics for the round.
    Usage {
        /// Turn identifier.
        turn: TurnId,
        /// Round number.
        round: u32,
        /// Token usage statistics.
        usage: Usage,
    },

    // Tools dispatch
    /// Tool requires interactive user permission confirmation.
    PermissionRequested(PermissionPrompt),
    /// Tool execution started.
    ToolStarted {
        /// Turn identifier.
        turn: TurnId,
        /// Call identifier.
        call_id: CallId,
        /// Tool name.
        name: String,
        /// Human-readable plan summary.
        summary: String,
        /// Whether tool was started eagerly during model streaming.
        eager: bool,
    },
    /// Progress update reported by executing tool.
    ToolProgress {
        /// Turn identifier.
        turn: TurnId,
        /// Call identifier.
        call_id: CallId,
        /// Progress message.
        message: String,
        /// Completion fraction (0.0 ..= 1.0) with zero floats.
        fraction: Option<Decimal>,
    },
    /// Tool execution completed.
    ToolFinished {
        /// Turn identifier.
        turn: TurnId,
        /// Call identifier.
        call_id: CallId,
        /// Tool name.
        name: String,
        /// Execution outcome.
        outcome: ToolOutcome,
        /// Elapsed execution time in milliseconds.
        elapsed_ms: u64,
        /// Optional UI display payload.
        display: Option<ToolDisplay>,
    },
}
