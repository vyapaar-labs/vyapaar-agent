//! In-process embedded Candle GGUF inference provider.
//!
//! Provides pure-Rust CPU/accelerated inference for GGUF models (e.g. LLaMA-3.2, ZenFinance-3B, SmolLM2)
//! using `candle-core`, `candle-transformers`, and `shimmytok`.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use shimmytok::Tokenizer;
use tokio::sync::{Mutex, mpsc};

use vyapaar_agent_core::decision::DecisionParser;
use vyapaar_core::Decision;

use crate::{
    ChatMessage, ChatRequest, ChatRole, ChatStreamEvent, EventStream, LlmError, LlmProvider,
    StopReason, Usage,
};

/// Environment variable for the default GGUF model path.
pub const ENV_GGUF_PATH: &str = "VYAPAAR_GGUF_PATH";

/// Environment variable for GGUF model temperature.
pub const ENV_GGUF_TEMPERATURE: &str = "VYAPAAR_GGUF_TEMPERATURE";

/// Environment variable for GGUF max generated tokens.
pub const ENV_GGUF_MAX_TOKENS: &str = "VYAPAAR_GGUF_MAX_TOKENS";

/// Configuration for in-process Candle GGUF provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandleGgufConfig {
    /// Filesystem path to the `.gguf` model file.
    pub model_path: PathBuf,
    /// Model name/identifier (e.g. "Llama-3.2-3B-Instruct", "zenfinance-3b-Instruct", "SmolLM2-360M-Instruct").
    pub model_name: String,
    /// Sampling temperature (0.0 for deterministic greedy sampling).
    pub temperature: f64,
    /// Top-p nucleus sampling cutoff (None to disable).
    pub top_p: Option<f64>,
    /// Repetition penalty (> 1.0 discourages repeating tokens).
    pub repeat_penalty: f32,
    /// Context window size in tokens.
    pub context_window: usize,
    /// Maximum tokens to generate in one completion.
    pub max_tokens: usize,
    /// Random seed for sampling reproducibility.
    pub seed: u64,
}

impl Default for CandleGgufConfig {
    fn default() -> Self {
        let candidates = [
            "models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
            "models/llama-3.2-3b-instruct-q4_k_m.gguf",
            "../models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
            "../models/llama-3.2-3b-instruct-q4_k_m.gguf",
            "../../models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
            "../../../models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
            "models/zenfinance-3b-Instruct.Q4_K_M.gguf",
            "models/zenfinance-3b-instruct.q4_k_m.gguf",
            "../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
            "../models/zenfinance-3b-instruct.q4_k_m.gguf",
            "../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
            "../../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
            "models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
            "models/smollm2-360m-instruct-q4_k_m.gguf",
            "../models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
            "../models/smollm2-360m-instruct-q4_k_m.gguf",
            "../../models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
            "../../../models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
        ];

        let path = std::env::var(ENV_GGUF_PATH)
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                for c in candidates {
                    let p = Path::new(c);
                    if p.exists() {
                        return p.to_path_buf();
                    }
                }
                PathBuf::from("models/Llama-3.2-3B-Instruct-Q4_K_M.gguf")
            });

        let model_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| {
                let low = s.to_lowercase();
                if low.contains("llama-3.2-3b") || low.contains("llama-3.2") {
                    "Llama-3.2-3B-Instruct".to_string()
                } else if low.contains("zenfinance") {
                    "zenfinance-3b-Instruct".to_string()
                } else if low.contains("smollm2") {
                    "SmolLM2-360M-Instruct".to_string()
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_else(|| "Llama-3.2-3B-Instruct".to_string());

        let temperature = std::env::var(ENV_GGUF_TEMPERATURE)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.7);

        let max_tokens = std::env::var(ENV_GGUF_MAX_TOKENS)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1024);

        Self {
            model_path: path,
            model_name,
            temperature,
            top_p: Some(0.9),
            repeat_penalty: 1.1,
            context_window: 8192,
            max_tokens,
            seed: 42,
        }
    }
}

