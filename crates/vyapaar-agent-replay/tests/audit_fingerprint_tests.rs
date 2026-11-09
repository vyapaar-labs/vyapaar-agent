//! Phase-4 determinism fingerprint tests (plan 0004 Phase 4 / plan 0005 §4).
//!
//! Verifies that the audited [`LlmAuditRecord`] captures the serving-stack
//! fingerprint (engine, engine version, model-file hash, quantization, thread
//! count, hardware) and that [`compute_record_hash`] chains on it, so two
//! records that differ only in serving stack hash differently — the property
//! that makes `prompt + seed + temperature = 0` actually reproducible with a
//! local engine.

use std::fs::File;
use std::io::Write;

use chrono::Utc;
use rust_decimal_macros::dec;
use vyapaar_agent_replay::{LlmAuditRecord, compute_record_hash, hash_file, verify_hash_chain};
use vyapaar_core::Decision;

fn sample_decision() -> Decision {
    Decision::Hold {
        reason: Some("rangebound".into()),
    }
}

fn base_record() -> LlmAuditRecord {
    LlmAuditRecord::new(
        Utc::now(),
        "Llama-3.2-3B-Instruct",
        dec!(0.0),
        42,
        r#"{"symbol":"NIFTY"}"#,
        "Buy NIFTY on breakout",
        r#"{"action":"hold","reason":"rangebound"}"#,
        sample_decision(),
    )
}

fn write_temp_file(content: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("vyapaar_fp_test_{}.bin", std::process::id()));
    let mut f = File::create(&path).expect("create temp file");
    f.write_all(content).expect("write temp file");
    path
}

#[test]
fn test_hash_file_is_deterministic_and_content_sensitive() {
    let path = write_temp_file(b"hello world");
    let h1 = hash_file(&path).expect("hash file");
    let h2 = hash_file(&path).expect("hash file again");
    assert_eq!(h1, h2, "hash_file must be deterministic");

    let path2 = write_temp_file(b"hello world!");
    let h3 = hash_file(&path2).expect("hash modified file");
    assert_ne!(h1, h3, "hash_file must be content-sensitive");

    // Cleanup.
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&path2);
}

#[test]
fn test_with_fingerprint_builders_set_fields() {
    let record = base_record()
        .with_engine("candle-gguf")
        .with_engine_version("0.11.1")
        .with_model_file_hash([1u8; 32])
        .with_quantization("Q4_K_M")
        .with_thread_count(1)
        .with_hardware("x86_64 linux");

    assert_eq!(record.engine, "candle-gguf");
    assert_eq!(record.engine_version, "0.11.1");
    assert_eq!(record.model_file_sha256, Some([1u8; 32]));
    assert_eq!(record.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(record.thread_count, Some(1));
    assert_eq!(record.hardware.as_deref(), Some("x86_64 linux"));
}

#[test]
fn test_new_leaf_fingerprint_fields_unset_by_default() {
    let record = base_record();
    assert!(record.engine.is_empty());
    assert!(record.engine_version.is_empty());
    assert_eq!(record.model_file_sha256, None);
    assert_eq!(record.quantization, None);
    assert_eq!(record.thread_count, None);
    assert_eq!(record.hardware, None);
}

#[test]
fn test_record_hash_chains_on_fingerprint() {
    let prev = [0u8; 32];
    let seq = 0u64;

    let plain = base_record();
    let plain_hash = compute_record_hash(&prev, seq, &plain);

    // Same record but with a fingerprint set must produce a different hash.
    let fingerprinted = base_record()
        .with_engine("candle-gguf")
        .with_engine_version("0.11.1")
        .with_model_file_hash([9u8; 32])
        .with_quantization("Q4_K_M")
        .with_thread_count(1)
        .with_hardware("x86_64 linux");
    let fp_hash = compute_record_hash(&prev, seq, &fingerprinted);

    assert_ne!(
        plain_hash, fp_hash,
        "fingerprint must affect the record hash"
    );

    // Only the model-file hash changing must also change the hash.
    let fp_other_model = base_record().with_model_file_hash([8u8; 32]);
    let fp_other_hash = compute_record_hash(&prev, seq, &fp_other_model);
    assert_ne!(
        fp_hash, fp_other_hash,
        "model file hash must affect the record hash"
    );
}

#[test]
fn test_verify_hash_chain_with_fingerprint() {
    let mut chain = Vec::new();
    let mut prev = [0u8; 32];

    for i in 0..3u64 {
        let record = base_record()
            .with_engine("candle-gguf")
            .with_engine_version("0.11.1")
            .with_model_file_hash([i as u8; 32])
            .with_quantization("Q4_K_M");
        let hash = compute_record_hash(&prev, i, &record);
        chain.push(vyapaar_agent_replay::ChainedLlmAuditRecord {
            sequence_number: i,
            prev_record_hash: prev,
            record_hash: hash,
            record,
        });
        prev = hash;
    }

    assert!(verify_hash_chain(&chain).is_ok(), "valid chain verifies");

    // Tampering with a fingerprint field breaks the chain.
    let mut tampered = chain.clone();
    tampered[1].record.engine = "openai-compat".to_string();
    assert!(
        verify_hash_chain(&tampered).is_err(),
        "tampered fingerprint breaks chain"
    );
}
