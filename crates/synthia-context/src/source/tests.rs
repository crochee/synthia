use super::{SourceContent, SourceDelta};

#[test]
fn source_content_hash_is_deterministic() {
    let a = SourceContent::from_text("hello");
    let b = SourceContent::from_text("hello");
    assert_eq!(a.hash(), b.hash());
}

#[test]
fn source_content_hash_differs_for_different_input() {
    let a = SourceContent::from_text("hello");
    let b = SourceContent::from_text("world");
    assert_ne!(a.hash(), b.hash());
}

#[test]
fn source_content_from_text_preserves_bytes() {
    let content = SourceContent::from_text("abc");
    assert_eq!(content.0, b"abc");
}

#[test]
fn source_delta_changed_construction() {
    let content = SourceContent::from_text("new");
    let delta = SourceDelta::Changed(content);
    assert!(matches!(delta, SourceDelta::Changed(_)));
}

#[test]
fn source_delta_unchanged_construction() {
    let delta = SourceDelta::Unchanged;
    assert!(matches!(delta, SourceDelta::Unchanged));
}

#[test]
fn source_delta_removed_construction() {
    let delta = SourceDelta::Removed;
    assert!(matches!(delta, SourceDelta::Removed));
}
