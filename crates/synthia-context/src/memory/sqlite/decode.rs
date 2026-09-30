//! Row decoding: raw `SQLite` columns → [`MemoryEntry`], plus the
//! lexicographically-sortable UTC timestamp form the tables store.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use crate::memory::MemoryEntry;
/// Read one long-term row into its raw parts.
///
/// The inner `Result` is not an accident: `query_map` needs a
/// `rusqlite::Result` closure, but a row can also be *well-formed and
/// still undecodable* (a timestamp that is not RFC 3339), which is a
/// typed [`SqliteMemoryError::Decode`] rather than a driver error.
pub(super) fn read_entry(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<Result<MemoryEntry, String>> {
    let id: String = row.get(0)?;
    let content: String = row.get(1)?;
    let metadata: Option<String> = row.get(2)?;
    let created_at: String = row.get(3)?;
    Ok(decode_entry(
        &id,
        &content,
        metadata.as_deref(),
        &created_at,
    ))
}

/// Turn raw columns into a [`MemoryEntry`], or an explanation.
fn decode_entry(
    id: &str,
    content: &str,
    metadata: Option<&str>,
    created_at: &str,
) -> Result<MemoryEntry, String> {
    let metadata = match metadata {
        Some(raw) => Some(
            serde_json::from_str::<Value>(raw)
                .map_err(|e| format!("metadata of {id:?} is not JSON: {e}"))?,
        ),
        None => None,
    };
    let created_at = DateTime::parse_from_rfc3339(created_at)
        .map_err(|e| format!("timestamp of {id:?} is not RFC 3339: {e}"))?
        .with_timezone(&Utc);
    Ok(MemoryEntry {
        id: id.to_string(),
        content: content.to_string(),
        metadata,
        created_at,
    })
}

/// Timestamp form that sorts lexicographically in UTC.
pub(super) fn rfc3339(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}
