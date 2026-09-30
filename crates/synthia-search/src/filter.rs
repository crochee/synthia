//! [`Filter`] — gate items before they enter the reranker chain.
//!
//! [`BasicFilter`] is the default. It enforces only
//! `QueryContext::required_tags` (all required tags must be present
//! in `item.tags()`). There is **no domain filter** — the
//! `Domain` enum was deliberately removed in spec v3; `T` is
//! itself the domain, and cross-`T` filtering happens at the
//! `Registry` layer (Task 7).

use std::collections::HashSet;

use crate::{QueryContext, searchable::Searchable};

pub trait Filter<T: Searchable>: Send + Sync + 'static {
    fn allow(&self, ctx: &QueryContext, item: &T) -> bool;
}

pub struct BasicFilter;

impl<T: Searchable> Filter<T> for BasicFilter {
    fn allow(&self, ctx: &QueryContext, item: &T) -> bool {
        if ctx.required_tags.is_empty() {
            return true;
        }
        let set: HashSet<&String> = item.tags().iter().collect();
        ctx.required_tags.iter().all(|t| set.contains(t))
    }
}

#[cfg(test)]
mod tests {
    use synthia_core::registry::RegistryItem;

    use super::*;

    #[derive(Clone)]
    struct TagItem {
        id: String,
        tags: Vec<String>,
    }

    impl RegistryItem for TagItem {
        fn name(&self) -> &str {
            &self.id
        }

        fn description(&self) -> &str {
            ""
        }
    }

    impl Searchable for TagItem {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            vec![(self.id.clone(), 1.0)]
        }

        fn tags(&self) -> &[String] {
            &self.tags
        }
    }

    #[test]
    fn no_required_tags_passes_everything() {
        let item = TagItem {
            id: "a".into(),
            tags: vec!["x".into()],
        };
        let ctx = QueryContext::new("q");
        assert!(BasicFilter.allow(&ctx, &item));
    }

    #[test]
    fn single_required_tag_must_be_present() {
        let item = TagItem {
            id: "a".into(),
            tags: vec!["x".into()],
        };
        let mut ctx = QueryContext::new("q");
        ctx.required_tags.push("y".into());
        assert!(!BasicFilter.allow(&ctx, &item));
    }

    #[test]
    fn all_required_tags_must_be_present() {
        let item = TagItem {
            id: "a".into(),
            tags: vec!["x".into(), "y".into()],
        };
        let mut ctx = QueryContext::new("q");
        ctx.required_tags.push("x".into());
        ctx.required_tags.push("y".into());
        assert!(BasicFilter.allow(&ctx, &item));
        ctx.required_tags.push("z".into());
        assert!(!BasicFilter.allow(&ctx, &item));
    }
}
