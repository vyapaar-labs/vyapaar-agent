#![allow(clippy::result_large_err, clippy::collapsible_if)]

//! # 本場・脳 vyapaar-agent-chat
//!
//! Conversational assistant engine, session persistence, turn state machine,
//! permission gating, and event streaming (§5, Task Phase A).
//!
//! Houses the open-ended conversation loop beside the ported autonomous decision engine,
//! adhering strictly to the NoFX untrusted AI principle (chat turns never produce a trade or
//! bypass the `DecisionValidator -> PriceBand -> RiskEngine` pipeline).

pub mod agent;
pub mod cancel;
pub mod error;
pub mod event;
pub mod headless;
pub mod permission;
pub mod session;
pub mod testing;
pub mod tool;
pub mod turn;

pub use agent::{ChatAgent, ChatAgentBuilder, ChatConfig};
pub use cancel::CancellationToken;
pub use error::{ChatError, ChatErrorView};
pub use event::{
    CallId, Notice, SessionEvent, ToolCallView, ToolDisplay, ToolOutcome, TurnId, TurnOutcome,
};
pub use headless::run_turn_headless;
pub use permission::{
    ConfirmOverride, PermissionDecision, PermissionPolicy, PermissionPrompt, PermissionResponder,
    ToolRisk, ToolTraits,
};
pub use session::{SessionActor, SessionEvents, SessionHandle, SessionSelector};
pub use testing::{FakeChatTool, InMemoryMemoryStore, InMemorySessionStore};
pub use tool::{ChatTool, PlanContext, ToolContext, ToolPlan, ToolRegistry, ToolSet};
pub use turn::{CancelPoint, TurnConfig, run_turn};
