//! [`VectorStore`] — pluggable dense-vector index.
//!
//! [`FlatVectorStore`] is the default. It is a `Vec<Option<Vec<f32>>>`
//! where `None` marks a tombstoned slot. `search` is a full scan
//! over alive slots with cosine similarity (assumes L2-normalised
//! vectors — both [`crate::HashingEmbedder`] and any
//! `ModelProviderEmbedder` are expected to L2-normalise before
//! insertion).
//!
//! `HnswStore` is left as a future seam: implementors write a
//! `VectorStore` impl and pass it to `SearchEngine::with_vector_store`.

use crate::error::{Result, SearchError};

pub trait VectorStore: Send + Sync + 'static {
    fn add(&mut self, v: Vec<f32>) -> Result<usize>;
    fn mark_deleted(&mut self, idx: usize);
    fn is_alive(&self, idx: usize) -> bool;
    fn n_alive(&self) -> usize;
    fn dim(&self) -> usize;
    fn search(&self, q: &[f32], top_k: usize) -> Vec<(usize, f32)>;
    /// 快照用：取槽位向量；tombstone 或越界槽返回 `None`。
    fn vector_at(&self, idx: usize) -> Option<Vec<f32>>;
}

pub struct FlatVectorStore {
    vectors: Vec<Option<Vec<f32>>>,
    alive: usize,
    dim: usize,
}

impl FlatVectorStore {
    pub fn new(dim: usize) -> Self {
        Self {
            vectors: Vec::new(),
            alive: 0,
            dim,
        }
    }
}

impl VectorStore for FlatVectorStore {
    fn add(&mut self, mut v: Vec<f32>) -> Result<usize> {
        if v.len() != self.dim {
            return Err(SearchError::DimMismatch {
                expected: self.dim,
                got: v.len(),
            });
        }
        let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n > 1e-12 {
            for x in v.iter_mut() {
                *x /= n;
            }
        }
        let idx = self.vectors.len();
        self.vectors.push(Some(v));
        self.alive += 1;
        Ok(idx)
    }

    fn mark_deleted(&mut self, idx: usize) {
        if let Some(slot) = self.vectors.get_mut(idx)
            && slot.is_some()
        {
            *slot = None;
            self.alive = self.alive.saturating_sub(1);
        }
    }

    fn is_alive(&self, idx: usize) -> bool {
        self.vectors.get(idx).and_then(|o| o.as_ref()).is_some()
    }

    fn n_alive(&self) -> usize {
        self.alive
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn vector_at(&self, idx: usize) -> Option<Vec<f32>> {
        self.vectors.get(idx).cloned().flatten()
    }

    fn search(&self, q: &[f32], top_k: usize) -> Vec<(usize, f32)> {
        if q.len() != self.dim {
            return Vec::new();
        }
        let mut scores: Vec<(usize, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (i, dot(q, v).max(0.0))))
            .collect();
        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        scores.truncate(top_k);
        scores
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_rejects_dim_mismatch() {
        let mut s = FlatVectorStore::new(4);
        let err = s.add(vec![1.0, 2.0, 3.0]).unwrap_err();
        match err {
            SearchError::DimMismatch { expected, got } => {
                assert_eq!(expected, 4);
                assert_eq!(got, 3);
            }
            other => panic!("expected DimMismatch, got {other:?}"),
        }
    }

    #[test]
    fn search_filters_tombstoned_slots() {
        let mut s = FlatVectorStore::new(3);
        let v0 = vec![1.0, 0.0, 0.0];
        let v1 = vec![0.0, 1.0, 0.0];
        let i0 = s.add(v0).unwrap();
        let i1 = s.add(v1).unwrap();
        s.mark_deleted(i0);
        let q = vec![0.0, 1.0, 0.0];
        let hits = s.search(&q, 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, i1);
    }

    #[test]
    fn search_returns_top_k_descending() {
        let mut s = FlatVectorStore::new(2);
        let a = s.add(vec![1.0, 0.0]).unwrap();
        let b = s.add(vec![0.9, 0.1]).unwrap();
        let c = s.add(vec![0.0, 1.0]).unwrap();
        let q = vec![1.0, 0.0];
        let hits = s.search(&q, 2);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].0, a);
        assert_eq!(hits[1].0, b);
        assert!(!hits.iter().any(|(i, _)| *i == c));
    }

    #[test]
    fn search_returns_empty_on_dim_mismatch() {
        let s = FlatVectorStore::new(2);
        let hits = s.search(&[1.0, 0.0, 0.0], 5);
        assert!(hits.is_empty());
    }

    #[test]
    fn n_alive_tracks_tombstones() {
        let mut s = FlatVectorStore::new(2);
        let a = s.add(vec![1.0, 0.0]).unwrap();
        s.add(vec![0.0, 1.0]).unwrap();
        assert_eq!(s.n_alive(), 2);
        s.mark_deleted(a);
        assert_eq!(s.n_alive(), 1);
    }

    #[test]
    fn vector_at_returns_alive_vector_and_none_for_tombstone() {
        let mut s = super::FlatVectorStore::new(2);
        let a = s.add(vec![1.0, 0.0]).unwrap();
        let b = s.add(vec![0.0, 1.0]).unwrap();
        assert_eq!(s.vector_at(a), Some(vec![1.0, 0.0]));
        s.mark_deleted(b);
        assert_eq!(s.vector_at(b), None);
        assert_eq!(s.vector_at(999), None);
    }
}
