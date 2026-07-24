use std::collections::HashMap;

use super::*;

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(String::from)
        .collect()
}

fn cosine_similarity(
    a: &HashMap<String, f64>,
    b: &HashMap<String, f64>,
) -> f64 {
    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for (term, &val) in a {
        norm_a += val * val;
        if let Some(&other) = b.get(term) {
            dot_product += val * other;
        }
    }

    for &val in b.values() {
        norm_b += val * val;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot_product / denom
    }
}

#[test]
fn test_tokenize_basic() {
    let tokens = tokenize("hello world test");
    assert!(tokens.contains(&"hello".to_string()));
    assert!(tokens.contains(&"world".to_string()));
    assert!(tokens.contains(&"test".to_string()));
}

#[test]
fn test_tokenize_filters_short() {
    let tokens = tokenize("a an be is the testing");
    assert!(!tokens.contains(&"a".to_string()));
    assert!(!tokens.contains(&"an".to_string()));
    assert!(tokens.contains(&"testing".to_string()));
}

#[test]
fn test_cosine_similarity_identical() {
    let mut a = HashMap::new();
    a.insert("test".to_string(), 1.0);
    let b = a.clone();
    let sim = cosine_similarity(&a, &b);
    assert!((sim - 1.0).abs() < 0.001);
}

#[test]
fn test_cosine_similarity_orthogonal() {
    let mut a = HashMap::new();
    a.insert("test".to_string(), 1.0);
    let mut b = HashMap::new();
    b.insert("other".to_string(), 1.0);
    let sim = cosine_similarity(&a, &b);
    assert!(sim < 0.001);
}

#[test]
fn test_cosine_similarity_empty() {
    let a = HashMap::new();
    let mut b = HashMap::new();
    b.insert("test".to_string(), 1.0);
    let sim = cosine_similarity(&a, &b);
    assert_eq!(sim, 0.0);
}

#[test]
fn test_embedding_index_build_and_search() {
    let mut index = SparseVectorIndex::new();
    let texts = vec![
        (
            "file_ops".to_string(),
            "read write delete create file directory".to_string(),
        ),
        (
            "git_ops".to_string(),
            "commit push pull branch merge git repository".to_string(),
        ),
        (
            "web_search".to_string(),
            "fetch url search web scrape http".to_string(),
        ),
    ];
    index.build_from_texts(&texts);

    assert_eq!(index.embedding_count(), 3);
    assert!(index.vocabulary_size() > 0);

    let results = index.search("read file directory", 2);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "file_ops");
}

#[test]
fn test_embedding_index_empty_search() {
    let index = SparseVectorIndex::new();
    let results = index.search("test query", 5);
    assert!(results.is_empty());
}

#[test]
fn test_embedding_index_partial_overlap() {
    let mut index = SparseVectorIndex::new();
    let texts = vec![
        (
            "python".to_string(),
            "python code function module import".to_string(),
        ),
        (
            "rust".to_string(),
            "rust code function module trait".to_string(),
        ),
    ];
    index.build_from_texts(&texts);

    let results = index.search("python import module", 2);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "python");
}

// ============================================================
// DenseVectorIndex Tests
// ============================================================

#[test]
fn test_cosine_similarity_dense_identical() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    let sim = cosine_similarity_dense(&a, &b);
    assert!((sim - 1.0).abs() < 0.001);
}

#[test]
fn test_cosine_similarity_dense_orthogonal() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    let sim = cosine_similarity_dense(&a, &b);
    assert!(sim.abs() < 0.001);
}

#[test]
fn test_cosine_similarity_dense_opposite() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![-1.0, 0.0, 0.0];
    let sim = cosine_similarity_dense(&a, &b);
    assert!((sim - -1.0).abs() < 0.001);
}

#[test]
fn test_cosine_similarity_zero_norm() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![1.0, 2.0, 3.0];
    let sim = cosine_similarity_dense(&a, &b);
    assert_eq!(sim, 0.0);
}

#[test]
fn test_dense_index_search_basic() {
    let mut index = DenseVectorIndex::new("test-model".to_string());
    index.insert("skill-a", vec![1.0, 0.0, 0.0]);
    index.insert("skill-b", vec![0.0, 1.0, 0.0]);
    index.insert("skill-c", vec![0.5, 0.5, 0.0]);

    let results = index.search(&[1.0, 0.0, 0.0], 2);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "skill-a");
    assert!((results[0].1 - 1.0).abs() < 0.001);
}

#[test]
fn test_dense_index_search_empty() {
    let index = DenseVectorIndex::new("test-model".to_string());
    let results = index.search(&[1.0, 0.0], 5);
    assert!(results.is_empty());
}

#[test]
fn test_dense_index_contains() {
    let mut index = DenseVectorIndex::new("test-model".to_string());
    index.insert("skill-a", vec![1.0, 0.0]);
    assert!(index.contains("skill-a"));
    assert!(!index.contains("skill-b"));
}

#[test]
fn test_dense_index_remove() {
    let mut index = DenseVectorIndex::new("test-model".to_string());
    index.insert("skill-a", vec![1.0, 0.0]);
    index.remove("skill-a");
    assert!(!index.contains("skill-a"));
}
