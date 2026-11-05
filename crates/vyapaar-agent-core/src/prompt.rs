//! Prompt templates, assembly, and deterministic SHA-256 prompt hashing.

use std::collections::HashMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A 32-byte cryptographic SHA-256 digest of a prompt payload.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PromptHash(pub [u8; 32]);

impl PromptHash {
    /// Compute the SHA-256 hash of a prompt string.
    pub fn compute(prompt: &str) -> Self {
        Self(compute_prompt_hash(prompt))
    }

    /// Return lowercase hex string representation.
    pub fn to_hex(&self) -> String {
        sha256_hex(&self.0)
    }

    /// Access raw 32 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for PromptHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Compute the raw 32-byte SHA-256 digest of a prompt text string.
pub fn compute_prompt_hash(prompt: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    let result = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&result);
    arr
}

/// Compute hex-encoded SHA-256 string for arbitrary bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{:02x}", byte);
    }
    hex
}

/// Compute hex-encoded SHA-256 string for a prompt string.
pub fn hash_prompt_hex(prompt: &str) -> String {
    let hash = compute_prompt_hash(prompt);
    sha256_hex(&hash)
}

/// Dynamic prompt template supporting key-value substitution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PromptTemplate {
    /// Template identifier name
    pub name: String,
    /// Body template containing `{{var}}` placeholders
    pub body: String,
}

impl PromptTemplate {
    /// Create a new prompt template.
    pub fn new(name: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            body: body.into(),
        }
    }

    /// Replace single `{{context}}` variable.
    pub fn render(&self, ctx: &str) -> String {
        self.body.replace("{{context}}", ctx)
    }

    /// Render template with arbitrary key-value mappings.
    pub fn render_map(&self, vars: &HashMap<&str, &str>) -> String {
        let mut rendered = self.body.clone();
        for (k, v) in vars {
            let placeholder = format!("{{{{{}}}}}", k);
            rendered = rendered.replace(&placeholder, v);
        }
        rendered
    }
}

/// Target placement for prompt sections to ensure cache breakpoint stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Placement {
    /// System prompt prefix, frozen for the duration of a session or day.
    CachedPrefix,
    /// Per-turn contextual section prepended to user input without invalidating system prefix cache.
    TurnContext,
}

/// A structured section contributing to prompt assembly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSection {
    /// Stable section identifier for ordering.
    pub id: String,
    /// Intended placement location.
    pub placement: Placement,
    /// Formatted content.
    pub text: String,
}

impl PromptSection {
    /// Create a new prompt section.
    pub fn new(id: impl Into<String>, placement: Placement, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            placement,
            text: text.into(),
        }
    }
}

/// Assembled prompt parts segregated by placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptAssembly {
    /// Frozen system prompt prefix (hit-stable for KV prompt caching).
    pub cached_prefix: String,
    /// Turn-specific context blocks (prepended to the current user message).
    pub turn_context: Vec<PromptSection>,
}

/// Assemble sections into a placement-typed assembly with stable sorting.
pub fn default_assemble(base: &str, sections: &[PromptSection]) -> PromptAssembly {
    let mut prefix_sections: Vec<&PromptSection> = sections
        .iter()
        .filter(|s| s.placement == Placement::CachedPrefix && !s.text.trim().is_empty())
        .collect();

    prefix_sections.sort_by(|a, b| a.id.cmp(&b.id));

    let mut prefix_parts = Vec::with_capacity(prefix_sections.len() + 1);
    if !base.trim().is_empty() {
        prefix_parts.push(base.trim().to_string());
    }
    for sec in prefix_sections {
        prefix_parts.push(sec.text.trim().to_string());
    }

    let cached_prefix = prefix_parts.join("\n\n");
    let turn_context = sections
        .iter()
        .filter(|s| s.placement == Placement::TurnContext && !s.text.trim().is_empty())
        .cloned()
        .collect();

    PromptAssembly {
        cached_prefix,
        turn_context,
    }
}

/// Builder contract for assembling model prompts deterministically.
pub trait PromptBuilder: Send + Sync {
    /// Assemble structured sections with a base system prompt.
    fn assemble(&self, base: &str, sections: &[PromptSection]) -> PromptAssembly {
        default_assemble(base, sections)
    }
}

/// Default unit implementation of PromptBuilder.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultPromptBuilder;

impl PromptBuilder for DefaultPromptBuilder {}
