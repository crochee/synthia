//! Stress test: 1000 appends must not block caller beyond expected latency.

use std::time::Instant;

use synthia_protocol::MessageId;
use synthia_session_v2::{SessionEntry, SessionManager};
use tempfile::tempdir;

#[tokio::test]
async fn append_1000_ops_does_not_block() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("session.jsonl");
    let mgr = SessionManager::open(&path).await.unwrap();

    let start = Instant::now();
    for i in 0..1000 {
        mgr.append(SessionEntry::Message {
            id: MessageId::new(),
            parent_message_id: if i == 0 {
                None
            } else {
                Some(MessageId::new())
            },
            role: "user".to_string(),
            parts: vec![],
            time: chrono::Utc::now(),
            agent_name: None,
            model_id: None,
        })
        .await
        .unwrap();
    }
    let elapsed = start.elapsed();

    assert!(elapsed.as_secs() < 5, "took too long: {:?}", elapsed);

    mgr.shutdown().await.unwrap();

    let line_count = std::fs::read_to_string(&path).unwrap().lines().count();
    assert_eq!(line_count, 1000, "expected 1000 lines, got {}", line_count);
}
