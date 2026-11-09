//! Append-only audit record schema and cryptographic hash chaining.
//!
//! Provides SEBI regulatory compliance by maintaining an immutable, tamper-evident
//! SHA-256 hash chain linking sequential LLM inference turns.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vyapaar_agent_core::{compute_prompt_hash, sha256_hex};
use vyapaar_core::Decision;

use crate::error::ReplayError;

/// Append-only audit record schema for an LLM query and response turn.
///
/// Implements exact field requirements from Master Plan §2.1 plus the Phase-4
/// determinism fingerprint (plan 0005 §4): when a local engine replaces the
/// free-tier router, the audit record must capture the serving-stack fingerprint
/// so `prompt + seed + temperature = 0` is actually reproducible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LlmAuditRecord {
    /// UTC timestamp of inference
    pub timestamp_utc: DateTime<Utc>,
    /// Model identifier name (e.g. "claude-3-5-sonnet", "gemini-1.5-pro",
    /// "Llama-3.2-3B-Instruct")
    pub model: String,
    /// Provider / endpoint that served the call (e.g. "omniroute",
    /// "openai", "ollama", "candle-gguf"). Part of the hash chain so a
    /// multi-provider router that varies the backend per call is captured in
    /// the audit trail.
    #[serde(default)]
    pub provider: String,
    /// Serving engine identifier (e.g. "candle-gguf", "openai-compat").
    /// Empty for backends that do not expose a discrete engine.
    #[serde(default)]
    pub engine: String,
    /// Serving engine version (e.g. candle-core "0.11.1"). Empty when unknown.
    #[serde(default)]
    pub engine_version: String,
    /// SHA-256 of the exact model file served (e.g. the .gguf file), when the
    /// backend loads a file. Captured instead of — or in addition to — the
    /// model name tag, because a name alone does not pin the weights.
    pub model_file_sha256: Option<[u8; 32]>,
    /// Quantization build of the served model file (e.g. "Q4_K_M"), when known.
    #[serde(default)]
    pub quantization: Option<String>,
    /// CPU thread count used for inference, when pinned for determinism.
    pub thread_count: Option<u32>,
    /// Hardware identity string for the inference device (e.g. CPU model).
    #[serde(default)]
    pub hardware: Option<String>,
    /// Sampling temperature (exact Decimal, zero floats)
    pub temperature: Decimal,
    /// Random seed for deterministic inference
    pub seed: u64,
    /// SHA-256 hash of the prompt (32 bytes)
    pub prompt_sha256: [u8; 32],
    /// Serialized market context JSON
    pub market_context_json: String,
    /// Exact raw prompt sent to the model
    pub raw_prompt: String,
    /// Raw string completion received from the model
    pub raw_response: String,
    /// Closed, validated [`Decision`] enum
    pub parsed_decision: Decision,
}

impl LlmAuditRecord {
    /// Construct a new audit record, automatically computing the SHA-256 hash of raw_prompt.
    ///
    /// Determinism fingerprint fields (`engine`, `engine_version`,
    /// `model_file_sha256`, `quantization`, `thread_count`, `hardware`) are left
    /// unset; attach them with the [`LlmAuditRecord::with_engine`]/`with_*` builders
    /// when a local engine is the backend of record.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        timestamp_utc: DateTime<Utc>,
        model: impl Into<String>,
        temperature: Decimal,
        seed: u64,
        market_context_json: impl Into<String>,
        raw_prompt: impl Into<String>,
        raw_response: impl Into<String>,
        parsed_decision: Decision,
    ) -> Self {
        let p_str = raw_prompt.into();
        let prompt_sha256 = compute_prompt_hash(&p_str);

        Self {
            timestamp_utc,
            model: model.into(),
            provider: String::new(),
            engine: String::new(),
            engine_version: String::new(),
            model_file_sha256: None,
            quantization: None,
            thread_count: None,
            hardware: None,
            temperature,
            seed,
            prompt_sha256,
            market_context_json: market_context_json.into(),
            raw_prompt: p_str,
            raw_response: raw_response.into(),
            parsed_decision,
        }
    }

    /// Attach the provider/endpoint identifier that served this call.
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = provider.into();
        self
    }

    /// Attach the serving engine identifier (e.g. "candle-gguf").
    pub fn with_engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = engine.into();
        self
    }

    /// Attach the serving engine version (e.g. candle-core "0.11.1").
    pub fn with_engine_version(mut self, version: impl Into<String>) -> Self {
        self.engine_version = version.into();
        self
    }

    /// Attach the SHA-256 of the exact model file served.
    pub fn with_model_file_hash(mut self, hash: [u8; 32]) -> Self {
        self.model_file_sha256 = Some(hash);
        self
    }

    /// Attach the quantization build of the served model file (e.g. "Q4_K_M").
    pub fn with_quantization(mut self, quantization: impl Into<String>) -> Self {
        self.quantization = Some(quantization.into());
        self
    }

    /// Attach the CPU thread count pinned for deterministic inference.
    pub fn with_thread_count(mut self, threads: u32) -> Self {
        self.thread_count = Some(threads);
        self
    }

    /// Attach a hardware identity string for the inference device.
    pub fn with_hardware(mut self, hardware: impl Into<String>) -> Self {
        self.hardware = Some(hardware.into());
        self
    }

    /// Return lowercase hex string of prompt_sha256.
    pub fn prompt_sha256_hex(&self) -> String {
        sha256_hex(&self.prompt_sha256)
    }

    /// Verify that the stored prompt_sha256 matches the hash of raw_prompt.
    pub fn verify_prompt_integrity(&self) -> Result<(), ReplayError> {
        let calculated = compute_prompt_hash(&self.raw_prompt);
        if calculated != self.prompt_sha256 {
            return Err(ReplayError::ChecksumMismatch {
                record_id: format!("{}:{}", self.model, self.seed),
                expected: self.prompt_sha256_hex(),
                actual: sha256_hex(&calculated),
            });
        }
        Ok(())
    }
}

