//! OpenAI-compatible HTTP LLM provider (the "OmniRoute" free-tier router backend).
//!
//! Implements [`LlmProvider`] over any OpenAI-compatible `/chat/completions`
//! endpoint using the workspace's standard `reqwest` 0.12 / `rustls-tls`
//! convention (feature-identical to `vyapaar-exec-zerodha` / `vyapaar-exec-upstox`).
//!
//! - [`OpenAiCompatibleProvider::generate`] issues a non-streaming completion and
//!   parses the response through [`vyapaar_agent_core::decision::DecisionParser::parse_str`]
//!   (reused, never reimplemented).
//! - [`OpenAiCompatibleProvider::chat_stream`] consumes the OpenAI SSE stream and
//!   maps `delta.content` → [`ChatStreamEvent::TextDelta`] and `delta.tool_calls`
//!   chunks → [`ChatStreamEvent::ToolCallChunk`] / [`ChatStreamEvent::ToolCallReady`].
//! - [`FreeTierPolicy`] guards against the router silently routing to a paid model:
//!   `Off` (default), `Warn` (log only), or `Enforce` (reject).

use std::collections::VecDeque;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use futures::{StreamExt, stream};
use reqwest::Client;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};

use vyapaar_agent_core::DecisionParser;
use vyapaar_core::Decision;

use crate::{
    ChatMessage, ChatRequest, ChatRole, ChatStreamEvent, EventStream, LlmError, LlmProvider,
    LlmToolCall, StopReason, Usage,
};

/// Common free-tier model name fragments accepted by the guardrail.
///
/// Matches by lowercase substring so router-qualified names such as
/// `openai/gpt-4o-mini` and `google/gemini-2.0-flash` are recognized.
const FREE_TIER_MODEL_FRAGMENTS: &[&str] = &[
    // OpenAI
    "gpt-4o-mini",
    "gpt-4.1-nano",
    "gpt-4.1-mini",
    "gpt-5-nano",
    "gpt-5-mini",
    "o4-mini",
    // Google Gemini
    "gemini-2.0-flash",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "gemini-2.0-flash-lite",
    // Anthropic
    "claude-3-5-haiku",
    "claude-3-haiku",
    "claude-haiku",
    // Groq / Llama
    "llama-3.3-70b-versatile",
    "llama-3.1-8b-instant",
    "llama-3.2-3b-preview",
    "llama-3.2-1b-preview",
    "llama-3.3-70b",
    // Mistral / others
    "mixtral-8x7b-32768",
    "qwen2.5-coder-32b",
    "deepseek-r1-distill-qwen-32b",
    "deepseek-chat",
    "deepseek-v3",
    "deepseek-reasoner",
];

/// Returns `true` when a reported model name matches a known free-tier fragment.
pub fn is_free_tier_model(model: &str) -> bool {
    let model_lc = model.to_ascii_lowercase();
    FREE_TIER_MODEL_FRAGMENTS
        .iter()
        .any(|frag| model_lc.contains(frag))
}

/// Free-tier guardrail policy applied to reported response models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FreeTierPolicy {
    /// No guardrail; any reported model is accepted.
    #[default]
    Off,
    /// Log a warning when a non-free-tier model is reported; still proceed.
    Warn,
    /// Reject with [`LlmError::NonFreeTierModel`] when a non-free-tier model is reported.
    Enforce,
}

/// An LLM provider speaking the OpenAI-compatible HTTP chat protocol.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    base_url: String,
    api_key: Option<String>,
    model: String,
    http: Client,
    free_tier: FreeTierPolicy,
    timeout: Duration,
    structured_output: bool,
}

