//! Turn state machine, tool dispatch loop, and execution policies.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use futures::StreamExt;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use vyapaar_agent_core::{PersistedMessage, SessionId, SessionStore, ToolError};
use vyapaar_agent_llm::{
    ChatMessage, ChatRequest, ChatRole, ChatStreamEvent, LlmProvider, LlmToolCall, LlmToolResult,
    StopReason, Usage,
};
use vyapaar_agent_replay::hash_request;

use crate::cancel::CancellationToken;
use crate::error::{ChatError, ChatErrorView};
use crate::event::{
    CallId, SessionEvent, ToolCallView, ToolDisplay, ToolOutcome, TurnId, TurnOutcome,
};
use crate::permission::{
    ConfirmOverride, PermissionDecision, PermissionPolicy, PermissionPrompt, PermissionResponder,
    ToolRisk,
};
use crate::tool::{PlanContext, ToolContext, ToolSet};

const MAX_TOOL_OUTPUT_BYTES: usize = 64 * 1024; // 64 KiB cap per tool result

/// Point at which a turn was cancelled.
#[derive(Debug, Clone)]
pub enum CancelPoint {
    /// Cancelled before sending prompt to provider.
    BeforeRequest,
    /// Cancelled while streaming text from provider.
    Streaming {
        /// Accumulated partial text up to cancellation.
        partial_text: String,
    },
    /// Cancelled during tool dispatch.
    Dispatching {
        /// Requested calls.
        calls: Vec<LlmToolCall>,
        /// Completed results so far.
        done: Vec<LlmToolResult>,
    },
}

/// Internal states of the turn state machine (§5.4).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum TurnState {
    Preparing,
    Requesting {
        round: u32,
        attempt: u32,
        compacted: bool,
    },
    Streaming {
        round: u32,
        attempt: u32,
    },
    Committing {
        round: u32,
        message: ChatMessage,
        stop_reason: StopReason,
        usage: Usage,
    },
    Dispatching {
        round: u32,
        calls: Vec<LlmToolCall>,
    },
    Cancelling {
        at: CancelPoint,
    },
    Finished(TurnOutcome),
}

/// Configuration parameters for conversational turn loops.
#[derive(Debug, Clone)]
pub struct TurnConfig {
    /// Maximum autonomous inference rounds before forcing a pause.
    pub max_rounds: u32,
    /// Mid-stream retry attempts for retryable errors.
    pub stream_retries: u32,
    /// Default execution timeout for tools.
    pub default_tool_timeout: Duration,
    /// Interactive or unattended permission policy.
    pub permission_policy: PermissionPolicy,
}

impl Default for TurnConfig {
    fn default() -> Self {
        Self {
            max_rounds: 24,
            stream_retries: 2,
            default_tool_timeout: Duration::from_secs(30),
            permission_policy: PermissionPolicy::Interactive,
        }
    }
}

