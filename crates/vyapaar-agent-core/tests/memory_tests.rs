//! Integration tests for the local memory engine (`vyapaar-agent-core::memory`).
//!
//! All tests live here (parallel to `src/`). A test double implements the
//! [`MemoryStore`] port so the engine stays free of concrete storage.

use std::collections::HashMap;
use std::sync::RwLock;

use chrono::Utc;
use rust_decimal::Decimal;

use vyapaar_agent_core::memory::{MemoryCategory, MemoryEngine, MemoryFact};
use vyapaar_agent_core::ports::{
    MemoryEntry, MemoryId, MemoryKind, MemoryOrigin, MemoryProfile, MemoryQuery, MemoryStore,
    NewMemory, ProfileBudget, SaveOutcome, ScoredMemory, StoreError,
};

/// Test double implementing [`MemoryStore`].
#[derive(Default)]
struct TestStore {
    memories: RwLock<HashMap<MemoryId, MemoryEntry>>,
    next_id: RwLock<MemoryId>,
    version: RwLock<u64>,
}

#[async_trait::async_trait]
impl MemoryStore for TestStore {
    async fn save(&self, e: NewMemory) -> Result<SaveOutcome, StoreError> {
        for existing in self.memories.read().unwrap().values() {
            if existing.content == e.content {
                return Ok(SaveOutcome::Duplicate(existing.id));
            }
        }
        let mut next = self.next_id.write().unwrap();
        let id = *next;
        *next += 1;
        let entry = MemoryEntry {
            id,
            kind: e.kind,
            content: e.content,
            tags: e.tags,
            importance: e.importance,
            pinned: e.pinned,
            origin: e.origin,
            source_session: e.source_session,
            created_at: Utc::now(),
            expires_at: e.expires_at,
        };
        self.memories.write().unwrap().insert(id, entry);
        *self.version.write().unwrap() += 1;
        Ok(SaveOutcome::Inserted(id))
    }

    async fn profile(&self, budget: ProfileBudget) -> Result<MemoryProfile, StoreError> {
        let memories = self.memories.read().unwrap();
        let mut entries: Vec<MemoryEntry> = memories
            .values()
            .filter(|e| e.pinned || e.importance >= 2)
            .cloned()
            .collect();
        entries.sort_by_key(|e| {
            (
                std::cmp::Reverse(e.pinned),
                std::cmp::Reverse(e.importance),
                e.id,
            )
        });
        entries.truncate(budget.max_entries);
        Ok(MemoryProfile {
            entries,
            total_count: memories.len(),
        })
    }

    async fn search(&self, q: &MemoryQuery) -> Result<Vec<ScoredMemory>, StoreError> {
        let memories = self.memories.read().unwrap();
        let q_lower = q.query.to_lowercase();
        let mut results: Vec<ScoredMemory> = memories
            .values()
            .filter(|e| {
                if let Some(kind) = q.kind {
                    e.kind == kind
                } else {
                    true
                }
            })
            .filter(|e| e.content.to_lowercase().contains(&q_lower))
            .map(|e| ScoredMemory {
                entry: e.clone(),
                score: 1.0,
            })
            .collect();
        results.truncate(q.limit);
        Ok(results)
    }

    async fn forget(&self, id: MemoryId) -> Result<bool, StoreError> {
        *self.version.write().unwrap() += 1;
        Ok(self.memories.write().unwrap().remove(&id).is_some())
    }

    async fn touch(&self, _ids: &[MemoryId]) -> Result<(), StoreError> {
        Ok(())
    }

    async fn version(&self) -> Result<u64, StoreError> {
        Ok(*self.version.read().unwrap())
    }
}

fn engine() -> MemoryEngine {
    MemoryEngine::new(std::sync::Arc::new(TestStore::default()))
}

#[tokio::test]
async fn test_remember_inserts_and_returns_id() {
    let eng = engine();
    let id = eng
        .remember(
            "Trader avoids PSU banks due to policy volatility",
            MemoryCategory::Preference,
            Decimal::new(9, 1), // 0.9
        )
        .await
        .expect("remember");
    let id2 = eng
        .remember(
            "Trader avoids PSU banks due to policy volatility",
            MemoryCategory::Preference,
            Decimal::new(9, 1),
        )
        .await
        .expect("remember duplicate");
    // Content dedup returns the existing id.
    assert_eq!(id2, id);
}

#[tokio::test]
async fn test_remember_maps_category_and_confidence() {
    let eng = engine();
    let _ = eng
        .remember(
            "Never trade F&O",
            MemoryCategory::RiskRule,
            Decimal::new(10, 1), // 1.0 -> importance 3
        )
        .await
        .expect("remember");

    let hits = eng.search("Never trade F&O", 5).await.expect("search");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entry.kind, MemoryKind::Instruction);
    assert_eq!(hits[0].entry.importance, 3);
    assert_eq!(hits[0].entry.origin, MemoryOrigin::Model);
}

#[tokio::test]
async fn test_search_filters_and_limits() {
    let eng = engine();
    let _ = eng
        .remember(
            "NIFTY weekly options expire Thursday",
            MemoryCategory::InstrumentFact,
            Decimal::ONE,
        )
        .await
        .expect("remember");
    let _ = eng
        .remember(
            "PSU banks are volatile",
            MemoryCategory::Preference,
            Decimal::new(5, 1),
        )
        .await
        .expect("remember");

    let hits = eng.search("NIFTY", 5).await.expect("search");
    assert_eq!(hits.len(), 1);
    assert!(hits[0].entry.content.contains("NIFTY"));

    // Limit respected.
    let hits = eng.search("banks", 1).await.expect("search");
    assert!(hits.len() <= 1);
}

#[tokio::test]
async fn test_distill_tick_persists_and_dedups() {
    let eng = engine();
    let candidates = vec![
        MemoryFact {
            id: 0,
            content: "Distilled fact one".to_string(),
            category: MemoryCategory::General,
            confidence: Decimal::new(8, 1),
            created_at: Utc::now().to_rfc3339(),
        },
        MemoryFact {
            id: 0,
            content: "Distilled fact one".to_string(), // duplicate content
            category: MemoryCategory::General,
            confidence: Decimal::new(8, 1),
            created_at: Utc::now().to_rfc3339(),
        },
    ];
    let outcomes = eng.distill_tick(&candidates).await.expect("distill");
    assert_eq!(outcomes.len(), 2);
    assert!(matches!(outcomes[0], SaveOutcome::Inserted(_)));
    assert!(matches!(outcomes[1], SaveOutcome::Duplicate(_)));
}

#[tokio::test]
async fn test_confidence_zero_floats() {
    // Confidence is Decimal, never f64.
    let c: Decimal = Decimal::new(75, 2); // 0.75
    assert_eq!(c, Decimal::new(75, 2));
    let eng = engine();
    let _id = eng
        .remember("x", MemoryCategory::General, c)
        .await
        .expect("remember");
    let hits = eng.search("x", 5).await.expect("search");
    assert_eq!(hits[0].entry.importance, 2); // 0.75 * 3 = 2.25 -> round 2
}

#[tokio::test]
async fn test_profile_budget() {
    let eng = engine();
    let _ = eng
        .remember(
            "pinned important fact",
            MemoryCategory::Preference,
            Decimal::ONE,
        )
        .await
        .expect("remember");
    // Not pinned and we don't set importance via remember (it uses confidence=1.0 -> importance 3).
    let profile = eng
        .profile(ProfileBudget::default())
        .await
        .expect("profile");
    assert!(profile.total_count >= 1);
}
