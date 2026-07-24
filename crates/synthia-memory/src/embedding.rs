//! Semantic embedding and vector retrieval module.
//!
//! Provides embedding model integration for semantic search capabilities.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub input: Vec<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub model: String,
    pub embeddings: Vec<Embedding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    pub index: usize,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub dimension: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "text-embedding-3-small".to_string(),
            dimension: 1536,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VectorStoreConfig {
    pub dimension: usize,
    pub metric: VectorMetric,
}

impl Default for VectorStoreConfig {
    fn default() -> Self {
        Self {
            dimension: 1536,
            metric: VectorMetric::Cosine,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VectorMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

pub struct InMemoryVectorStore {
    dimension: usize,
    metric: VectorMetric,
    vectors: Vec<(String, Vec<f32>)>,
}

impl InMemoryVectorStore {
    pub fn new(config: VectorStoreConfig) -> Self {
        Self {
            dimension: config.dimension,
            metric: config.metric,
            vectors: Vec::new(),
        }
    }

    pub fn add(&mut self, id: String, vector: Vec<f32>) {
        if vector.len() == self.dimension {
            self.vectors.push((id, vector));
        }
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Vec<(String, f32)> {
        let mut results: Vec<(String, f32)> = self
            .vectors
            .iter()
            .map(|(id, vec)| {
                let score = match self.metric {
                    VectorMetric::Cosine => cosine_similarity(query, vec),
                    VectorMetric::Euclidean => -euclidean_distance(query, vec),
                    VectorMetric::DotProduct => dot_product(query, vec),
                };
                (id.clone(), score)
            })
            .collect();

        results.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);
        results
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_product(a, b);
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

pub struct SemanticRetriever {
    embedding_provider: std::sync::Arc<EmbeddingProvider>,
    vector_store: InMemoryVectorStore,
    bm25_weight: f64,
    semantic_weight: f64,
}

impl SemanticRetriever {
    pub fn new(embedding_provider: std::sync::Arc<EmbeddingProvider>) -> Self {
        Self {
            embedding_provider,
            vector_store: InMemoryVectorStore::new(VectorStoreConfig::default()),
            bm25_weight: 0.5,
            semantic_weight: 0.5,
        }
    }

    pub fn with_weights(
        mut self,
        bm25_weight: f64,
        semantic_weight: f64,
    ) -> Self {
        self.bm25_weight = bm25_weight;
        self.semantic_weight = semantic_weight;
        self
    }

    pub fn add_entry(
        &mut self,
        id: String,
        _content: String,
        vector: Vec<f32>,
    ) {
        self.vector_store.add(id, vector);
    }

    pub async fn embed_text(
        &self,
        text: &str,
    ) -> Result<Vec<f32>, synthia_core::Error> {
        let embeddings = self
            .embedding_provider
            .embed(vec![text.to_string()])
            .await?;
        Ok(embeddings
            .first()
            .map(|e| e.embedding.clone())
            .unwrap_or_default())
    }

    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, f32)>, synthia_core::Error> {
        let query_embedding = self.embed_text(query).await?;
        Ok(self.vector_store.search(&query_embedding, limit))
    }

    pub fn len(&self) -> usize {
        self.vector_store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub struct EmbeddingProvider {
    config: EmbeddingConfig,
    client: reqwest::Client,
}

impl EmbeddingProvider {
    pub fn new(config: EmbeddingConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.config.api_key = Some(api_key);
        self
    }

    pub async fn embed(
        &self,
        texts: Vec<String>,
    ) -> Result<Vec<Embedding>, synthia_core::Error> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = EmbeddingRequest {
            input: texts,
            model: self.config.model.clone(),
        };

        let url = format!("{}/embeddings", self.config.base_url);
        let mut req = self.client.post(&url).json(&request);

        if let Some(ref key) = self.config.api_key {
            req = req.bearer_auth(key);
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_default();
            return Err(synthia_core::Error::RequestFailed { status, message });
        }

        let emb_response: EmbeddingResponse = response.json().await?;
        Ok(emb_response.embeddings)
    }

    pub fn dimension(&self) -> usize {
        self.config.dimension
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_provider_creation() {
        let config = EmbeddingConfig::default();
        let provider = EmbeddingProvider::new(config);
        assert_eq!(provider.dimension(), 1536);
    }

    #[test]
    fn test_vector_store_add_and_search() {
        let mut store = InMemoryVectorStore::new(VectorStoreConfig {
            dimension: 3,
            metric: VectorMetric::Cosine,
        });
        store.add("1".to_string(), vec![1.0, 0.0, 0.0]);
        store.add("2".to_string(), vec![0.0, 1.0, 0.0]);
        store.add("3".to_string(), vec![0.0, 0.0, 1.0]);

        let results = store.search(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results[0].0, "1");
        assert!((results[0].1 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 0.001);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_semantic_retriever_creation() {
        let config = EmbeddingConfig::default();
        let provider = std::sync::Arc::new(EmbeddingProvider::new(config));
        let retriever = SemanticRetriever::new(provider);
        assert_eq!(retriever.len(), 0);
    }
}
