//! Pipeline entry points — `prepare` and `finalize`.
//!
//! [`ContextAssembler::prepare`] reads messages through a
//! [`crate::traits::MessageReader`], applies the
//! [`crate::protector::ProtectionZone`] guard, and emits a
//! [`synthia_provider::CompletionRequest`].
//!
//! [`ContextAssembler::finalize`] is the post-pipeline step:
//! takes a prepared request + optional override system
//! prompt, prepends the system message, and returns the
//! final request.
//!
//! The two methods are kept in one module because the
//! "prepare then finalize" protocol is the primary intended
//! call path and reading them side-by-side helps the
//! reviewer verify the protection-zone semantics.

use std::sync::Arc;

use synthia_provider::{CachePolicy, CompletionRequest, Message, ToolChoice};

use super::types::ContextAssembler;
use crate::{
    traits::{MessageReader, estimate_message_tokens},
    types::ContextError,
};

impl ContextAssembler {
    /// Prepare a CompletionRequest using the MessageReader and applying the ProtectionZone.
    pub async fn prepare(
        &self,
        reader: &dyn MessageReader,
    ) -> Result<CompletionRequest, ContextError> {
        let messages = reader.read_messages().await?;
        let total_tokens = self.estimate_total_tokens(&messages);

        let protected_messages =
            self.protection_zone.get_recent_messages_owned(
                &messages,
                self.protection_zone.min_rounds,
            );

        let protected_messages = if total_tokens > self.max_tokens {
            // Pre-compute per-message token counts once (O(n) counter
            // invocations), then determine how many oldest messages to drop
            // by walking the suffix sum, and remove them with a single
            // `Vec::drain` (O(n) shift). The previous `trimmed.remove(0)`
            // loop was O(n²): each removal shifted every remaining element
            // and `estimate_total_tokens` recomputed the full sum.
            let msg_tokens: Vec<usize> = protected_messages
                .iter()
                .map(|m| match &self.token_counter {
                    Some(counter) => counter.count_message(m),
                    None => estimate_message_tokens(m),
                })
                .collect();
            let mut remaining: usize = msg_tokens.iter().sum();
            let mut remove_count: usize = 0;
            while remove_count + 1 < protected_messages.len()
                && remaining > self.max_tokens
            {
                remaining -= msg_tokens[remove_count];
                remove_count += 1;
            }
            let mut trimmed = protected_messages;
            trimmed.drain(0..remove_count);
            trimmed
        } else {
            protected_messages
        };

        let model = "default".to_string();

        Ok(CompletionRequest {
            model,
            messages: Arc::new(protected_messages),
            tools: Arc::new(vec![]),
            tool_choice: ToolChoice::Auto,
            temperature: None,
            max_tokens: Some(self.max_tokens),
            stop_sequences: vec![],
            extra_body: None,
            cache_policy: Some(CachePolicy::default()),
        })
    }

    pub fn finalize(
        self,
        request: CompletionRequest,
        system_prompt: Option<String>,
    ) -> CompletionRequest {
        let mut req = request;
        let final_prompt =
            system_prompt.unwrap_or_else(|| self.build_system_prompt());
        if !final_prompt.is_empty() {
            let sys_msg = Message::system(final_prompt);
            Arc::make_mut(&mut req.messages).insert(0, sys_msg);
        }
        req
    }
}
