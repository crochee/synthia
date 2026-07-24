use super::*;

#[test]
fn test_checkpoint_save_and_load() {
    let dir = tempfile::tempdir().unwrap();
    let checkpoint =
        Checkpoint::new("s1".to_string(), dir.path().to_path_buf());
    checkpoint.save().unwrap();
    let loaded = Checkpoint::load(dir.path().to_path_buf(), 0).unwrap();
    assert_eq!(loaded.session_id, "s1");
}

#[test]
fn test_checkpoint_rotate() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..7 {
        let checkpoint =
            Checkpoint::new("s1".to_string(), dir.path().to_path_buf())
                .with_step(i);
        checkpoint.save().unwrap();
    }
    let entries: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), CHECKPOINT_MAX_COUNT);
}

#[test]
fn test_patch_tool_calls() {
    let mut calls = vec![PendingToolCall {
        id: "call_1".to_string(),
        name: "test_tool".to_string(),
        input: serde_json::json!({}),
    }];
    patch_tool_calls(&mut calls);
    assert!(!calls[0].input.is_null());

    let mut calls2 = vec![PendingToolCall {
        id: "call_2".to_string(),
        name: "test_tool".to_string(),
        input: serde_json::Value::Null,
    }];
    patch_tool_calls(&mut calls2);
    assert!(calls2[0].input.get("_error").is_some());
}

#[test]
fn test_checkpoint_advance_step() {
    let dir = tempfile::tempdir().unwrap();
    let mut checkpoint =
        Checkpoint::new("s1".to_string(), dir.path().to_path_buf());
    assert_eq!(checkpoint.step, 0);
    checkpoint.advance_step();
    assert_eq!(checkpoint.step, 1);
}
