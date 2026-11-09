//! Integration tests for session checkpointing
//! (`vyapaar-agent-chat::session::checkpoint`).

use std::sync::Arc;

use chrono::Utc;
use vyapaar_agent_chat::{ExportFormat, InMemorySessionStore, SessionCheckpointer, TurnId};
use vyapaar_agent_core::{NewSession, PersistedMessage, RuntimeMode, SessionId, SessionStore};

fn msg(seq: u64, role: &str, content: &str) -> PersistedMessage {
    PersistedMessage {
        seq,
        role: role.to_string(),
        content: serde_json::json!(content),
        request_hash: None,
        input_tokens: None,
        output_tokens: None,
        created_at: Utc::now(),
    }
}

fn new_store() -> InMemorySessionStore {
    InMemorySessionStore::new()
}

async fn create_session(store: &InMemorySessionStore, id: SessionId) {
    store
        .create(NewSession {
            id,
            profile: "default".to_string(),
            mode: RuntimeMode::Live,
            provider: "test".to_string(),
            model: "test".to_string(),
            title: None,
        })
        .await
        .expect("create session");
}

#[tokio::test]
async fn test_checkpoint_undo_redo() {
    let store = new_store();
    let ck = SessionCheckpointer::new(Arc::new(store.clone()));
    let session = SessionId::new_v4();
    create_session(&store, session).await;

    let m1 = msg(1, "user", "hello");
    let m2 = msg(2, "assistant", "hi");
    ck.checkpoint(session, TurnId(1), vec![m1.clone(), m2.clone()], "turn 1")
        .await
        .expect("checkpoint 1");

    let m3 = msg(3, "user", "next");
    ck.checkpoint(
        session,
        TurnId(2),
        vec![m1.clone(), m2.clone(), m3.clone()],
        "turn 2",
    )
    .await
    .expect("checkpoint 2");

    assert_eq!(ck.list_checkpoints(session).len(), 2);

    // Undo turn 2 -> back to turn 1 state (last_seq 2).
    let undone = ck.undo(session).await.expect("undo").expect("has undo");
    assert_eq!(undone.turn_id, TurnId(1));
    assert_eq!(undone.last_seq, 2);

    // Redo -> back to turn 2 state (last_seq 3).
    let redone = ck.redo(session).await.expect("redo").expect("has redo");
    assert_eq!(redone.turn_id, TurnId(2));
    assert_eq!(redone.last_seq, 3);

    // Undo again, then a new checkpoint invalidates the redo branch.
    let _ = ck.undo(session).await.expect("undo");
    ck.checkpoint(session, TurnId(3), vec![m1.clone()], "turn 3")
        .await
        .expect("checkpoint 3");
    assert!(
        ck.redo(session).await.expect("redo").is_none(),
        "redo invalidated"
    );
}

#[tokio::test]
async fn test_undo_empty_returns_none() {
    let store = new_store();
    let ck = SessionCheckpointer::new(Arc::new(store.clone()));
    let session = SessionId::new_v4();
    create_session(&store, session).await;
    assert!(ck.undo(session).await.expect("undo").is_none());
    assert!(ck.redo(session).await.expect("redo").is_none());
}

#[tokio::test]
async fn test_export_json_markdown_plain() {
    let store = new_store();
    let ck = SessionCheckpointer::new(Arc::new(store.clone()));
    let session = SessionId::new_v4();
    create_session(&store, session).await;
    store
        .append(session, &[msg(1, "user", "buy NIFTY")])
        .await
        .expect("append");

    let json = ck
        .export(session, ExportFormat::Json)
        .await
        .expect("export json");
    assert!(json.contains("buy NIFTY"));

    let md = ck
        .export(session, ExportFormat::Markdown)
        .await
        .expect("export md");
    assert!(md.contains("buy NIFTY"));
    assert!(md.contains("# Session Transcript"));

    let plain = ck
        .export(session, ExportFormat::PlainText)
        .await
        .expect("export plain");
    assert!(plain.contains("buy NIFTY"));
}

#[tokio::test]
async fn test_export_missing_session() {
    let store = new_store();
    let ck = SessionCheckpointer::new(Arc::new(store.clone()));
    let session = SessionId::new_v4();
    // messages() on a missing session returns empty vec (InMemorySessionStore), so export
    // yields an empty JSON array.
    let json = ck
        .export(session, ExportFormat::Json)
        .await
        .expect("export missing");
    assert_eq!(json, "[]");
}