/// SHA-256 digest of a file's contents, used to pin the exact model file
/// (e.g. a `.gguf`) served by a local engine in the audit fingerprint.
///
/// Reads the file in 64 KiB chunks so large model files do not need to be
/// held entirely in memory.
pub fn hash_file(path: impl AsRef<Path>) -> Result<[u8; 32], ReplayError> {
    let path = path.as_ref();
    let mut file = File::open(path).map_err(|e| {
        ReplayError::Io(format!(
            "failed to open model file '{}': {e}",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| {
            ReplayError::Io(format!(
                "failed to read model file '{}': {e}",
                path.display()
            ))
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    Ok(arr)
}

/// Cryptographically chained audit record linking sequential records via SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChainedLlmAuditRecord {
    /// 0-indexed sequential monotonically increasing sequence number
    pub sequence_number: u64,
    /// Hash of the immediately preceding record (32 zeros for genesis)
    pub prev_record_hash: [u8; 32],
    /// Hash of this record chaining prev_record_hash and inner fields
    pub record_hash: [u8; 32],
    /// Inner audit record payload
    pub record: LlmAuditRecord,
}

impl ChainedLlmAuditRecord {
    /// Return lowercase hex string of record_hash.
    pub fn record_hash_hex(&self) -> String {
        sha256_hex(&self.record_hash)
    }

    /// Return lowercase hex string of prev_record_hash.
    pub fn prev_record_hash_hex(&self) -> String {
        sha256_hex(&self.prev_record_hash)
    }
}

/// Compute the cryptographic record hash chaining prev_hash with record contents.
pub fn compute_record_hash(prev_hash: &[u8; 32], seq: u64, record: &LlmAuditRecord) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash);
    hasher.update(&seq.to_be_bytes());
    if let Some(nanos) = record.timestamp_utc.timestamp_nanos_opt() {
        hasher.update(&nanos.to_be_bytes());
    } else {
        hasher.update(&record.timestamp_utc.timestamp().to_be_bytes());
    }
    hasher.update(record.model.as_bytes());
    hasher.update(record.provider.as_bytes());
    // Phase-4 determinism fingerprint: two records that differ only in serving
    // stack must hash differently so the chain pins the exact engine/weights.
    hasher.update(record.engine.as_bytes());
    hasher.update(record.engine_version.as_bytes());
    if let Some(h) = record.model_file_sha256 {
        hasher.update(&h);
    }
    if let Some(q) = &record.quantization {
        hasher.update(q.as_bytes());
    }
    if let Some(t) = record.thread_count {
        hasher.update(&t.to_be_bytes());
    }
    if let Some(hw) = &record.hardware {
        hasher.update(hw.as_bytes());
    }
    hasher.update(record.temperature.to_string().as_bytes());
    hasher.update(&record.seed.to_be_bytes());
    hasher.update(&record.prompt_sha256);
    hasher.update(record.market_context_json.as_bytes());
    hasher.update(record.raw_prompt.as_bytes());
    hasher.update(record.raw_response.as_bytes());

    if let Ok(decision_bytes) = serde_json::to_vec(&record.parsed_decision) {
        hasher.update(&decision_bytes);
    }

    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    arr
}

/// Verify that a sequence of chained audit records forms an unbroken, untampered hash chain.
pub fn verify_hash_chain(chain: &[ChainedLlmAuditRecord]) -> Result<(), ReplayError> {
    if chain.is_empty() {
        return Ok(());
    }

    let mut expected_prev = [0u8; 32];

    for (idx, item) in chain.iter().enumerate() {
        let expected_seq = idx as u64;
        if item.sequence_number != expected_seq {
            return Err(ReplayError::ChainBroken {
                sequence_number: item.sequence_number,
                expected_prev: format!("seq {expected_seq}"),
                actual_prev: format!("seq {}", item.sequence_number),
            });
        }

        if item.prev_record_hash != expected_prev {
            return Err(ReplayError::ChainBroken {
                sequence_number: item.sequence_number,
                expected_prev: sha256_hex(&expected_prev),
                actual_prev: sha256_hex(&item.prev_record_hash),
            });
        }

        // Verify inner prompt hash
        item.record.verify_prompt_integrity()?;

        // Verify record hash calculation
        let calculated =
            compute_record_hash(&item.prev_record_hash, item.sequence_number, &item.record);
        if calculated != item.record_hash {
            return Err(ReplayError::ChecksumMismatch {
                record_id: format!("chain_seq_{}", item.sequence_number),
                expected: sha256_hex(&item.record_hash),
                actual: sha256_hex(&calculated),
            });
        }

        expected_prev = item.record_hash;
    }

    Ok(())
}

/// Builder and state tracker for appending records into an unbroken hash chain.
#[derive(Debug, Default, Clone)]
pub struct AuditChainLedger {
    chain: Vec<ChainedLlmAuditRecord>,
    last_hash: [u8; 32],
}

impl AuditChainLedger {
    /// Create a new empty audit ledger.
    pub fn new() -> Self {
        Self {
            chain: Vec::new(),
            last_hash: [0u8; 32],
        }
    }

    /// Number of records in chain.
    pub fn len(&self) -> usize {
        self.chain.len()
    }

    /// True if chain is empty.
    pub fn is_empty(&self) -> bool {
        self.chain.is_empty()
    }

    /// Append an audit record to the chain, computing its hash and linking it to the previous record.
    pub fn append(&mut self, record: LlmAuditRecord) -> &ChainedLlmAuditRecord {
        let seq = self.chain.len() as u64;
        let prev_record_hash = self.last_hash;
        let record_hash = compute_record_hash(&prev_record_hash, seq, &record);

        let chained = ChainedLlmAuditRecord {
            sequence_number: seq,
            prev_record_hash,
            record_hash,
            record,
        };

        self.last_hash = record_hash;
        self.chain.push(chained);
        self.chain.last().unwrap()
    }

    /// Get reference to all chained records.
    pub fn records(&self) -> &[ChainedLlmAuditRecord] {
        &self.chain
    }

    /// Verify integrity of entire internal chain.
    pub fn verify(&self) -> Result<(), ReplayError> {
        verify_hash_chain(&self.chain)
    }
}

/// Canonical request hash for assistant messages in chat sessions (§5.8).
///
/// Computes SHA-256 over:
/// - System prompt
/// - Tool specifications (sorted canonically by name)
/// - Message history serialized to canonical JSON
/// - Generation options (model, temperature, etc.)
pub fn hash_request(
    system_prompt: &str,
    tool_specs: &[vyapaar_agent_core::ToolSpec],
    messages_json: &serde_json::Value,
    generation_options: &serde_json::Value,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(system_prompt.as_bytes());
    hasher.update(b"\n---TOOLS---\n");

    let mut sorted_specs = tool_specs.to_vec();
    sorted_specs.sort_by(|a, b| a.name.cmp(&b.name));
    for spec in &sorted_specs {
        let spec_json = serde_json::to_string(spec).unwrap_or_default();
        hasher.update(spec_json.as_bytes());
        hasher.update(b"\n");
    }

    hasher.update(b"\n---MESSAGES---\n");
    let msgs_str = serde_json::to_string(messages_json).unwrap_or_default();
    hasher.update(msgs_str.as_bytes());

    hasher.update(b"\n---OPTIONS---\n");
    let opts_str = serde_json::to_string(generation_options).unwrap_or_default();
    hasher.update(opts_str.as_bytes());

    let result = hasher.finalize();
    sha256_hex(&result)
}
