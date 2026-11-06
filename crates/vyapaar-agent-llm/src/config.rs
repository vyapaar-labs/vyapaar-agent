//! Env-var configuration for the LLM provider backend.
//!
//! Follows the same shape as `vyapaar-exec-auth`'s `HONBA_{BROKER}_{FIELD}`
//! convention (env-var driven credentials) without depending on that crate, per
//! dependency rules R2/R4 of `vyapaar-agent/docs/design/architecture.md`.
//!
//! Variables:
//! - `VYAPAAR_LLM_BASE_URL` (required) — OpenAI-compatible endpoint root, e.g.
//!   `http://localhost:11434/v1` (OmniRoute local router).
//! - `VYAPAAR_LLM_MODEL` (required) — model name to request.
//! - `VYAPAAR_LLM_API_KEY` (optional) — bearer token if the endpoint requires one.

use crate::LlmError;
use crate::openai_compat::OpenAiCompatibleProvider;

/// Env var for the OpenAI-compatible base URL.
pub const ENV_BASE_URL: &str = "VYAPAAR_LLM_BASE_URL";
/// Env var for the API key / bearer token.
pub const ENV_API_KEY: &str = "VYAPAAR_LLM_API_KEY";
/// Env var for the default model name.
pub const ENV_MODEL: &str = "VYAPAAR_LLM_MODEL";

/// Resolved LLM provider configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmConfig {
    /// OpenAI-compatible endpoint root (e.g. `http://localhost:11434/v1`).
    pub base_url: String,
    /// Optional bearer API key.
    pub api_key: Option<String>,
    /// Default model identifier.
    pub model: String,
}

/// Resolve provider configuration from an arbitrary env lookup function.
///
/// Kept separate from the `std::env` read so callers can test the loader
/// deterministically with a map.
pub fn llm_config_from_map(env: &impl Fn(&str) -> Option<String>) -> Result<LlmConfig, LlmError> {
    let base_url = env(ENV_BASE_URL)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| LlmError::Transport(format!("{ENV_BASE_URL} not set")))?;
    let model = env(ENV_MODEL)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| LlmError::Transport(format!("{ENV_MODEL} not set")))?;
    let api_key = env(ENV_API_KEY).filter(|s| !s.trim().is_empty());
    Ok(LlmConfig {
        base_url,
        api_key,
        model,
    })
}

/// Resolve provider configuration from the process environment.
pub fn llm_config_from_env() -> Result<LlmConfig, LlmError> {
    llm_config_from_map(&|k| std::env::var(k).ok())
}

/// Build an [`OpenAiCompatibleProvider`] from the process environment.
///
/// Missing required variables produce a transport error explaining which
/// `VYAPAAR_LLM_*` variable was not set.
pub fn openai_provider_from_env() -> Result<OpenAiCompatibleProvider, LlmError> {
    let config = llm_config_from_env()?;
    Ok(OpenAiCompatibleProvider::new(
        config.base_url,
        config.api_key,
        config.model,
    ))
}
