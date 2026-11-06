//! LLM provider abstraction.
//!
//! Defines the trait contract for LLM providers (Claude, Gemini, OpenAI, etc.),
//! streaming conversational chat events, and deterministic replay audit records.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(unsafe_code)]
#![allow(clippy::collapsible_if)]

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::Stream;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use vyapaar_agent_core::ToolSpec;
use vyapaar_core::Decision;

pub mod config;
pub mod openai_compat;

pub use config::{LlmConfig, llm_config_from_env, llm_config_from_map, openai_provider_from_env};
pub use openai_compat::{FreeTierPolicy, OpenAiCompatibleProvider, is_free_tier_model};

/// A cryptographic hash of a prompt, used for deterministic replay.
///
/// SHA-256 hash of the prompt string, encoded as 32 bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptHash(pub [u8; 32]);

/// LlmAuditRecord stores the audit trail for an LLM decision.
///
/// This is an append-only record that captures everything needed for
/// deterministic replay and SEBI regulatory compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAuditRecord {
    /// UTC timestamp when the decision was made
    pub timestamp_utc: chrono::DateTime<chrono::Utc>,
    /// Model identifier (e.g. "claude-3-opus-20240229")
    pub model: String,
    /// Provider / endpoint that served the call (e.g. "omniroute",
    /// "openai", "ollama"). Captured so the replay hash distinguishes which
    /// backend answered a logical request when a router may vary it.
    #[serde(default)]
    pub provider: String,
    /// Temperature used for sampling (0.0 to 1.0) with Decimal precision
    pub temperature: Decimal,
    /// Random seed for reproducibility
    pub seed: u64,
    /// SHA-256 hash of the prompt (enables deterministic replay)
    pub prompt_sha256: PromptHash,
    /// The raw prompt sent to the LLM
    pub raw_prompt: String,
    /// The raw response received from the LLM
    pub raw_response: String,
    /// The parsed Decision enum from the response
    pub parsed_decision: Decision,
}

/// Errors that can occur during LLM provider operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LlmError {
    /// Transport error occurred during LLM communication.
    #[error("transport: {0}")]
    Transport(String),
    /// Authentication failure (invalid credentials, expired token, etc.).
    #[error("auth failure")]
    Auth,
    /// Rate limit exceeded; too many requests in a given time window.
    #[error("rate limited")]
    RateLimited,
    /// Context length exceeded model maximum window.
    #[error("context length exceeded")]
    ContextLengthExceeded,
    /// Audit record invalid or malformed.
    #[error("audit record invalid: {0}")]
    AuditRecord(String),
    /// Decision parse failure from raw model output.
    #[error("decision parse failed: {0}")]
    ParsingFailed(String),
    /// The configured endpoint reported a model outside the free-tier allowlist.
    #[error("endpoint reported non-free-tier model: {0}")]
    NonFreeTierModel(String),
}

/// Role of a participant in a conversation turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    /// System prompt instructions.
    System,
    /// Human user prompt.
    User,
    /// Assistant response.
    Assistant,
    /// Tool result payload.
    Tool,
}

/// Model tool invocation instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmToolCall {
    /// Unique call ID assigned by model.
    pub id: String,
    /// Name of the target tool.
    pub name: String,
    /// JSON argument string.
    pub arguments: String,
}

/// Tool execution result returned to model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmToolResult {
    /// Target call ID.
    pub call_id: String,
    /// Output payload string.
    pub output: String,
    /// Whether execution failed.
    pub is_error: bool,
}

/// Structured chat message exchanged with model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role of message author.
    pub role: ChatRole,
    /// Text content.
    pub content: String,
    /// Tool calls requested by assistant.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<LlmToolCall>,
    /// Tool call ID if role is Tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// Create a user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Create an assistant text message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    /// Create an assistant message with tool calls.
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<LlmToolCall>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_calls,
            tool_call_id: None,
        }
    }

    /// Create a tool response message.
    pub fn tool(call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: output.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
        }
    }
}

/// Token usage accounting.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Prompt tokens consumed.
    pub input_tokens: u32,
    /// Generated completion tokens.
    pub output_tokens: u32,
    /// Cached prompt tokens read from cache.
    pub cache_read_tokens: Option<u32>,
    /// Tokens written to prompt cache.
    pub cache_write_tokens: Option<u32>,
}

