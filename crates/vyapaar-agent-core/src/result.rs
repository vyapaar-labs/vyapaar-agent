//! Universal Result Protocol (`vyapaar/result/v1`).
//!
//! Every quantitative tool in `vyapaar-agent-mcp` and `vyapaar-agent-chat-tools`
//! returns the same structured envelope: a schema tag, a status, execution
//! metadata, a machine-readable `data` payload, and typed visual
//! [`DisplayBlock`] presentation blocks. The envelope is the single rendering
//! contract for tearsheets, metric grids, tables, and charts across MCP
//! clients (Claude Desktop, Cursor, Gemini CLI, internal chat loops).

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Status of a tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    /// Execution completed successfully.
    Ok,
    /// Execution failed; `data` carries the error payload.
    Error,
}

/// Execution metadata attached to every result envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExecutionMeta {
    /// Wall-clock elapsed time in milliseconds.
    pub elapsed_ms: u64,
    /// RFC3339 timestamp rendered in Asia/Kolkata (IST).
    pub timestamp_ist: String,
    /// Engine crate version that produced the result.
    pub version: String,
    /// Numeric precision contract, always `"exact_decimal_zero_floats"`.
    pub precision: String,
}

/// A single metric item inside a [`DisplayBlock::MetricGrid`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MetricItem {
    /// Metric label (e.g. "Sharpe").
    pub label: String,
    /// Formatted value (e.g. "1.42").
    pub value: String,
}

/// Typed visual presentation block for MCP clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DisplayBlock {
    /// A titled table of string rows.
    Table {
        /// Table title.
        title: String,
        /// Column header names.
        columns: Vec<String>,
        /// Row cells, one inner vector per row.
        rows: Vec<Vec<String>>,
    },
    /// A grid of named metric values.
    MetricGrid {
        /// Metric items.
        items: Vec<MetricItem>,
    },
    /// A chart reference into the ephemeral data-reference store.
    Chart {
        /// Chart title.
        title: String,
        /// Chart kind: `"candlestick"`, `"line"`, or `"sparkline"`.
        chart_type: String,
        /// `$vyapaar/ref/...` URI of the underlying series.
        series_ref: String,
    },
    /// A text alert of `"info"`, `"warning"`, or `"critical"` severity.
    Alert {
        /// Alert severity level.
        level: String,
        /// Human-readable alert message.
        message: String,
    },
}

/// The universal result envelope (`vyapaar/result/v1`) returned by every tool.
///
/// `T` is the exact typed machine-readable payload for the tool; presentation
/// always flows through [`DisplayBlock`], and large series are referenced
/// (not inlined) via `refs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VyapaarResultEnvelope<T> {
    /// Always `"vyapaar/result/v1"`.
    pub schema: String,
    /// Execution status.
    pub status: ResultStatus,
    /// Execution metadata.
    pub meta: ExecutionMeta,
    /// Natural-language one-line summary.
    pub summary: String,
    /// Exact typed machine-readable JSON payload.
    pub data: T,
    /// Typed visual presentation blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<DisplayBlock>,
    /// Reference URIs for large datasets.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub refs: HashMap<String, String>,
}

impl<T> VyapaarResultEnvelope<T> {
    /// Create a successful envelope with the given summary and typed payload.
    pub fn ok(summary: impl Into<String>, data: T) -> Self {
        Self {
            schema: "vyapaar/result/v1".to_string(),
            status: ResultStatus::Ok,
            meta: default_meta(),
            summary: summary.into(),
            data,
            blocks: Vec::new(),
            refs: HashMap::new(),
        }
    }

    /// Create an error envelope carrying a string error payload.
    pub fn error(summary: impl Into<String>, message: impl Into<String>) -> Self
    where
        T: From<String>,
    {
        Self {
            schema: "vyapaar/result/v1".to_string(),
            status: ResultStatus::Error,
            meta: default_meta(),
            summary: summary.into(),
            data: T::from(message.into()),
            blocks: Vec::new(),
            refs: HashMap::new(),
        }
    }

    /// Append a typed presentation block.
    pub fn with_block(mut self, block: DisplayBlock) -> Self {
        self.blocks.push(block);
        self
    }

    /// Append multiple typed presentation blocks.
    pub fn with_blocks(mut self, blocks: impl IntoIterator<Item = DisplayBlock>) -> Self {
        self.blocks.extend(blocks);
        self
    }

    /// Insert a reference URI for a large dataset.
    pub fn with_ref(mut self, key: impl Into<String>, uri: impl Into<String>) -> Self {
        self.refs.insert(key.into(), uri.into());
        self
    }
}

/// Default execution metadata for a freshly-created envelope.
fn default_meta() -> ExecutionMeta {
    ExecutionMeta {
        elapsed_ms: 0,
        timestamp_ist: String::new(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        precision: "exact_decimal_zero_floats".to_string(),
    }
}