impl CandleGgufConfig {
    /// Create a config for a specific GGUF model file.
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        let path = model_path.into();
        let model_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| {
                let low = s.to_lowercase();
                if low.contains("llama-3.2-3b") || low.contains("llama-3.2") {
                    "Llama-3.2-3B-Instruct".to_string()
                } else if low.contains("zenfinance") {
                    "zenfinance-3b-Instruct".to_string()
                } else if low.contains("smollm2") {
                    "SmolLM2-360M-Instruct".to_string()
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_else(|| "Llama-3.2-3B-Instruct".to_string());

        Self {
            model_path: path,
            model_name,
            ..Default::default()
        }
    }

    /// Set model name.
    pub fn with_model_name(mut self, name: impl Into<String>) -> Self {
        self.model_name = name.into();
        self
    }

    /// Set temperature.
    pub fn with_temperature(mut self, temp: f64) -> Self {
        self.temperature = temp;
        self
    }

    /// Set max tokens.
    pub fn with_max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max;
        self
    }
}

/// An in-process loaded quantized model.
struct LoadedModel {
    weights: ModelWeights,
    tokenizer: Tokenizer,
    device: Device,
}

impl LoadedModel {
    fn load(raw_path: &Path) -> Result<Self, LlmError> {
        let resolved = if raw_path.exists() {
            raw_path.to_path_buf()
        } else if raw_path.is_relative() {
            let mut found = None;
            for parent in ["..", "../..", "../../.."] {
                let candidate = Path::new(parent).join(raw_path);
                if candidate.exists() {
                    found = Some(candidate);
                    break;
                }
            }
            if found.is_none() {
                let fallback_candidates = [
                    "models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
                    "models/llama-3.2-3b-instruct-q4_k_m.gguf",
                    "../models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
                    "../models/llama-3.2-3b-instruct-q4_k_m.gguf",
                    "../../models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
                    "../../../models/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
                    "models/zenfinance-3b-Instruct.Q4_K_M.gguf",
                    "models/zenfinance-3b-instruct.q4_k_m.gguf",
                    "../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
                    "../models/zenfinance-3b-instruct.q4_k_m.gguf",
                    "../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
                    "../../../models/zenfinance-3b-Instruct.Q4_K_M.gguf",
                    "models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
                    "models/smollm2-360m-instruct-q4_k_m.gguf",
                    "../models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
                    "../models/smollm2-360m-instruct-q4_k_m.gguf",
                    "../../models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
                    "../../../models/SmolLM2-360M-Instruct-Q4_K_M.gguf",
                ];
                for c in fallback_candidates {
                    let p = Path::new(c);
                    if p.exists() {
                        found = Some(p.to_path_buf());
                        break;
                    }
                }
            }
            found.unwrap_or_else(|| raw_path.to_path_buf())
        } else {
            raw_path.to_path_buf()
        };

        if !resolved.exists() {
            return Err(LlmError::Transport(format!(
                "GGUF model file not found at '{}'. Please verify the path or set {ENV_GGUF_PATH}.",
                raw_path.display()
            )));
        }

        let path = &resolved;

        let mut file = File::open(path).map_err(|e| {
            LlmError::Transport(format!(
                "Failed to open GGUF file '{}': {e}",
                path.display()
            ))
        })?;

        let content = gguf_file::Content::read(&mut file).map_err(|e| {
            LlmError::Transport(format!(
                "Failed to read GGUF header '{}': {e}",
                path.display()
            ))
        })?;

        let device = Device::Cpu;
        let weights = ModelWeights::from_gguf(content, &mut file, &device).map_err(|e| {
            LlmError::Transport(format!(
                "Failed to load quantized weights from '{}': {e}",
                path.display()
            ))
        })?;

        let tokenizer = Tokenizer::from_gguf_file(path).map_err(|e| {
            LlmError::Transport(format!(
                "Failed to load tokenizer from GGUF file '{}': {e}",
                path.display()
            ))
        })?;

        Ok(Self {
            weights,
            tokenizer,
            device,
        })
    }
}

/// In-process embedded Candle GGUF provider.
#[derive(Clone)]
pub struct CandleGgufProvider {
    config: CandleGgufConfig,
    model: Arc<Mutex<Option<LoadedModel>>>,
}

impl CandleGgufProvider {
    /// Create a new Candle GGUF provider from configuration.
    pub fn new(config: CandleGgufConfig) -> Self {
        Self {
            config,
            model: Arc::new(Mutex::new(None)),
        }
    }

