//! Session actor, command handles, and asynchronous event streams.

use std::collections::HashSet;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::{mpsc, oneshot};

use vyapaar_agent_core::{NewSession, PersistedMessage, SessionId, SessionScope, SessionStore};
use vyapaar_agent_llm::LlmProvider;

use crate::cancel::CancellationToken;
use crate::error::ChatError;
use crate::event::{Notice, SessionEvent, TurnId};
use crate::tool::ToolSet;
use crate::turn::{TurnConfig, run_turn};

/// Selector defining how a session is resolved on open (§5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSelector {
    /// Always create a brand-new session for the scope.
    New(SessionScope),
    /// Open the most recent existing session for the scope, or create a new one.
    Continue(SessionScope),
    /// Resume an explicit session by UUID.
    Resume(SessionId),
}

/// Commands sent to the background SessionActor task.
pub enum SessionCommand {
    Submit {
        text: String,
        reply: oneshot::Sender<Result<TurnId, ChatError>>,
    },
    Cancel,
    Notify(Notice),
    Rename {
        title: String,
        reply: oneshot::Sender<Result<(), ChatError>>,
    },
    History {
        reply: oneshot::Sender<Vec<PersistedMessage>>,
    },
    Close,
}

/// Asynchronous event stream yielding sequential [`SessionEvent`] notifications.
pub struct SessionEvents {
    receiver: mpsc::Receiver<SessionEvent>,
}

impl SessionEvents {
    /// Create new session event stream.
    pub fn new(receiver: mpsc::Receiver<SessionEvent>) -> Self {
        Self { receiver }
    }

    /// Receive the next event asynchronously.
    pub async fn recv(&mut self) -> Option<SessionEvent> {
        self.receiver.recv().await
    }
}

impl Stream for SessionEvents {
    type Item = SessionEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Lightweight, cloneable handle to an active conversation session (§5.3).
#[derive(Debug, Clone)]
pub struct SessionHandle {
    session_id: Option<SessionId>,
    cmd_tx: mpsc::Sender<SessionCommand>,
}

impl SessionHandle {
    /// Create a new session handle.
    pub fn new(session_id: Option<SessionId>, cmd_tx: mpsc::Sender<SessionCommand>) -> Self {
        Self { session_id, cmd_tx }
    }

    /// Active session identifier if persisted.
    pub fn session_id(&self) -> Option<SessionId> {
        self.session_id
    }

    /// Submit a user prompt to start the next turn.
    pub async fn submit(&self, text: impl Into<String>) -> Result<TurnId, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::Submit {
                text: text.into(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::SessionClosed)?;

        reply_rx.await.map_err(|_| ChatError::SessionClosed)?
    }

    /// Cancel any currently running turn.
    pub async fn cancel(&self) -> Result<(), ChatError> {
        self.cmd_tx
            .send(SessionCommand::Cancel)
            .await
            .map_err(|_| ChatError::SessionClosed)
    }

    /// Inject a background or system notice into the session.
    pub async fn notify(&self, notice: Notice) -> Result<(), ChatError> {
        self.cmd_tx
            .send(SessionCommand::Notify(notice))
            .await
            .map_err(|_| ChatError::SessionClosed)
    }

    /// Update the human-readable title of this session.
    pub async fn rename(&self, title: impl Into<String>) -> Result<(), ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::Rename {
                title: title.into(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| ChatError::SessionClosed)?;

        reply_rx.await.map_err(|_| ChatError::SessionClosed)?
    }

    /// Fetch a copy of the persisted message history.
    pub async fn history(&self) -> Result<Vec<PersistedMessage>, ChatError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::History { reply: reply_tx })
            .await
            .map_err(|_| ChatError::SessionClosed)?;

        reply_rx.await.map_err(|_| ChatError::SessionClosed)
    }

    /// Close the session.
    pub async fn close(&self) -> Result<(), ChatError> {
        let _ = self.cmd_tx.send(SessionCommand::Close).await;
        Ok(())
    }
}

/// Actor task owning session state and running turn iterations (§5.7).
pub struct SessionActor {
    session_id: Option<SessionId>,
    scope: SessionScope,
    system_prompt: String,
    history: Vec<PersistedMessage>,
    tools: ToolSet,
    provider: Arc<dyn LlmProvider>,
    store: Arc<dyn SessionStore>,
    config: TurnConfig,
    cmd_rx: mpsc::Receiver<SessionCommand>,
    event_tx: mpsc::Sender<SessionEvent>,
    turn_counter: u64,
    session_grants: HashSet<String>,
}

