use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SparseVector {
    pub skill_name: String,
    pub vector: HashMap<String, f64>,
}

pub struct SparseVectorIndex {
    embeddings: Vec<SparseVector>,
    idf: HashMap<String, f64>,
    vocabulary: Vec<String>,
}

impl Default for SparseVectorIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SparseVectorIndex {
    pub fn new() -> Self {
        Self {
            embeddings: Vec::new(),
            idf: HashMap::new(),
            vocabulary: Vec::new(),
        }
    }

    pub fn build_from_texts(&mut self, texts: &[(String, String)]) {
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        let mut tokenized_docs: Vec<(String, HashMap<String, usize>)> =
            Vec::new();

        for (name, text) in texts {
            let tokens = tokenize(text);
            let mut term_freq: HashMap<String, usize> = HashMap::new();
            for token in &tokens {
                *term_freq.entry(token.clone()).or_insert(0) += 1;
                *doc_freq.entry(token.clone()).or_insert(0) += 1;
            }
            tokenized_docs.push((name.clone(), term_freq));
        }

        let total_docs = texts.len() as f64;
        self.idf.clear();
        for (term, df) in &doc_freq {
            self.idf
                .insert(term.clone(), (total_docs / (*df as f64)).ln() + 1.0);
        }

        self.embeddings.clear();
        self.vocabulary = doc_freq.keys().cloned().collect();
        self.vocabulary.sort();

        for (name, term_freq) in &tokenized_docs {
            let mut vector = HashMap::new();
            for (term, tf) in term_freq {
                let tfidf = (*tf as f64) * self.idf.get(term).unwrap_or(&1.0);
                vector.insert(term.clone(), tfidf);
            }
            self.embeddings.push(SparseVector {
                skill_name: name.clone(),
                vector,
            });
        }
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(String, f64)> {
        if self.embeddings.is_empty() {
            return Vec::new();
        }

        let query_tokens = tokenize(query);
        let mut query_vec: HashMap<String, f64> = HashMap::new();
        for token in &query_tokens {
            let idf = self.idf.get(token).copied().unwrap_or(1.0);
            *query_vec.entry(token.clone()).or_insert(0.0) += idf;
        }

        let mut scores: Vec<(String, f64)> = self
            .embeddings
            .iter()
            .map(|emb| {
                let score = cosine_similarity(&query_vec, &emb.vector);
                (emb.skill_name.clone(), score)
            })
            .collect();

        scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        scores.truncate(top_k);
        scores
    }

    pub fn embedding_count(&self) -> usize {
        self.embeddings.len()
    }

    pub fn vocabulary_size(&self) -> usize {
        self.vocabulary.len()
    }
}

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
