//! Tool/args hashing utilities.
//!
//! Allocation-free hashing based on `ahash::AHasher`. The `(tool_id, args_hash)`
//! pair lets callers index either dimension independently (e.g. detect "same
//! tool, different args" via tool_id match, or vice versa).

use std::hash::{Hash, Hasher};

use ahash::AHasher;

/// Hashes a tool call into a `(tool_id, args_hash)` pair.
///
/// Allocation-free: uses stack-allocated `AHasher`. The two hashes are
/// independent so callers can index either dimension (e.g. detect "same
/// tool, different args" via tool_id match).
pub fn hash_tool_args(tool_name: &str, args_json: &str) -> (u64, u64) {
    let mut h1 = AHasher::default();
    tool_name.hash(&mut h1);
    let tool_id = h1.finish();

    let mut h2 = AHasher::default();
    args_json.hash(&mut h2);
    let args_hash = h2.finish();

    (tool_id, args_hash)
}

/// Hashes an arbitrary value (for poll result deduplication).
pub(super) fn hash_value(value: &str) -> u64 {
    let mut hasher = AHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_tool_args_deterministic() {
        let (t1, a1) = hash_tool_args("tool", "args");
        let (t2, a2) = hash_tool_args("tool", "args");
        assert_eq!(t1, t2);
        assert_eq!(a1, a2);
    }

    #[test]
    fn hash_tool_args_distinct_by_tool() {
        let (t1, a1) = hash_tool_args("tool_a", "args");
        let (t2, a2) = hash_tool_args("tool_b", "args");
        assert_ne!(t1, t2);
        assert_eq!(a1, a2);
    }

    #[test]
    fn hash_tool_args_distinct_by_args() {
        let (t1, a1) = hash_tool_args("tool", "args_1");
        let (t2, a2) = hash_tool_args("tool", "args_2");
        assert_eq!(t1, t2);
        assert_ne!(a1, a2);
    }
}
