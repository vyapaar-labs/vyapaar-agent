//! Ports (trait contracts) for persistence and external boundaries.
//!
//! Defined in `vyapaar-agent-core` so that `vyapaar-agent-*` crates can interact with
//! persistence and runtime state via dependency inversion without depending directly on
//! `vyapaar-runtime-*` crates (Rule R1, Tier 3A -> 3B inversion).

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Operational mode of the trading runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// Live execution with actual broker capital.
    Live,
    /// Paper execution with simulated order fills against live market feeds.
    Paper,
    /// Backtesting against historical market data.
    Backtest,
}

impl fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Live => write!(f, "live"),
            Self::Paper => write!(f, "paper"),
            Self::Backtest => write!(f, "backtest"),
        }
    }
}

/// Scope identifier for user sessions (profile name + runtime mode).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionScope {
    /// Account profile name.
    pub profile: String,
    /// Trading runtime mode.
    pub mode: RuntimeMode,
}

impl SessionScope {
    /// Create a new session scope.
    pub fn new(profile: impl Into<String>, mode: RuntimeMode) -> Self {
        Self {
            profile: profile.into(),
            mode,
        }
    }
}

/// Strongly-typed identifier for agent chat sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionId(pub uuid::Uuid);

impl SessionId {
    /// Generate a new UUIDv7 session ID (time-ordered).
    pub fn new() -> Self {
        Self(uuid::Uuid::now_v7())
    }

    /// Generate a random UUIDv4 session ID.
    pub fn new_v4() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Access inner UUID.
    pub const fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SessionId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        uuid::Uuid::parse_str(s).map(SessionId)
    }
}

impl From<uuid::Uuid> for SessionId {
    fn from(u: uuid::Uuid) -> Self {
        Self(u)
    }
}

/// Monotonically increasing sequence number of a message within a session.
pub type MessageSeq = u64;

/// Persisted chat message record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedMessage {
    /// Sequence number inside the session.
    pub seq: MessageSeq,
    /// Message role ("user", "assistant", "tool", or "system").
    pub role: String,
    /// Structured payload / content.
    pub content: serde_json::Value,
    /// Canonical request hash for assistant messages.
    pub request_hash: Option<String>,
    /// Recorded input token count, if available.
    pub input_tokens: Option<u32>,
    /// Recorded output token count, if available.
    pub output_tokens: Option<u32>,
    /// Timestamp when this message was committed.
    pub created_at: DateTime<Utc>,
}

/// Metadata needed to open / initialize a new session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSession {
    /// Session identifier.
    pub id: SessionId,
    /// Associated profile name.
    pub profile: String,
    /// Runtime mode.
    pub mode: RuntimeMode,
    /// LLM provider identifier (e.g. "anthropic", "openai", "local").
    pub provider: String,
    /// LLM model name.
    pub model: String,
    /// Optional human-readable session title.
    pub title: Option<String>,
}

/// Summary information for an existing chat session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session identifier.
    pub id: SessionId,
    /// Associated profile name.
    pub profile: String,
    /// Runtime mode.
    pub mode: RuntimeMode,
    /// LLM provider identifier.
    pub provider: String,
    /// LLM model name.
    pub model: String,
    /// Optional title.
    pub title: Option<String>,
    /// Whether this session has been archived.
    pub archived: bool,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
}

/// Query filter for listing sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFilter {
    /// Optional filter by profile.
    pub profile: Option<String>,
    /// Optional filter by mode.
    pub mode: Option<RuntimeMode>,
    /// Optional filter by archived status.
    pub archived: Option<bool>,
    /// Maximum number of records to return.
    pub limit: Option<usize>,
}

/// Categories of store failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum StoreErrorKind {
    /// Storage resource is busy or locked.
    #[error("store busy")]
    Busy,
    /// Target entity was not found.
    #[error("not found")]
    NotFound,
    /// Unique constraint or concurrency conflict.
    #[error("conflict")]
    Conflict,
    /// Data is corrupt or failed deserialization.
    #[error("data corrupt")]
    Corrupt,
    /// Database or storage service is unavailable.
    #[error("store unavailable")]
    Unavailable,
    /// Other unclassified error.
    #[error("other error")]
    Other,
}

/// Storage error wrapper for port implementations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{kind}: {message}")]
pub struct StoreError {
    /// Error category.
    pub kind: StoreErrorKind,
    /// Informational message.
    pub message: String,
}

impl StoreError {
    /// Create a new StoreError.
    pub fn new(kind: StoreErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Convenience constructor for NotFound.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StoreErrorKind::NotFound, message)
    }

    /// Convenience constructor for Conflict.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StoreErrorKind::Conflict, message)
    }

    /// Convenience constructor for Corrupt.
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::new(StoreErrorKind::Corrupt, message)
    }

    /// Convenience constructor for Busy.
    pub fn busy(message: impl Into<String>) -> Self {
        Self::new(StoreErrorKind::Busy, message)
    }

    /// Convenience constructor for Unavailable.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(StoreErrorKind::Unavailable, message)
    }

    /// Convenience constructor for Other.
    pub fn other(message: impl Into<String>) -> Self {
        Self::new(StoreErrorKind::Other, message)
    }
}