    /// Create from a GGUF file path.
    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        Self::new(CandleGgufConfig::new(path))
    }

    /// Format chat messages into prompt string.
    pub fn format_prompt(messages: &[ChatMessage], system_prompt: Option<&str>) -> String {
        let mut prompt = String::new();
        let default_sys = "You are Vyapaar, an intelligent quantitative finance and algorithmic trading assistant for Indian equities (NSE/BSE). You provide clear, insightful, disciplined financial and algorithmic trading analysis.";
        let sys = system_prompt.unwrap_or(default_sys);
        if !sys.trim().is_empty() {
            prompt.push_str("<|im_start|>system\n");
            prompt.push_str(sys.trim());
            prompt.push_str("<|im_end|>\n");
        }

        for msg in messages {
            let role_str = match msg.role {
                ChatRole::System => "system",
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "tool",
            };
            prompt.push_str(&format!("<|im_start|>{role_str}\n"));
            prompt.push_str(msg.content.trim());
            prompt.push_str("<|im_end|>\n");
        }

        prompt.push_str("<|im_start|>assistant\n");
        prompt
    }

    /// Get current configuration.
    pub fn config(&self) -> &CandleGgufConfig {
        &self.config
    }
}

#[async_trait]
impl LlmProvider for CandleGgufProvider {
    fn model_name(&self) -> &str {
        &self.config.model_name
    }

    fn context_window(&self) -> usize {
        self.config.context_window
    }

