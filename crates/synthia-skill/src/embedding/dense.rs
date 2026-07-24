use std::{cmp::Ordering, collections::HashMap};

/// Dense vector index storing LLM-generated embeddings for skills.
pub struct DenseVectorIndex {
    vectors: HashMap<String, Vec<f64>>,
    model: String,
}

impl DenseVectorIndex {
    pub fn new(model: String) -> Self {
        Self {
            vectors: HashMap::new(),
            model,
        }
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Insert a pre-computed dense vector for a skill.
    pub fn insert(&mut self, skill_name: &str, vector: Vec<f64>) {
        self.vectors.insert(skill_name.to_string(), vector);
    }

    /// Remove a skill's vector from the index.
    pub fn remove(&mut self, skill_name: &str) {
        self.vectors.remove(skill_name);
    }

    /// Cosine similarity search against all stored vectors.
    pub fn search(&self, query: &[f64], top_k: usize) -> Vec<(String, f64)> {
        if self.vectors.is_empty() {
            return Vec::new();
        }

        let mut scores: Vec<_> = self
            .vectors
            .iter()
            .map(|(name, vec)| {
                (name.clone(), cosine_similarity_dense(query, vec))
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        scores.truncate(top_k);
        scores
    }

    pub fn contains(&self, skill_name: &str) -> bool {
        self.vectors.contains_key(skill_name)
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    /// Iterate over (skill_name, vector) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<f64>)> {
        self.vectors.iter()
    }
}

/// Cosine similarity between two dense vectors.
pub fn cosine_similarity_dense(a: &[f64], b: &[f64]) -> f64 {
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Search a pre-built snapshot of (skill_name, vector) pairs.
/// Used by registry::match_skills_dense to avoid holding a lock across await.
pub fn cosine_similarity_dense_search(
    query: &[f64],
    snapshot: &[(String, Vec<f64>)],
    top_k: usize,
) -> Vec<(String, f64)> {
    if snapshot.is_empty() {
        return Vec::new();
    }

    let mut scores: Vec<_> = snapshot
        .iter()
        .map(|(name, vec)| (name.clone(), cosine_similarity_dense(query, vec)))
        .collect();

    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    scores.truncate(top_k);
    scores
}
