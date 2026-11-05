//! Shared tool types across decision and conversation surfaces.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Canonical tool specification for prompt assembly and provider schemas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolSpec {
    /// Canonical tool identifier (e.g. `market_quote`, `screener_scan`).
    pub name: String,
    /// Human-readable description explaining capabilities and arguments.
    pub description: String,
    /// JSON Schema defining the expected parameter structure.
    pub parameters: serde_json::Value,
}

impl ToolSpec {
    /// Create a new tool specification.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// A validated tool invocation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Target tool identifier.
    pub name: String,
    /// Arguments provided by the caller or model.
    pub arguments: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool invocation.
    pub fn new(name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            name: name.into(),
            arguments,
        }
    }
}

/// Structured outcome of tool execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Canonical name of the executing tool.
    pub tool: String,
    /// Whether the tool operation succeeded.
    pub ok: bool,
    /// Output payload (JSON data or error message).
    pub payload: serde_json::Value,
}

impl ToolOutput {
    /// Create a successful tool output.
    pub fn ok(tool: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            tool: tool.into(),
            ok: true,
            payload,
        }
    }

    /// Create a failed tool output.
    pub fn failed(tool: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            tool: tool.into(),
            ok: false,
            payload,
        }
    }

    /// Return true if the execution was an error.
    pub fn is_error(&self) -> bool {
        !self.ok
    }
}

/// Errors occurring during tool resolution or execution.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ToolError {
    /// Target tool was not found in the registry.
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    /// Invalid arguments provided for tool.
    #[error("invalid arguments for `{tool}`: {message}")]
    InvalidArguments {
        /// Tool name.
        tool: String,
        /// Description of validation failure.
        message: String,
    },
    /// Internal execution failure.
    #[error("tool `{0}` failed: {1}")]
    Execution(String, String),
    /// Permission denied by user policy.
    #[error("permission denied for `{0}`: {1}")]
    PermissionDenied(String, String),
    /// Tool execution timed out.
    #[error("tool `{0}` timed out")]
    Timeout(String),
}
