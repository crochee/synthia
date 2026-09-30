//! The schema and its probe.
//!
//! Every statement is idempotent, so opening an existing database
//! is a no-op; `FTS5` is compiled into the bundled `SQLite` this
//! crate builds against.

use rusqlite::Connection;

use super::error::{SqliteMemoryError, query_err};
/// Schema created on first open. Every statement is idempotent, so
/// opening an existing database is a no-op; `FTS5` is compiled into
/// the bundled `SQLite` this crate builds against.
pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS long_term_memory (
    id         TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    metadata   TEXT,
    created_at TEXT NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS long_term_memory_fts USING fts5(
    content,
    metadata,
    content = 'long_term_memory',
    content_rowid = 'rowid',
    tokenize = 'unicode61'
);
CREATE TRIGGER IF NOT EXISTS long_term_memory_ai
AFTER INSERT ON long_term_memory BEGIN
    INSERT INTO long_term_memory_fts(rowid, content, metadata)
    VALUES (new.rowid, new.content, new.metadata);
END;
CREATE TRIGGER IF NOT EXISTS long_term_memory_ad
AFTER DELETE ON long_term_memory BEGIN
    INSERT INTO long_term_memory_fts(
        long_term_memory_fts, rowid, content, metadata
    ) VALUES ('delete', old.rowid, old.content, old.metadata);
END;
CREATE TRIGGER IF NOT EXISTS long_term_memory_au
AFTER UPDATE ON long_term_memory BEGIN
    INSERT INTO long_term_memory_fts(
        long_term_memory_fts, rowid, content, metadata
    ) VALUES ('delete', old.rowid, old.content, old.metadata);
    INSERT INTO long_term_memory_fts(rowid, content, metadata)
    VALUES (new.rowid, new.content, new.metadata);
END;
CREATE TABLE IF NOT EXISTS working_memory (
    session_id TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    PRIMARY KEY (session_id, key)
);
CREATE TABLE IF NOT EXISTS sessions (
    id         TEXT PRIMARY KEY,
    created_at TEXT NOT NULL
);
";

/// `true` when the long-term schema exists.
pub(super) fn has_schema(conn: &Connection) -> Result<bool, SqliteMemoryError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name = 'long_term_memory'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| query_err("check schema", e))?;
    Ok(count > 0)
}
