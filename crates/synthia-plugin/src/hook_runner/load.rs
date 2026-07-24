//! The hooks.json loading pipeline:
//!
//! - [`load_from_path`]: locate the `hooks.json` next to a plugin
//!   directory and load it.
//! - [`load_from_file`]: read a specific file and load it.
//! - [`load_from_json`]: parse a JSON string. Tries the
//!   `{ "hooks": [...] }` envelope first, then falls back to a
//!   bare array.
//! - [`parse_raw_hooks`]: convert the raw deserialized hooks
//!   into the runner's `configs` + `matchers` vectors, sorted by
//!   priority (lower = first).

use std::{fs, path::Path};

use regex::Regex;

use super::{
    core::HookRunner,
    types::{HookRunnerError, RawHook, RawHooks},
};
use crate::types::HookSpec;

impl HookRunner {
    /// Load hooks from hooks.json at the given path
    pub fn load_from_path(
        &mut self,
        path: &Path,
    ) -> Result<(), HookRunnerError> {
        let hooks_path = path.join("hooks.json");
        self.load_from_file(&hooks_path)
    }

    /// Load hooks from a specific file path
    pub fn load_from_file(
        &mut self,
        path: &Path,
    ) -> Result<(), HookRunnerError> {
        let content = fs::read_to_string(path)?;
        self.load_from_json(&content)?;
        self.base_dir = path.parent().unwrap_or(path).to_path_buf();
        Ok(())
    }

    /// Load hooks from JSON string content
    pub fn load_from_json(
        &mut self,
        json: &str,
    ) -> Result<(), HookRunnerError> {
        // Try parsing as { "hooks": [...] }
        let with_hooks: Result<RawHooks, _> = serde_json::from_str(json);
        if let Ok(raw) = with_hooks {
            self.parse_raw_hooks(raw.hooks)?;
            return Ok(());
        }

        // Fall back to direct array [...]
        let raw_hooks: Vec<RawHook> = serde_json::from_str(json)?;
        self.parse_raw_hooks(raw_hooks)?;
        Ok(())
    }

    pub(super) fn parse_raw_hooks(
        &mut self,
        raw_hooks: Vec<RawHook>,
    ) -> Result<(), HookRunnerError> {
        self.configs.clear();
        self.matchers.clear();

        for raw in raw_hooks {
            let priority = raw.priority.unwrap_or(0);

            let matcher = if let Some(ref pattern) = raw.matcher {
                let re = Regex::new(pattern).map_err(|e| {
                    HookRunnerError::InvalidRegex(pattern.clone(), e)
                })?;
                Some(re)
            } else {
                None
            };

            let config = HookSpec {
                event: raw.event,
                matcher: raw.matcher,
                handler: raw.handler,
                priority,
            };

            self.configs.push(config);
            self.matchers.push(matcher);
        }

        // Sort by priority (lower = first)
        let mut sorted: Vec<_> = (0..self.configs.len())
            .map(|i| (self.configs[i].priority, i))
            .collect();
        sorted.sort_by_key(|(pri, _)| *pri);

        // Reorder configs and matchers
        let sorted_configs: Vec<_> = sorted
            .iter()
            .map(|(_, i)| self.configs[*i].clone())
            .collect();
        let sorted_matchers: Vec<_> = sorted
            .iter()
            .map(|(_, i)| self.matchers[*i].clone())
            .collect();

        self.configs = sorted_configs;
        self.matchers = sorted_matchers;

        Ok(())
    }
}
