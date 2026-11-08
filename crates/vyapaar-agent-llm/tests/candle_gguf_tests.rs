use std::path::PathBuf;

use vyapaar_agent_llm::candle_gguf::{CandleGgufConfig, CandleGgufProvider, ENV_GGUF_PATH};
use vyapaar_agent_llm::{ChatMessage, ChatRequest, LlmError, LlmProvider};

#[test]
fn test_candle_gguf_config_defaults() {
    let config = CandleGgufConfig::default();
    assert_eq!(config.model_name, "SmolLM2-360M-Instruct");
    assert_eq!(config.temperature, 0.7);
    assert_eq!(config.top_p, Some(0.9));
    assert_eq!(config.context_window, 8192);
    assert_eq!(config.max_tokens, 1024);
}

#[test]
fn test_candle_gguf_config_builder() {
    let config = CandleGgufConfig::new("/tmp/test-smollm.gguf")
        .with_model_name("SmolLM2-1.7B-Instruct")
        .with_temperature(0.2)
        .with_max_tokens(256);

    assert_eq!(config.model_path, PathBuf::from("/tmp/test-smollm.gguf"));
    assert_eq!(config.model_name, "SmolLM2-1.7B-Instruct");
    assert_eq!(config.temperature, 0.2);
    assert_eq!(config.max_tokens, 256);
}

#[test]
fn test_format_chatml_prompt_smollm_structure() {
    let messages = vec![
        ChatMessage::user("What is NIFTY 50?"),
        ChatMessage::assistant("NIFTY 50 is the benchmark Indian stock market index."),
        ChatMessage::user("Which sectors dominate it?"),
    ];

    let prompt =
        CandleGgufProvider::format_prompt(&messages, Some("Custom system trading instruction"));

    assert!(prompt.contains("<|im_start|>system\nCustom system trading instruction<|im_end|>\n"));
    assert!(prompt.contains("<|im_start|>user\nWhat is NIFTY 50?<|im_end|>\n"));
    assert!(prompt.contains(
        "<|im_start|>assistant\nNIFTY 50 is the benchmark Indian stock market index.<|im_end|>\n"
    ));
    assert!(prompt.contains("<|im_start|>user\nWhich sectors dominate it?<|im_end|>\n"));
    assert!(prompt.ends_with("<|im_start|>assistant\n"));
}

#[tokio::test]
async fn test_provider_nonexistent_model_fails_with_clear_error() {
    let provider = CandleGgufProvider::from_file("/path/that/definitely/does/not/exist.gguf");
    let req = ChatRequest {
        messages: vec![ChatMessage::user("Hello")],
        system_prompt: None,
        tools: Vec::new(),
        temperature: None,
        max_tokens: Some(10),
    };

    let mut stream = provider.chat_stream(&req).await.unwrap();
    use futures::StreamExt;
    let first = stream.next().await;
    assert!(first.is_some());
    match first.unwrap() {
        Err(LlmError::Transport(msg)) => {
            assert!(msg.contains("GGUF model file not found"));
            assert!(msg.contains("/path/that/definitely/does/not/exist.gguf"));
            assert!(msg.contains(ENV_GGUF_PATH));
        }
        other => panic!("expected Transport error for missing file, got {other:?}"),
    }
}
