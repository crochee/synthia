use std::fs;

use synthia_telemetry::tracer::init_file_logging;

#[test]
fn test_file_logging_writes_to_synthia_log() {
    let tmp = tempfile::TempDir::new().unwrap();
    let log_dir = tmp.path().to_path_buf();
    let log_path = log_dir.join("synthia.log");

    init_file_logging(&log_dir).unwrap();
    tracing::info!(target: "synthia.test", "test log message");

    // The fmt layer writes each event directly through the `MakeWriter`
    // (a `Mutex<File>`), so the bytes reach the OS page cache before
    // `tracing::info!` returns. `read_to_string` reads from the same cache.
    let content = fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        content.contains("test log message"),
        "expected log file to contain message, got: {}",
        content
    );
    // No ANSI codes in file output.
    assert!(
        !content.contains("\u{1b}["),
        "file logs must not contain ANSI codes, got: {}",
        content
    );
}
