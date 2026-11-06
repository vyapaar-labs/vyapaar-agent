//! Snapshot source port for lifting pure tools (§4.1, §4.4).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ports::error::PortError;

/// Pre-execution snapshot needs requested by a pure tool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotNeeds {
    /// Tool identifiers whose pre-requisite state is being collected.
    pub tools: Vec<String>,
}

impl SnapshotNeeds {
    /// Create needs for a single tool.
    pub fn for_tool(name: impl Into<String>) -> Self {
        Self {
            tools: vec![name.into()],
        }
    }
}

/// Live snapshot data used to ground pure tools (§4.1, §4.4).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SnapshotData {
    /// Effective snapshot timestamp.
    pub as_of: DateTime<Utc>,
    /// Screener universe symbols if queried.
    pub universe: Option<Vec<String>>,
    /// Current market regime classification label if queried.
    pub regime: Option<String>,
    /// Pre-trade risk metrics table (e.g. "price_band", "kill_switch", "otr").
    pub risk_metrics: HashMap<String, serde_json::Value>,
}

/// Snapshot source port implemented by runtime (§4.4).
#[async_trait]
pub trait SnapshotSource: Send + Sync + 'static {
    /// Build an AgentSnapshot representation from live runtime state.
    async fn snapshot(&self, needs: &SnapshotNeeds) -> Result<SnapshotData, PortError>;
}
