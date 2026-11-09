use std::path::PathBuf;

use vyapaar_agent_llm::candle_gguf::{CandleGgufConfig, CandleGgufProvider, ENV_GGUF_PATH};
use vyapaar_agent_llm::{ChatMessage, ChatRequest, LlmError, LlmProvider};

#[test]
fn test_candle_gguf_config_defaults() {
    let config = CandleGgufConfig::default();
    assert_eq!(config.model_name, "Llama-3.2-3B-Instruct");
    assert_eq!(config.temperature, 0.7);
    assert_eq!(config.top_p, Some(0.9));
    assert_eq!(config.context_window, 8192);
    assert_eq!(config.max_tokens, 1024);
}

#[test]
fn test_candle_gguf_config_builder() {
    let config = CandleGgufConfig::new("/tmp/test-smollm.gguf")
        .with_model_name("zenfinance-3b-Instruct")
        .with_temperature(0.2)
        .with_max_tokens(256);

    assert_eq!(config.model_path, PathBuf::from("/tmp/test-smollm.gguf"));
    assert_eq!(config.model_name, "zenfinance-3b-Instruct");
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

    assert!(prompt.contains("system\nCustom system trading instruction"));
    assert!(prompt.contains("user\nWhat is NIFTY 50?"));
    assert!(prompt.contains("assistant\nNIFTY 50 is the benchmark Indian stock market index."));
    assert!(prompt.contains("user\nWhich sectors dominate it?"));
    assert!(prompt.ends_with("assistant\n") || prompt.ends_with("assistant<|end_header_id|>\n\n"));
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

#[tokio::test]
async fn test_live_zenfinance_if_present() {
    let candidates = [
        "../../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
        "../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
        "models/zenfinance-3b-Instruct.Q4_K_M.gguf",
    ];
    let path = candidates.iter().find(|p| std::path::Path::new(p).exists());
    let Some(path) = path else {
        eprintln!("Skipping live zenfinance test: model file not found");
        return;
    };

    let config = CandleGgufConfig::new(*path)
        .with_model_name("zenfinance-3b-Instruct")
        .with_max_tokens(30);
    let provider = CandleGgufProvider::new(config);
    let req = ChatRequest {
        messages: vec![ChatMessage::user("What is a call option in finance?")],
        system_prompt: None,
        tools: Vec::new(),
        temperature: Some(rust_decimal::Decimal::ZERO),
        max_tokens: Some(30),
    };

    let mut stream = provider.chat_stream(&req).await.unwrap();
    use futures::StreamExt;
    let mut tokens_received = 0;
    while let Some(event) = stream.next().await {
        let ev = event.unwrap();
        if let vyapaar_agent_llm::ChatStreamEvent::TextDelta { text } = ev {
            tokens_received += 1;
            print!("{text}");
        }
    }
    assert!(tokens_received > 0);
}

#[tokio::test]
async fn test_live_zenfinance_generate_trade_decision() {
    let candidates = [
        "../../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
        "../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
        "models/zenfinance-3b-Instruct.Q4_K_M.gguf",
    ];
    let path = candidates.iter().find(|p| std::path::Path::new(p).exists());
    let Some(path) = path else {
        eprintln!("Skipping live zenfinance trade decision test: model file not found");
        return;
    };

    let config = CandleGgufConfig::new(*path)
        .with_model_name("zenfinance-3b-Instruct")
        .with_max_tokens(150);
    let provider = CandleGgufProvider::new(config);

    let prompt = "Bullish breakout on INFY. Generate a place_order decision to buy 15 shares of INFY with algo_id SEBI_ALGO_01 at limit price 1550.";
    let (raw_output, decision) = provider
        .generate(prompt)
        .await
        .expect("Decision parsing failed");
    println!(
        "\n[ZenFinance Live Generation]\nRaw output:\n{}\nParsed decision:\n{:?}\n",
        raw_output, decision
    );

    match decision {
        vyapaar_core::Decision::PlaceOrder {
            symbol,
            side,
            qty,
            algo_id,
            ..
        } => {
            assert!(symbol.contains("INFY"), "symbol was {}", symbol);
            assert_eq!(side, vyapaar_core::OrderSide::Buy);
            assert_eq!(qty.0, rust_decimal_macros::dec!(15));
            assert_eq!(algo_id, "SEBI_ALGO_01");
        }
        vyapaar_core::Decision::Hold { reason } => {
            println!("Model opted to Hold with reason: {:?}", reason);
        }
        other => panic!("Unexpected decision type: {other:?}"),
    }
}

#[tokio::test]
async fn test_live_zenfinance_generate_hold_decision() {
    let candidates = [
        "../../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
        "../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
        "models/zenfinance-3b-Instruct.Q4_K_M.gguf",
    ];
    let path = candidates.iter().find(|p| std::path::Path::new(p).exists());
    let Some(path) = path else {
        eprintln!("Skipping live zenfinance hold decision test: model file not found");
        return;
    };

    let config = CandleGgufConfig::new(*path)
        .with_model_name("zenfinance-3b-Instruct")
        .with_max_tokens(100);
    let provider = CandleGgufProvider::new(config);

    let prompt = "Market is choppy with no clear trend in NIFTY 50 before election results. Generate a hold decision.";
    let (raw_output, decision) = provider
        .generate(prompt)
        .await
        .expect("Decision parsing failed");
    println!(
        "\n[ZenFinance Live Hold Generation]\nRaw output:\n{}\nParsed decision:\n{:?}\n",
        raw_output, decision
    );

    match decision {
        vyapaar_core::Decision::Hold { reason } => {
            assert!(
                reason.is_some(),
                "Hold should include an explanation reason"
            );
        }
        other => panic!("Expected Hold decision, got: {other:?}"),
    }
}
