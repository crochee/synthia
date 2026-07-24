//! Typed system context source abstraction.
//!
//! Sources track typed values that contribute to the system context (e.g.
//! environment variables). Each source reports a baseline value and deltas
//! via [`Source::update`].

use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// A source of typed system-context content.
///
/// Implementors track a specific category of system-context content
/// (environment variables, working directory, locale) and report changes
/// via [`update`](Source::update).
pub trait Source: Send + Sync {
    /// The typed value tracked by this source.
    type Value: PartialEq + Serialize + DeserializeOwned;

    /// Stable identifier for this source.
    fn key(&self) -> &str;

    /// Load the current value from the environment.
    fn load(&self) -> anyhow::Result<Self::Value>;

    /// Initial baseline value captured at registration time.
    fn baseline(&self) -> Self::Value;

    /// Compute the delta since `prev`. Returns `Some(new_value)` when the
    /// value has changed, `None` when unchanged.
    fn update(&self, prev: &Self::Value)
    -> anyhow::Result<Option<Self::Value>>;

    /// Whether this source has been removed and should no longer contribute.
    fn removed(&self) -> bool;
}

/// A versioned snapshot of a source value.
///
/// The [`revision`](Snapshot::revision) increments each time the value is
/// replaced via [`bump`](Snapshot::bump).
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot<V> {
    /// The captured value.
    pub value: V,
    /// Monotonically increasing revision counter.
    pub revision: u64,
}

impl<V> Snapshot<V> {
    /// Create a new snapshot with the given value and revision.
    pub fn new(value: V, revision: u64) -> Self {
        Self { value, revision }
    }

    /// Replace the value and increment the revision.
    pub fn bump(&mut self, new_value: V) {
        self.value = new_value;
        self.revision += 1;
    }
}

impl<V: PartialEq> Snapshot<V> {
    /// True when the snapshot's value equals `other`.
    pub fn is_unchanged_from(&self, other: &V) -> bool {
        self.value == *other
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Snapshot<V> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Helper<V> {
            value: V,
            revision: u64,
        }
        let h: Helper<V> = Helper::deserialize(deserializer)?;
        Ok(Self {
            value: h.value,
            revision: h.revision,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSource {
        key: &'static str,
        value: u32,
    }

    impl Source for TestSource {
        type Value = u32;

        fn key(&self) -> &str {
            self.key
        }

        fn load(&self) -> anyhow::Result<Self::Value> {
            Ok(self.value)
        }

        fn baseline(&self) -> Self::Value {
            0
        }

        fn update(
            &self,
            _prev: &Self::Value,
        ) -> anyhow::Result<Option<Self::Value>> {
            Ok(Some(self.value))
        }

        fn removed(&self) -> bool {
            false
        }
    }

    #[test]
    fn source_trait_has_5_functions() {
        let src = TestSource {
            key: "test",
            value: 42,
        };
        // 1. key
        assert_eq!(src.key(), "test");
        // 2. load
        assert_eq!(src.load().unwrap(), 42);
        // 3. baseline
        assert_eq!(src.baseline(), 0);
        // 4. update
        let prev = 0u32;
        assert_eq!(src.update(&prev).unwrap(), Some(42));
        // 5. removed
        assert!(!src.removed());
    }

    #[test]
    fn snapshot_serializes_to_json() {
        let snap = Snapshot::new("hello".to_string(), 1);
        let json = serde_json::to_value(&snap).unwrap();
        let obj = json.as_object().unwrap();
        assert!(obj.contains_key("value"));
        assert!(obj.contains_key("revision"));
        assert_eq!(
            obj.get("value").unwrap(),
            &serde_json::Value::String("hello".to_string())
        );
        assert_eq!(obj.get("revision").unwrap().as_u64(), Some(1));
    }

    #[test]
    fn snapshot_bump_increments_revision() {
        let mut snap = Snapshot::new("a".to_string(), 1);
        snap.bump("b".to_string());
        assert_eq!(snap.revision, 2);
        assert_eq!(snap.value, "b");
    }
}
