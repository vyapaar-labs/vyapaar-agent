#![allow(clippy::collapsible_if, clippy::needless_borrows_for_generic_args)]

//! # 本場・脳 vyapaar-agent-replay
//!
//! Cryptographic prompt and decision replay harness.
//!
//! Guarantees:
//! - Append-only audit record schema capturing full context, seed, exact Decimal temperature,
//!   and SHA-256 prompt hash.
//! - Cryptographic hash chaining linking sequential LLM inference turns for SEBI regulatory compliance.
//! - Deterministic [`PromptReplayer`] with SQLite WAL and JSONL prompt caches keyed by `prompt_sha256`.
//! - Interception of LLM queries during offline backtesting: zero latency and zero API cost.
//! - Research-live parity verification.
//! - Canonical request hashing helper for conversational assistant audit provenance.

pub mod audit;
pub mod cache;
pub mod error;
pub mod fixture;
pub mod replayer;

pub use audit::{
    AuditChainLedger, ChainedLlmAuditRecord, LlmAuditRecord, compute_record_hash, hash_file,
    hash_request, verify_hash_chain,
};
pub use cache::{InMemoryPromptCache, JsonlPromptCache, PromptCache, SqlitePromptCache};
pub use error::ReplayError;
pub use fixture::{FixtureWorkload, OmnirouteFixture, OmnirouteFixtureSet};
pub use replayer::{PromptReplayer, ReplayMode};
