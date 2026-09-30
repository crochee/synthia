//! `Searchable` — the trait every indexable value implements.
//!
//! `Searchable` extends [`synthia_core::registry::RegistryItem`]
//! so that any value the engine holds can also be listed by a
//! standard `Registry<Searchable>` listing — same `name()` /
//! `description()` vocabulary, no parallel type system.
//!
//! All `indexed_fields` texts are concatenated at weight (rounded
//! up to integer repetitions) to form the BM25 index text. The
//! embedder sees `embed_text()` which defaults to the same
//! concatenation. Implementors with a pre-computed embedding can
//! override [`Searchable::embedding`] to skip the embedder call.

use synthia_core::registry::RegistryItem;

pub trait Searchable: RegistryItem + Send + Sync + Clone + 'static {
    fn indexed_fields(&self) -> Vec<(String, f32)>;

    fn when_to_use(&self) -> &[String] {
        &[]
    }

    fn not_for(&self) -> &[String] {
        &[]
    }

    fn tags(&self) -> &[String] {
        &[]
    }

    fn embedding(&self) -> Option<&[f32]> {
        None
    }

    /// Short snippet surfaced on the [`Hit`](crate::Hit) so a
    /// caller can act without a follow-up load. `None` (the
    /// default) for catalog rows whose `description()` already
    /// is the payload; memory-style bodies override this with
    /// the first content excerpt.
    fn preview(&self) -> Option<String> {
        None
    }

    fn embed_text(&self) -> String {
        let mut s = String::new();
        for (text, weight) in self.indexed_fields() {
            let reps = weight.round().max(1.0) as usize;
            for _ in 0..reps {
                s.push_str(&text);
                s.push(' ');
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Demo {
        name: String,
        title: String,
        fields: Vec<(String, f32)>,
    }

    impl RegistryItem for Demo {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.title
        }
    }

    impl Searchable for Demo {
        fn indexed_fields(&self) -> Vec<(String, f32)> {
            self.fields.clone()
        }
    }

    #[test]
    fn embed_text_repeats_by_weight() {
        let d = Demo {
            name: "id".into(),
            title: "t".into(),
            fields: vec![("alpha".into(), 2.0), ("beta".into(), 1.0)],
        };
        assert_eq!(d.embed_text(), "alpha alpha beta ");
    }

    #[test]
    fn embed_text_weight_below_one_still_emits_once() {
        let d = Demo {
            name: "id".into(),
            title: "t".into(),
            fields: vec![("only".into(), 0.3)],
        };
        assert_eq!(d.embed_text(), "only ");
    }

    #[test]
    fn default_when_to_use_is_empty() {
        let d = Demo {
            name: "i".into(),
            title: "t".into(),
            fields: vec![],
        };
        assert!(d.when_to_use().is_empty());
    }
}
