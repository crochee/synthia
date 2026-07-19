#![allow(deprecated)]
//! Integration tests for `synthia_agent::turn::TurnId`.
//!
//! The unit-test target is `crates/synthia-agent/src/turn.rs`. To keep
//! that file under the 30-line budget specified in the `turn-id-mvp`
//! spec, the roundtrip / hash / uniqueness tests live here as a
//! separate integration test instead of an inline `#[cfg(test)] mod`.

use std::collections::HashSet;

use synthia_agent::turn::TurnId;

#[test]
fn test_turn_id_new_returns_unique_uuids() {
    let a = TurnId::new();
    let b = TurnId::new();
    assert_ne!(a, b);
}

#[test]
fn test_turn_id_serializes_to_json() {
    let id = TurnId::new();
    let json = serde_json::to_string(&id).expect("serialize");
    let back: TurnId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(id, back);
}

#[test]
fn test_turn_id_hash_eq_consistency() {
    let id = TurnId::new();
    let mut set = HashSet::new();
    set.insert(id);
    assert!(set.contains(&id));
}
