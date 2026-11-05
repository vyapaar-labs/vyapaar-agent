//! Error types for vyapaar-agent-chat.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vyapaar_agent_core::{MessageSeq, StoreError, ToolError};
use vyapaar_agent_llm::LlmError;

/// Core error type for conversation assistant operations.
#[derive(Debug, Error)]
pub enum ChatError {
    /// Underlying session or memory persistence failure.
    #[error("store error: {0}")]
    Store(#[from] StoreError),

    /// LLM provider failure or transport error.
    #[error("llm error: {0}")]
    Llm(#[from] LlmError),

    /// Tool invocation or execution error.
    #[error("tool error: {0}")]
    Tool(#[from] ToolError),

    /// Corrupted message history encountered during load/replay.
    #[error("corrupt history at seq {seq}")]
    CorruptHistory {
        /// Sequence number of invalid message.
        seq: MessageSeq,
    },

    /// The conversation session has been closed or terminated.
    #[error("session is closed")]
    SessionClosed,

    /// Another turn is currently in progress for this session.
    #[error("turn is already in progress")]
    TurnInProgress,

    /// Turn was cancelled by user or timeout.
    #[error("operation cancelled")]
    Cancelled,

    /// Maximum tool round limit reached.
    #[error("round limit reached ({0} rounds)")]
    RoundLimit(u32),

    /// Permission prompt was denied.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// Invalid configuration supplied.
    #[error("configuration error: {0}")]
    Config(String),

    /// Internal agent error.
    #[error("internal chat error: {0}")]
    Internal(String),
}

/// Serializable view of a chat error for UI presentation and event streams.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatErrorView {
    /// Error category identifier.
    pub kind: String,
    /// Human-readable message.
    pub message: String,
}

impl From<&ChatError> for ChatErrorView {
    fn from(err: &ChatError) -> Self {
        let kind = match err {
            ChatError::Store(_) => "store",
            ChatError::Llm(_) => "llm",
            ChatError::Tool(_) => "tool",
            ChatError::CorruptHistory { .. } => "corrupt_history",
            ChatError::SessionClosed => "session_closed",
            ChatError::TurnInProgress => "turn_in_progress",
            ChatError::Cancelled => "cancelled",
            ChatError::RoundLimit(_) => "round_limit",
            ChatError::PermissionDenied(_) => "permission_denied",
            ChatError::Config(_) => "config",
            ChatError::Internal(_) => "internal",
        };
        Self {
            kind: kind.to_string(),
            message: err.to_string(),
        }
    }
}
