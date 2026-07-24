//! V4A (Anthropic "Apply Patch" v4A) parser.
//!
//! Parses a patch in the V4A delimited format into a sequence of [`PatchOp`]s.
//! Format reference: <https://docs.claude.com/en/docs/agents-and-tools/tool-use/apply-patch>
//!
//! V4A grammar (simplified):
//! ```text
//! patch        = "*** Begin Patch" LF { file_change } "*** End Patch" LF
//! file_change  = add_file | update_file | delete_file
//! add_file     = "*** Add File: " path LF { "+" line LF }
//! update_file  = "*** Update File: " path LF [ "*** Move to: " path LF ] hunk { hunk }
//! delete_file  = "*** Delete File: " path LF
//! hunk         = [ "@@" LF ] { ( " " line | "+" line | "-" line ) LF } [ "*** End of File" LF ]
//! ```
//!
//! Notes:
//! - `*** Move to:` is parsed as `update.move_to`. The runtime ApplyPatchTool
//!   decides whether to honor moves (default: disabled).
//! - Hunk line prefixes: ` ` (space) = context, `+` = insert, `-` = delete.
//! - `*** End of File` is a marker indicating the hunk must match the file's
//!   exact end (no trailing newline after the last line).
//! - Hunks store lines in source order so interleaved context/deletion
//!   patterns (e.g. ` line1 / -line2 /  line3`) reconstruct the original
//!   surrounding text correctly when computing `old_text`.

mod error;
mod parser;
mod types;

#[cfg(test)]
mod tests;

pub use error::ParseError;
pub use parser::parse_v4a;
pub use types::{Hunk, HunkLine, PatchOp};
