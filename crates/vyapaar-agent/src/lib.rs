//! # 本場・脳 vyapaar-agent-agent
//!
//! Closed AI/LLM Decision Parser, Safety Harness, Prompt Audit Logger, and Replay Engine.
//!
//! Guarantees:
//! - LLMs cannot inject arbitrary fields: strictly parsed into closed [`vyapaar-core::Decision`].
//! - Pre-trade safety validations: universe bounds, strictly positive quantities, grounded price limits.
//! - Direct, mandatory integration with [`vyapaar_risk::RiskEngine`].
//! - Deterministic prompt & output logging with SHA256 checksums, seed tracking, and model version tags.
//! - Deterministic [`PromptReplayer`] enabling recorded LLM decision trails to be replayed through
//!   the event-driven backtester with research-live parity.

pub mod audit;
pub mod error;
pub mod parser;
pub mod replayer;

pub use audit::{LlmRecord, LlmTrailLogger, sha256_hex};
pub use error::{DecisionError, ReplayError};
pub use parser::DecisionParser;
pub use replayer::PromptReplayer;
pub use vyapaar_core::Decision;
