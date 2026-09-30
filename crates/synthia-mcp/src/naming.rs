//! Tool-name namespacing for multi-server MCP fleets (dsh parity).
//!
//! Two MCP servers routinely publish the same raw tool name
//! (`echo`, `search`, …). Registering both into one
//! [`synthia_tool::ToolRegistry`] under their raw names would
//! shadow one with the other. [`public_tool_name`] derives a
//! stable, model-facing name from the pair `(server, raw)` so
//! every remote tool occupies a unique registry slot.
//!
//! The name is a pure function of that pair — reconnects,
//! re-syncs, and process restarts all derive the same string, so
//! a name learned in one generation still resolves in the next.

use sha2::{Digest, Sha256};

/// Model-facing tool names cap at 64 chars (wire contract shared
/// with every provider's function-name limits).
pub const MAX_TOOL_NAME_LENGTH: usize = 64;

/// Hex chars of the SHA-256 identity hash appended on lossy
/// normalization.
const HASH_SUFFIX_LENGTH: usize = 12;

/// Derive the model-facing public name for one MCP tool.
///
/// The clean case is `mcp__<server>__<raw>` verbatim. When
/// normalization — replacing every character outside
/// `[A-Za-z0-9_-]` with `_` — or truncation to
/// [`MAX_TOOL_NAME_LENGTH`] would change the name, a 12-hex-char
/// SHA-256 digest of the identity `("<server>", "\0", "<raw>")`
/// is appended, so two distinct MCP identities can never collapse
/// onto the same public name.
///
/// Deterministic and allocation-bounded; safe to call on every
/// re-sync.
#[must_use]
pub fn public_tool_name(server: &str, raw: &str) -> String {
    let joined = format!("mcp__{server}__{raw}");
    let normalized = normalize_tool_name(&joined);
    if normalized == joined && normalized.len() <= MAX_TOOL_NAME_LENGTH {
        return normalized;
    }
    let hash = identity_hash(server, raw);
    let stem = MAX_TOOL_NAME_LENGTH - HASH_SUFFIX_LENGTH - 1; // room for '_'
    format!("{}_{}", truncate_chars(&normalized, stem), hash)
}

/// Split a namespaced public name back into `(server, raw)`.
///
/// `"mcp__fs__read_file"` → `("fs", "read_file")`. Returns `None`
/// for names without the `mcp__<server>__<raw>` shape. The split
/// takes the first `__` after the marker, so a *raw* part that
/// itself contains `__` round-trips exactly, while a *server*
/// name containing `__` cannot (avoid such server names).
/// Lossy names carry a `_hash` suffix that belongs to the raw
/// part — use this to attribute a name to its server, not to
/// recover the exact raw spelling.
#[must_use]
pub fn resolve_prefix(public: &str) -> Option<(&str, &str)> {
    let rest = public.strip_prefix("mcp__")?;
    let (server, raw) = rest.split_once("__")?;
    if server.is_empty() || raw.is_empty() {
        return None;
    }
    Some((server, raw))
}

/// How [`crate::register_mcp_tools`] derives registry names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NamingPolicy {
    /// Derive `mcp__<server>__<raw>` names (default). Distinct
    /// servers never collide, and the `mcp__` marker makes
    /// remote tools identifiable in logs and prompts.
    #[default]
    Namespaced,
    /// Register under the server's raw tool name. Opt-in for
    /// single-server deployments that want the pristine names;
    /// the caller owns any collision that follows.
    Raw,
}

impl NamingPolicy {
    /// The registry name for `raw` on `server` under this policy.
    #[must_use]
    pub fn apply(&self, server: &str, raw: &str) -> String {
        match self {
            Self::Namespaced => public_tool_name(server, raw),
            Self::Raw => raw.to_string(),
        }
    }
}

/// Replace every character outside `[A-Za-z0-9_-]` with `_`.
fn normalize_tool_name(name: &str) -> String {
    name.chars()
        .map(|c| if is_name_char(c) { c } else { '_' })
        .collect()
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// 12-hex-char SHA-256 of the `(server, raw)` identity.
fn identity_hash(server: &str, raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(server.as_bytes());
    hasher.update([0]);
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)[..HASH_SUFFIX_LENGTH].to_string()
}

