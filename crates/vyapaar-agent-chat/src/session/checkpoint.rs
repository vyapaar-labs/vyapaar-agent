//! Turn checkpoints, undo/redo, and transcript export (§2.4 of plan 0002).
//!
//! [`SessionCheckpointer`] snapshots a session turn as
//! `(TurnId, ConversationState, ContextSummary)` and supports rolling the session
//! back (undo) or forward (redo) and exporting the transcript as JSON/Markdown.
//! Persistence is delegated to the [`SessionStore`] port.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use vyapaar_agent_core::{MessageSeq, PersistedMessage, SessionId, SessionStore, StoreError};

use crate::event::TurnId;

/// A point-in-time snapshot of a session turn.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub turn_id: TurnId,
    /// Last message sequence number included in this snapshot.
    pub last_seq: MessageSeq,
    /// Full conversation state at this turn.
    pub conversation_state: Vec<PersistedMessage>,
    /// Human-readable summary of the context at this point.
    pub context_summary: String,
    pub created_at: DateTime<Utc>,
}

impl std::fmt::Debug for Checkpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkpoint")
            .field("turn_id", &self.turn_id)
            .field("last_seq", &self.last_seq)
            .field("message_count", &self.conversation_state.len())
            .field("context_summary", &self.context_summary)
            .field("created_at", &self.created_at)
            .finish()
    }
}

/// Export format for session transcripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Json,
    Markdown,
    PlainText,
}

/// Manages turn checkpoints enabling `/undo`, `/redo`, and transcript export.
#[derive(Clone)]
pub struct SessionCheckpointer {
    store: Arc<dyn SessionStore>,
    checkpoints: Arc<RwLock<HashMap<SessionId, Vec<Checkpoint>>>>,
    redo_stack: Arc<RwLock<HashMap<SessionId, Vec<Checkpoint>>>>,
}

impl std::fmt::Debug for SessionCheckpointer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionCheckpointer").finish()
    }
}

impl SessionCheckpointer {
    /// Wrap an existing [`SessionStore`].
    pub fn new(store: Arc<dyn SessionStore>) -> Self {
        Self {
            store,
            checkpoints: Arc::default(),
            redo_stack: Arc::default(),
        }
    }

    /// Record a checkpoint after turn `turn_id` with the given conversation state.
    pub async fn checkpoint(
        &self,
        session_id: SessionId,
        turn_id: TurnId,
        conversation_state: Vec<PersistedMessage>,
        context_summary: impl Into<String>,
    ) -> Result<(), StoreError> {
        let last_seq = conversation_state.last().map(|m| m.seq).unwrap_or(0);
        let ck = Checkpoint {
            turn_id,
            last_seq,
            conversation_state,
            context_summary: context_summary.into(),
            created_at: Utc::now(),
        };
        self.checkpoints
            .write()
            .unwrap()
            .entry(session_id)
            .or_default()
            .push(ck);
        // A new checkpoint invalidates any pending redo branch.
        self.redo_stack.write().unwrap().remove(&session_id);
        Ok(())
    }

    /// Undo the most recent turn: pop the current checkpoint onto the redo stack and
    /// return the previous checkpoint so the caller can restore that conversation state.
    /// Persisted history is truncated to the returned checkpoint's last sequence.
    pub async fn undo(&self, session_id: SessionId) -> Result<Option<Checkpoint>, StoreError> {
        let restore = {
            let mut cks = self.checkpoints.write().unwrap();
            let session_cks = match cks.get_mut(&session_id) {
                Some(v) => v,
                None => return Ok(None),
            };
            // The top checkpoint is the current state; move it to the redo stack.
            let current = match session_cks.pop() {
                Some(c) => c,
                None => return Ok(None),
            };
            self.redo_stack
                .write()
                .unwrap()
                .entry(session_id)
                .or_default()
                .push(current);
            // The new top is the state to restore to.
            session_cks.last().cloned()
        };
        if let Some(ref ck) = restore {
            self.store.truncate_after(session_id, ck.last_seq).await?;
        }
        Ok(restore)
    }

    /// Redo the last undone turn, provided no intervening turn has branched the tree.
    pub async fn redo(&self, session_id: SessionId) -> Result<Option<Checkpoint>, StoreError> {
        let next = {
            let mut redos = self.redo_stack.write().unwrap();
            let session_redos = match redos.get_mut(&session_id) {
                Some(v) => v,
                None => return Ok(None),
            };
            match session_redos.pop() {
                Some(c) => c,
                None => return Ok(None),
            }
        };
        self.checkpoints
            .write()
            .unwrap()
            .entry(session_id)
            .or_default()
            .push(next.clone());
        self.store.truncate_after(session_id, next.last_seq).await?;
        Ok(Some(next))
    }

    /// List all checkpoints for a session, oldest first.
    pub fn list_checkpoints(&self, session_id: SessionId) -> Vec<Checkpoint> {
        self.checkpoints
            .read()
            .unwrap()
            .get(&session_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Export the session transcript in the requested format.
    pub async fn export(
        &self,
        session_id: SessionId,
        format: ExportFormat,
    ) -> Result<String, StoreError> {
        let messages = self.store.messages(session_id).await?;
        Ok(match format {
            ExportFormat::Json => serde_json::to_string_pretty(&messages).expect("serializable"),
            ExportFormat::Markdown => render_markdown(&messages),
            ExportFormat::PlainText => render_plain(&messages),
        })
    }
}

fn render_markdown(messages: &[(MessageSeq, PersistedMessage)]) -> String {
    let mut out = String::from("# Session Transcript\n\n");
    for (seq, msg) in messages {
        out.push_str(&format!(
            "## Message {}\n\n**Role:** {}\n**Created:** {}\n\n{}\n\n---\n\n",
            seq,
            msg.role,
            msg.created_at.to_rfc3339(),
            serde_json::to_string(&msg.content).unwrap_or_default()
        ));
    }
    out
}

fn render_plain(messages: &[(MessageSeq, PersistedMessage)]) -> String {
    let mut out = String::new();
    for (seq, msg) in messages {
        out.push_str(&format!(
            "[{}] {}: {}\n",
            seq,
            msg.role,
            serde_json::to_string(&msg.content).unwrap_or_default()
        ));
    }
    out
}