/// Asynchronous port for persisting agent conversation sessions and messages.
#[async_trait::async_trait]
pub trait SessionStore: Send + Sync + 'static {
    /// Create a new session row.
    async fn create(&self, s: NewSession) -> Result<SessionId, StoreError>;

    /// Append messages sequentially to a session.
    async fn append(
        &self,
        id: SessionId,
        msgs: &[PersistedMessage],
    ) -> Result<Vec<MessageSeq>, StoreError>;

    /// Retrieve all persisted messages for a session ordered by sequence.
    async fn messages(
        &self,
        id: SessionId,
    ) -> Result<Vec<(MessageSeq, PersistedMessage)>, StoreError>;

    /// Fetch the most recent active session matching the given scope.
    async fn most_recent(&self, scope: &SessionScope) -> Result<Option<SessionInfo>, StoreError>;

    /// List sessions matching an optional filter.
    async fn list(&self, filter: SessionFilter) -> Result<Vec<SessionInfo>, StoreError>;

    /// Rename session title.
    async fn rename(&self, id: SessionId, title: &str) -> Result<(), StoreError>;

    /// Update provider and model for a session.
    async fn update_provider(
        &self,
        id: SessionId,
        provider: &str,
        model: &str,
    ) -> Result<(), StoreError>;

    /// Truncate message history after a given sequence number.
    async fn truncate_after(&self, id: SessionId, seq: MessageSeq) -> Result<(), StoreError>;
}

/// Strongly-typed identifier for a user memory entry.
pub type MemoryId = i64;

/// Classification of durable memory items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Concrete factual assertion about user or environment.
    Fact,
    /// User preference (e.g. trading style, risk tolerance).
    Preference,
    /// Standing instruction (e.g. "never trade F&O").
    Instruction,
}

/// Source origin of a memory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOrigin {
    /// Inferred or saved by the LLM assistant.
    Model,
    /// Explicitly stated by the human user.
    User,
    /// Imported from configuration or external seed.
    Import,
}

/// Stored user memory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique memory id.
    pub id: MemoryId,
    /// Memory classification.
    pub kind: MemoryKind,
    /// Text content (up to 2000 characters).
    pub content: String,
    /// Associated classification tags.
    pub tags: Vec<String>,
    /// Importance score (0 to 3).
    pub importance: u8,
    /// Whether this entry is pinned into the system prompt prefix.
    pub pinned: bool,
    /// Origin of this entry.
    pub origin: MemoryOrigin,
    /// Session in which this memory was created, if any.
    pub source_session: Option<SessionId>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Optional expiration timestamp.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Input payload to record a new memory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewMemory {
    /// Memory classification.
    pub kind: MemoryKind,
    /// Content string.
    pub content: String,
    /// Tags array.
    pub tags: Vec<String>,
    /// Importance rating (0..=3).
    pub importance: u8,
    /// Whether pinned in prompt prefix.
    pub pinned: bool,
    /// Origin.
    pub origin: MemoryOrigin,
    /// Originating session.
    pub source_session: Option<SessionId>,
    /// Expiration timestamp.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Scored memory query hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredMemory {
    /// Underlying memory entry.
    pub entry: MemoryEntry,
    /// Normalized relevance score (0.0 ..= 1.0).
    pub score: f64,
}

/// Budget constraint for stable memory profile rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileBudget {
    /// Maximum token estimation for profile memories.
    pub max_tokens: usize,
    /// Maximum count of entries.
    pub max_entries: usize,
}

impl Default for ProfileBudget {
    fn default() -> Self {
        Self {
            max_tokens: 1024,
            max_entries: 32,
        }
    }
}

/// Stable memory profile loaded once per session open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProfile {
    /// Entries to place in the cached prompt prefix.
    pub entries: Vec<MemoryEntry>,
    /// Total count of memories in store.
    pub total_count: usize,
}

/// Query parameters for relevance search across memories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Free-form query text.
    pub query: String,
    /// Maximum entries to return.
    pub limit: usize,
    /// Minimum similarity score filter.
    pub min_score: Option<rust_decimal::Decimal>,
    /// Optional kind filter.
    pub kind: Option<MemoryKind>,
}

/// Result of attempting to save a memory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveOutcome {
    /// Successfully inserted as a new entry.
    Inserted(MemoryId),
    /// Deduplicated against existing identical content.
    Duplicate(MemoryId),
}

/// Asynchronous port for persistent user memory storage.
#[async_trait::async_trait]
pub trait MemoryStore: Send + Sync + 'static {
    /// Save a new memory entry with content hash deduplication.
    async fn save(&self, e: NewMemory) -> Result<SaveOutcome, StoreError>;

    /// Fetch the stable profile set for prompt caching.
    async fn profile(&self, budget: ProfileBudget) -> Result<MemoryProfile, StoreError>;

    /// Perform relevance search across memory content.
    async fn search(&self, q: &MemoryQuery) -> Result<Vec<ScoredMemory>, StoreError>;

    /// Hard delete a memory entry.
    async fn forget(&self, id: MemoryId) -> Result<bool, StoreError>;

    /// Bump last_used_at and use_count for accessed memories.
    async fn touch(&self, ids: &[MemoryId]) -> Result<(), StoreError>;

    /// Monotonic version counter, incremented whenever memory content changes.
    async fn version(&self) -> Result<u64, StoreError>;
}
