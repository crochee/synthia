//! `FTS5` query sanitising.
//!
//! Every term is quoted, so `AND` / `NEAR(` / `*` in user text
//! are literal terms instead of `FTS5` syntax — the operator
//! injection the module docs promise cannot happen.
/// Turn free text into an `FTS5` expression, or `None` when there is
/// nothing searchable.
///
/// Every term is quoted, so `AND`/`NEAR(`/`*` in user text are
/// literal terms instead of `FTS5` syntax.
pub(super) fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.to_lowercase()))
        .collect();
    if terms.is_empty() {
        return None;
    }
    Some(terms.join(" OR "))
}