/// Termination reason for model generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Normal completion of turn.
    EndTurn,
    /// Stopped to request tool execution.
    ToolUse,
    /// Context or max_tokens limit reached.
    MaxTokens,
    /// Model encountered stop sequence.
    StopSequence,
    /// Turn paused.
    PauseTurn,
    /// Model refused prompt.
    Refusal,
    /// Content safety filter triggered.
    ContentFilter,
    /// Other reason.
    Other(String),
}

/// Streaming event emitted by an LLM provider during chat generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatStreamEvent {
    /// Delta text generated for assistant message.
    TextDelta {
        /// Generated fragment.
        text: String,
    },
    /// Model thinking / chain of thought delta.
    ThinkingDelta {
        /// Thinking text fragment.
        text: String,
    },
    /// Beginning of a tool invocation block.
    ToolCallStreaming {
        /// Assigned call ID.
        call_id: String,
        /// Target tool name.
        name: String,
    },
    /// Incremental chunk of tool call argument JSON.
    ToolCallChunk {
        /// Target call ID.
        call_id: String,
        /// Partial JSON text.
        arguments_delta: String,
    },
    /// Completed tool call ready for execution.
    ToolCallReady {
        /// Call ID.
        call_id: String,
        /// Target tool name.
        name: String,
        /// Parsed JSON arguments if valid.
        arguments: serde_json::Value,
        /// Raw arguments string.
        raw_arguments: String,
        /// True if JSON parsing failed.
        is_malformed: bool,
    },
    /// Token usage report.
    Usage(Usage),
    /// Message stream completion.
    MessageEnd {
        /// Termination reason.
        stop_reason: StopReason,
        /// Final assembled message.
        message: ChatMessage,
        /// Turn usage statistics.
        usage: Usage,
    },
}

/// Pinned asynchronous stream of chat generation events.
pub type EventStream =
    Pin<Box<dyn Stream<Item = Result<ChatStreamEvent, LlmError>> + Send + 'static>>;

/// Chat request parameters sent to an LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// History of conversation messages.
    pub messages: Vec<ChatMessage>,
    /// System prompt instructions.
    pub system_prompt: Option<String>,
    /// Available tools.
    pub tools: Vec<ToolSpec>,
    /// Generation temperature.
    pub temperature: Option<Decimal>,
    /// Maximum completion tokens.
    pub max_tokens: Option<usize>,
}

/// Computes the SHA-256 hash of a prompt string.
pub fn compute_prompt_hash(prompt: &str) -> PromptHash {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    PromptHash(arr)
}

/// Validates that an audit record is structurally sound.
pub fn validate_audit_record(record: &LlmAuditRecord) -> Result<(), LlmError> {
    if record.timestamp_utc.naive_utc() == chrono::NaiveDateTime::MIN {
        return Err(LlmError::AuditRecord("invalid timestamp".to_string()));
    }
    if record.model.trim().is_empty() {
        return Err(LlmError::AuditRecord("model cannot be empty".to_string()));
    }
    if record.provider.trim().is_empty() {
        return Err(LlmError::AuditRecord(
            "provider cannot be empty".to_string(),
        ));
    }
    if record.temperature < Decimal::ZERO || record.temperature > Decimal::ONE {
        return Err(LlmError::AuditRecord(
            "temperature must be in [0,1]".to_string(),
        ));
    }
    if record.seed == 0 {
        return Err(LlmError::AuditRecord("seed must be non-zero".to_string()));
    }
    Ok(())
}

/// Trait for LLM providers that can generate decisions and stream chat.
#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    /// Name of the LLM model.
    fn model_name(&self) -> &str;

    /// Generates a decision based on the prompt.
    ///
    /// Returns the raw response and the parsed decision.
    async fn generate(&self, prompt: &str) -> Result<(String, Decision), LlmError>;

    /// Stream conversational turn events.
    async fn chat_stream(&self, _request: &ChatRequest) -> Result<EventStream, LlmError> {
        Err(LlmError::Transport(
            "chat streaming not implemented for this provider".to_string(),
        ))
    }

    /// Gets the context window size in tokens.
    fn context_window(&self) -> usize;

    /// Checks if the model supports a given feature.
    fn supports(&self, _feature: &str) -> bool {
        false
    }
}

/// Scriptable mock LLM provider for deterministic unit and integration tests.
pub struct ScriptedFakeLlm {
    model: String,
    streams: Mutex<VecDeque<Vec<ChatStreamEvent>>>,
}

