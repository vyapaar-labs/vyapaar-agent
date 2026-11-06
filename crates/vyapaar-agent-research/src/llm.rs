//! LLM-backed research-loop helpers (feature: `llm`).
//!
//! The autonomous research loop (Phase 5) generates closed-grammar enhancement
//! prompts via [`crate::enhancer::EnhancerPromptGenerator`]; this module plugs
//! the configured [`vyapaar_agent_llm::LlmProvider`] into that path so a live
//! model can suggest an indicator filter for a diagnosed failure regime. The
//! model output is free text (a suggestion, not an order), so it is never fed
//! to the risk gate — the closed-grammar `Decision` path remains trader-loop
//! territory.

use std::sync::Arc;

use vyapaar_agent_llm::{LlmError, LlmProvider};

use crate::enhancer::{EnhancerPromptGenerator, FailureRegimeDiagnostic};

/// LLM client for the research loop, wrapping a provider.
#[derive(Clone)]
pub struct ResearchLlm {
    provider: Arc<dyn LlmProvider>,
}

impl ResearchLlm {
    /// Wrap a provider for research-loop use.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Model name of the wrapped provider.
    pub fn model_name(&self) -> &str {
        self.provider.model_name()
    }

    /// Build the Task 5.2 enhancement prompt for a failure diagnostic and ask
    /// the model for an indicator-filter suggestion.
    ///
    /// Returns the raw model response. The caller is expected to validate any
    /// suggested indicator against the [`crate::enhancer::ClosedGrammarMutator`] /
    /// `IndicatorCatalog` before mutating a strategy spec.
    pub async fn suggest_enhancement(
        &self,
        diagnostic: &FailureRegimeDiagnostic,
    ) -> Result<String, LlmError> {
        let prompt = EnhancerPromptGenerator::build_prompt(diagnostic);
        tracing::info!(model = self.provider.model_name(), "research loop llm call");
        let (raw_response, _decision) = self.provider.generate(&prompt).await?;
        Ok(raw_response)
    }
}