/// Executes a complete conversational assistant turn until completion or cancellation.
#[allow(clippy::too_many_arguments)]
pub async fn run_turn(
    session_id: Option<SessionId>,
    turn_id: TurnId,
    system_prompt: &str,
    mut history: Vec<PersistedMessage>,
    user_input: &str,
    tools: &ToolSet,
    provider: Arc<dyn LlmProvider>,
    store: Arc<dyn SessionStore>,
    event_tx: mpsc::Sender<SessionEvent>,
    config: &TurnConfig,
    cancel_token: CancellationToken,
    mut session_grants: HashSet<String>,
) -> Result<(TurnOutcome, Vec<PersistedMessage>, HashSet<String>), ChatError> {
    let mut total_usage = Usage::default();
    let mut current_round = 1u32;
    let mut error_counts: HashMap<String, (String, u32)> = HashMap::new();

    // 1. Write-ahead persist user message
    let user_msg_seq = (history.len() as u64) + 1;
    let user_persisted = PersistedMessage {
        seq: user_msg_seq,
        role: "user".to_string(),
        content: serde_json::json!({ "text": user_input }),
        request_hash: None,
        input_tokens: None,
        output_tokens: None,
        created_at: Utc::now(),
    };

    if let Some(sid) = session_id {
        let _ = store
            .append(sid, std::slice::from_ref(&user_persisted))
            .await;
    }
    history.push(user_persisted);

    let mut turn_state = TurnState::Requesting {
        round: current_round,
        attempt: 1,
        compacted: false,
    };

    let tool_specs = tools.specs();

    loop {
        if cancel_token.is_cancelled() {
            turn_state = TurnState::Cancelling {
                at: CancelPoint::BeforeRequest,
            };
        }

        match turn_state {
            TurnState::Preparing => {
                turn_state = TurnState::Requesting {
                    round: current_round,
                    attempt: 1,
                    compacted: false,
                };
            }

            TurnState::Requesting {
                round,
                attempt,
                compacted: _,
            } => {
                if round > config.max_rounds {
                    let outcome = TurnOutcome::RoundLimit {
                        limit: config.max_rounds,
                    };
                    turn_state = TurnState::Finished(outcome);
                    continue;
                }

                let _ = event_tx
                    .send(SessionEvent::RoundStarted {
                        turn: turn_id,
                        round,
                    })
                    .await;

                // Build LLM ChatRequest from history
                let messages: Vec<ChatMessage> = history
                    .iter()
                    .map(|pm| {
                        let role = match pm.role.as_str() {
                            "user" => ChatRole::User,
                            "assistant" => ChatRole::Assistant,
                            "tool" => ChatRole::Tool,
                            _ => ChatRole::System,
                        };
                        let text = pm
                            .content
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let mut msg = ChatMessage {
                            role,
                            content: text,
                            tool_calls: Vec::new(),
                            tool_call_id: None,
                        };
                        if let Some(tc_array) =
                            pm.content.get("tool_calls").and_then(|v| v.as_array())
                        {
                            for tc in tc_array {
                                if let Ok(parsed_tc) =
                                    serde_json::from_value::<LlmToolCall>(tc.clone())
                                {
                                    msg.tool_calls.push(parsed_tc);
                                }
                            }
                        }
                        if let Some(cid) = pm.content.get("tool_call_id").and_then(|v| v.as_str()) {
                            msg.tool_call_id = Some(cid.to_string());
                        }
                        msg
                    })
                    .collect();

                let req = ChatRequest {
                    messages,
                    system_prompt: Some(system_prompt.to_string()),
                    tools: tool_specs.clone(),
                    temperature: None,
                    max_tokens: None,
                };

                let stream_res = tokio::select! {
                    _ = cancel_token.cancelled() => {
                        turn_state = TurnState::Cancelling { at: CancelPoint::BeforeRequest };
                        continue;
                    }
                    res = provider.chat_stream(&req) => res
                };

                match stream_res {
                    Ok(mut stream) => {
                        let mut accumulated_text = String::new();
                        let mut assembled_tool_calls = Vec::new();
                        let mut stream_stop_reason = StopReason::EndTurn;
                        let mut stream_usage = Usage::default();

                        let mut stream_failed_err = None;

                        while let Some(item) = tokio::select! {
                            _ = cancel_token.cancelled() => {
                                None
                            }
                            next_item = stream.next() => next_item
                        } {
                            match item {
                                Ok(ChatStreamEvent::TextDelta { text }) => {
                                    accumulated_text.push_str(&text);
                                    let _ = event_tx
                                        .send(SessionEvent::TextDelta {
                                            turn: turn_id,
                                            round,
                                            text,
                                        })
                                        .await;
                                }
                                Ok(ChatStreamEvent::ThinkingDelta { text }) => {
                                    let _ = event_tx
                                        .send(SessionEvent::ThinkingDelta {
                                            turn: turn_id,
                                            round,
                                            text,
                                        })
                                        .await;
                                }
                                Ok(ChatStreamEvent::ToolCallStreaming { call_id, name }) => {
                                    let _ = event_tx
                                        .send(SessionEvent::ToolCallStreaming {
                                            turn: turn_id,
                                            round,
                                            call_id: CallId::new(call_id),
                                            name,
                                        })
                                        .await;
                                }
                                Ok(ChatStreamEvent::ToolCallChunk { .. }) => {}
                                Ok(ChatStreamEvent::ToolCallReady {
                                    call_id,
                                    name,
                                    arguments,
                                    raw_arguments,
                                    is_malformed: _,
                                }) => {
                                    let view = ToolCallView {
                                        call_id: CallId::new(call_id.clone()),
                                        name: name.clone(),
                                        arguments: arguments.clone(),
                                    };
                                    let _ = event_tx
                                        .send(SessionEvent::ToolCallReady {
                                            turn: turn_id,
                                            round,
                                            call: view,
                                        })
                                        .await;

                                    assembled_tool_calls.push(LlmToolCall {
                                        id: call_id,
                                        name,
                                        arguments: raw_arguments,
                                    });
                                }
                                Ok(ChatStreamEvent::Usage(u)) => {
                                    stream_usage = u;
                                    let _ = event_tx
                                        .send(SessionEvent::Usage {
                                            turn: turn_id,
                                            round,
                                            usage: u,
                                        })
                                        .await;
                                }
                                Ok(ChatStreamEvent::MessageEnd {
                                    stop_reason,
                                    message,
                                    usage,
                                }) => {
                                    stream_stop_reason = stop_reason;
                                    stream_usage = usage;
                                    if !message.tool_calls.is_empty()
                                        && assembled_tool_calls.is_empty()
                                    {
                                        assembled_tool_calls = message.tool_calls;
                                    }
                                }
                                Err(err) => {
                                    stream_failed_err = Some(err);
                                    break;
                                }
                            }
                        }

                        if cancel_token.is_cancelled() {
                            turn_state = TurnState::Cancelling {
                                at: CancelPoint::Streaming {
                                    partial_text: accumulated_text,
                                },
                            };
                            continue;
                        }

                        if let Some(err) = stream_failed_err {
                            if attempt <= config.stream_retries {
                                let _ = event_tx
                                    .send(SessionEvent::RoundRetrying {
                                        turn: turn_id,
                                        round,
                                        attempt,
                                        reason: err.to_string(),
                                        discard_partial: true,
                                    })
                                    .await;
                                turn_state = TurnState::Requesting {
                                    round,
                                    attempt: attempt + 1,
                                    compacted: false,
                                };
                                continue;
                            } else {
                                turn_state = TurnState::Finished(TurnOutcome::Failed {
                                    error: ChatErrorView {
                                        kind: "llm_stream_error".into(),
                                        message: err.to_string(),
                                    },
                                });
                                continue;
                            }
                        }

                        total_usage.input_tokens += stream_usage.input_tokens;
                        total_usage.output_tokens += stream_usage.output_tokens;

                        let assistant_msg = if !assembled_tool_calls.is_empty() {
                            ChatMessage::assistant_with_tools(
                                accumulated_text,
                                assembled_tool_calls,
                            )
                        } else {
                            ChatMessage::assistant(accumulated_text)
                        };

                        turn_state = TurnState::Committing {
                            round,
                            message: assistant_msg,
                            stop_reason: stream_stop_reason,
                            usage: stream_usage,
                        };
                    }
                    Err(err) => {
                        if attempt <= config.stream_retries {
                            let _ = event_tx
                                .send(SessionEvent::RoundRetrying {
                                    turn: turn_id,
                                    round,
                                    attempt,
                                    reason: err.to_string(),
                                    discard_partial: true,
                                })
                                .await;
                            turn_state = TurnState::Requesting {
                                round,
                                attempt: attempt + 1,
                                compacted: false,
                            };
                        } else {
                            turn_state = TurnState::Finished(TurnOutcome::Failed {
                                error: ChatErrorView {
                                    kind: "llm_request_error".into(),
                                    message: err.to_string(),
                                },
                            });
                        }
                    }
                }
            }

            TurnState::Streaming { .. } => {}

            TurnState::Committing {
                round,
                message,
                stop_reason,
                usage,
            } => {
                // Compute canonical request hash (§5.8)
                let msgs_json = serde_json::to_value(&history).unwrap_or(Value::Null);
                let opts_json = serde_json::json!({ "model": provider.model_name() });
                let req_hash = hash_request(system_prompt, &tool_specs, &msgs_json, &opts_json);

                let next_seq = (history.len() as u64) + 1;
                let persisted_assistant = PersistedMessage {
                    seq: next_seq,
                    role: "assistant".to_string(),
                    content: serde_json::json!({
                        "text": message.content,
                        "tool_calls": message.tool_calls,
                    }),
                    request_hash: Some(req_hash),
                    input_tokens: Some(usage.input_tokens),
                    output_tokens: Some(usage.output_tokens),
                    created_at: Utc::now(),
                };

                if let Some(sid) = session_id {
                    let _ = store
                        .append(sid, std::slice::from_ref(&persisted_assistant))
                        .await;
                }
                history.push(persisted_assistant);

                let _ = event_tx
                    .send(SessionEvent::AssistantCommitted {
                        turn: turn_id,
                        round,
                        seq: Some(next_seq),
                        stop_reason: stop_reason.clone(),
                    })
                    .await;

                if !message.tool_calls.is_empty() {
                    turn_state = TurnState::Dispatching {
                        round,
                        calls: message.tool_calls,
                    };
                } else {
                    let outcome = match stop_reason {
                        StopReason::MaxTokens => TurnOutcome::MaxTokens,
                        StopReason::Refusal | StopReason::ContentFilter => TurnOutcome::Refused {
                            reason: format!("{:?}", stop_reason),
                        },
                        _ => TurnOutcome::Completed,
                    };
                    turn_state = TurnState::Finished(outcome);
                }
            }

            TurnState::Dispatching {
                round: _,
                ref calls,
            } => {
                let mut round_results = Vec::new();

                for call in calls {
                    if cancel_token.is_cancelled() {
                        turn_state = TurnState::Cancelling {
                            at: CancelPoint::Dispatching {
                                calls: calls.clone(),
                                done: round_results.clone(),
                            },
                        };
                        break;
                    }

                    let call_id = CallId::new(call.id.clone());
                    let tool_name = call.name.clone();
                    let input_val: Value =
                        serde_json::from_str(&call.arguments).unwrap_or(Value::Null);

                    let chat_tool = tools.get(&tool_name);

                    if chat_tool.is_none() {
                        let err_msg = format!("Unknown tool `{tool_name}`");
                        let _ = event_tx
                            .send(SessionEvent::ToolFinished {
                                turn: turn_id,
                                call_id: call_id.clone(),
                                name: tool_name.clone(),
                                outcome: ToolOutcome::Error(err_msg.clone()),
                                elapsed_ms: 0,
                                display: None,
                            })
                            .await;

                        round_results.push(LlmToolResult {
                            call_id: call.id.clone(),
                            output: err_msg,
                            is_error: true,
                        });
                        continue;
                    }

                    let tool = chat_tool.unwrap();
                    let traits = tool.traits();
                    let plan = tool
                        .plan(&input_val, &PlanContext::new(session_id.as_ref()))
                        .unwrap_or_default();

                    let effective_risk = plan.risk_override.unwrap_or(traits.risk);

                    // Permission check
                    let needs_prompt = match traits.confirm {
                        ConfirmOverride::Always => true,
                        ConfirmOverride::Never => false,
                        ConfirmOverride::Default => {
                            effective_risk != ToolRisk::ReadOnly
                                && !session_grants.contains(&tool_name)
                        }
                    };

                    let permitted = if needs_prompt {
                        match &config.permission_policy {
                            PermissionPolicy::NonInteractive { allow } => {
                                allow.contains(&tool_name)
                            }
                            PermissionPolicy::Interactive => {
                                let (resp_tx, resp_rx) = oneshot::channel();
                                let prompt = PermissionPrompt {
                                    tool: tool_name.clone(),
                                    risk: effective_risk,
                                    summary: plan.summary.clone(),
                                    input: input_val.clone(),
                                    responder: Some(PermissionResponder::new(resp_tx)),
                                };

                                let _ = event_tx
                                    .send(SessionEvent::PermissionRequested(prompt))
                                    .await;

                                match resp_rx.await {
                                    Ok(PermissionDecision::AllowOnce) => true,
                                    Ok(PermissionDecision::AllowForSession) => {
                                        session_grants.insert(tool_name.clone());
                                        true
                                    }
                                    Ok(PermissionDecision::Deny { feedback }) => {
                                        let reason = feedback
                                            .unwrap_or_else(|| "Denied by user policy".to_string());
                                        let _ = event_tx
                                            .send(SessionEvent::ToolFinished {
                                                turn: turn_id,
                                                call_id: call_id.clone(),
                                                name: tool_name.clone(),
                                                outcome: ToolOutcome::Denied(reason.clone()),
                                                elapsed_ms: 0,
                                                display: None,
                                            })
                                            .await;

                                        round_results.push(LlmToolResult {
                                            call_id: call.id.clone(),
                                            output: format!("Execution denied: {reason}"),
                                            is_error: true,
                                        });
                                        continue;
                                    }
                                    Ok(PermissionDecision::DenyAndStop) => {
                                        turn_state = TurnState::Finished(TurnOutcome::Stopped);
                                        break;
                                    }
                                    Err(_) => {
                                        let _ = event_tx
                                            .send(SessionEvent::ToolFinished {
                                                turn: turn_id,
                                                call_id: call_id.clone(),
                                                name: tool_name.clone(),
                                                outcome: ToolOutcome::Denied(
                                                    "Permission prompt cancelled".into(),
                                                ),
                                                elapsed_ms: 0,
                                                display: None,
                                            })
                                            .await;

                                        round_results.push(LlmToolResult {
                                            call_id: call.id.clone(),
                                            output: "Permission prompt was dismissed".into(),
                                            is_error: true,
                                        });
                                        continue;
                                    }
                                }
                            }
                        }
                    } else {
                        true
                    };

                    if !permitted {
                        let _ = event_tx
                            .send(SessionEvent::ToolFinished {
                                turn: turn_id,
                                call_id: call_id.clone(),
                                name: tool_name.clone(),
                                outcome: ToolOutcome::Denied("Not allowed by policy".into()),
                                elapsed_ms: 0,
                                display: None,
                            })
                            .await;

                        round_results.push(LlmToolResult {
                            call_id: call.id.clone(),
                            output: "Execution denied by automated policy".into(),
                            is_error: true,
                        });
                        continue;
                    }

                    // Execute tool
                    let _ = event_tx
                        .send(SessionEvent::ToolStarted {
                            turn: turn_id,
                            call_id: call_id.clone(),
                            name: tool_name.clone(),
                            summary: plan.summary,
                            eager: false,
                        })
                        .await;

                    let tool_ctx =
                        ToolContext::new(session_id, turn_id, call_id.clone(), event_tx.clone());
                    let timeout_dur = traits.timeout.unwrap_or(config.default_tool_timeout);
                    let start_time = Instant::now();

                    let exec_fut = tool.execute(input_val, tool_ctx);
                    let mut cancelled = false;
                    let exec_res = tokio::select! {
                        _ = cancel_token.cancelled() => {
                            cancelled = true;
                            Err(ToolError::Execution(tool_name.clone(), "cancelled".into()))
                        }
                        res = tokio::time::timeout(timeout_dur, exec_fut) => match res {
                            Ok(inner) => inner,
                            Err(_) => Err(ToolError::Timeout(tool_name.clone())),
                        }
                    };

                    if cancelled {
                        turn_state = TurnState::Cancelling {
                            at: CancelPoint::Dispatching {
                                calls: calls.clone(),
                                done: round_results.clone(),
                            },
                        };
                        break;
                    }

                    let elapsed_ms = start_time.elapsed().as_millis() as u64;

                    match exec_res {
                        Ok(output) => {
                            let mut payload_str =
                                serde_json::to_string(&output.payload).unwrap_or_default();
                            if payload_str.len() > MAX_TOOL_OUTPUT_BYTES {
                                payload_str.truncate(MAX_TOOL_OUTPUT_BYTES);
                                payload_str.push_str(
                                    "
[truncated: content exceeded 64 KiB]",
                                );
                            }

                            let outcome = if output.is_error() {
                                ToolOutcome::Error(payload_str.clone())
                            } else {
                                ToolOutcome::Success
                            };

                            let _ = event_tx
                                .send(SessionEvent::ToolFinished {
                                    turn: turn_id,
                                    call_id: call_id.clone(),
                                    name: tool_name.clone(),
                                    outcome,
                                    elapsed_ms,
                                    display: Some(ToolDisplay::Text(payload_str.clone())),
                                })
                                .await;

                            round_results.push(LlmToolResult {
                                call_id: call.id.clone(),
                                output: payload_str,
                                is_error: output.is_error(),
                            });
                        }
                        Err(err) => {
                            let mut err_str = err.to_string();

                            // Repeat error check (§5.5)
                            let entry = error_counts
                                .entry(tool_name.clone())
                                .or_insert((err_str.clone(), 0));
                            if entry.0 == err_str {
                                entry.1 += 1;
                                if entry.1 >= 3 {
                                    err_str.push_str(
                                        "
This tool has failed repeatedly; stop retrying and tell the user.",
                                    );
                                }
                            } else {
                                *entry = (err_str.clone(), 1);
                            }

                            let _ = event_tx
                                .send(SessionEvent::ToolFinished {
                                    turn: turn_id,
                                    call_id: call_id.clone(),
                                    name: tool_name.clone(),
                                    outcome: ToolOutcome::Error(err_str.clone()),
                                    elapsed_ms,
                                    display: None,
                                })
                                .await;

                            round_results.push(LlmToolResult {
                                call_id: call.id.clone(),
                                output: err_str,
                                is_error: true,
                            });
                        }
                    }
                }

                if matches!(turn_state, TurnState::Cancelling { .. }) {
                    continue;
                }

                // Persist tool results message
                let results_seq = (history.len() as u64) + 1;
                let results_persisted = PersistedMessage {
                    seq: results_seq,
                    role: "tool".to_string(),
                    content: serde_json::json!({
                        "results": round_results,
                    }),
                    request_hash: None,
                    input_tokens: None,
                    output_tokens: None,
                    created_at: Utc::now(),
                };

                if let Some(sid) = session_id {
                    let _ = store
                        .append(sid, std::slice::from_ref(&results_persisted))
                        .await;
                }
                history.push(results_persisted);

                current_round += 1;
                turn_state = TurnState::Requesting {
                    round: current_round,
                    attempt: 1,
                    compacted: false,
                };
            }

            TurnState::Cancelling { at } => {
                if let CancelPoint::Dispatching { calls, done } = at {
                    let mut completed_ids: HashSet<String> =
                        done.iter().map(|r| r.call_id.clone()).collect();
                    let mut synthetic_results = done;

                    for call in calls {
                        if !completed_ids.contains(&call.id) {
                            synthetic_results.push(LlmToolResult {
                                call_id: call.id.clone(),
                                output: "not executed: turn cancelled before call completed"
                                    .to_string(),
                                is_error: true,
                            });
                            completed_ids.insert(call.id);
                        }
                    }

                    let repair_seq = (history.len() as u64) + 1;
                    let repair_msg = PersistedMessage {
                        seq: repair_seq,
                        role: "tool".to_string(),
                        content: serde_json::json!({
                            "results": synthetic_results,
                            "synthetic": true,
                        }),
                        request_hash: None,
                        input_tokens: None,
                        output_tokens: None,
                        created_at: Utc::now(),
                    };

                    if let Some(sid) = session_id {
                        let _ = store.append(sid, std::slice::from_ref(&repair_msg)).await;
                    }
                    history.push(repair_msg);
                }

                turn_state = TurnState::Finished(TurnOutcome::Cancelled);
            }

            TurnState::Finished(outcome) => {
                let _ = event_tx
                    .send(SessionEvent::TurnEnded {
                        turn: turn_id,
                        outcome: outcome.clone(),
                        usage: total_usage,
                        rounds: current_round,
                        persisted: session_id.is_some(),
                    })
                    .await;

                return Ok((outcome, history, session_grants));
            }
        }
    }
}