impl SessionActor {
    /// Construct a new session actor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: Option<SessionId>,
        scope: SessionScope,
        system_prompt: String,
        history: Vec<PersistedMessage>,
        tools: ToolSet,
        provider: Arc<dyn LlmProvider>,
        store: Arc<dyn SessionStore>,
        config: TurnConfig,
        cmd_rx: mpsc::Receiver<SessionCommand>,
        event_tx: mpsc::Sender<SessionEvent>,
    ) -> Self {
        Self {
            session_id,
            scope,
            system_prompt,
            history,
            tools,
            provider,
            store,
            config,
            cmd_rx,
            event_tx,
            turn_counter: 0,
            session_grants: HashSet::new(),
        }
    }

    /// Run actor event processing loop until closed.
    pub async fn run(mut self) {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                SessionCommand::Submit { text, reply } => {
                    self.turn_counter += 1;
                    let turn_id = TurnId(self.turn_counter);
                    let _ = reply.send(Ok(turn_id));

                    let cancel_token = CancellationToken::new();

                    let _ = self
                        .event_tx
                        .send(SessionEvent::TurnStarted {
                            turn: turn_id,
                            queued_behind: 0,
                        })
                        .await;

                    // Lazy session initialization on first submission (§5.7)
                    if self.session_id.is_none() {
                        let new_id = SessionId::new();
                        let new_session = NewSession {
                            id: new_id,
                            profile: self.scope.profile.clone(),
                            mode: self.scope.mode,
                            provider: self.provider.model_name().to_string(),
                            model: self.provider.model_name().to_string(),
                            title: None,
                        };

                        if self.store.create(new_session).await.is_ok() {
                            self.session_id = Some(new_id);
                            let _ = self
                                .event_tx
                                .send(SessionEvent::SessionPersisted { session: new_id })
                                .await;
                        }
                    }

                    let history_snapshot = self.history.clone();
                    let grants = self.session_grants.clone();
                    let turn_fut = run_turn(
                        self.session_id,
                        turn_id,
                        &self.system_prompt,
                        history_snapshot,
                        &text,
                        &self.tools,
                        self.provider.clone(),
                        self.store.clone(),
                        self.event_tx.clone(),
                        &self.config,
                        cancel_token.clone(),
                        grants,
                    );
                    tokio::pin!(turn_fut);

                    loop {
                        tokio::select! {
                            cmd_opt = self.cmd_rx.recv() => {
                                match cmd_opt {
                                    Some(SessionCommand::Cancel) => {
                                        cancel_token.cancel();
                                    }
                                    Some(SessionCommand::Notify(notice)) => {
                                        let _ = self.event_tx.send(SessionEvent::Notice(notice)).await;
                                    }
                                    Some(SessionCommand::Submit { reply, .. }) => {
                                        let _ = reply.send(Err(ChatError::TurnInProgress));
                                    }
                                    Some(SessionCommand::Rename { title, reply }) => {
                                        let res = if let Some(sid) = self.session_id {
                                            let r = self.store.rename(sid, &title).await.map_err(ChatError::from);
                                            if r.is_ok() {
                                                let _ = self.event_tx.send(SessionEvent::TitleSet { title: title.clone() }).await;
                                            }
                                            r
                                        } else {
                                            Ok(())
                                        };
                                        let _ = reply.send(res);
                                    }
                                    Some(SessionCommand::History { reply }) => {
                                        let _ = reply.send(self.history.clone());
                                    }
                                    Some(SessionCommand::Close) | None => {
                                        cancel_token.cancel();
                                        return;
                                    }
                                }
                            }
                            turn_res = &mut turn_fut => {
                                if let Ok((_, updated_history, updated_grants)) = turn_res {
                                    self.history = updated_history;
                                    self.session_grants = updated_grants;
                                }
                                break;
                            }
                        }
                    }
                }

                SessionCommand::Cancel => {}

                SessionCommand::Notify(notice) => {
                    let _ = self.event_tx.send(SessionEvent::Notice(notice)).await;
                }

                SessionCommand::Rename { title, reply } => {
                    let res = if let Some(sid) = self.session_id {
                        let r = self
                            .store
                            .rename(sid, &title)
                            .await
                            .map_err(ChatError::from);
                        if r.is_ok() {
                            let _ = self
                                .event_tx
                                .send(SessionEvent::TitleSet {
                                    title: title.clone(),
                                })
                                .await;
                        }
                        r
                    } else {
                        Ok(())
                    };
                    let _ = reply.send(res);
                }

                SessionCommand::History { reply } => {
                    let _ = reply.send(self.history.clone());
                }

                SessionCommand::Close => {
                    break;
                }
            }
        }
    }
}
