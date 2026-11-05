//! In-memory test utilities and fake tool implementations for unit and integration testing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use vyapaar_agent_core::{
    MemoryId, MemoryProfile, MemoryQuery, MemoryStore, MessageSeq, NewMemory, NewSession,
    PersistedMessage, ProfileBudget, SaveOutcome, ScoredMemory, SessionFilter, SessionId,
    SessionInfo, SessionScope, SessionStore, StoreError, ToolError, ToolOutput, ToolSpec,
};

use crate::permission::{ConfirmOverride, ToolRisk, ToolTraits};
use crate::tool::{ChatTool, PlanContext, ToolContext, ToolPlan};

/// In-memory implementation of [`SessionStore`].
type MessageHistoryMap = HashMap<SessionId, Vec<(MessageSeq, PersistedMessage)>>;

#[derive(Default, Clone)]
pub struct InMemorySessionStore {
    sessions: Arc<Mutex<HashMap<SessionId, SessionInfo>>>,
    messages: Arc<Mutex<MessageHistoryMap>>,
}

impl InMemorySessionStore {
    /// Create a new in-memory session store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionStore for InMemorySessionStore {
    async fn create(&self, s: NewSession) -> Result<SessionId, StoreError> {
        let mut sessions = self.sessions.lock().unwrap();
        let info = SessionInfo {
            id: s.id,
            profile: s.profile,
            mode: s.mode,
            provider: s.provider,
            model: s.model,
            title: s.title,
            archived: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        sessions.insert(s.id, info);
        Ok(s.id)
    }

    async fn append(
        &self,
        id: SessionId,
        msgs: &[PersistedMessage],
    ) -> Result<Vec<MessageSeq>, StoreError> {
        let mut messages = self.messages.lock().unwrap();
        let list = messages.entry(id).or_default();
        let mut seqs = Vec::new();
        for msg in msgs {
            list.push((msg.seq, msg.clone()));
            seqs.push(msg.seq);
        }
        Ok(seqs)
    }

    async fn messages(
        &self,
        id: SessionId,
    ) -> Result<Vec<(MessageSeq, PersistedMessage)>, StoreError> {
        let messages = self.messages.lock().unwrap();
        Ok(messages.get(&id).cloned().unwrap_or_default())
    }

    async fn most_recent(&self, scope: &SessionScope) -> Result<Option<SessionInfo>, StoreError> {
        let sessions = self.sessions.lock().unwrap();
        let found = sessions
            .values()
            .filter(|s| s.profile == scope.profile && s.mode == scope.mode && !s.archived)
            .max_by_key(|s| s.updated_at)
            .cloned();
        Ok(found)
    }

    async fn list(&self, filter: SessionFilter) -> Result<Vec<SessionInfo>, StoreError> {
        let sessions = self.sessions.lock().unwrap();
        let mut list: Vec<SessionInfo> = sessions
            .values()
            .filter(|s| {
                if let Some(ref p) = filter.profile {
                    if &s.profile != p {
                        return false;
                    }
                }
                if let Some(m) = filter.mode {
                    if s.mode != m {
                        return false;
                    }
                }
                if let Some(archived) = filter.archived {
                    if s.archived != archived {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        list.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        if let Some(lim) = filter.limit {
            list.truncate(lim);
        }
        Ok(list)
    }

    async fn rename(&self, id: SessionId, title: &str) -> Result<(), StoreError> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(info) = sessions.get_mut(&id) {
            info.title = Some(title.to_string());
            info.updated_at = Utc::now();
            Ok(())
        } else {
            Err(StoreError::not_found("session not found"))
        }
    }

    async fn update_provider(
        &self,
        id: SessionId,
        provider: &str,
        model: &str,
    ) -> Result<(), StoreError> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(info) = sessions.get_mut(&id) {
            info.provider = provider.to_string();
            info.model = model.to_string();
            info.updated_at = Utc::now();
            Ok(())
        } else {
            Err(StoreError::not_found("session not found"))
        }
    }

    async fn truncate_after(&self, id: SessionId, seq: MessageSeq) -> Result<(), StoreError> {
        let mut messages = self.messages.lock().unwrap();
        if let Some(list) = messages.get_mut(&id) {
            list.retain(|(s, _)| *s <= seq);
        }
        Ok(())
    }
}

/// In-memory implementation of [`MemoryStore`].
#[derive(Default, Clone)]
pub struct InMemoryMemoryStore {
    version: Arc<Mutex<u64>>,
    memories: Arc<Mutex<HashMap<MemoryId, vyapaar_agent_core::MemoryEntry>>>,
}

impl InMemoryMemoryStore {
    /// Create new in-memory memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl MemoryStore for InMemoryMemoryStore {
    async fn save(&self, e: NewMemory) -> Result<SaveOutcome, StoreError> {
        let mut memories = self.memories.lock().unwrap();
        let mut ver = self.version.lock().unwrap();
        *ver += 1;

        // Content deduplication check
        for (id, existing) in memories.iter() {
            if existing.content.trim() == e.content.trim() {
                return Ok(SaveOutcome::Duplicate(*id));
            }
        }

        let new_id = (memories.len() as i64) + 1;
        let entry = vyapaar_agent_core::MemoryEntry {
            id: new_id,
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
        memories.insert(new_id, entry);
        Ok(SaveOutcome::Inserted(new_id))
    }

    async fn profile(&self, budget: ProfileBudget) -> Result<MemoryProfile, StoreError> {
        let memories = self.memories.lock().unwrap();
        let mut entries: Vec<vyapaar_agent_core::MemoryEntry> = memories
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
        let total = memories.len();
        entries.truncate(budget.max_entries);
        Ok(MemoryProfile {
            entries,
            total_count: total,
        })
    }

    async fn search(&self, q: &MemoryQuery) -> Result<Vec<ScoredMemory>, StoreError> {
        let memories = self.memories.lock().unwrap();
        let mut results = Vec::new();
        let q_lower = q.query.to_lowercase();

        for entry in memories.values() {
            if let Some(kind) = q.kind {
                if entry.kind != kind {
                    continue;
                }
            }
            if entry.content.to_lowercase().contains(&q_lower) {
                results.push(ScoredMemory {
                    entry: entry.clone(),
                    score: 1.0,
                });
            }
        }

        results.truncate(q.limit);
        Ok(results)
    }

    async fn forget(&self, id: MemoryId) -> Result<bool, StoreError> {
        let mut memories = self.memories.lock().unwrap();
        let mut ver = self.version.lock().unwrap();
        *ver += 1;
        Ok(memories.remove(&id).is_some())
    }

    async fn touch(&self, _ids: &[MemoryId]) -> Result<(), StoreError> {
        Ok(())
    }

    async fn version(&self) -> Result<u64, StoreError> {
        let ver = self.version.lock().unwrap();
        Ok(*ver)
    }
}

/// Configurable test tool.
pub struct FakeChatTool {
    spec: ToolSpec,
    traits: ToolTraits,
    result: Mutex<Result<Value, String>>,
}

impl FakeChatTool {
    /// Create a new fake tool with given name and risk.
    pub fn new(name: impl Into<String>, risk: ToolRisk) -> Self {
        let name = name.into();
        Self {
            spec: ToolSpec::new(
                name.clone(),
                format!("Fake test tool {name}"),
                serde_json::json!({ "type": "object" }),
            ),
            traits: ToolTraits::new(risk),
            result: Mutex::new(Ok(serde_json::json!({ "status": "ok" }))),
        }
    }

    /// Set user confirmation override.
    pub fn with_confirm(mut self, confirm: ConfirmOverride) -> Self {
        self.traits.confirm = confirm;
        self
    }

    /// Set response payload for subsequent executions.
    pub fn set_result(&self, res: Result<Value, String>) {
        *self.result.lock().unwrap() = res;
    }
}

#[async_trait]
impl ChatTool for FakeChatTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn traits(&self) -> ToolTraits {
        self.traits.clone()
    }

    fn plan(&self, _input: &Value, _ctx: &PlanContext<'_>) -> Result<ToolPlan, ToolError> {
        Ok(ToolPlan::new(format!("Run fake tool {}", self.spec.name)))
    }

    async fn execute(&self, _input: Value, _ctx: ToolContext) -> Result<ToolOutput, ToolError> {
        let res = self.result.lock().unwrap().clone();
        match res {
            Ok(payload) => Ok(ToolOutput::ok(&self.spec.name, payload)),
            Err(err_msg) => Err(ToolError::Execution(self.spec.name.clone(), err_msg)),
        }
    }
}
