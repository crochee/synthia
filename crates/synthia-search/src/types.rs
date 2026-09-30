//! Self-contained demo types that implement [`Searchable`].
//!
//! These are *not* the canonical Skill / Tool / Memory types
//! the agent runtime uses. They exist so the lib has something
//! to demo against without taking a dependency on
//! `synthia-skill` or `synthia-context`. A lib consumer wanting
//! to search real `synthia_skill::Skill` values writes their
//! own `impl Searchable for synthia_skill::Skill` — one
//! screen-long (see `lib.rs` doc-comment example).

use synthia_core::registry::RegistryItem;

use crate::searchable::Searchable;

/// Sealed trait: types that carry a creation timestamp.
///
/// [`crate::RecencyReranker`] only applies to types that implement
/// this — for everything else, the bound fails to satisfy and
/// the reranker is silently absent from the chain.
pub trait HasCreatedAt {
    fn created_at(&self) -> f64;
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub when_to_use: Vec<String>,
    pub not_for: Vec<String>,
    pub tags: Vec<String>,
    pub examples: Vec<String>,
}

#[allow(clippy::misnamed_getters)]
impl RegistryItem for Skill {
    fn name(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.name
    }
}

impl Searchable for Skill {
    fn indexed_fields(&self) -> Vec<(String, f32)> {
        let mut v =
            vec![(self.name.clone(), 2.0), (self.description.clone(), 1.0)];
        for w in &self.when_to_use {
            v.push((w.clone(), 2.0));
        }
        for e in &self.examples {
            v.push((e.clone(), 1.0));
        }
        for t in &self.tags {
            v.push((t.clone(), 1.5));
        }
        v
    }

    fn when_to_use(&self) -> &[String] {
        &self.when_to_use
    }

    fn not_for(&self) -> &[String] {
        &self.not_for
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}

impl HasCreatedAt for Skill {
    fn created_at(&self) -> f64 {
        0.0
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub description: String,
    pub when_to_use: Vec<String>,
    pub not_for: Vec<String>,
    pub tags: Vec<String>,
    pub schema_hint: String,
}

#[allow(clippy::misnamed_getters)]
impl RegistryItem for Tool {
    fn name(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.name
    }
}

impl Searchable for Tool {
    fn indexed_fields(&self) -> Vec<(String, f32)> {
        let mut v = vec![
            (self.name.clone(), 2.0),
            (self.description.clone(), 1.0),
            (self.schema_hint.clone(), 0.5),
        ];
        for w in &self.when_to_use {
            v.push((w.clone(), 2.0));
        }
        for t in &self.tags {
            v.push((t.clone(), 1.5));
        }
        v
    }

    fn when_to_use(&self) -> &[String] {
        &self.when_to_use
    }

    fn not_for(&self) -> &[String] {
        &self.not_for
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Memory {
    pub id: String,
    pub summary: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: f64,
    pub importance: f32,
}

impl RegistryItem for Memory {
    fn name(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.summary
    }
}

impl Searchable for Memory {
    fn indexed_fields(&self) -> Vec<(String, f32)> {
        let mut v =
            vec![(self.summary.clone(), 1.5), (self.content.clone(), 2.0)];
        for t in &self.tags {
            v.push((t.clone(), 1.0));
        }
        v
    }

    fn tags(&self) -> &[String] {
        &self.tags
    }

    fn preview(&self) -> Option<String> {
        let first = self.content.lines().find(|l| !l.trim().is_empty())?;
        Some(first.chars().take(160).collect())
    }
}

impl HasCreatedAt for Memory {
    fn created_at(&self) -> f64 {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_indexed_fields_include_when_and_examples() {
        let s = Skill {
            id: "pdf".into(),
            name: "PDF 提取".into(),
            description: "d".into(),
            when_to_use: vec!["读 PDF".into()],
            not_for: vec![],
            tags: vec!["pdf".into()],
            examples: vec!["提取表格".into()],
        };
        let fields = s.indexed_fields();
        let texts: Vec<&str> = fields.iter().map(|(t, _)| t.as_str()).collect();
        assert!(texts.contains(&"PDF 提取"));
        assert!(texts.contains(&"读 PDF"));
        assert!(texts.contains(&"提取表格"));
    }

    #[test]
    fn memory_has_created_at_returns_real_value() {
        let m = Memory {
            id: "m".into(),
            summary: "s".into(),
            content: "c".into(),
            tags: vec![],
            created_at: 12345.678,
            importance: 0.7,
        };
        assert_eq!(m.created_at(), 12345.678);
    }

    #[test]
    fn skill_has_created_at_returns_zero() {
        let s = Skill {
            id: "s".into(),
            name: "n".into(),
            description: "".into(),
            when_to_use: vec![],
            not_for: vec![],
            tags: vec![],
            examples: vec![],
        };
        assert_eq!(s.created_at(), 0.0);
    }
}
