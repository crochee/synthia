pub mod cold;
pub mod cold_jsonl;
pub mod compaction;
pub mod context;
pub mod embedding;
pub mod episodic;
pub mod episodic_jsonl;
pub mod hot;
pub mod learning;
pub mod memory_retriever;
pub mod persistence;
pub mod retrieval;
pub mod service;
pub mod store;
pub mod summarizer;
pub mod types;

#[cfg(test)]
mod types_test;

pub use embedding::{
    Embedding,
    EmbeddingConfig,
    EmbeddingProvider,
    InMemoryVectorStore,
    SemanticRetriever,
    VectorMetric,
    VectorStoreConfig,
};
pub use learning::{
    ActionSuggestion,
    ExperienceLearner,
    ExperienceRecord,
    LearnedExperience,
    LearningReport,
};
pub use memory_retriever::{MemoryRetriever, MemorySearchResult};
pub use persistence::{MemoryPersistence, PersistenceConfig};
pub use service::{Memory, MemoryQuery, MemoryService};
pub use summarizer::MemorySummarizer;
pub use types::*;
