//! The 1 `/api/v2/memory/search` handler + its 3
//! request/response types.
//!
//! Memory search is a placeholder: it iterates the
//! `.agents/skills/` directory and uses skill files as a
//! memory proxy, ranking by simple keyword match count
//! (score = `min(occurrences * 0.5, 1.0)`).

use std::sync::Arc;

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use synthia_core::ApiResponse;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct MemorySearchQuery {
    pub q: String,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct MemoryResult {
    pub id: String,
    pub content: String,
    pub score: f64,
}

#[derive(Serialize)]
pub struct MemorySearchResponse {
    pub query: String,
    pub results: Vec<MemoryResult>,
    pub count: usize,
}

/// GET /api/v2/memory/search?q=<query> - Search memory.
pub async fn search_memory(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemorySearchQuery>,
) -> Json<ApiResponse<MemorySearchResponse>> {
    let limit = params.limit.unwrap_or(10);
    let skills_dir = state.workspace_root.join(".agents").join("skills");

    // Simple in-memory search over skill files as a memory proxy
    let mut results = Vec::new();
    if skills_dir.exists()
        && let Ok(entries) = std::fs::read_dir(&skills_dir)
    {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                let skill_md = entry.path().join("SKILL.md");
                if skill_md.exists()
                    && let Ok(content) = std::fs::read_to_string(&skill_md)
                {
                    let content_lower = content.to_lowercase();
                    let query_lower = params.q.to_lowercase();
                    if content_lower.contains(&query_lower) {
                        // Calculate simple relevance score
                        let occurrences =
                            content_lower.matches(&query_lower).count();
                        let score = (occurrences as f64 * 0.5).min(1.0);
                        results.push(MemoryResult {
                            id: name.to_string(),
                            content: content
                                .lines()
                                .take(5)
                                .collect::<Vec<_>>()
                                .join(" "),
                            score,
                        });
                    }
                }
            }
        }
    }

    // Sort by score descending
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    Json(ApiResponse::ok(MemorySearchResponse {
        query: params.q,
        count: results.len(),
        results,
    }))
}
