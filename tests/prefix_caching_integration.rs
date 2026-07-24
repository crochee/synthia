use synthia_telemetry::compute_prefix_hash;

#[test]
fn test_compute_prefix_hash_deterministic() {
    let messages: Vec<String> = vec!["Hello".to_string(), "World".to_string()];
    let hash1 = compute_prefix_hash(&messages);
    let hash2 = compute_prefix_hash(&messages);
    assert_eq!(hash1, hash2, "Prefix hash should be deterministic");
}

#[test]
fn test_compute_prefix_hash_different_for_different_content() {
    let messages1 = vec!["Hello".to_string()];
    let messages2 = vec!["Goodbye".to_string()];
    let hash1 = compute_prefix_hash(&messages1);
    let hash2 = compute_prefix_hash(&messages2);
    assert_ne!(
        hash1, hash2,
        "Different content should produce different hashes"
    );
}

#[test]
fn test_compute_prefix_hash_empty_messages() {
    let messages: Vec<String> = vec![];
    let hash = compute_prefix_hash(&messages);
    assert!(
        !hash.is_empty(),
        "Hash should not be empty even for empty messages"
    );
}
