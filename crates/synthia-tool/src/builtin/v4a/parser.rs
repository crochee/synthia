//! V4A parser: text → `Vec<PatchOp>`.

use std::path::PathBuf;

use super::{
    error::ParseError,
    types::{Hunk, HunkLine, PatchOp},
};

/// Parse a V4A patch into a sequence of operations.
///
/// On success, returns a non-empty `Vec<PatchOp>`. The parser is strict:
/// malformed hunks, unknown headers, or missing markers produce a [`ParseError`]
/// and the caller MUST NOT proceed to filesystem mutation.
pub fn parse_v4a(input: &str) -> Result<Vec<PatchOp>, ParseError> {
    let normalized = input.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    // 1. Locate Begin Patch marker
    let begin_idx = lines
        .iter()
        .position(|l| l.trim() == "*** Begin Patch")
        .ok_or(ParseError::MissingBeginMarker)?;

    // 2. Locate End Patch marker (must come after Begin)
    let end_idx = lines
        .iter()
        .enumerate()
        .skip(begin_idx + 1)
        .find(|(_, l)| l.trim() == "*** End Patch")
        .map(|(i, _)| i)
        .ok_or(ParseError::MissingEndMarker)?;

    // 3. Reject trailing content after End Patch (must be empty or whitespace)
    for line in &lines[end_idx + 1..] {
        if !line.trim().is_empty() {
            return Err(ParseError::TrailingGarbage(line.to_string()));
        }
    }

    // 4. Iterate the file_change block
    let mut ops: Vec<PatchOp> = Vec::new();
    let mut i = begin_idx + 1;
    while i < end_idx {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        if let Some(rest) = line.strip_prefix("*** Add File:") {
            let path = parse_path(rest.trim())?;
            i += 1;
            let mut content = String::new();
            while i < end_idx {
                let l = lines[i];
                if let Some(stripped) = l.strip_prefix('+') {
                    content.push_str(stripped);
                    content.push('\n');
                    i += 1;
                } else if l.trim().is_empty() && i + 1 == end_idx {
                    i += 1;
                    break;
                } else {
                    break;
                }
            }
            ops.push(PatchOp::Add { path, content });
        } else if let Some(rest) = line.strip_prefix("*** Update File:") {
            let path = parse_path(rest.trim())?;
            i += 1;
            // Optional Move to:
            let mut move_to: Option<PathBuf> = None;
            if i < end_idx
                && let Some(rest) = lines[i].trim().strip_prefix("*** Move to:")
            {
                move_to = Some(parse_path(rest.trim())?);
                i += 1;
            }
            // Hunks
            let mut hunks: Vec<Hunk> = Vec::new();
            let mut current: Option<Hunk> = None;
            while i < end_idx {
                let l = lines[i];
                if l.is_empty() {
                    i += 1;
                    continue;
                }
                let trimmed = l.trim();
                if trimmed == "*** End Patch" {
                    break;
                }
                if trimmed.starts_with("*** Add File:")
                    || trimmed.starts_with("*** Update File:")
                    || trimmed.starts_with("*** Delete File:")
                {
                    break;
                }
                if trimmed == "@@" {
                    // Start of a new hunk
                    if let Some(h) = current.take() {
                        hunks.push(h);
                    }
                    current = Some(Hunk::default());
                    i += 1;
                    continue;
                }
                if trimmed == "*** End of File" {
                    if let Some(ref mut h) = current {
                        h.end_of_file = true;
                    }
                    i += 1;
                    continue;
                }
                if current.is_none() {
                    // First line of an Update's first hunk may omit the @@ marker
                    current = Some(Hunk::default());
                }
                let h = current.as_mut().unwrap();
                if let Some(stripped) = l.strip_prefix(' ') {
                    h.lines.push(HunkLine::Context(stripped.to_string()));
                } else if let Some(stripped) = l.strip_prefix('+') {
                    h.lines.push(HunkLine::Insertion(stripped.to_string()));
                } else if let Some(stripped) = l.strip_prefix('-') {
                    h.lines.push(HunkLine::Deletion(stripped.to_string()));
                } else {
                    // Bare line (no prefix) — treat as context for V4A
                    // permissiveness (some implementations allow this)
                    h.lines.push(HunkLine::Context(l.to_string()));
                }
                i += 1;
            }
            if let Some(h) = current.take() {
                hunks.push(h);
            }
            if hunks.is_empty() {
                return Err(ParseError::HunkWithoutUpdate);
            }
            ops.push(PatchOp::Update {
                path,
                hunks,
                move_to,
            });
        } else if let Some(rest) = line.strip_prefix("*** Delete File:") {
            let path = parse_path(rest.trim())?;
            ops.push(PatchOp::Delete { path });
            i += 1;
        } else if line.starts_with("*** ") {
            return Err(ParseError::UnknownOpHeader(line.to_string()));
        } else {
            // Stray hunk line outside any op block
            return Err(ParseError::HunkWithoutUpdate);
        }
    }

    if ops.is_empty() {
        return Err(ParseError::EmptyPatch);
    }
    Ok(ops)
}

pub(crate) fn parse_path(s: &str) -> Result<PathBuf, ParseError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ParseError::InvalidPath(s.to_string()));
    }
    if s.contains('\0') {
        return Err(ParseError::InvalidPath(s.to_string()));
    }
    Ok(PathBuf::from(s))
}
