//! [`Bm25Index`] — the classic BM25 (Robertson / Sparck Jones /
//! Walker) inverted index used for keyword scoring in
//! `synthia-search`.
//!
//! It is a deliberately small implementation:
//!
//! - Configurable `k1` (term-frequency saturation; default `1.5`) and
//!   `b` (length normalisation; default `0.75`).
//! - IDF is the smoothed Robertson form `ln((n - df + 0.5) / (df + 0.5) + 1)`
//!   where `n` is the number of **alive** docs and `df` counts only
//!   alive docs that contain the term. A query token contributes at
//!   most one IDF per query (duplicate tokens are de-duplicated).
//! - Length normalisation uses `avg_dl` over the alive corpus only,
//!   so tombstoning one short doc does not skew average document
//!   length.
//! - Tombstones: `mark_deleted` flips an `alive` flag and updates
//!   counters. The posting list still contains the slot, but
//!   `score_all` skips it. The engine's `compact` path (Task 7)
//!   rebuilds the index without the dead slots and resets the
//!   tombstone counter via [`Bm25Index::reset_deleted_counter`].

use std::collections::{HashMap, HashSet};

pub struct Bm25Index {
    doc_len: Vec<usize>,
    postings: HashMap<String, Vec<(usize, u32)>>,
    alive: Vec<bool>,
    n_alive: usize,
    n_deleted: usize,
    total_dl: usize,
    k1: f64,
    b: f64,
}

impl Bm25Index {
    pub fn new(k1: f64, b: f64) -> Self {
        Self {
            doc_len: Vec::new(),
            postings: HashMap::new(),
            alive: Vec::new(),
            n_alive: 0,
            n_deleted: 0,
            total_dl: 0,
            k1,
            b,
        }
    }

    pub fn add(&mut self, tokens: &[String]) -> usize {
        let idx = self.doc_len.len();
        self.doc_len.push(tokens.len());
        self.alive.push(true);
        self.n_alive += 1;
        self.total_dl += tokens.len();

        let mut tf: HashMap<&str, u32> = HashMap::new();
        for t in tokens {
            *tf.entry(t.as_str()).or_insert(0) += 1;
        }
        for (t, c) in tf {
            self.postings
                .entry(t.to_string())
                .or_default()
                .push((idx, c));
        }
        idx
    }

    pub fn mark_deleted(&mut self, idx: usize) {
        if idx < self.alive.len() && self.alive[idx] {
            self.alive[idx] = false;
            self.n_alive = self.n_alive.saturating_sub(1);
            self.n_deleted += 1;
            self.total_dl = self.total_dl.saturating_sub(self.doc_len[idx]);
        }
    }

    pub fn is_alive(&self, idx: usize) -> bool {
        self.alive.get(idx).copied().unwrap_or(false)
    }

    pub fn n_alive(&self) -> usize {
        self.n_alive
    }

    pub fn n_deleted(&self) -> usize {
        self.n_deleted
    }

    /// Returns the configured `k1` BM25 constant.
    pub fn k1(&self) -> f64 {
        self.k1
    }

    /// Returns the configured `b` BM25 constant.
    pub fn b(&self) -> f64 {
        self.b
    }

    /// Reset the tombstone counter to zero. Called by `SearchEngine`
    /// after a full rebuild (Task 7), where every rebuilt doc is alive
    /// and the old tombstone slots are gone.
    pub fn reset_deleted_counter(&mut self) {
        self.n_deleted = 0;
    }

    fn avg_dl(&self) -> f64 {
        if self.n_alive == 0 {
            1.0
        } else {
            (self.total_dl as f64 / self.n_alive as f64).max(1e-6)
        }
    }

    pub fn score_all(&self, query_tokens: &[String]) -> Vec<f32> {
        let mut scores = vec![0f32; self.doc_len.len()];
        if self.n_alive == 0 {
            return scores;
        }
        let avg_dl = self.avg_dl();
        let n = self.n_alive as f64;
        let mut seen: HashSet<&str> = HashSet::new();

        for qt in query_tokens {
            if !seen.insert(qt.as_str()) {
                continue;
            }
            let Some(postings) = self.postings.get(qt) else {
                continue;
            };

            // df counts only alive docs.
            let df =
                postings.iter().filter(|(d, _)| self.alive[*d]).count() as f64;
            if df <= 0.0 {
                continue;
            }
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

            for &(doc, tf) in postings {
                if !self.alive[doc] {
                    continue;
                }
                let dl = self.doc_len[doc] as f64;
                let tf = tf as f64;
                let denom =
                    tf + self.k1 * (1.0 - self.b + self.b * dl / avg_dl);
                scores[doc] += (idf * (tf * (self.k1 + 1.0)) / denom) as f32;
            }
        }
        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx() -> Bm25Index {
        Bm25Index::new(1.5, 0.75)
    }

    #[test]
    fn add_returns_monotonic_indices() {
        let mut i = idx();
        assert_eq!(i.add(&["a".into(), "b".into()]), 0);
        assert_eq!(i.add(&["c".into()]), 1);
        assert_eq!(i.n_alive(), 2);
    }

    #[test]
    fn tombstone_decrements_alive_and_scores_zero() {
        let mut i = idx();
        let a = i.add(&["alpha".into(), "beta".into()]);
        let _b = i.add(&["gamma".into()]);
        i.mark_deleted(a);
        assert!(!i.is_alive(a));
        assert_eq!(i.n_alive(), 1);
        assert_eq!(i.n_deleted(), 1);
        let scores = i.score_all(&["alpha".into()]);
        assert_eq!(scores[a], 0.0);
    }

    #[test]
    fn idf_outranks_common_term() {
        let mut i = idx();
        i.add(&["the".into(), "rust".into()]);
        i.add(&["the".into(), "the".into(), "rust".into()]);
        i.add(&["the".into(), "the".into(), "the".into(), "kotlin".into()]);
        let scores = i.score_all(&["rust".into()]);
        // doc 0 has 1 rust + 1 the; doc 1 has 1 rust + 2 the; doc 2 has 0 rust.
        // TF saturation + length norm should still keep doc 0/1 well above doc 2.
        assert!(scores[0] > 0.0);
        assert!(scores[1] > 0.0);
        assert_eq!(scores[2], 0.0);
    }

    #[test]
    fn duplicate_query_token_counted_once_for_idf() {
        let mut i = idx();
        i.add(&["alpha".into()]);
        i.add(&["alpha".into(), "beta".into()]);
        let scores = i.score_all(&["alpha".into(), "alpha".into()]);
        assert!(scores[0] > 0.0);
    }

    #[test]
    fn empty_query_returns_zeros() {
        let mut i = idx();
        i.add(&["x".into()]);
        let scores = i.score_all(&[]);
        assert_eq!(scores, vec![0.0]);
    }

    #[test]
    fn avg_dl_handles_empty() {
        let i = idx();
        assert_eq!(i.avg_dl(), 1.0);
    }
}