impl OpenAiCompatibleProvider {
    /// Create a provider for the given endpoint root and model name.
    ///
    /// `base_url` is the OpenAI-compatible root, e.g. `http://localhost:11434/v1`;
    /// `/chat/completions` is appended by the provider.
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            http: Client::new(),
            free_tier: FreeTierPolicy::default(),
            timeout: Duration::from_secs(120),
            structured_output: false,
        }
    }

    /// Attach a free-tier guardrail policy.
    pub fn with_free_tier_policy(mut self, policy: FreeTierPolicy) -> Self {
        self.free_tier = policy;
        self
    }

    /// Set the per-request HTTP timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Enable OpenAI `response_format: {"type": "json_object"}` structured
    /// output on [`OpenAiCompatibleProvider::generate`]. Disabled by default for
    /// maximum router compatibility (plain JSON-mode prompting instead).
    pub fn with_structured_output(mut self, enabled: bool) -> Self {
        self.structured_output = enabled;
        self
    }

    /// Human-readable identifier for this endpoint (the base URL).
    pub fn provider_name(&self) -> &str {
        &self.base_url
    }

    /// Full `{base_url}/chat/completions` URL.
    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// POST a JSON body to the completions endpoint with optional bearer auth.
    async fn post(&self, body: &Value) -> Result<reqwest::Response, LlmError> {
        let mut req = self
            .http
            .post(self.completions_url())
            .json(body)
            .timeout(self.timeout);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        req.send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))
    }

    /// Apply the free-tier guardrail to a reported model name.
    fn check_model(&self, model: &str) -> Result<(), LlmError> {
        match self.free_tier {
            FreeTierPolicy::Off => Ok(()),
            FreeTierPolicy::Warn => {
                if !is_free_tier_model(model) {
                    tracing::warn!(model, "endpoint reported a non-free-tier model");
                }
                Ok(())
            }
            FreeTierPolicy::Enforce => {
                if is_free_tier_model(model) {
                    Ok(())
                } else {
                    Err(LlmError::NonFreeTierModel(model.to_string()))
                }
            }
        }
    }

    /// Build the streaming chat-completions request body from a [`ChatRequest`].
    fn build_chat_payload(&self, request: &ChatRequest) -> Value {
        let mut messages: Vec<Value> = request.messages.iter().map(openai_message).collect();
        if let Some(system) = &request.system_prompt {
            messages.insert(0, json!({ "role": "system", "content": system }));
        }

        let mut body = json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });
        if let Some(temperature) = request.temperature {
            if let Some(f) = temperature.to_f64() {
                body["temperature"] = json!(f);
            }
        }
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if !request.tools.is_empty() {
            let tools: Vec<Value> = request
                .tools
                .iter()
                .map(|spec| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": spec.name,
                            "description": spec.description,
                            "parameters": spec.parameters,
                        },
                    })
                })
                .collect();
            body["tools"] = Value::Array(tools);
        }
        body
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    async fn generate(&self, prompt: &str) -> Result<(String, Decision), LlmError> {
        let mut body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }],
            "temperature": 0.0,
            "stream": false,
        });
        if self.structured_output {
            body["response_format"] = json!({ "type": "json_object" });
        }

        let response = self.post(&body).await?;
        let status = response.status();
        let value: Value = response
            .json()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(map_http_status(status, &value));
        }

        if let Some(model) = value.get("model").and_then(|v| v.as_str()) {
            self.check_model(model)?;
        }

        let content = value
            .pointer("/choices/0/message/content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                LlmError::Transport("response missing choices[0].message.content".to_string())
            })?;

        let decision = DecisionParser::new()
            .parse_str(content)
            .map_err(|e| LlmError::ParsingFailed(e.to_string()))?;

        Ok((content.to_string(), decision))
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<EventStream, LlmError> {
        let body = self.build_chat_payload(request);
        let response = self.post(&body).await?;
        let status = response.status();
        if !status.is_success() {
            let value: Value = response.json().await.unwrap_or(Value::Null);
            return Err(map_http_status(status, &value));
        }

        let bytes: BoxStream<'static, Result<Bytes, reqwest::Error>> =
            Box::pin(response.bytes_stream());
        let state = SseState::new(bytes, self.free_tier);

        let stream = stream::unfold(state, |mut state| async move {
            loop {
                if let Some(item) = state.pending.pop_front() {
                    return Some((item, state));
                }
                if state.done {
                    return None;
                }
                match state.bytes.next().await {
                    Some(Ok(chunk)) => state.feed_chunk(&chunk),
                    Some(Err(err)) => {
                        state.done = true;
                        return Some((Err(LlmError::Transport(err.to_string())), state));
                    }
                    None => {
                        state.finish();
                        state.done = true;
                        if let Some(item) = state.pending.pop_front() {
                            return Some((item, state));
                        }
                        return None;
                    }
                }
            }
        });

        Ok(Box::pin(stream))
    }

    fn context_window(&self) -> usize {
        128_000
    }

    fn supports(&self, feature: &str) -> bool {
        match feature {
            "tool_calls" => true,
            "json" => self.structured_output,
            _ => false,
        }
    }
}

