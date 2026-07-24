//! `extends` chain resolution.
//!
//! The `extends:` frontmatter field lets one
//! agent file inherit (and override) the
//! frontmatter of another. The walk is
//! depth-bounded by [`super::types::MAX_EXTENDS_DEPTH`]
//! and cycle-detected via the `visited` vec the
//! caller threads through.
//!
//! Kept separate from [`super::loader`] so the
//! inheritance semantics can be unit-tested in
//! isolation (see [`super::tests`]) — the loader
//! itself is just an "I/O + cache" surface and
//! the inheritance walk doesn't need either.

use super::loader::AgentFileLoader;
use crate::agent_file::{
    frontmatter::FileAgentFrontmatter,
    merge::merge_frontmatter,
};

/// Resolve the `extends` chain for `id`, merging
/// each ancestor's frontmatter into its child
/// with child priority.
///
/// `visited` accumulates the chain ids already
/// walked; on the outermost call it should be
/// empty. Exposing it as a parameter lets
/// callers compose `resolve_extends` into larger
/// walks that share cycle detection state.
///
/// Returns an error when:
///
/// - the chain would exceed
///   [`super::types::MAX_EXTENDS_DEPTH`] (currently
///   4),
/// - an id is revisited (circular extends), or
/// - [`AgentFileLoader::load`] fails for any id
///   in the chain.
pub fn resolve_extends(
    id: &str,
    loader: &AgentFileLoader,
    visited: &mut Vec<String>,
) -> Result<FileAgentFrontmatter, String> {
    if visited.len() >= super::types::MAX_EXTENDS_DEPTH {
        return Err(format!(
            "extends chain depth exceeded {} for '{}'",
            super::types::MAX_EXTENDS_DEPTH,
            id
        ));
    }
    if visited.contains(&id.to_string()) {
        return Err(format!("circular extends detected: {}", id));
    }
    visited.push(id.to_string());
    let file = loader.load(id)?;
    let extends_parent =
        file.frontmatter.as_ref().and_then(|f| f.extends.clone());
    if let Some(parent_id) = extends_parent {
        let parent = resolve_extends(&parent_id, loader, visited)?;
        let child = file
            .frontmatter
            .as_ref()
            .expect("extends implies frontmatter present");
        Ok(merge_frontmatter(&parent, child))
    } else {
        Ok(file.frontmatter.unwrap_or_default())
    }
}
