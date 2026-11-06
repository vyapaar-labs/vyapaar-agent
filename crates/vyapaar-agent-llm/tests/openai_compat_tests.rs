//! Tests for [`OpenAiCompatibleProvider`] against a local axum stub server
//! (no live network, per the 0004 plan's Phase 1 verification guidance).

use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::http::{Response, StatusCode};
use axum::routing::post;
use futures::StreamExt;
use serde_json::Value;

use vyapaar_agent_llm::{
    ChatMessage, ChatRequest, ChatStreamEvent, FreeTierPolicy, LlmProvider,
    OpenAiCompatibleProvider, Usage, llm_config_from_map,
};
use vyapaar_core::Decision;

/// Bind a TCP listener on an ephemeral port and run the given router on it.
async fn spawn_router(app: Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("listener local address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("axum server");
    });
    addr
}

/// Parse the raw request body bytes as JSON.
async fn read_json_body(body: axum::body::Bytes) -> Value {
    serde_json::from_slice(&body).unwrap_or(Value::Null)
}

fn json_response(value: Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(value.to_string()))
        .expect("build response")
}

fn sse_response(body: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .body(Body::from(body))
        .expect("build SSE response")
}

#[tokio::test]
async fn test_generate_parses_hold_decision() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|body: axum::body::Bytes| async move {
            let value = read_json_body(body).await;
            assert_eq!(value["stream"], false);
            assert_eq!(value["model"], "test-model");
            json_response(serde_json::json!({
                "model": "gpt-4o-mini",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "{\"action\":\"hold\",\"reason\":\"calm market\"}"
                    }
                }]
            }))
        }),
    );
    let addr = spawn_router(app).await;

    let provider = OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), None, "test-model");
    let (raw, decision) = provider
        .generate("Act on the market")
        .await
        .expect("generate");
    assert_eq!(
        decision,
        Decision::Hold {
            reason: Some("calm market".into())
        }
    );
    assert!(raw.contains("hold"));
}

#[tokio::test]
async fn test_generate_rejects_non_free_tier_model_when_enforced() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|_: axum::body::Bytes| async move {
            json_response(serde_json::json!({
                "model": "gpt-4-turbo",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "{\"action\":\"hold\"}"
                    }
                }]
            }))
        }),
    );
    let addr = spawn_router(app).await;

    let provider = OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), None, "test-model")
        .with_free_tier_policy(FreeTierPolicy::Enforce);
    let err = provider.generate("Act").await.expect_err("must reject");
    assert!(matches!(
        err,
        vyapaar_agent_llm::LlmError::NonFreeTierModel(_)
    ));
}

#[tokio::test]
async fn test_chat_stream_emits_text_and_message_end() {
    let sse = [
        "data: {\"id\":\"1\",\"model\":\"gemini-2.0-flash\",\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"1\",\"model\":\"gemini-2.0-flash\",\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"1\",\"model\":\"gemini-2.0-flash\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n",
        "data: [DONE]\n\n",
    ]
    .concat();
    let sse_clone = sse.clone();

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |body: axum::body::Bytes| async move {
            let value = read_json_body(body).await;
            assert_eq!(value["stream"], true);
            assert_eq!(value["messages"][0]["role"], "user");
            sse_response(sse_clone.clone())
        }),
    );
    let addr = spawn_router(app).await;

    let provider = OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), None, "test-model")
        .with_free_tier_policy(FreeTierPolicy::Enforce);
    let request = ChatRequest {
        messages: vec![ChatMessage::user("hello")],
        system_prompt: None,
        tools: Vec::new(),
        temperature: None,
        max_tokens: None,
    };

    let mut stream = provider.chat_stream(&request).await.expect("chat_stream");
    let mut texts = Vec::new();
    let mut end_message = None;
    let mut usage = Usage::default();
    while let Some(event) = stream.next().await {
        match event.expect("stream event") {
            ChatStreamEvent::TextDelta { text } => texts.push(text),
            ChatStreamEvent::MessageEnd {
                message, usage: u, ..
            } => {
                end_message = Some(message);
                usage = u;
            }
            _ => {}
        }
    }

    assert_eq!(texts, vec!["Hel".to_string(), "lo".to_string()]);
    let message = end_message.expect("MessageEnd emitted");
    assert_eq!(message.content, "Hello");
    assert_eq!(usage.output_tokens, 5);
}

#[tokio::test]
async fn test_chat_stream_assembles_tool_calls() {
    let sse = [
        "data: {\"model\":\"gpt-4o-mini\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"market_quote\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"model\":\"gpt-4o-mini\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"symbol\\\":\\\"TCS\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"model\":\"gpt-4o-mini\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n",
    ]
    .concat();
    let sse_clone = sse.clone();

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move |_body: axum::body::Bytes| async move { sse_response(sse_clone.clone()) }),
    );
    let addr = spawn_router(app).await;

    let provider = OpenAiCompatibleProvider::new(format!("http://{addr}/v1"), None, "test-model");
    let request = ChatRequest {
        messages: vec![ChatMessage::user("quote TCS")],
        system_prompt: None,
        tools: Vec::new(),
        temperature: None,
        max_tokens: None,
    };

    let mut stream = provider.chat_stream(&request).await.expect("chat_stream");
    let mut streaming_name = None;
    let mut chunks = Vec::new();
    let mut ready = None;
    let mut stop_reason = None;
    while let Some(event) = stream.next().await {
        match event.expect("stream event") {
            ChatStreamEvent::ToolCallStreaming { call_id, name } => {
                assert_eq!(call_id, "call_1");
                streaming_name = Some(name);
            }
            ChatStreamEvent::ToolCallChunk {
                arguments_delta, ..
            } => {
                chunks.push(arguments_delta);
            }
            ChatStreamEvent::ToolCallReady {
                name,
                arguments,
                is_malformed,
                ..
            } => {
                assert!(!is_malformed);
                assert_eq!(name, "market_quote");
                ready = Some(arguments);
            }
            ChatStreamEvent::MessageEnd {
                stop_reason: sr,
                message,
                ..
            } => {
                stop_reason = Some(sr);
                assert_eq!(message.tool_calls.len(), 1);
                assert_eq!(message.tool_calls[0].id, "call_1");
            }
            _ => {}
        }
    }

    assert_eq!(streaming_name.as_deref(), Some("market_quote"));
    assert_eq!(chunks, vec!["{\"symbol\":\"TCS\"}".to_string()]);
    assert_eq!(ready, Some(serde_json::json!({ "symbol": "TCS" })));
    assert_eq!(stop_reason, Some(vyapaar_agent_llm::StopReason::ToolUse));
}

#[test]
fn test_llm_config_from_map_resolves_env() {
    let env = |key: &str| match key {
        "VYAPAAR_LLM_BASE_URL" => Some("http://localhost:8080/v1".to_string()),
        "VYAPAAR_LLM_MODEL" => Some("gpt-4o-mini".to_string()),
        "VYAPAAR_LLM_API_KEY" => Some("secret".to_string()),
        _ => None,
    };
    let config = llm_config_from_map(&env).expect("resolve");
    assert_eq!(config.base_url, "http://localhost:8080/v1");
    assert_eq!(config.model, "gpt-4o-mini");
    assert_eq!(config.api_key.as_deref(), Some("secret"));
}

#[test]
fn test_llm_config_from_map_requires_base_url() {
    let env = |_key: &str| None;
    let err = llm_config_from_map(&env).expect_err("missing base url");
    assert!(err.to_string().contains("VYAPAAR_LLM_BASE_URL"));
}
