use super::*;

#[test]
fn test_task_context_default() {
    let ctx = TaskContext::default();
    assert!(ctx.description.is_empty());
    assert!(ctx.file_references.is_empty());
    assert!(ctx.code_snippets.is_empty());
    assert!(ctx.constraints.is_empty());
}

#[test]
fn test_task_context_builder() {
    let ctx = TaskContext::new("Do something".to_string())
        .with_files(vec!["file.rs".to_string()])
        .with_constraints(vec!["Don't break tests".to_string()]);

    assert_eq!(ctx.description, "Do something");
    assert_eq!(ctx.file_references, vec!["file.rs"]);
    assert_eq!(ctx.constraints, vec!["Don't break tests"]);
}

#[test]
fn test_task_priority_ordering() {
    assert!(TaskPriority::High.is_higher_than(&TaskPriority::Medium));
    assert!(TaskPriority::Medium.is_higher_than(&TaskPriority::Low));
    assert!(!TaskPriority::Low.is_higher_than(&TaskPriority::Medium));
    assert!(TaskPriority::High.is_higher_than(&TaskPriority::Low));
}

#[test]
fn test_task_priority_default() {
    let p = TaskPriority::default();
    assert_eq!(p, TaskPriority::Medium);
    assert_eq!(p.as_u8(), 1);
}

#[test]
fn test_task_status_variants() {
    assert_eq!(TaskStatus::Success, TaskStatus::Success);
    assert_ne!(TaskStatus::Success, TaskStatus::Error);
    assert_ne!(TaskStatus::Success, TaskStatus::Timeout);
}

#[test]
fn test_task_result_success() {
    let result = TaskResult::success("done".to_string());
    assert!(result.is_success());
    assert_eq!(result.status, TaskStatus::Success);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.output, "done");
}

#[test]
fn test_task_result_error() {
    let result = TaskResult::error("something failed".to_string());
    assert!(!result.is_success());
    assert_eq!(result.status, TaskStatus::Error);
    assert_eq!(result.exit_code, Some(1));
}

#[test]
fn test_task_result_timeout() {
    let result = TaskResult::timeout();
    assert!(!result.is_success());
    assert_eq!(result.status, TaskStatus::Timeout);
    assert_eq!(result.exit_code, None);
}

#[test]
fn test_task_result_with_exit_code() {
    let result = TaskResult::success("ok".to_string()).with_exit_code(42);
    assert_eq!(result.exit_code, Some(42));
}

#[test]
fn test_task_result_with_artifacts() {
    let artifacts = vec!["out.txt".to_string(), "data.json".to_string()];
    let result =
        TaskResult::success("ok".to_string()).with_artifacts(artifacts.clone());
    assert_eq!(result.artifacts, artifacts);
}

#[test]
fn test_code_snippet_creation() {
    let snippet =
        CodeSnippet::new("example.rs".to_string(), "fn main() {}".to_string());
    assert_eq!(snippet.name, "example.rs");
    assert_eq!(snippet.content, "fn main() {}");
}

#[test]
fn test_format_task_context() {
    let ctx = TaskContext::new("Build feature".to_string())
        .with_constraints(vec!["Use async".to_string()]);

    let formatted = format_task_context(&ctx, "");
    assert!(formatted.contains("## Task Description"));
    assert!(formatted.contains("Build feature"));
    assert!(formatted.contains("## Constraints"));
    assert!(formatted.contains("Use async"));
}

#[test]
fn test_format_task_context_with_snippets() {
    let ctx = TaskContext::new("Refactor".to_string()).with_snippets(vec![
        CodeSnippet::new("old.rs".to_string(), "fn old() {}".to_string()),
    ]);

    let formatted = format_task_context(&ctx, "");
    assert!(formatted.contains("## Code Snippets"));
    assert!(formatted.contains("old.rs"));
    assert!(formatted.contains("fn old() {}"));
}

#[test]
fn test_format_task_context_full() {
    let ctx = TaskContext::new("Fix bug".to_string())
        .with_files(vec!["bug.rs".to_string()])
        .with_snippets(vec![CodeSnippet::new(
            "fix.rs".to_string(),
            "fn fix() {}".to_string(),
        )])
        .with_constraints(vec!["Preserve API".to_string()]);

    let resolved = "=== /tmp/bug.rs ===\nold code\n\n";
    let formatted = format_task_context(&ctx, resolved);

    assert!(formatted.contains("## Task Description"));
    assert!(formatted.contains("## Referenced Files"));
    assert!(formatted.contains("## Code Snippets"));
    assert!(formatted.contains("## Constraints"));
    assert!(formatted.contains("Preserve API"));
}

#[test]
fn test_default_task_timeout() {
    assert_eq!(DEFAULT_TASK_TIMEOUT, std::time::Duration::from_secs(30));
}

#[tokio::test]
async fn test_resolve_file_references_existing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("test.txt");
    std::fs::write(&file_path, "hello world").unwrap();

    let resolved = resolve_file_references(
        &[file_path.to_string_lossy().to_string()],
        tmp.path(),
    )
    .await;
    assert!(resolved.contains("=== "));
    assert!(resolved.contains("test.txt"));
    assert!(resolved.contains("hello world"));
}

#[tokio::test]
async fn test_resolve_file_references_nonexistent_file() {
    let tmp = tempfile::tempdir().unwrap();
    let resolved =
        resolve_file_references(&["nonexistent.txt".to_string()], tmp.path())
            .await;
    assert!(resolved.is_empty());
}

#[tokio::test]
async fn test_resolve_file_references_relative_path() {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("relative.txt");
    std::fs::write(&file_path, "relative content").unwrap();

    let resolved =
        resolve_file_references(&["relative.txt".to_string()], tmp.path())
            .await;
    assert!(resolved.contains("relative content"));
}
