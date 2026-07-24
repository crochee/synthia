//! Environment variable system-context source.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::system_context::source::Source;

/// Snapshot of environment variables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentValue {
    /// Map of environment variable name to value.
    pub vars: HashMap<String, String>,
}

/// Source tracking the process environment.
pub struct EnvironmentSource {
    baseline: EnvironmentValue,
}

impl EnvironmentSource {
    /// Capture the current environment as the baseline.
    pub fn new() -> Self {
        Self {
            baseline: EnvironmentValue {
                vars: std::env::vars().collect(),
            },
        }
    }
}

impl Default for EnvironmentSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for EnvironmentSource {
    type Value = EnvironmentValue;

    fn key(&self) -> &str {
        "environment"
    }

    fn load(&self) -> anyhow::Result<Self::Value> {
        Ok(EnvironmentValue {
            vars: std::env::vars().collect(),
        })
    }

    fn baseline(&self) -> Self::Value {
        self.baseline.clone()
    }

    fn update(
        &self,
        prev: &Self::Value,
    ) -> anyhow::Result<Option<Self::Value>> {
        let current = self.load()?;
        if current == *prev {
            Ok(None)
        } else {
            Ok(Some(current))
        }
    }

    fn removed(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_source_load_reads_env() {
        // SAFETY: unique test-only env var; no other test mutates this name.
        unsafe { std::env::set_var("SYNTHIA_TEST_4_4_8_LOAD", "hello") };
        let src = EnvironmentSource::new();
        let val = src.load().unwrap();
        assert_eq!(
            val.vars.get("SYNTHIA_TEST_4_4_8_LOAD"),
            Some(&"hello".to_string())
        );
    }

    #[test]
    fn environment_source_update_detects_diff() {
        // SAFETY: unique test-only env var; no other test mutates this name.
        unsafe { std::env::set_var("SYNTHIA_TEST_4_4_8_UPDATE", "v1") };
        let src = EnvironmentSource::new();
        let first = src.load().unwrap();
        // SAFETY: same var, mutating to a new value to exercise the diff path.
        unsafe { std::env::set_var("SYNTHIA_TEST_4_4_8_UPDATE", "v2") };
        let result = src.update(&first).unwrap();
        assert!(result.is_some());
        let new_val = result.unwrap();
        assert_eq!(
            new_val.vars.get("SYNTHIA_TEST_4_4_8_UPDATE"),
            Some(&"v2".to_string())
        );
    }

    #[test]
    fn environment_source_baseline_is_initial_snapshot() {
        // SAFETY: unique test-only env var; no other test mutates this name.
        unsafe { std::env::set_var("SYNTHIA_TEST_4_4_8_BASELINE", "initial") };
        let src = EnvironmentSource::new();
        // SAFETY: same var, mutating after construction to verify baseline
        // is frozen at `new()` time.
        unsafe { std::env::set_var("SYNTHIA_TEST_4_4_8_BASELINE", "changed") };
        let baseline = src.baseline();
        assert_eq!(
            baseline.vars.get("SYNTHIA_TEST_4_4_8_BASELINE"),
            Some(&"initial".to_string())
        );
    }
}
