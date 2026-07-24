//! Tests for the protection zone system.

use synthia_provider::Message;

use super::*;
use crate::traits::{estimate_message_tokens, extract_message_text};

fn make_user_msg(text: &str) -> Message {
    Message::user(text)
}

fn make_assistant_msg(text: &str) -> Message {
    Message::assistant(text)
}

fn make_rounds(n: usize) -> Vec<Message> {
    let mut msgs = Vec::new();
    for i in 0..n {
        msgs.push(make_user_msg(&format!("user message {}", i)));
        msgs.push(make_assistant_msg(&format!("assistant response {}", i)));
    }
    msgs
}

#[test]
fn test_protection_zone_default() {
    let zone = ProtectionZone::default();
    assert_eq!(zone.min_rounds, 3);
    assert!((zone.token_ratio - 0.35).abs() < f64::EPSILON);
}

#[test]
fn test_protection_zone_custom() {
    let zone = ProtectionZone::new(5, 0.4);
    assert_eq!(zone.min_rounds, 5);
    assert!((zone.token_ratio - 0.4).abs() < f64::EPSILON);
}

#[test]
fn test_calculate_boundary_token_budget() {
    let zone = ProtectionZone::new(3, 0.35);
    let messages = make_rounds(10);
    let total_tokens: usize =
        messages.iter().map(estimate_message_tokens).sum();

    let boundary = zone.calculate_boundary(&messages, total_tokens);
    let min_budget = ((total_tokens as f64) * 0.35) as usize;
    assert!(boundary >= min_budget);
}

#[test]
fn test_calculate_boundary_empty_messages() {
    let zone = ProtectionZone::new(3, 0.35);
    let boundary = zone.calculate_boundary(&[], 1000);
    let min_budget = ((1000.0) * 0.35) as usize;
    assert_eq!(boundary, min_budget);
}

#[test]
fn test_get_recent_messages() {
    let zone = ProtectionZone::default();
    let messages = make_rounds(10);

    let recent = zone.get_recent_messages(&messages, 3);
    assert_eq!(recent.len(), 6);
    assert!(extract_message_text(recent[0]).contains("user message 7"));
    assert!(extract_message_text(recent[5]).contains("assistant response 9"));
}

#[test]
fn test_get_recent_messages_fewer_than_requested() {
    let zone = ProtectionZone::default();
    let messages = make_rounds(2);

    let recent = zone.get_recent_messages(&messages, 5);
    assert_eq!(recent.len(), 4);
}

#[test]
fn test_get_recent_messages_empty() {
    let zone = ProtectionZone::default();
    let recent = zone.get_recent_messages(&[], 3);
    assert!(recent.is_empty());
}

#[test]
fn test_get_recent_messages_zero_rounds() {
    let zone = ProtectionZone::default();
    let messages = make_rounds(5);
    let recent = zone.get_recent_messages(&messages, 0);
    assert!(recent.is_empty());
}

#[test]
fn test_estimate_message_tokens() {
    let msg = make_user_msg("Hello world");
    let tokens = estimate_message_tokens(&msg);
    assert!(tokens > 0);
}

#[test]
fn test_count_recent_round_tokens() {
    let messages = make_rounds(5);
    let count = ProtectionZone::count_recent_round_tokens(&messages, 2);
    assert!(count > 0);
}

#[test]
fn test_count_recent_round_tokens_zero_rounds() {
    let messages = make_rounds(5);
    let count = ProtectionZone::count_recent_round_tokens(&messages, 0);
    assert_eq!(count, 0);
}

#[test]
fn test_count_recent_round_tokens_empty() {
    let count = ProtectionZone::count_recent_round_tokens(&[], 3);
    assert_eq!(count, 0);
}

#[test]
fn test_compaction_boundary_empty_messages() {
    let zone = ProtectionZone::default();
    let boundary = zone.calculate_compaction_boundary_simple(&[]);
    assert_eq!(boundary.protected_start_index, 0);
    assert_eq!(boundary.compact_end_index, 0);
    assert_eq!(boundary.protected_tokens, 0);
    assert_eq!(boundary.compactable_tokens, 0);
}

#[test]
fn test_compaction_boundary_simple() {
    let zone = ProtectionZone::new(3, 0.35);
    let messages = make_rounds(10);
    let boundary = zone.calculate_compaction_boundary_simple(&messages);

    assert!(boundary.protected_start_index > 0);
    assert!(boundary.protected_tokens > 0);
    assert!(boundary.compactable_tokens > 0);
}

#[test]
fn test_compaction_boundary_with_checkpoints() {
    let zone = ProtectionZone::new(3, 0.35);
    let messages = make_rounds(10);
    let checkpoint_indices = vec![4, 8];
    let boundary =
        zone.calculate_compaction_boundary(&messages, &checkpoint_indices);

    assert!(boundary.protected_start_index > 0);
    assert!(boundary.compactable_tokens + boundary.protected_tokens > 0);
}

#[test]
fn test_compaction_boundary_no_checkpoints() {
    let zone = ProtectionZone::new(3, 0.35);
    let messages = make_rounds(5);
    let boundary = zone.calculate_compaction_boundary(&messages, &[]);

    assert!(boundary.protected_start_index <= messages.len());
}

#[test]
fn test_compaction_boundary_checkpoint_beyond_messages() {
    let zone = ProtectionZone::new(3, 0.35);
    let messages = make_rounds(3);
    let checkpoint_indices = vec![100, 200];
    let boundary =
        zone.calculate_compaction_boundary(&messages, &checkpoint_indices);

    assert!(boundary.protected_start_index > 0);
    assert!(boundary.protected_tokens > 0);
}

#[test]
fn test_protection_zone_checkpoint_excludes_last_n() {
    let zone = ProtectionZone::new(2, 0.35);
    let messages = make_rounds(10);
    // Checkpoint at index 5 means messages from index 5 onward are protected
    let checkpoint_indices = vec![5];
    let boundary =
        zone.calculate_compaction_boundary(&messages, &checkpoint_indices);

    assert!(boundary.protected_start_index <= 5);
    assert!(boundary.compactable_tokens > 0);
    assert!(boundary.protected_tokens > 0);
}

#[test]
fn test_protection_zone_multiple_checkpoints_picks_earliest() {
    let zone = ProtectionZone::new(3, 0.35);
    let messages = make_rounds(10);
    let checkpoint_indices = vec![2, 6, 8];
    let boundary =
        zone.calculate_compaction_boundary(&messages, &checkpoint_indices);

    assert_eq!(boundary.protected_start_index, 2);
}

#[test]
fn test_protection_zone_checkpoint_outside_valid_range() {
    let zone = ProtectionZone::new(3, 0.35);
    let messages = make_rounds(5);
    let checkpoint_indices = vec![0, 3];
    let boundary =
        zone.calculate_compaction_boundary(&messages, &checkpoint_indices);

    assert!(boundary.protected_start_index <= 3);
}
