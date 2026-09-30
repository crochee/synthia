//! Frontmatter / one-line text helpers shared by the writer
//! (entry rendering) and the reader (entry parsing, index
//! rendering).

/// Characters kept when a name is derived from an entry body.
pub(super) const DERIVED_NAME_CHARS: usize = 80;
/// Characters kept when a description is derived from a body.
pub(super) const DERIVED_DESCRIPTION_CHARS: usize = 160;

/// Split `raw` into frontmatter text (without the delimiters) and
/// body. The opening `---` must be the first line; without a closing
/// delimiter the whole file is body.
pub(super) fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    if !raw.starts_with("---\n") {
        return (None, raw);
    }
    let rest = &raw[4..];
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let bare = line.trim_end_matches('\n').trim_end_matches('\r');
        if bare.trim() == "---" {
            let front = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (Some(front), body);
        }
        offset += line.len();
    }
    (None, raw)
}

/// Strip one layer of surrounding double quotes (hand-authored
/// YAML tolerance).
pub(super) fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    let quoted =
        bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"';
    if quoted {
        return &value[1..value.len() - 1];
    }
    value
}

/// Collapse a value onto one line (frontmatter values and index
/// lines must stay on one line).
pub(super) fn one_line(value: &str) -> String {
    let mut parts = value.split_whitespace();
    let mut collapsed = String::new();
    if let Some(first) = parts.next() {
        collapsed.push_str(first);
        for part in parts {
            collapsed.push(' ');
            collapsed.push_str(part);
        }
    }
    collapsed
}

/// First non-empty line of `text`, trimmed and clipped to `max`
/// characters.
pub(super) fn derive_line(text: &str, max: usize) -> Option<String> {
    let mut line = None;
    for candidate in text.lines() {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() {
            line = Some(trimmed);
            break;
        }
    }
    line.map(|value| value.chars().take(max).collect())
}
