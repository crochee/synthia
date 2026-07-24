//! The BM25 inverted index — build, search, and
//! incremental update.
//!
//! `BM25Index` stores the per-corpus statistics
//! (average document length, document frequency per
//! term) and a flat `Vec<(name, doc)>` of normalised
//! documents. The classic BM25 scoring formula uses
//! `k1` and `b` (defaulted to 1.2 and 0.75 respectively)
//! to dampen the impact of term-frequency saturation
//! and document-length normalisation.
//!
//! The implementation supports two update modes:
//!
//! 1. **Full rebuild** — [`BM25Index::build`] /
//!    [`BM25Index::rebuild`] recreate the index from
//!    scratch. Use this when the underlying skill set
//!    has changed substantially (registrations, removals,
//!    metadata edits).
//! 2. **Incremental add** — [`BM25Index::add_skill`]
//!    appends a single document and updates the
//!    `df` / `avg_dl` statistics in place. Use this
//!    during the warm-up of a long-lived index.

use std::collections::{HashMap, HashSet};

use super::score::SkillScore;
use crate::types::Skill;

/// BM25 inverted index for skill matching.
///
/// Uses `k1=1.2, b=0.75` as default parameters, matching
/// the BM25 "Robertson–Walker–Zaragoza" reference
/// implementation.
#[derive(Clone)]
pub struct BM25Index {
    /// `(skill_name, normalised_document_text)` pairs.
    /// `normalised` = lowercased, whitespace-tokenised.
    documents: Vec<(String, String)>,
    /// Average document length in whitespace tokens.
    pub(super) avg_dl: f64,
    /// BM25 `k1` parameter (term-frequency saturation).
    pub(super) k1: f64,
    /// BM25 `b` parameter (document-length normalisation).
    pub(super) b: f64,
    /// Document frequency per term (number of documents
    /// that contain the term at least once).
    df: HashMap<String, usize>,
    /// Total number of documents in the index.
    pub n_docs: usize,
}

impl Default for BM25Index {
    fn default() -> Self {
        Self::new()
    }
}

impl BM25Index {
    /// Construct an empty index with the default `k1`
    /// and `b` parameters.
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            avg_dl: 0.0,
            k1: 1.2,
            b: 0.75,
            df: HashMap::new(),
            n_docs: 0,
        }
    }

    /// Build a fresh index from a slice of skills. The
    /// resulting index is fully populated — `n_docs`,
    /// `avg_dl`, and `df` are all derived from
    /// `skills` and cannot be in an inconsistent state.
    pub fn build(skills: &[Skill]) -> Self {
        let documents: Vec<(String, String)> = skills
            .iter()
            .map(|s| {
                let text = format!(
                    "{} {} {} {}",
                    s.metadata.name,
                    s.metadata.description,
                    s.metadata.triggers.join(" "),
                    s.metadata.tags.join(" ")
                );
                (s.metadata.name.clone(), text.to_lowercase())
            })
            .collect();

        let n_docs = documents.len();
        let total_len: usize = documents
            .iter()
            .map(|(_, d)| d.split_whitespace().count())
            .sum();
        let avg_dl = if n_docs > 0 {
            total_len as f64 / n_docs as f64
        } else {
            0.0
        };

        let mut df: HashMap<String, usize> = HashMap::new();
        for (_, doc) in &documents {
            let mut seen = HashSet::new();
            for term in doc.split_whitespace() {
                if seen.insert(term) {
                    *df.entry(term.to_string()).or_insert(0) += 1;
                }
            }
        }

        Self {
            documents,
            avg_dl,
            k1: 1.2,
            b: 0.75,
            df,
            n_docs,
        }
    }

    /// Rebuild the index from the current skill set.
    /// Equivalent to `*self = Self::build(skills)`.
    pub fn rebuild(&mut self, skills: &[Skill]) {
        *self = Self::build(skills);
    }

    /// Search the index for documents matching `query`.
    ///
    /// Returns a sorted-by-descending-score `Vec<SkillScore>`.
    /// Documents with a non-positive score are filtered
    /// out (an exact-zero score means no query term
    /// appeared in the document, which is never
    /// interesting to the caller).
    pub fn search(&self, query: &str) -> Vec<SkillScore> {
        if self.n_docs == 0 {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
        let mut scores: HashMap<String, f64> = HashMap::new();

        for (name, doc) in &self.documents {
            let dl = doc.split_whitespace().count() as f64;
            let tf_map = Self::term_frequencies(doc);
            let mut doc_score = 0.0;

            for term in &query_terms {
                let tf = *tf_map.get(*term).unwrap_or(&0) as f64;
                if tf == 0.0 {
                    continue;
                }
                let df_val = *self.df.get(*term).unwrap_or(&0) as f64;
                let idf = ((self.n_docs as f64 - df_val + 0.5)
                    / (df_val + 0.5)
                    + 1.0)
                    .ln();
                let tf_norm = (tf * (self.k1 + 1.0))
                    / (tf
                        + self.k1
                            * (1.0 - self.b + self.b * (dl / self.avg_dl)));
                doc_score += idf * tf_norm;
            }

            if doc_score > 0.0 {
                scores.insert(name.clone(), doc_score);
            }
        }

        let mut results: Vec<_> = scores.into_iter().collect();
        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        results
            .into_iter()
            .map(|(name, bm25_score)| SkillScore { name, bm25_score })
            .collect()
    }

    /// Build a term-frequency map for a single document.
    /// The map's keys borrow from `doc` (zero-copy
    /// counting) — callers must keep `doc` alive for
    /// the lifetime of the returned map.
    fn term_frequencies(doc: &str) -> HashMap<&str, usize> {
        let mut tf = HashMap::new();
        for term in doc.split_whitespace() {
            *tf.entry(term).or_insert(0) += 1;
        }
        tf
    }

    /// Incrementally add a single skill to the index.
    ///
    /// The `df` map is updated for every distinct term
    /// in the new document, and `avg_dl` is recomputed
    /// from the full document set after the append.
    pub fn add_skill(&mut self, skill: &Skill) {
        let text = format!(
            "{} {} {} {}",
            skill.metadata.name,
            skill.metadata.description,
            skill.metadata.triggers.join(" "),
            skill.metadata.tags.join(" ")
        );
        let doc_text = text.to_lowercase();

        let mut seen = HashSet::new();
        for term in doc_text.split_whitespace() {
            if seen.insert(term) {
                *self.df.entry(term.to_string()).or_insert(0) += 1;
            }
        }

        self.documents.push((skill.metadata.name.clone(), doc_text));
        self.n_docs = self.documents.len();

        let total_len: usize = self
            .documents
            .iter()
            .map(|(_, d)| d.split_whitespace().count())
            .sum();
        self.avg_dl = total_len as f64 / self.n_docs as f64;
    }
}