/// Accumulated tool-call state across SSE chunks, keyed by stream `index`.
#[derive(Debug, Default, Clone)]
struct ToolAccum {
    id: String,
    name: String,
    arguments: String,
}

/// Incremental SSE parsing state over the completions byte stream.
struct SseState {
    bytes: BoxStream<'static, Result<Bytes, reqwest::Error>>,
    line_buf: Vec<u8>,
    pending: VecDeque<Result<ChatStreamEvent, LlmError>>,
    text: String,
    tool_calls: Vec<ToolAccum>,
    usage: Usage,
    finish_reason: Option<String>,
    finished: bool,
    done: bool,
    free_tier: FreeTierPolicy,
}

impl SseState {
    fn new(
        bytes: BoxStream<'static, Result<Bytes, reqwest::Error>>,
        free_tier: FreeTierPolicy,
    ) -> Self {
        Self {
            bytes,
            line_buf: Vec::new(),
            pending: VecDeque::new(),
            text: String::new(),
            tool_calls: Vec::new(),
            usage: Usage::default(),
            finish_reason: None,
            finished: false,
            done: false,
            free_tier,
        }
    }

    /// Append a byte chunk and process any complete `data:` lines it contains.
    fn feed_chunk(&mut self, chunk: &Bytes) {
        self.line_buf.extend_from_slice(chunk);
        while let Some(pos) = self.line_buf.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.line_buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&raw);
            let line = line.trim_end_matches(['\r', '\n']).trim();
            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data == "[DONE]" {
                    self.finish();
                    self.done = true;
                    return;
                }
                if self.finished {
                    continue;
                }
                self.handle_chunk(data);
            }
        }
    }

    /// Process one SSE `data:` payload (an OpenAI chat-completion chunk).
    fn handle_chunk(&mut self, data: &str) {
        let value: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => {
                self.pending.push_back(Err(LlmError::Transport(format!(
                    "invalid SSE chunk: {data}"
                ))));
                self.done = true;
                return;
            }
        };

        if let Some(model) = value.get("model").and_then(|v| v.as_str()) {
            if let Err(err) = self.check_model(model) {
                self.pending.push_back(Err(err));
                self.done = true;
                return;
            }
        }

        if let Some(usage) = value.get("usage") {
            self.usage.input_tokens = usage
                .get("prompt_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            self.usage.output_tokens = usage
                .get("completion_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
        }

        let Some(choice) = value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
        else {
            return;
        };

        if let Some(delta) = choice.get("delta") {
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                if !content.is_empty() {
                    self.text.push_str(content);
                    self.pending.push_back(Ok(ChatStreamEvent::TextDelta {
                        text: content.to_string(),
                    }));
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for tc in tool_calls {
                    let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    if self.tool_calls.len() <= index {
                        self.tool_calls.resize(index + 1, ToolAccum::default());
                    }
                    let accum = &mut self.tool_calls[index];

                    if let Some(tc_id) = tc.get("id").and_then(|v| v.as_str()) {
                        if accum.id.is_empty() {
                            accum.id = tc_id.to_string();
                            let name = tc
                                .pointer("/function/name")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            accum.name = name.clone();
                            self.pending
                                .push_back(Ok(ChatStreamEvent::ToolCallStreaming {
                                    call_id: tc_id.to_string(),
                                    name,
                                }));
                        }
                    }
                    if let Some(name) = tc.pointer("/function/name").and_then(|v| v.as_str()) {
                        if accum.name.is_empty() {
                            accum.name = name.to_string();
                        }
                    }
                    let arguments_delta = tc
                        .pointer("/function/arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    if !arguments_delta.is_empty() {
                        accum.arguments.push_str(&arguments_delta);
                        let call_id = accum.id.clone();
                        self.pending.push_back(Ok(ChatStreamEvent::ToolCallChunk {
                            call_id,
                            arguments_delta,
                        }));
                    }
                }
            }
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            if !finish_reason.is_empty() && self.finish_reason.is_none() {
                self.finish_reason = Some(finish_reason.to_string());
                self.finish();
            }
        }
    }

    /// Apply the free-tier guardrail to a reported model name.
    fn check_model(&self, model: &str) -> Result<(), LlmError> {
        match self.free_tier {
            FreeTierPolicy::Off => Ok(()),
            FreeTierPolicy::Warn => {
                if !is_free_tier_model(model) {
                    tracing::warn!(model, "endpoint reported a non-free-tier model");
                }
                Ok(())
            }
            FreeTierPolicy::Enforce => {
                if is_free_tier_model(model) {
                    Ok(())
                } else {
                    Err(LlmError::NonFreeTierModel(model.to_string()))
                }
            }
        }
    }

    /// Emit [`ChatStreamEvent::ToolCallReady`] + [`ChatStreamEvent::MessageEnd`].
    fn finish(&mut self) {
        if self.finished {
            return;
        }
        for accum in self.tool_calls.iter().filter(|a| !a.id.is_empty()) {
            let (arguments, is_malformed) = match serde_json::from_str::<Value>(&accum.arguments) {
                Ok(v) => (v, false),
                Err(_) => (Value::Null, true),
            };
            self.pending.push_back(Ok(ChatStreamEvent::ToolCallReady {
                call_id: accum.id.clone(),
                name: accum.name.clone(),
                arguments,
                raw_arguments: accum.arguments.clone(),
                is_malformed,
            }));
        }

        let tool_calls: Vec<LlmToolCall> = self
            .tool_calls
            .iter()
            .filter(|a| !a.id.is_empty())
            .map(|a| LlmToolCall {
                id: a.id.clone(),
                name: a.name.clone(),
                arguments: a.arguments.clone(),
            })
            .collect();
        let message = if tool_calls.is_empty() {
            ChatMessage::assistant(self.text.clone())
        } else {
            ChatMessage::assistant_with_tools(self.text.clone(), tool_calls)
        };
        let stop_reason = match self.finish_reason.as_deref() {
            Some("tool_calls") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            Some("content_filter") => StopReason::ContentFilter,
            Some("stop") | None => StopReason::EndTurn,
            Some(other) => StopReason::Other(other.to_string()),
        };

        self.pending.push_back(Ok(ChatStreamEvent::MessageEnd {
            stop_reason,
            message,
            usage: self.usage,
        }));
        self.finished = true;
    }
}

/// Map an OpenAI-compatible chat message into the request JSON shape.
fn openai_message(message: &ChatMessage) -> Value {
    let role = match message.role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
    };
    let mut msg = json!({ "role": role, "content": message.content });
    if !message.tool_calls.is_empty() {
        let calls: Vec<Value> = message
            .tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments },
                })
            })
            .collect();
        msg["tool_calls"] = Value::Array(calls);
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        msg["tool_call_id"] = Value::String(tool_call_id.clone());
    }
    msg
}

/// Map an HTTP status and optional error body onto [`LlmError`].
fn map_http_status(status: reqwest::StatusCode, value: &Value) -> LlmError {
    let message = value
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("HTTP {status}"));
    match status.as_u16() {
        401 | 403 => LlmError::Auth,
        429 => LlmError::RateLimited,
        _ => LlmError::Transport(format!("HTTP {status}: {message}")),
    }
}
