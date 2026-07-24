//! Final-assembly step: applies
//! [`EffectivePromptConfig`](super::super::config::EffectivePromptConfig)
//! overrides on top of the [`ResolvedPrompt`] produced by
//! [`super::resolve::resolve`].
//!
//! Override precedence (top wins):
//!
//! 1. `override_prompt` — replaces the entire prompt.
//! 2. `coordinator_prompt` (when `use_coordinator_mode`) — replaces the
//!    body, then `append_prompt` is appended.
//! 3. Resolved prompt + optional `prompt` prefix + `dynamic_content`.
//! 4. `agent_prompt`, `custom_prompt`, `append_prompt` (in that order).

use super::{
    super::{config::EffectivePromptConfig, state::PromptState},
    core::PromptBuilder,
};
use crate::prompt::{PromptContext, SYSTEM_PROMPT_DYNAMIC_BOUNDARY};

impl PromptBuilder {
    /// Resolve the prompt, then apply the
    /// [`EffectivePromptConfig`](super::super::config::EffectivePromptConfig)
    /// overrides in the precedence order documented at the module
    /// level. Returns the final `String` to send to the LLM.
    pub fn build_effective_prompt(
        &self,
        ctx: &PromptContext<'_>,
        state: &mut PromptState,
        effective_config: EffectivePromptConfig,
    ) -> anyhow::Result<String> {
        if let Some(override_prompt) = effective_config.override_prompt {
            return Ok(override_prompt);
        }

        if effective_config.use_coordinator_mode
            && let Some(ref coordinator_prompt) =
                effective_config.coordinator_prompt
        {
            let mut result = coordinator_prompt.clone();
            if let Some(ref append) = effective_config.append_prompt {
                result.push_str("\n\n");
                result.push_str(append);
            }
            return Ok(result);
        }

        let resolved = self.resolve(ctx, state)?;

        let mut final_prompt = if let Some(ref prompt) = effective_config.prompt
        {
            if resolved.dynamic_content.is_empty() {
                prompt.clone()
            } else {
                format!(
                    "{}\n\n{}\n\n{}",
                    prompt,
                    SYSTEM_PROMPT_DYNAMIC_BOUNDARY,
                    resolved.dynamic_content
                )
            }
        } else {
            resolved.full_prompt()
        };

        if let Some(ref agent_prompt) = effective_config.agent_prompt {
            final_prompt.push_str("\n\n");
            final_prompt.push_str(agent_prompt);
        }

        if let Some(ref custom_prompt) = effective_config.custom_prompt {
            final_prompt.push_str("\n\n");
            final_prompt.push_str(custom_prompt);
        }

        if let Some(ref append) = effective_config.append_prompt {
            final_prompt.push_str("\n\n");
            final_prompt.push_str(append);
        }

        Ok(final_prompt)
    }
}
