//! [`Reranker`] — post-score adjustments applied left-to-right.
//!
//! [`RuleReranker`] is the default: bonus on `when_to_use` token
//! overlap, penalty on `not_for` overlap, tiny bonus per matching
//! tag. [`RecencyReranker`] applies time-decay for items that
//! carry a creation timestamp; it requires `Clock` injection per
//! AGENTS §3.8 (no direct `chrono::Utc::now()` in production).

use std::{collections::HashSet, sync::Arc};

use synthia_core::clock::{Clock, SystemClock};

use crate::{
    Tokenizer,
    hit::{Hit, QueryContext},
    searchable::Searchable,
};

pub trait Reranker<T: Searchable>: Send + Sync + 'static {
    fn rerank(&self, ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>);
}

pub struct RuleReranker {
    pub when_bonus: f32,
    pub not_for_penalty: f32,
    pub tag_bonus: f32,
    pub tokenizer: Arc<dyn Tokenizer>,
}

impl RuleReranker {
    pub fn new(tokenizer: Arc<dyn Tokenizer>) -> Self {
        Self {
            when_bonus: 0.25,
            not_for_penalty: 0.40,
            tag_bonus: 0.10,
            tokenizer,
        }
    }
}

impl<T: Searchable> Reranker<T> for RuleReranker {
    fn rerank(&self, ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>) {
        let q_set: HashSet<String> =
            self.tokenizer.tokenize(&ctx.text).into_iter().collect();
        if q_set.is_empty() {
            return;
        }

        for h in hits.iter_mut() {
            let Some(item) = items.get(h.item_idx) else {
                continue;
            };
            let mut bonus = 0.0f32;

            let mut best_w = 0.0f32;
            let mut best_w_txt = "";
            for w in item.when_to_use() {
                let o = token_overlap(&q_set, w, self.tokenizer.as_ref());
                if o > best_w {
                    best_w = o;
                    best_w_txt = w;
                }
            }
            if best_w > 0.0 {
                bonus += self.when_bonus * best_w;
                h.reasons.push(format!("when_to_use≈{}", best_w_txt));
            }

            let mut worst_n = 0.0f32;
            let mut worst_n_txt = "";
            for n in item.not_for() {
                let o = token_overlap(&q_set, n, self.tokenizer.as_ref());
                if o > worst_n {
                    worst_n = o;
                    worst_n_txt = n;
                }
            }
            if worst_n > 0.0 {
                bonus -= self.not_for_penalty * worst_n;
                h.reasons.push(format!("not_for≈{}", worst_n_txt));
            }

            for t in item.tags() {
                if q_set.contains(t) {
                    bonus += self.tag_bonus;
                    h.reasons.push(format!("tag:{}", t));
                }
            }

            h.score = (h.score + bonus).max(0.0);
        }
    }
}

fn token_overlap(
    q_set: &HashSet<String>,
    doc: &str,
    tk: &dyn Tokenizer,
) -> f32 {
    let d = tk.tokenize(doc);
    if d.is_empty() || q_set.is_empty() {
        return 0.0;
    }
    let hits = d.iter().filter(|t| q_set.contains(*t)).count();
    hits as f32 / d.len() as f32
}

