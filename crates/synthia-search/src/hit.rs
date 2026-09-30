//! [`Hit`] — the wire-shaped score record the engine returns.
//!
//! [`AgentCandidate`] is the agent-facing projection: only the
//! fields a model needs to decide which candidate to load.
//! [`agent_view`] and [`search_tool`] produce JSON for the
//! agent-loop integration.
//!
//! `domain` is stamped by [`Registry::search`](crate::Registry::search)
//! from the key each engine sits under — engines themselves leave
//! it empty, so a `SearchEngine<T>` used standalone never carries
//! a stale label.

use std::collections::HashMap;

use serde::Serialize;

use crate::Registry;

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    #[serde(skip)]
    pub item_idx: usize,
    /// Which catalog the hit came from — the engine's key in the
    /// [`Registry`] (`"tool"`, `"skill"`, …). Empty for standalone
    /// `SearchEngine` searches that never passed through a
    /// registry.
    pub domain: String,
    pub id: String,
    pub title: String,
    pub score: f32,
    pub bm25: f32,
    pub vector: f32,
    pub reasons: Vec<String>,
    /// Short content snippet the caller can act on without a
    /// follow-up load (memory text, session transcript excerpt).
    /// `None` for catalog rows whose `title` already is the
    /// payload (tool / skill / agent descriptors).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct QueryContext {
    pub text: String,
    pub required_tags: Vec<String>,
    pub top_k: usize,
    /// Restrict the search to these domain labels. Empty means
    /// "every registered engine"; the registry skips an engine
    /// whose key is not listed here.
    pub domains: Vec<String>,
    pub extra: HashMap<String, serde_json::Value>,
}

impl QueryContext {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            required_tags: Vec::new(),
            top_k: 5,
            domains: Vec::new(),
            extra: HashMap::new(),
        }
    }

    pub fn tag(mut self, t: impl Into<String>) -> Self {
        self.required_tags.push(t.into());
        self
    }

    pub fn top_k(mut self, k: usize) -> Self {
        self.top_k = k.max(1);
        self
    }

    /// Restrict the query to the given domain labels. An empty
    /// iterator is a no-op: it keeps the "every engine" default,
    /// so callers can forward an optional filter unchecked.
    pub fn domains(
        mut self,
        domains: impl IntoIterator<Item: Into<String>>,
    ) -> Self {
        self.domains = domains.into_iter().map(Into::into).collect();
        self
    }

    pub fn extra(mut self, k: impl Into<String>, v: serde_json::Value) -> Self {
        self.extra.insert(k.into(), v);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentCandidate {
    pub domain: String,
    pub id: String,
    pub title: String,
    pub why: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

pub fn agent_view(hits: &[Hit]) -> serde_json::Result<String> {
    let v: Vec<AgentCandidate> = hits
        .iter()
        .map(|h| AgentCandidate {
            domain: h.domain.clone(),
            id: h.id.clone(),
            title: h.title.clone(),
            why: h.reasons.join("; "),
            score: h.score,
            preview: h.preview.clone(),
        })
        .collect();
    serde_json::to_string_pretty(&v)
}

/// Fan one query out across every engine in `reg` and render the
/// agent-facing JSON projection.
///
/// Async because an engine may delegate to an async retriever (a
/// session full-text index, a memory recall tier); CPU-only
/// engines complete without ever yielding.
pub async fn search_tool(reg: &Registry, query: &str, limit: usize) -> String {
    search_tool_ctx(reg, QueryContext::new(query).top_k(limit)).await
}

/// [`search_tool`] with a caller-built [`QueryContext`] — the
/// variant a caller with a domain filter (or extra routing facts)
/// uses.
pub async fn search_tool_ctx(reg: &Registry, ctx: QueryContext) -> String {
    let hits = reg.search(&ctx).await;
    agent_view(&hits).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, score: f32, reasons: Vec<String>) -> Hit {
        Hit {
            item_idx: 0,
            domain: "skill".into(),
            id: id.into(),
            title: id.into(),
            score,
            bm25: score * 0.5,
            vector: score * 0.5,
            reasons,
            preview: None,
        }
    }

    #[test]
    fn query_context_default_top_k_is_five() {
        let ctx = QueryContext::new("q");
        assert_eq!(ctx.top_k, 5);
    }

    #[test]
    fn query_context_top_k_floor_is_one() {
        let ctx = QueryContext::new("q").top_k(0);
        assert_eq!(ctx.top_k, 1);
    }

    #[test]
    fn query_context_domains_builder_sets_labels() {
        let ctx = QueryContext::new("q").domains(["tool", "skill"]);
        assert_eq!(ctx.domains, vec!["tool", "skill"]);
        let untouched = QueryContext::new("q").domains(Vec::<String>::new());
        assert!(untouched.domains.is_empty());
    }

    #[test]
    fn agent_view_produces_expected_json_shape() {
        let mut hits = vec![
            hit(
                "pdf_extract",
                1.15,
                vec!["when_to_use≈把 PDF 转成文本".into(), "tag:pdf".into()],
            ),
            hit("ocr_scan", 0.28, vec!["semantic match".into()]),
        ];
        hits[0].preview = Some("把 PDF 解析成文本…".into());
        let j = agent_view(&hits).unwrap();
        assert!(j.contains("\"id\": \"pdf_extract\""));
        assert!(j.contains("\"domain\": \"skill\""));
        assert!(
            j.contains("\"why\": \"when_to_use≈把 PDF 转成文本; tag:pdf\"")
        );
        assert!(j.contains("\"score\": 1.15"));
        assert!(j.contains("\"preview\": \"把 PDF 解析成文本…\""));
        // `None` previews must not serialize at all.
        let ocr = j.split("ocr_scan").nth(1).unwrap();
        assert!(!ocr.contains("\"preview\""));
    }

    #[test]
    fn hit_serializes_skipping_item_idx() {
        let h = hit("x", 1.0, vec![]);
        let j = serde_json::to_string(&h).unwrap();
        assert!(!j.contains("item_idx"));
        assert!(j.contains("\"id\":\"x\""));
    }
}
