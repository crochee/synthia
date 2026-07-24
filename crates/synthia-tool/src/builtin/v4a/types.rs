//! V4A patch data model: `PatchOp`, `HunkLine`, `Hunk`.

use std::path::PathBuf;

/// A single file operation in a V4A patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchOp {
    /// Create a new file with the given content. Codex allows overwriting an
    /// existing file via `*** Add File:` (scenario 011); we mirror that.
    Add { path: PathBuf, content: String },
    /// Apply one or more hunks to an existing file. Optionally move the file
    /// afterward (parsed as `Update { path, hunks, move_to: Some(...) }` —
    /// the ApplyPatchTool decides whether to honor `move_to` at runtime).
    Update {
        path: PathBuf,
        hunks: Vec<Hunk>,
        move_to: Option<PathBuf>,
    },
    /// Delete an existing file.
    Delete { path: PathBuf },
}

/// One line inside a hunk. Stored in source order so the hunk can be
/// reconstructed into `old_text` (context+deletions) and `new_text`
/// (context+insertions) in the right interleaving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkLine {
    /// A context line (V4A prefix: ` `). Stays in both old and new.
    Context(String),
    /// An inserted line (V4A prefix: `+`). Appears only in new_text.
    Insertion(String),
    /// A deleted line (V4A prefix: `-`). Appears only in old_text.
    Deletion(String),
}

/// A single hunk inside an `Update` op.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hunk {
    /// Lines in source order. Empty for a pure addition hunk; pure deletion
    /// hunks have only `Context`+`Deletion` entries; mixed hunks have all
    /// three kinds interleaved.
    pub lines: Vec<HunkLine>,
    /// `true` if the hunk's last context line is followed by `*** End of File`
    /// (meaning the file must end exactly at this point, no trailing newline).
    pub end_of_file: bool,
}

impl Hunk {
    /// Returns the hunk's "context + deletions" (i.e. the lines that must be
    /// found in the original file, in order) and "context + insertions"
    /// (i.e. the lines that will replace them).
    pub fn old_text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                HunkLine::Context(s) | HunkLine::Deletion(s) => {
                    out.push_str(s);
                    out.push('\n');
                }
                HunkLine::Insertion(_) => {}
            }
        }
        out
    }

    pub fn new_text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                HunkLine::Context(s) | HunkLine::Insertion(s) => {
                    out.push_str(s);
                    out.push('\n');
                }
                HunkLine::Deletion(_) => {}
            }
        }
        out
    }
}