    async fn generate(&self, prompt: &str) -> Result<(String, Decision), LlmError> {
        let decision_sys = "You are a trading decision engine. Emit ONLY a valid JSON decision matching one of these schema shapes:\n\
            For placing orders:\n\
            {\n  \"action\": \"place_order\",\n  \"symbol\": \"<TICKER>\",\n  \"side\": \"buy\" | \"sell\",\n  \"qty\": <positive integer>,\n  \"algo_id\": \"<UPPERCASE_ALGO_ID>\",\n  \"price_limit\": <optional limit price>,\n  \"rationale\": \"<reasoning>\"\n}\n\
            For holding / no trade:\n\
            {\n  \"action\": \"hold\",\n  \"reason\": \"<explanation>\"\n}";
        let chat_request = ChatRequest {
            messages: vec![ChatMessage::user(prompt)],
            system_prompt: Some(decision_sys.to_string()),
            tools: Vec::new(),
            temperature: Some(Decimal::ZERO),
            max_tokens: Some(512),
        };

        let mut stream = self.chat_stream(&chat_request).await?;
        let mut full_text = String::new();

        use futures::StreamExt;
        while let Some(event_res) = stream.next().await {
            match event_res? {
                ChatStreamEvent::TextDelta { text } => full_text.push_str(&text),
                ChatStreamEvent::MessageEnd { message, .. } => {
                    if full_text.is_empty() {
                        full_text = message.content;
                    }
                }
                _ => {}
            }
        }

        let decision = DecisionParser::new()
            .parse_str(&full_text)
            .map_err(|e| LlmError::ParsingFailed(format!("err={e}, full_text={full_text:?}")))?;

        Ok((full_text, decision))
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<EventStream, LlmError> {
        let prompt = Self::format_prompt(&request.messages, request.system_prompt.as_deref());
        let (tx, rx) = mpsc::channel(64);

        let config = self.config.clone();
        let model_arc = Arc::clone(&self.model);

        let req_temp = request
            .temperature
            .and_then(|t| t.to_f64())
            .unwrap_or(config.temperature);
        let req_max_tokens = request.max_tokens.unwrap_or(config.max_tokens);

        tokio::task::spawn_blocking(move || {
            let res = (|| -> Result<(), LlmError> {
                let mut guard = model_arc.blocking_lock();
                if guard.is_none() {
                    let loaded = LoadedModel::load(&config.model_path)?;
                    *guard = Some(loaded);
                }

                let loaded = guard.as_mut().expect("loaded model present");
                loaded.weights.clear_kv_cache();

                let prompt_tokens = loaded
                    .tokenizer
                    .encode(&prompt, true)
                    .map_err(|e| LlmError::Transport(format!("Tokenizer encode error: {e:?}")))?;

                if prompt_tokens.is_empty() {
                    return Err(LlmError::Transport(
                        "Prompt encoded into empty token sequence".to_string(),
                    ));
                }

                let input_tensor = Tensor::new(prompt_tokens.as_slice(), &loaded.device)
                    .and_then(|t| t.unsqueeze(0))
                    .map_err(|e| LlmError::Transport(format!("Tensor create error: {e}")))?;

                let logits = loaded
                    .weights
                    .forward(&input_tensor, 0)
                    .map_err(|e| LlmError::Transport(format!("Forward pass error: {e}")))?;

                let mut logits_processor = LogitsProcessor::new(
                    config.seed,
                    if req_temp == 0.0 {
                        None
                    } else {
                        Some(req_temp)
                    },
                    config.top_p,
                );

                let seq_len = prompt_tokens.len();
                let last_logits = logits
                    .squeeze(0)
                    .map_err(|e| LlmError::Transport(format!("Logits extraction error: {e}")))?;

                let mut next_token = logits_processor
                    .sample(&last_logits)
                    .map_err(|e| LlmError::Transport(format!("Sampling error: {e}")))?;

                let eos_token = loaded.tokenizer.eos_token();
                let mut generated_tokens = Vec::new();
                let mut generated_text = String::new();

                for step in 0..req_max_tokens {
                    if next_token == eos_token {
                        break;
                    }

                    let piece = loaded
                        .tokenizer
                        .decode_single(next_token, false)
                        .map_err(|e| LlmError::Transport(format!("Decode token error: {e:?}")))?;

                    if piece == "<|im_end|>"
                        || piece == "<|endoftext|>"
                        || piece == "</s>"
                        || piece == "<|eot_id|>"
                        || piece == "<|end_of_text|>"
                        || piece.contains("<|im_end|>")
                        || piece.contains("<|eot_id|>")
                    {
                        break;
                    }
                    if piece == "<|im_start|>"
                        || piece == "<|im_start|>assistant"
                        || piece == "<|start_header_id|>"
                        || piece == "<|start_header_id|>assistant<|end_header_id|>"
                    {
                        continue;
                    }

                    generated_tokens.push(next_token);
                    generated_text.push_str(&piece);

                    if generated_text.contains("<|im_end|>")
                        || generated_text.contains("<|eot_id|>")
                        || generated_text.contains("<|end_of_text|>")
                        || generated_text.contains("<|endoftext|>")
                        || generated_text.ends_with("</s>")
                    {
                        break;
                    }

                    if tx
                        .blocking_send(Ok(ChatStreamEvent::TextDelta { text: piece }))
                        .is_err()
                    {
                        return Ok(());
                    }

                    let next_input = Tensor::new(&[next_token], &loaded.device)
                        .and_then(|t| t.unsqueeze(0))
                        .map_err(|e| {
                            LlmError::Transport(format!("Next token tensor error: {e}"))
                        })?;

                    let next_logits = loaded
                        .weights
                        .forward(&next_input, seq_len + step)
                        .map_err(|e| LlmError::Transport(format!("KV forward step error: {e}")))?;

                    let sampled_logits = next_logits
                        .squeeze(0)
                        .and_then(|t| t.squeeze(0))
                        .map_err(|e| {
                            LlmError::Transport(format!("KV logits squeeze error: {e}"))
                        })?;

                    next_token = logits_processor.sample(&sampled_logits).map_err(|e| {
                        LlmError::Transport(format!("Autoregressive sample error: {e}"))
                    })?;
                }

                let usage = Usage {
                    input_tokens: prompt_tokens.len() as u32,
                    output_tokens: generated_tokens.len() as u32,
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                };

                let _ = tx.blocking_send(Ok(ChatStreamEvent::Usage(usage)));
                let clean_text = generated_text
                    .replace("<|im_end|>", "")
                    .replace("<|im_start|>", "")
                    .replace("<|eot_id|>", "")
                    .replace("<|start_header_id|>", "")
                    .replace("<|end_header_id|>", "")
                    .replace("<|end_of_text|>", "")
                    .trim()
                    .to_string();
                let _ = tx.blocking_send(Ok(ChatStreamEvent::MessageEnd {
                    stop_reason: StopReason::EndTurn,
                    message: ChatMessage::assistant(clean_text),
                    usage,
                }));

                Ok(())
            })();

            if let Err(e) = res {
                let _ = tx.blocking_send(Err(e));
            }
        });

        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(Box::pin(stream))
    }
}