/// First `max` chars of `s` (char boundaries, multibyte safe).
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_names_pass_through_verbatim() {
        assert_eq!(public_tool_name("fs", "read_file"), "mcp__fs__read_file");
        // Dashes and digits are already legal.
        assert_eq!(
            public_tool_name("git-2", "create-commit"),
            "mcp__git-2__create-commit"
        );
    }

    #[test]
    fn names_are_stable_across_calls() {
        let a = public_tool_name("fs", "read file");
        let b = public_tool_name("fs", "read file");
        assert_eq!(a, b, "same identity must derive the same name");
    }

    #[test]
    fn space_in_raw_name_is_lossy_and_hashed() {
        let name = public_tool_name("fs", "read file");
        assert!(
            name.starts_with("mcp__fs__read_file_"),
            "normalized stem then `_` separator: {name}"
        );
        let suffix = name.rsplit('_').next().unwrap_or_default();
        assert_eq!(
            suffix.len(),
            HASH_SUFFIX_LENGTH,
            "hash suffix is 12 chars: {name}"
        );
        assert!(
            suffix.chars().all(|c| c.is_ascii_hexdigit()),
            "hash suffix is hex: {name}"
        );
        // Short-but-lossy names stay short: stem + `_` + hash.
        assert_eq!(
            name.len(),
            "mcp__fs__read_file".len() + 1 + HASH_SUFFIX_LENGTH
        );
    }

    #[test]
    fn multibyte_raw_name_is_lossy_and_hashed() {
        let name = public_tool_name("fs", "读取文件");
        // Every CJK char becomes `_`, so the name differs from the
        // joined original and must carry the hash suffix.
        assert!(name.starts_with("mcp__fs______"));
        // 9-char prefix + 4 underscores + `_` + hash.
        assert_eq!(name.len(), "mcp__fs__".len() + 4 + 1 + HASH_SUFFIX_LENGTH);
        // Distinct multibyte names must not collide after
        // normalization: the hash keeps them apart.
        let other = public_tool_name("fs", "写入文件");
        assert_ne!(name, other);
    }

    #[test]
    fn distinct_identities_never_collide() {
        // Same server, raw names that normalize identically.
        assert_ne!(
            public_tool_name("srv", "a b"),
            public_tool_name("srv", "a_b")
        );
        // Same raw name, different servers.
        assert_ne!(
            public_tool_name("one", "echo"),
            public_tool_name("two", "echo")
        );
    }

    #[test]
    fn overlong_names_are_capped_at_64() {
        let raw = "r".repeat(80);
        let name = public_tool_name("srv", &raw);
        assert_eq!(name.len(), MAX_TOOL_NAME_LENGTH);
        // 51-char stem + `_` + 12 hex.
        assert_eq!(
            name.split('_').next_back().map(str::len),
            Some(HASH_SUFFIX_LENGTH)
        );
    }

    #[test]
    fn exactly_64_clean_name_stays_verbatim() {
        // `mcp__srv__` is 10 chars; 10 + 54 = 64, all legal.
        let raw = "r".repeat(54);
        let joined = format!("mcp__srv__{raw}");
        assert_eq!(joined.len(), MAX_TOOL_NAME_LENGTH);
        assert_eq!(public_tool_name("srv", &raw), joined);
    }

    #[test]
    fn resolve_prefix_round_trips_clean_names() {
        let public = public_tool_name("fs", "read_file");
        assert_eq!(resolve_prefix(&public), Some(("fs", "read_file")));
        // A raw name containing the separator splits at the first
        // `__`, which is the server boundary — documented shape.
        let public = public_tool_name("fs", "a__b");
        assert_eq!(resolve_prefix(&public), Some(("fs", "a__b")));
    }

    #[test]
    fn resolve_prefix_rejects_non_namespaced_shapes() {
        assert_eq!(resolve_prefix("read_file"), None);
        assert_eq!(resolve_prefix("mcp__fs"), None);
        assert_eq!(resolve_prefix("mcp__fs__"), None);
        assert_eq!(resolve_prefix("fs__read_file"), None);
    }

    #[test]
    fn raw_policy_keeps_the_raw_name() {
        assert_eq!(NamingPolicy::Raw.apply("fs", "read_file"), "read_file");
        assert_eq!(
            NamingPolicy::Namespaced.apply("fs", "read_file"),
            "mcp__fs__read_file"
        );
    }
}
