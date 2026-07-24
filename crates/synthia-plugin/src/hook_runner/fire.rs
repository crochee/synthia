//! The [`fire`] method — the public event dispatch.
//!
//! Iterates all loaded hooks in priority order, applies the regex
//! matcher (target or extras), calls
//! [`super::execute::execute_hook`], records the result, and
//! applies short-circuit logic via `HookResult::Stop` (always) or
//! `HookResult::Failed` (only under `FailMode::Closed`).

use super::{core::HookRunner, types::SingleHookResult};
use crate::types::{HookEvent, HookResult};

impl HookRunner {
    /// Fire a hook event with metadata
    pub async fn fire(
        &self,
        event: HookEvent,
        metadata: super::types::HookMetadata,
    ) -> Result<Vec<SingleHookResult>, super::types::HookRunnerError> {
        let mut results = Vec::new();

        for (config, matcher) in self.configs.iter().zip(self.matchers.iter()) {
            if config.event != event {
                continue;
            }

            // Check regex matcher if present
            if let Some(re) = matcher {
                let target_str = metadata.target_str();
                if !target_str.is_empty() && !re.is_match(&target_str) {
                    continue;
                }
                // Also check extras values
                let matches_extra =
                    metadata.extras.values().any(|v| re.is_match(v));
                if target_str.is_empty() && !matches_extra {
                    continue;
                }
            }

            // Execute the hook
            let start = std::time::Instant::now();
            let result =
                super::execute::execute_hook(self, config, &metadata).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            results.push(SingleHookResult {
                config: config.clone(),
                result,
                duration_ms,
            });

            // Check for short-circuit conditions
            if let Some(last_result) = results.last() {
                match &last_result.result {
                    Ok(HookResult::Stop) | Ok(HookResult::Failed)
                        if self.config.fail_mode.is_closed() =>
                    {
                        break;
                    }
                    Ok(HookResult::Stop) => {
                        break;
                    }
                    _ => {}
                }
            }
        }

        Ok(results)
    }
}
