use super::{
    format::{format_entry, key_to_filename, parse_entry},
    store::HotMemory,
};

#[tokio::test]
async fn test_write_and_read_hot_memory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mem = HotMemory::new(temp_dir.path().to_path_buf());
    mem.write("key1", "value1").await.unwrap();
    let val = mem.read("key1").await.unwrap();
    assert_eq!(val, Some("value1".to_string()));
}

#[tokio::test]
async fn test_read_nonexistent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mem = HotMemory::new(temp_dir.path().to_path_buf());
    let val = mem.read("nonexistent").await.unwrap();
    assert!(val.is_none());
}

#[tokio::test]
async fn test_memory_md_key_mapping() {
    assert_eq!(key_to_filename("memory"), "MEMORY.md");
    assert_eq!(key_to_filename("user"), "USER.md");
    assert_eq!(key_to_filename("custom"), "custom.md");
}

#[tokio::test]
async fn test_write_and_read_memory_md() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mem = HotMemory::new(temp_dir.path().to_path_buf());
    mem.write("memory", "# Project Memory\nImportant facts")
        .await
        .unwrap();

    // Read via generic key
    let val = mem.read("memory").await.unwrap();
    assert!(val.unwrap().contains("# Project Memory"));

    // Read via dedicated method
    let val = mem.read_memory().await.unwrap();
    assert!(val.unwrap().contains("# Project Memory"));
}

#[tokio::test]
async fn test_write_and_read_user_md() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mem = HotMemory::new(temp_dir.path().to_path_buf());
    mem.write("user", "# User Preferences\nverbose: true")
        .await
        .unwrap();

    let val = mem.read_user().await.unwrap();
    assert!(val.unwrap().contains("# User Preferences"));
}

#[tokio::test]
async fn test_read_all_returns_written_entries() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mem = HotMemory::new(temp_dir.path().to_path_buf());
    mem.write("memory", "mem content").await.unwrap();
    mem.write("user", "user content").await.unwrap();

    let all = mem.read_all().await.unwrap();
    assert_eq!(all.get("memory"), Some(&"mem content".to_string()));
    assert_eq!(all.get("user"), Some(&"user content".to_string()));
}

#[tokio::test]
async fn test_frontmatter_written_to_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mem = HotMemory::new(temp_dir.path().to_path_buf());
    mem.write("memory", "# Hello World").await.unwrap();

    let raw = tokio::fs::read_to_string(temp_dir.path().join("MEMORY.md"))
        .await
        .unwrap();
    assert!(raw.starts_with("---"));
    assert!(raw.contains("key: memory"));
    assert!(raw.contains("timestamp:"));
    assert!(raw.contains("# Hello World"));
}

#[test]
fn test_parse_entry_with_frontmatter() {
    let content = "---\nkey: test\ntimestamp: 2026-05-06T00:00:00Z\n---\n\nActual content\n";
    let parsed = parse_entry(content).unwrap();
    assert_eq!(parsed, "Actual content");
}

#[test]
fn test_parse_entry_without_frontmatter() {
    let content = "Plain content without frontmatter";
    let parsed = parse_entry(content).unwrap();
    assert_eq!(parsed, "Plain content without frontmatter");
}

#[test]
fn test_format_entry_contains_frontmatter() {
    let formatted = format_entry("test_key", "some content");
    assert!(formatted.contains("---"));
    assert!(formatted.contains("key: test_key"));
    assert!(formatted.contains("timestamp:"));
    assert!(formatted.contains("some content"));
}
