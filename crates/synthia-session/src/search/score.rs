//! Term-coverage scoring with a length penalty, whole-word
//! matching, and match snippets (see the module docs on
//! [`super`] for the formula and why it is deliberately simple).

/// Score one entry against the query terms: term coverage with a length
/// penalty (see the module docs for the formula and why).
pub(super) fn entry_score(text: &str, terms: &[String]) -> Option<f32> {
    if terms.is_empty() {
        return None;
    }
    let matched = terms
        .iter()
        .filter(|term| contains_word(text, term))
        .count();
    if matched == 0 {
        return None;
    }
    let entry_terms = text.split_whitespace().count().max(1) as f32;
    let coverage = matched as f32 / terms.len() as f32;
    let penalty = 1.0 / (1.0 + entry_terms.ln());
    Some(coverage * penalty)
}

/// Case-insensitive whole-word containment (`text` is already lowercased).
fn contains_word(text: &str, term: &str) -> bool {
    let mut from = 0;
    while let Some(offset) = text[from..].find(term) {
        let start = from + offset;
        let end = start + term.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after_ok = text[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        from = end.min(text.len());
        if from >= text.len() {
            break;
        }
    }
    false
}

/// The text around the first match, `radius` characters either side,
/// trimmed at char boundaries.
pub(super) fn snippet(text: &str, terms: &[String], radius: usize) -> String {
    let hit = terms
        .iter()
        .filter_map(|term| text.find(term))
        .min()
        .unwrap_or(0);
    let start = char_boundary_at_or_before(text, hit.saturating_sub(radius));
    let end = char_boundary_at_or_after(text, (hit + radius).min(text.len()));
    let mut out = text[start..end].trim().to_string();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < text.len() {
        out.push('…');
    }
    out
}

fn char_boundary_at_or_before(text: &str, index: usize) -> usize {
    (0..=index.min(text.len()))
        .rev()
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(0)
}

fn char_boundary_at_or_after(text: &str, index: usize) -> usize {
    (index.min(text.len())..=text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(text.len())
}
