//! 12 unit tests for the `memory_retriever` module family.
//!
//! Coverage map:
//!
//! - [`super::scoring::compute_text_relevance`]: 6 tests
//!   (exact match / partial match / no match / multi-term
//!   / empty query / frequency bonus).
//! - [`super::scoring::apply_recency_weight_hours`]:
//!   3 tests (recent / old / fresh).
//! - Integration tests against
//!   [`super::search::MemoryRetriever::search`]: 3 tests
//!   (all layers / empty query / respects limit).

use chrono::Utc;

use super::{
    scoring::{apply_recency_weight_hours, compute_text_relevance},
    *,
};
use crate::{
    cold::ColdMemory,
    episodic::EpisodicMemory,
    hot::HotMemory,
    types::{ColdEntry, EpisodicSkill},
};

#[test]
fn test_compute_text_relevance_exact_match() {
    let score =
        compute_text_relevance("Rust programming", "Rust programming language");
    assert!(score > 0.8, "Exact match should score high, got {}", score);
}

#[test]
fn test_compute_text_relevance_partial_match() {
    let score = compute_text_relevance("Rust", "Rustacean community");
    assert!(score > 0.0, "Partial match should score > 0, got {}", score);
    assert!(
        score < 0.6,
        "Partial match should score lower than exact, got {}",
        score
    );
}

#[test]
fn test_compute_text_relevance_no_match() {
    let score =
        compute_text_relevance("JavaScript", "Rust programming language");
    assert!(
        (score - 0.0).abs() < f64::EPSILON,
        "No match should score 0, got {}",
        score
    );
}

#[test]
fn test_compute_text_relevance_multi_term() {
    let score1 =
        compute_text_relevance("Rust async", "Rust async programming patterns");
    let score2 = compute_text_relevance("Rust async", "Rust programming only");
    assert!(
        score1 > score2,
        "Both terms should score higher than one, got {} vs {}",
        score1,
        score2
    );
}

#[test]
fn test_compute_text_relevance_empty_query() {
    let score = compute_text_relevance("", "some content");
    assert_eq!(score, 0.0);
}

#[test]
fn test_compute_text_relevance_frequency_bonus() {
    let score = compute_text_relevance("Rust", "Rust Rust Rust programming");
    // Should have frequency bonus
    assert!(
        score > 0.5,
        "Frequency bonus should increase score, got {}",
        score
    );
}

#[test]
fn test_apply_recency_weight_hours_recent() {
    let recency_weight = 0.3;
    let score = apply_recency_weight_hours(0.8, 1.0, recency_weight);
    // recency_score = 1.0 / (1.0 + 1.0) = 0.5
    // final = 0.8 * 0.7 + 0.5 * 0.3 = 0.56 + 0.15 = 0.71
    assert!((score - 0.71).abs() < 0.01, "Expected ~0.71, got {}", score);
}

#[test]
fn test_apply_recency_weight_hours_old() {
    let recency_weight = 0.3;
    let score = apply_recency_weight_hours(0.8, 100.0, recency_weight);
    // recency_score = 1.0 / (1.0 + 100.0) ≈ 0.0099
    // final = 0.8 * 0.7 + 0.0099 * 0.3 ≈ 0.563
    assert!(
        (score - 0.563).abs() < 0.01,
        "Expected ~0.563 for old entry, got {}",
        score
    );
}

#[test]
fn test_apply_recency_weight_hours_fresh() {
    let recency_weight = 0.3;
    let score = apply_recency_weight_hours(0.5, 0.0, recency_weight);
    // recency_score = 1.0 / (1.0 + 0.0) = 1.0
    // final = 0.5 * 0.7 + 1.0 * 0.3 = 0.35 + 0.3 = 0.65
    assert!((score - 0.65).abs() < 0.01, "Expected ~0.65, got {}", score);
}

/// Integration test: MemoryRetriever.search across hot, cold, and episodic.
#[tokio::test]
async fn test_retriever_search_all_layers() {
    let temp_dir = tempfile::tempdir().unwrap();
    let hot = HotMemory::new(temp_dir.path().to_path_buf());
    let cold = ColdMemory::new_in_memory().await.unwrap();
    let episodic = EpisodicMemory::new_in_memory().await.unwrap();

    // Populate hot memory
    hot.write("notes", "Rust async programming with tokio")
        .await
        .unwrap();

    // Populate cold memory
    cold.append(ColdEntry {
        id: "c1".to_string(),
        content: "Python web framework tutorial".to_string(),
        metadata: serde_json::json!({}),
        created_at: Utc::now(),
        ..Default::default()
    })
    .await
    .unwrap();

    // Populate episodic memory
    episodic
        .save(EpisodicSkill {
            task_hint: "Rust async development".to_string(),
            skill_content: "Using tokio runtime for async tasks".to_string(),
            success_rate: 0.9,
            used_at: Utc::now(),
        })
        .await
        .unwrap();

    let retriever = MemoryRetriever::new(hot, cold, episodic);
    let results = retriever.search("Rust async", 10).await;

    // Should find results from hot and episodic
    assert!(!results.is_empty(), "Should find Rust async results");

    // Hot memory should match
    let hot_results: Vec<_> =
        results.iter().filter(|r| r.source == "hot").collect();
    assert!(
        !hot_results.is_empty(),
        "Hot memory should have results, got {:?}",
        results
    );

    // Episodic should match
    let ep_results: Vec<_> =
        results.iter().filter(|r| r.source == "episodic").collect();
    assert!(
        !ep_results.is_empty(),
        "Episodic memory should have results, got {:?}",
        results
    );

    // Python cold entry should not match
    let cold_results: Vec<_> =
        results.iter().filter(|r| r.source == "cold").collect();
    assert!(
        cold_results.is_empty(),
        "Cold memory should not match Rust async, got {:?}",
        results
    );
}

#[tokio::test]
async fn test_retriever_search_empty_query() {
    let temp_dir = tempfile::tempdir().unwrap();
    let hot = HotMemory::new(temp_dir.path().to_path_buf());
    let cold = ColdMemory::new_in_memory().await.unwrap();
    let episodic = EpisodicMemory::new_in_memory().await.unwrap();

    let retriever = MemoryRetriever::new(hot, cold, episodic);
    let results = retriever.search("", 10).await;
    assert!(results.is_empty());
}

#[tokio::test]
async fn test_retriever_search_respects_limit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let hot = HotMemory::new(temp_dir.path().to_path_buf());
    let cold = ColdMemory::new_in_memory().await.unwrap();
    let episodic = EpisodicMemory::new_in_memory().await.unwrap();

    // Add multiple cold entries that match
    for i in 0..10 {
        cold.append(ColdEntry {
            id: format!("c{}", i),
            content: format!("Rust task number {}", i),
            metadata: serde_json::json!({}),
            created_at: Utc::now(),
            ..Default::default()
        })
        .await
        .unwrap();
    }

    let retriever = MemoryRetriever::new(hot, cold, episodic);
    let results = retriever.search("Rust", 3).await;
    assert!(
        results.len() <= 3,
        "Should respect limit of 3, got {}",
        results.len()
    );
}
