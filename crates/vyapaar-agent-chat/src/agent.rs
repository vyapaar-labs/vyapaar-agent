//! ChatAgent engine and builder.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::mpsc;

use vyapaar_agent_core::{
    MemoryStore, Placement, ProfileBudget, PromptSection, RuntimeMode, SessionScope, SessionStore,
    default_assemble,
};
use vyapaar_agent_llm::LlmProvider;
use vyapaar_agent_prompt::{
    DEFAULT_CHAT_BASE_PROMPT, format_day_date_prefix, render_memory_profile_section,
};

use crate::error::ChatError;
use crate::event::SessionEvent;
use crate::permission::PermissionPolicy;
use crate::session::{SessionActor, SessionEvents, SessionHandle, SessionSelector};
use crate::tool::ToolRegistry;
use crate::turn::TurnConfig;

/// Engine configuration for ChatAgent sessions.
#[derive(Debug, Clone)]
pub struct ChatConfig {
    /// Maximum autonomous rounds allowed per turn.
    pub max_rounds: u32,
    /// Mid-stream retries for transient failures.
    pub stream_retries: u32,
    /// Default execution timeout for tools.
    pub default_tool_timeout: Duration,
    /// Permission confirmation policy.
    pub permission_policy: PermissionPolicy,
    /// Base system prompt instructions.
    pub base_prompt: String,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            max_rounds: 24,
            stream_retries: 2,
            default_tool_timeout: Duration::from_secs(30),
            permission_policy: PermissionPolicy::Interactive,
            base_prompt: DEFAULT_CHAT_BASE_PROMPT.to_string(),
        }
    }
}

/// Core conversational assistant engine (§5.3).
#[derive(Clone)]
pub struct ChatAgent {
    provider: Arc<dyn LlmProvider>,
    registry: ToolRegistry,
    session_store: Arc<dyn SessionStore>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    config: ChatConfig,
}

impl ChatAgent {
    /// Construct a new ChatAgent builder.
    pub fn builder(
        provider: Arc<dyn LlmProvider>,
        registry: ToolRegistry,
        session_store: Arc<dyn SessionStore>,
    ) -> ChatAgentBuilder {
        ChatAgentBuilder::new(provider, registry, session_store)
    }

    /// Open or resume a conversation session.
    pub async fn open(
        &self,
        selector: SessionSelector,
    ) -> Result<(SessionHandle, SessionEvents), ChatError> {
        let (session_id, scope, resumed, history) = match selector {
            SessionSelector::New(scope) => (None, scope, false, Vec::new()),
            SessionSelector::Continue(scope) => {
                if let Ok(Some(recent)) = self.session_store.most_recent(&scope).await {
                    let msgs = self
                        .session_store
                        .messages(recent.id)
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(_, pm)| pm)
                        .collect();
                    (Some(recent.id), scope, true, msgs)
                } else {
                    (None, scope, false, Vec::new())
                }
            }
            SessionSelector::Resume(id) => {
                let msgs = self
                    .session_store
                    .messages(id)
                    .await
                    .map_err(ChatError::from)?
                    .into_iter()
                    .map(|(_, pm)| pm)
                    .collect();

                let scope = SessionScope::new("default", RuntimeMode::Live);
                (Some(id), scope, true, msgs)
            }
        };

        // Assemble stable cached prefix (§7.3, §7.4)
        let mut prefix_sections = Vec::new();
        prefix_sections.push(PromptSection::new(
            "00_date",
            Placement::CachedPrefix,
            format_day_date_prefix(Utc::now()),
        ));

        if let Some(mem_store) = &self.memory_store {
            if let Ok(profile) = mem_store.profile(ProfileBudget::default()).await {
                prefix_sections.push(render_memory_profile_section(
                    &profile.entries,
                    profile.total_count,
                ));
            }
        }

        let assembly = default_assemble(&self.config.base_prompt, &prefix_sections);
        let system_prompt = assembly.cached_prefix;

        let (cmd_tx, cmd_rx) = mpsc::channel(128);
        let (event_tx, event_rx) = mpsc::channel(1024);

        let turn_config = TurnConfig {
            max_rounds: self.config.max_rounds,
            stream_retries: self.config.stream_retries,
            default_tool_timeout: self.config.default_tool_timeout,
            permission_policy: self.config.permission_policy.clone(),
        };

        let tool_snapshot = self.registry.snapshot();

        let actor = SessionActor::new(
            session_id,
            scope.clone(),
            system_prompt,
            history.clone(),
            tool_snapshot,
            self.provider.clone(),
            self.session_store.clone(),
            turn_config,
            cmd_rx,
            event_tx.clone(),
        );

        tokio::spawn(actor.run());

        // Emit initial SessionOpened event
        let _ = event_tx
            .send(SessionEvent::SessionOpened {
                session: session_id,
                resumed,
                history: history.len(),
                profile: scope,
                model: Some(self.provider.model_name().to_string()),
            })
            .await;

        let handle = SessionHandle::new(session_id, cmd_tx);
        let events = SessionEvents::new(event_rx);

        Ok((handle, events))
    }
}

/// Fluent builder for [`ChatAgent`].
pub struct ChatAgentBuilder {
    provider: Arc<dyn LlmProvider>,
    registry: ToolRegistry,
    session_store: Arc<dyn SessionStore>,
    memory_store: Option<Arc<dyn MemoryStore>>,
    config: ChatConfig,
}

impl ChatAgentBuilder {
    /// Create a new builder with required dependencies.
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        registry: ToolRegistry,
        session_store: Arc<dyn SessionStore>,
    ) -> Self {
        Self {
            provider,
            registry,
            session_store,
            memory_store: None,
            config: ChatConfig::default(),
        }
    }

    /// Attach an optional persistent memory store.
    pub fn memory_store(mut self, memory_store: Arc<dyn MemoryStore>) -> Self {
        self.memory_store = Some(memory_store);
        self
    }

    /// Supply custom engine configuration.
    pub fn config(mut self, config: ChatConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the configured ChatAgent.
    pub fn build(self) -> Result<ChatAgent, ChatError> {
        Ok(ChatAgent {
            provider: self.provider,
            registry: self.registry,
            session_store: self.session_store,
            memory_store: self.memory_store,
            config: self.config,
        })
    }
}