impl ScriptedFakeLlm {
    /// Create a new ScriptedFakeLlm with given model name.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            streams: Mutex::new(VecDeque::new()),
        }
    }

    /// Enqueue a scripted stream of events.
    pub fn push_stream(&self, events: Vec<ChatStreamEvent>) {
        self.streams.lock().unwrap().push_back(events);
    }

    /// Convenience: push a pure text completion response.
    pub fn push_text_response(&self, text: impl Into<String>) {
        let text = text.into();
        let events = vec![
            ChatStreamEvent::TextDelta { text: text.clone() },
            ChatStreamEvent::Usage(Usage {
                input_tokens: 10,
                output_tokens: 10,
                ..Default::default()
            }),
            ChatStreamEvent::MessageEnd {
                stop_reason: StopReason::EndTurn,
                message: ChatMessage::assistant(text),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 10,
                    ..Default::default()
                },
            },
        ];
        self.push_stream(events);
    }

    /// Convenience: push a tool call response followed by an EndTurn completion.
    pub fn push_tool_call_response(
        &self,
        call_id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) {
        let call_id = call_id.into();
        let name = name.into();
        let raw_args = serde_json::to_string(&arguments).unwrap();

        let tool_call = LlmToolCall {
            id: call_id.clone(),
            name: name.clone(),
            arguments: raw_args.clone(),
        };

        let events = vec![
            ChatStreamEvent::ToolCallStreaming {
                call_id: call_id.clone(),
                name: name.clone(),
            },
            ChatStreamEvent::ToolCallChunk {
                call_id: call_id.clone(),
                arguments_delta: raw_args.clone(),
            },
            ChatStreamEvent::ToolCallReady {
                call_id,
                name,
                arguments,
                raw_arguments: raw_args,
                is_malformed: false,
            },
            ChatStreamEvent::MessageEnd {
                stop_reason: StopReason::ToolUse,
                message: ChatMessage::assistant_with_tools("", vec![tool_call]),
                usage: Usage {
                    input_tokens: 20,
                    output_tokens: 15,
                    ..Default::default()
                },
            },
        ];
        self.push_stream(events);
    }
}

#[async_trait]
impl LlmProvider for ScriptedFakeLlm {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn generate(&self, _prompt: &str) -> Result<(String, Decision), LlmError> {
        Ok((
            "{\"action\": \"hold\"}".to_string(),
            Decision::Hold { reason: None },
        ))
    }

    async fn chat_stream(&self, _request: &ChatRequest) -> Result<EventStream, LlmError> {
        let events =
            self.streams.lock().unwrap().pop_front().ok_or_else(|| {
                LlmError::Transport("no scripted response available in fake".into())
            })?;

        let stream = futures::stream::iter(events.into_iter().map(Ok));
        Ok(Box::pin(stream))
    }

    fn context_window(&self) -> usize {
        128_000
    }
}

/// Trait for audit record persistence.
pub trait AuditRecorder: Send + Sync {
    /// Records an LLM decision audit entry.
    fn record(&self, record: &LlmAuditRecord) -> Result<(), LlmError>;

    /// Retrieves an audit record by prompt hash.
    fn retrieve(&self, prompt_hash: &PromptHash) -> Result<LlmAuditRecord, LlmError>;

    /// Lists all audit records within a time range.
    fn list_in_range(
        &self,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<LlmAuditRecord>, LlmError>;
}

/// In-memory implementation of AuditRecorder for testing and lightweight use.
#[derive(Debug, Default)]
pub struct InMemoryAuditRecorder {
    records: Mutex<Vec<LlmAuditRecord>>,
}

impl InMemoryAuditRecorder {
    /// Creates a new in-memory audit recorder.
    pub fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
        }
    }
}

impl AuditRecorder for InMemoryAuditRecorder {
    fn record(&self, record: &LlmAuditRecord) -> Result<(), LlmError> {
        self.records.lock().unwrap().push(record.clone());
        Ok(())
    }

    fn retrieve(&self, prompt_hash: &PromptHash) -> Result<LlmAuditRecord, LlmError> {
        let records = self.records.lock().unwrap();
        records
            .iter()
            .find(|r| &r.prompt_sha256 == prompt_hash)
            .cloned()
            .ok_or_else(|| LlmError::AuditRecord("replay not found".to_string()))
    }

    fn list_in_range(
        &self,
        _start: chrono::DateTime<chrono::Utc>,
        _end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<LlmAuditRecord>, LlmError> {
        let records = self.records.lock().unwrap();
        Ok(records.clone())
    }
}

/// Hash a prompt string deterministically.
pub fn hash_prompt(prompt: &str) -> PromptHash {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    PromptHash(arr)
}

/// Validates an audit record.
pub fn validate_record(record: &LlmAuditRecord) -> Result<(), LlmError> {
    validate_audit_record(record)
}
