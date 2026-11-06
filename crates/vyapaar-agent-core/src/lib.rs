#![allow(clippy::result_large_err, clippy::collapsible_if)]

//! # 本場・脳 vyapaar-agent-core
//!
//! Core domain traits, closed decision parser, grounding checks, and verified indicator catalog.
//!
//! # Core Capabilities
//! - Closed decision grammar parser with `serde(deny_unknown_fields)` enforcement.
//! - Pre-trade price grounding checks against instrument [`vyapaar-core::PriceBand`] and price collars.
//! - Mandatory SEBI `algo_id` validation (`^[A-Z0-9_]{4,32}$`).
//! - Integration with [`vyapaar_risk::RiskEngine`].
//! - Verified mathematical [`IndicatorCatalog`] preventing hallucinations.
//! - Deterministic prompt hashing, structured section assembly, and template rendering.
//! - Persistence ports (`SessionStore`, `MemoryStore`) decoupled from concrete runtimes.

pub mod decision;
pub mod error;
pub mod indicators;
pub mod ports;
pub mod prompt;
pub mod result;
pub mod tool;

pub use decision::{
    DecisionParser, SEBI_ALGO_ID_REGEX, extract_json_payload, is_valid_sebi_algo_id,
};
pub use error::{CatalogError, DecisionError};
pub use indicators::{
    IndicatorCatalog, IndicatorCategory, IndicatorInvocation, IndicatorSpec, ParameterSpec,
    StrategySpec, validate_spec_indicators,
};
pub use ports::{
    MemoryEntry, MemoryId, MemoryKind, MemoryOrigin, MemoryProfile, MemoryQuery, MemoryStore,
    MessageSeq, NewMemory, NewSession, PersistedMessage, ProfileBudget, RuntimeMode, SaveOutcome,
    ScoredMemory, SessionFilter, SessionId, SessionInfo, SessionScope, SessionStore, StoreError,
    StoreErrorKind,
};
pub use prompt::{
    DefaultPromptBuilder, Placement, PromptAssembly, PromptBuilder, PromptHash, PromptSection,
    PromptTemplate, compute_prompt_hash, default_assemble, hash_prompt_hex, sha256_hex,
};
pub use result::{DisplayBlock, ExecutionMeta, MetricItem, ResultStatus, VyapaarResultEnvelope};
pub use tool::{ToolCall, ToolError, ToolOutput, ToolSpec};
