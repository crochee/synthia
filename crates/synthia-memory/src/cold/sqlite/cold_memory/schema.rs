//! The [`init_schema`] helper — FTS5 virtual table + metadata table
//! creation. Called by both [`super::core::ColdMemory::new`] and
//! [`super::core::ColdMemory::new_in_memory`].

use sqlx::SqlitePool;

/// Create the FTS5 virtual table (`cold_entries_fts`) and the
/// metadata table (`cold_entries_meta`) if they do not already exist.
pub(super) async fn init_schema(
    pool: &SqlitePool,
) -> Result<(), synthia_core::Error> {
    // FTS5 virtual table stores entry_id + content for full-text search
    sqlx::query(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS cold_entries_fts USING fts5(entry_id, content)
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        synthia_core::Error::Io(std::io::Error::other(format!(
            "Failed to create FTS5 table: {}",
            e
        )))
    })?;

    // Separate table for metadata (keeps FTS5 lean)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS cold_entries_meta (
            entry_id TEXT PRIMARY KEY,
            metadata TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL,
            importance_score REAL NOT NULL DEFAULT 0.5,
            access_count INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        synthia_core::Error::Io(std::io::Error::other(format!(
            "Failed to create cold_entries_meta table: {}",
            e
        )))
    })?;

    Ok(())
}
