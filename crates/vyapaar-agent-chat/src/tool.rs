//! Chat tool contract, execution context, and tool registry.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::Value;
use tokio::sync::mpsc;
use vyapaar_agent_core::{SessionId, ToolError, ToolOutput, ToolSpec};

use crate::event::{CallId, SessionEvent, TurnId};
use crate::permission::{ToolRisk, ToolTraits};

/// Validation and planning context passed to `ChatTool::plan`.
#[derive(Debug, Clone)]
pub struct PlanContext<'a> {
    /// Active session identifier if known.
    pub session_id: Option<&'a SessionId>,
}

impl<'a> PlanContext<'a> {
    /// Create new plan context.
    pub fn new(session_id: Option<&'a SessionId>) -> Self {
        Self { session_id }
    }
}

/// Pre-execution plan summary produced by `ChatTool::plan`.
#[derive(Debug, Clone, Default)]
pub struct ToolPlan {
    /// Short human-readable summary for permission prompts and logs.
    pub summary: String,
    /// Dynamic risk override if computed dynamically.
    pub risk_override: Option<ToolRisk>,
}

impl ToolPlan {
    /// Create a plan with a descriptive summary.
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            risk_override: None,
        }
    }
}

/// Execution context provided to `ChatTool::execute`.
#[derive(Clone)]
pub struct ToolContext {
    /// Active session ID.
    pub session_id: Option<SessionId>,
    /// Turn ID.
    pub turn_id: TurnId,
    /// Call ID.
    pub call_id: CallId,
    /// Channel to emit live events (such as ToolProgress).
    pub event_tx: mpsc::Sender<SessionEvent>,
}

impl ToolContext {
    /// Create a new execution context.
    pub fn new(
        session_id: Option<SessionId>,
        turn_id: TurnId,
        call_id: CallId,
        event_tx: mpsc::Sender<SessionEvent>,
    ) -> Self {
        Self {
            session_id,
            turn_id,
            call_id,
            event_tx,
        }
    }

    /// Report incremental progress to UI.
    pub async fn report_progress(&self, message: impl Into<String>, fraction: Option<Decimal>) {
        let _ = self
            .event_tx
            .send(SessionEvent::ToolProgress {
                turn: self.turn_id,
                call_id: self.call_id.clone(),
                message: message.into(),
                fraction,
            })
            .await;
    }
}

/// Conversational tool interface capable of asynchronous I/O (§4.1).
#[async_trait]
pub trait ChatTool: Send + Sync + 'static {
    /// Tool specification (name, description, schema).
    fn spec(&self) -> &ToolSpec;

    /// Execution traits (risk level, concurrency safety, timeout).
    fn traits(&self) -> ToolTraits {
        ToolTraits::default()
    }

    /// Synchronous validation and plan generation before execution.
    fn plan(&self, input: &Value, ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        let _ = (input, ctx);
        Ok(ToolPlan::new(format!("Execute `{}`", self.spec().name)))
    }

    /// Execute the tool with given input arguments.
    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput, ToolError>;
}

/// Registry storing available chat tools for an agent.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ChatTool>>,
}

impl ToolRegistry {
    /// Create a new empty tool registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. Shadows any previously registered tool with the same name.
    pub fn register(&mut self, tool: Arc<dyn ChatTool>) {
        self.tools.insert(tool.spec().name.clone(), tool);
    }

    /// Register a batch of tools.
    pub fn register_all(&mut self, tools: Vec<Arc<dyn ChatTool>>) {
        for tool in tools {
            self.register(tool);
        }
    }

    /// Retrieve a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ChatTool>> {
        self.tools.get(name).cloned()
    }

    /// Return all tool specifications.
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut list: Vec<ToolSpec> = self.tools.values().map(|t| t.spec().clone()).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Create an immutable snapshot of registered tools for turn execution.
    pub fn snapshot(&self) -> ToolSet {
        ToolSet {
            tools: self.tools.clone(),
        }
    }
}

/// Immutable snapshot of tools pinned for a conversation turn.
#[derive(Clone)]
pub struct ToolSet {
    tools: HashMap<String, Arc<dyn ChatTool>>,
}

impl ToolSet {
    /// Retrieve tool from snapshot.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ChatTool>> {
        self.tools.get(name).cloned()
    }

    /// Return tool specifications in snapshot.
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut list: Vec<ToolSpec> = self.tools.values().map(|t| t.spec().clone()).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}
