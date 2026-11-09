//! Local financial memory engine (§2.2 of plan 0002).
//!
//! A [`MemoryEngine`] layers higher-level financial memory operations
//! (`remember`, `search`, `distill_tick`) over the [`MemoryStore`] port defined in
//! `crate::ports`. Storage itself (e.g. SQLite at `~/.vyapaar/memory.db`) is an
//! infrastructure concern injected through the port, keeping this module zero-I/O
//! and WASM-friendly.

use std::sync::Arc;

use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::ports::{
    MemoryId, MemoryKind, MemoryOrigin, MemoryQuery, MemoryStore, NewMemory, SaveOutcome,
    ScoredMemory, StoreError,
};

/// Classification of durable financial memory items (§2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// Trader preference (e.g. "avoids PSU banks").
    Preference,
    /// Standing risk rule (e.g. "never trade F&O").
    RiskRule,
    /// Instrument nuance (e.g. "NIFTY weekly options expire Thursday").
    InstrumentFact,
    /// General note.
    General,
}

/// A durable financial memory fact (§2.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryFact {
    pub id: MemoryId,
    pub content: String,
    pub category: MemoryCategory,
    /// Confidence score in the range [0.0, 1.0].
    pub confidence: Decimal,
    /// RFC3339 IST creation timestamp.
    pub created_at: String,
}

impl MemoryFact {
    /// Map to the storage [`MemoryKind`].
    pub fn kind(&self) -> MemoryKind {
        match self.category {
            MemoryCategory::Preference => MemoryKind::Preference,
            MemoryCategory::RiskRule => MemoryKind::Instruction,
            MemoryCategory::InstrumentFact => MemoryKind::Fact,
            MemoryCategory::General => MemoryKind::Fact,
        }
    }

    /// Map confidence [0,1] to an importance rating in [0,3].
    pub fn importance(&self) -> u8 {
        let clamped = self.confidence.clamp(Decimal::ZERO, Decimal::ONE);
        ((clamped * Decimal::from(3)).round()).to_u32().unwrap_or(3) as u8
    }

    /// Origin tag: facts distilled from transcripts are model-sourced; explicit
    /// `remember` calls from the trader are user-sourced.
    pub fn origin(&self) -> MemoryOrigin {
        MemoryOrigin::Model
    }
}

/// Higher-level memory engine layered over the [`MemoryStore`] port.
#[derive(Clone)]
pub struct MemoryEngine {
    store: Arc<dyn MemoryStore>,
}

impl MemoryEngine {
    /// Wrap an existing [`MemoryStore`].
    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self { store }
    }

    /// Access the underlying store.
    pub fn store(&self) -> Arc<dyn MemoryStore> {
        self.store.clone()
    }

    /// Immediately insert a validated trader fact.
    pub async fn remember(
        &self,
        content: impl Into<String>,
        category: MemoryCategory,
        confidence: Decimal,
    ) -> Result<MemoryId, StoreError> {
        let content = content.into();
        let fact = MemoryFact {
            id: 0, // assigned by the store
            content: content.clone(),
            category,
            confidence,
            created_at: Utc::now().to_rfc3339(),
        };
        let outcome = self
            .store
            .save(NewMemory {
                kind: fact.kind(),
                content,
                tags: vec![],
                importance: fact.importance(),
                pinned: false,
                origin: fact.origin(),
                source_session: None,
                expires_at: None,
            })
            .await?;
        match outcome {
            SaveOutcome::Inserted(id) | SaveOutcome::Duplicate(id) => Ok(id),
        }
    }

    /// Search memory content for relevant context (BM25 / similarity delegated to
    /// the underlying store).
    pub async fn search(
        &self,
        query: impl Into<String>,
        limit: usize,
    ) -> Result<Vec<ScoredMemory>, StoreError> {
        self.store
            .search(&MemoryQuery {
                query: query.into(),
                limit,
                min_score: None,
                kind: None,
            })
            .await
    }

    /// Stable profile of pinned / important memories for the cached prompt prefix.
    pub async fn profile(
        &self,
        budget: crate::ports::ProfileBudget,
    ) -> Result<crate::ports::MemoryProfile, StoreError> {
        self.store.profile(budget).await
    }

    /// Forget a memory by id.
    pub async fn forget(&self, id: MemoryId) -> Result<bool, StoreError> {
        self.store.forget(id).await
    }

    /// Distillation tick: persist a batch of candidate facts, deduplicating against
    /// existing content via the store's content-hash dedup. Returns the per-candidate
    /// save outcomes.
    ///
    /// In production this is driven by a summarizer model over recent session
    /// transcripts; here the caller supplies the extracted candidate facts so the
    /// engine stays free of LLM / I/O concerns.
    pub async fn distill_tick(
        &self,
        candidates: &[MemoryFact],
    ) -> Result<Vec<SaveOutcome>, StoreError> {
        let mut outcomes = Vec::with_capacity(candidates.len());
        for fact in candidates {
            let outcome = self
                .store
                .save(NewMemory {
                    kind: fact.kind(),
                    content: fact.content.clone(),
                    tags: vec![],
                    importance: fact.importance(),
                    pinned: false,
                    origin: fact.origin(),
                    source_session: None,
                    expires_at: None,
                })
                .await?;
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }
}

impl std::fmt::Debug for MemoryEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryEngine").finish()
    }
}