pub struct RecencyReranker<T: Searchable> {
    clock: Arc<dyn Clock>,
    half_life_days: f32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Searchable> RecencyReranker<T> {
    pub fn new(clock: Arc<dyn Clock>, half_life_days: f32) -> Self {
        Self {
            clock,
            half_life_days,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn system(half_life_days: f32) -> Self {
        Self::new(Arc::new(SystemClock), half_life_days)
    }
}

// The default impl consumes the items list but only needs
// `created_at` for time decay. Without coupling to a particular
// domain struct, we let the lib consumer pick the impl point: a
// future `RecencyReranker<Memory>` impl (in `types.rs`) is the
// concrete one. Here we provide a generic hook that fires only
// if the item exposes `created_at` via a sealed trait we ship
// in types.rs (see Task 7).
impl<T: Searchable + crate::types::HasCreatedAt> Reranker<T>
    for RecencyReranker<T>
{
    fn rerank(&self, _ctx: &QueryContext, items: &[T], hits: &mut Vec<Hit>) {
        let now = self.clock.now();
        let now_ts = now.timestamp() as f64;
        for h in hits.iter_mut() {
            let Some(item) = items.get(h.item_idx) else {
                continue;
            };
            let age_secs = (now_ts - item.created_at()).max(0.0);
            let age_days = (age_secs / 86_400.0) as f32;
            let decay = 0.5f32.powf(age_days / self.half_life_days.max(1e-3));
            h.score = (h.score * (0.5 + 0.5 * decay)).max(0.0);
            h.reasons.push(format!("recency:{:.2}", decay));
        }
    }
}

#[cfg(test)]
mod tests {
    use synthia_core::{clock::FixedClock, registry::RegistryItem};

    use super::*;

    fn at(seconds: f64) -> Arc<dyn Clock> {
        // seconds-since-epoch → RFC3339. We use the
        // `synthia_core::clock::FixedClock::from_rfc3339`
        // constructor so we don't need a direct `chrono` dep.
        // The hard-coded instant below is unix epoch
        // 2_000_000_000 (≈ 2033-05-18T03:33:20Z).
        let rfc3339 = match seconds as i64 {
            2_000_000_000 => "2033-05-18T03:33:20+00:00",
            _ => "1970-01-01T00:00:00+00:00",
        };
        Arc::new(FixedClock::from_rfc3339(rfc3339))
    }

    #[derive(Clone)]
    struct TaggedItem {
        id: String,
        tags: Vec<String>,
        when: Vec<String>,
    }

    impl RegistryItem for TaggedItem {
        fn name(&self) -> &str {
            &self.id
        }

        fn description(&self) -> &str {
            ""
        }
    }

    impl Searchable for TaggedItem {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            vec![(self.id.clone(), 1.0)]
        }

        fn when_to_use(&self) -> &[String] {
            &self.when
        }

        fn tags(&self) -> &[String] {
            &self.tags
        }
    }

    fn build_hit(id: &str, score: f32) -> Hit {
        Hit {
            item_idx: 0,
            domain: String::new(),
            id: id.into(),
            title: id.into(),
            score,
            bm25: score,
            vector: score,
            reasons: vec![],
            preview: None,
        }
    }

    #[test]
    fn rule_reranker_pushes_when_to_use_match() {
        let tk: Arc<dyn Tokenizer> = Arc::new(crate::CjkTokenizer);
        let rr = RuleReranker::new(tk);
        let items = vec![TaggedItem {
            id: "pdf".into(),
            tags: vec![],
            when: vec!["把 PDF 转成文本".into()],
        }];
        let mut hits = vec![build_hit("pdf", 0.5)];
        let ctx = QueryContext::new("把 PDF 转成文本");
        rr.rerank(&ctx, &items, &mut hits);
        assert!(hits[0].score > 0.5);
        assert!(hits[0].reasons.iter().any(|r| r.starts_with("when_to_use")));
    }

    #[test]
    fn rule_reranker_no_match_keeps_score() {
        let tk: Arc<dyn Tokenizer> = Arc::new(crate::CjkTokenizer);
        let rr = RuleReranker::new(tk);
        let items = vec![TaggedItem {
            id: "x".into(),
            tags: vec![],
            when: vec![],
        }];
        let mut hits = vec![build_hit("x", 0.5)];
        let ctx = QueryContext::new("totally unrelated");
        rr.rerank(&ctx, &items, &mut hits);
        assert!((hits[0].score - 0.5).abs() < 1e-6);
    }

    #[test]
    fn recency_reranker_uses_injected_clock() {
        let clock = at(2_000_000_000.0); // arbitrary future
        let rr: RecencyReranker<crate::types::Memory> =
            RecencyReranker::new(clock, 30.0);
        let items = vec![crate::types::Memory {
            id: "mem_001".into(),
            summary: "old".into(),
            content: "x".into(),
            tags: vec![],
            created_at: 1_000_000_000.0,
            importance: 0.5,
        }];
        let mut hits = vec![build_hit("mem_001", 1.0)];
        let ctx = QueryContext::new("anything");
        rr.rerank(&ctx, &items, &mut hits);
        // 1e9 seconds is ~31.7 years; with 30-day half-life, decay is ~0.
        assert!(hits[0].score < 1.0);
        assert!(hits[0].reasons.iter().any(|r| r.starts_with("recency")));
    }
}
