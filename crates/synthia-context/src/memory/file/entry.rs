//! The entry shape: frontmatter kind, the in-memory entry,
//! and the on-disk rendering of one entry file.

use super::text::{
    DERIVED_DESCRIPTION_CHARS,
    DERIVED_NAME_CHARS,
    derive_line,
    one_line,
};
use crate::memory::MemoryEntry;

/// Entry kind carried by the frontmatter `type` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryKind {
    /// A fact about the user (`type: user`).
    User,
    /// Process feedback to keep applying (`type: feedback`).
    Feedback,
    /// A fact about this project (`type: project`).
    Project,
    /// A pointer to an external resource (`type: reference`).
    Reference,
}

impl MemoryKind {
    /// Kind used for entries stored through the
    /// [`Memory`](crate::memory::Memory) trait,
    /// which carries no kind of its own.
    pub const DEFAULT: MemoryKind = MemoryKind::Project;

    /// Stable label written to the frontmatter.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryKind::User => "user",
            MemoryKind::Feedback => "feedback",
            MemoryKind::Project => "project",
            MemoryKind::Reference => "reference",
        }
    }

    /// Parse a frontmatter label; unknown labels yield `None` and
    /// the reader falls back to [`MemoryKind::DEFAULT`].
    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        let parsed = match label {
            "user" => MemoryKind::User,
            "feedback" => MemoryKind::Feedback,
            "project" => MemoryKind::Project,
            "reference" => MemoryKind::Reference,
            _ => return None,
        };
        Some(parsed)
    }
}

/// One memory file: the frontmatter fields plus the detail body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMemoryEntry {
    /// Short human-readable name (the index link text).
    pub name: String,
    /// One-line description (the index gloss).
    pub description: String,
    /// Entry kind written to the frontmatter `type` field.
    pub kind: MemoryKind,
    /// Detail body written below the frontmatter.
    pub content: String,
}

impl FileMemoryEntry {
    /// Build an entry from `name` + `content`, deriving the
    /// description from the body's first non-empty line.
    #[must_use]
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let description = derive_line(&content, DERIVED_DESCRIPTION_CHARS)
            .unwrap_or_default();
        Self {
            name: name.into(),
            description,
            kind: MemoryKind::DEFAULT,
            content,
        }
    }

    /// Project a trait-level [`MemoryEntry`] onto the file shape.
    ///
    /// The trait carries no frontmatter, so `name`, `description`,
    /// and `type` are read from the entry's metadata when present
    /// and derived from the body otherwise.
    pub(super) fn from_memory_entry(entry: &MemoryEntry) -> Self {
        let name = metadata_field(entry, "name")
            .unwrap_or_else(|| derive_name(&entry.content));
        let description =
            metadata_field(entry, "description").unwrap_or_else(|| {
                derive_line(&entry.content, DERIVED_DESCRIPTION_CHARS)
                    .unwrap_or_default()
            });
        let kind = metadata_field(entry, "type")
            .and_then(|label| MemoryKind::parse(&label))
            .unwrap_or(MemoryKind::DEFAULT);
        Self {
            name,
            description,
            kind,
            content: entry.content.clone(),
        }
    }
}

/// Name derived from a body when no frontmatter name exists.
fn derive_name(content: &str) -> String {
    derive_line(content, DERIVED_NAME_CHARS).unwrap_or_default()
}

/// Read a non-empty string field from an entry's metadata.
fn metadata_field(entry: &MemoryEntry, key: &str) -> Option<String> {
    let value = entry.metadata.as_ref()?.get(key)?.as_str()?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

/// Render one entry file: frontmatter, blank line, body.
pub(super) fn render_entry(entry: &FileMemoryEntry) -> String {
    format!(
        "---\nname: {}\ndescription: {}\ntype: {}\n---\n\n{}\n",
        one_line(&entry.name),
        one_line(&entry.description),
        entry.kind.as_str(),
        entry.content.trim_end(),
    )
}
