//! Error types for domain read ports.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vyapaar_agent_core::ToolError;

/// Classification of domain port failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum PortErrorKind {
    /// Requested entity or data was not found.
    #[error("not found")]
    NotFound,
    /// Upstream data source is temporarily unavailable.
    #[error("unavailable")]
    Unavailable,
    /// Invalid arguments passed to port.
    #[error("invalid argument")]
    InvalidArgument,
    /// Read operation timed out.
    #[error("timeout")]
    Timeout,
    /// Internal failure.
    #[error("internal")]
    Internal,
}

/// Unified error returned across all chat tool read ports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[error("{kind}: {message}")]
pub struct PortError {
    /// Port error classification.
    pub kind: PortErrorKind,
    /// Detailed diagnostic message.
    pub message: String,
}

impl PortError {
    /// Create a new port error.
    pub fn new(kind: PortErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Construct a NotFound error.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(PortErrorKind::NotFound, message)
    }

    /// Construct an Unavailable error.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(PortErrorKind::Unavailable, message)
    }

    /// Construct an InvalidArgument error.
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(PortErrorKind::InvalidArgument, message)
    }

    /// Construct a Timeout error.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(PortErrorKind::Timeout, message)
    }

    /// Construct an Internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(PortErrorKind::Internal, message)
    }

    /// Convert into a model-facing ToolError.
    pub fn into_tool_error(self, tool_name: &str) -> ToolError {
        match self.kind {
            PortErrorKind::InvalidArgument => ToolError::InvalidArguments {
                tool: tool_name.to_string(),
                message: self.message,
            },
            PortErrorKind::Timeout => ToolError::Timeout(tool_name.to_string()),
            _ => ToolError::Execution(tool_name.to_string(), self.to_string()),
        }
    }
}
