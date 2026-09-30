//! The read side: scan a scope directory, score entries
//! against a query, and render the bounded `MEMORY.md` index.

use std::{cmp::Reverse, fs, io::ErrorKind};

use chrono::{DateTime, Utc};
use serde_json::json;
use ulid::Ulid;

use super::{
    ENTRIES_DIR,
    MAX_MEMORY_INDEX_LINES,
    config::ResolvedScope,
    entry::MemoryKind,
    error::FileMemoryError,
    guards::{check_dir_below, io_error, refuse_symlink},
    scope::MemoryScope,
    text::{
        DERIVED_DESCRIPTION_CHARS,
        DERIVED_NAME_CHARS,
        derive_line,
        one_line,
        split_frontmatter,
        unquote,
    },
};
use crate::memory::MemoryEntry;

/// One entry as read from disk.
pub(super) struct FoundEntry {
    pub(super) id: Ulid,
    name: String,
    description: String,
    kind: MemoryKind,
    content: String,
}

impl FoundEntry {
    /// Parse one entry file.
    ///
    /// Frontmatter is optional: a hand-written file keeps its body
    /// and derives the index fields from the first line, so the
    /// directory stays usable outside the writer.
    fn parse(id: Ulid, raw: &str) -> Self {
        let (front, body) = split_frontmatter(raw);
        let mut name = None;
        let mut description = None;
        let mut kind = None;
        if let Some(front) = front {
            for line in front.lines() {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let value = unquote(value.trim());
                match key.trim() {
                    "name" => name = Some(value.to_string()),
                    "description" => description = Some(value.to_string()),
                    "type" => kind = MemoryKind::parse(value),
                    _ => {}
                }
            }
        }
        let content = body.trim_start_matches('\n').trim_end().to_string();
        let fallback = derive_line(&content, DERIVED_DESCRIPTION_CHARS);
        let name = name
            .filter(|value| !value.is_empty())
            .or_else(|| derive_line(&content, DERIVED_NAME_CHARS))
            .unwrap_or_else(|| id.to_string());
        let description = description
            .filter(|value| !value.is_empty())
            .or(fallback)
            .unwrap_or_default();
        Self {
            id,
            name,
            description,
            kind: kind.unwrap_or(MemoryKind::DEFAULT),
            content,
        }
    }

    /// Project the entry onto the trait's value type. Metadata
    /// carries the frontmatter plus the scope it was read from.
    pub(super) fn into_memory_entry(self, scope: MemoryScope) -> MemoryEntry {
        MemoryEntry {
            created_at: created_at(self.id),
            id: self.id.to_string(),
            metadata: Some(json!({
                "name": self.name,
                "description": self.description,
                "type": self.kind.as_str(),
                "scope": scope.as_str(),
            })),
            content: self.content,
        }
    }
}

/// Read every `entries/<ulid>.md` file of one scope, newest first.
///
/// A missing directory is an empty scope. Files whose stem is not a
/// ULID are ignored (the directory is user-visible, so stray files
/// must not break recall); symlinks are refused.
pub(super) fn scan_dir(
    resolved: &ResolvedScope,
) -> Result<Vec<FoundEntry>, FileMemoryError> {
    let entries_relative = resolved.relative.join(ENTRIES_DIR);
    check_dir_below(&resolved.base, &entries_relative, false)?;
    let entries_dir = resolved.dir.join(ENTRIES_DIR);
    let read_dir = match fs::read_dir(&entries_dir) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(error) => return Err(io_error(&entries_dir, error)),
    };
    let mut found = Vec::new();
    for item in read_dir {
        let item = item.map_err(|error| io_error(&entries_dir, error))?;
        let path = item.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let stem = match path.file_stem().and_then(|stem| stem.to_str()) {
            Some(stem) => stem,
            None => continue,
        };
        let Ok(id) = Ulid::from_string(stem) else {
            tracing::warn!(
                target: "synthia.context",
                path = %path.display(),
                "ignoring memory entry whose name is not a ULID"
            );
            continue;
        };
        refuse_symlink(&path)?;
        let raw = fs::read_to_string(&path)
            .map_err(|error| io_error(&path, error))?;
        found.push(FoundEntry::parse(id, &raw));
    }
    found.sort_by_key(|entry| Reverse(entry.id));
    Ok(found)
}

/// Deterministic keyword score: 3× for a name hit, 2× for a
/// description hit, 1× for a body hit. The name and description are
/// exactly the text the index carries, so an index hit scores like
/// the line it came from.
pub(super) fn score(entry: &FoundEntry, tokens: &[String]) -> usize {
    let name = entry.name.to_lowercase();
    let description = entry.description.to_lowercase();
    let body = entry.content.to_lowercase();
    let mut total = 0;
    for token in tokens {
        total += 3 * name.matches(token.as_str()).count();
        total += 2 * description.matches(token.as_str()).count();
        total += body.matches(token.as_str()).count();
    }
    total
}

/// Split a query into lowercase keyword tokens. Non-alphanumeric
/// characters separate tokens and duplicates collapse, so a repeated
/// word cannot inflate a score.
pub(super) fn tokenize(query: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    for token in query.split(|c: char| !c.is_alphanumeric()) {
        if token.is_empty() {
            continue;
        }
        let token = token.to_lowercase();
        if !tokens.contains(&token) {
            tokens.push(token);
        }
    }
    tokens
}

/// Render the bounded index: header, one bullet per entry (newest
/// first), then an omission note when entries were dropped.
pub(super) fn render_index(entries: &[FoundEntry]) -> String {
    let mut lines = vec![String::from("# Agent Memory Index")];
    // The header owns line 1; when entries do not fit, the last line
    // belongs to the omission note.
    let available = MAX_MEMORY_INDEX_LINES.saturating_sub(1);
    let shown = if entries.len() > available {
        available.saturating_sub(1)
    } else {
        entries.len()
    };
    for entry in entries.iter().take(shown) {
        let mut line = format!(
            "- [{}]({}/{}.md)",
            one_line(&entry.name),
            ENTRIES_DIR,
            entry.id,
        );
        if !entry.description.is_empty() {
            line.push_str(" — ");
            line.push_str(&one_line(&entry.description));
        }
        lines.push(line);
    }
    if shown < entries.len() {
        lines.push(format!(
            "- ... {} older entries omitted",
            entries.len() - shown,
        ));
    }
    let mut rendered = lines.join("\n");
    rendered.push('\n');
    rendered
}

/// Wall-clock creation time of an entry, carried by its ULID.
fn created_at(id: Ulid) -> DateTime<Utc> {
    let millis = i64::try_from(id.timestamp_ms()).unwrap_or(0);
    DateTime::<Utc>::from_timestamp_millis(millis)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}
