use anyhow::Result;
use synthia_provider::types::Message;
use tiktoken_rs::CoreBPE;

use crate::config::AgentConfig;

const FALLBACK_MODEL: &str = "gpt-4o";

/// Counts tokens in a message list using tiktoken.
///
/// Uses the model from config to select encoding. Falls back to cl100k_base
/// for unknown models.
pub fn count_tokens(messages: &[Message], config: &AgentConfig) -> usize {
    let Ok(encoding) = encoder_for_model(&config.model) else {
        return 0;
    };

    messages
        .iter()
        .map(|m| {
            let text = m.content.extract_text().unwrap_or_default();
            encoding.encode_ordinary(&text).len()
        })
        .sum()
}

fn encoder_for_model(model: &str) -> Result<CoreBPE> {
    match tiktoken_rs::get_bpe_from_model(model) {
        Ok(bpe) => Ok(bpe),
        Err(_) => {
            tracing::warn!(
                model = %model,
                "Unknown model for tiktoken, falling back to {FALLBACK_MODEL}"
            );
            tiktoken_rs::get_bpe_from_model(FALLBACK_MODEL)
        }
    }
}

#[cfg(test)]
mod tests {
    use synthia_provider::types::Message;

    use super::*;

    #[test]
    fn test_tiktoken_counts_known_string() {
        // "hello" is a single token in cl100k_base
        let msg = Message::user("hello");
        let config = AgentConfig {
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        assert_eq!(count_tokens(&[msg], &config), 1);
    }

    #[test]
    fn test_tiktoken_counts_multiple_messages() {
        let msgs = vec![Message::user("hello"), Message::assistant("world")];
        let config = AgentConfig {
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        // "hello" = 1 token, "world" = 1 token
        assert_eq!(count_tokens(&msgs, &config), 2);
    }

    #[test]
    fn test_tiktoken_falls_back_for_unknown_model() {
        let msg = Message::user("hello");
        let config = AgentConfig {
            model: "unknown-model-xyz".to_string(),
            ..Default::default()
        };
        // Should not panic, should still produce a count via fallback
        assert_eq!(count_tokens(&[msg], &config), 1);
    }

    #[test]
    fn test_tiktoken_empty_messages() {
        let config = AgentConfig {
            model: "gpt-4o".to_string(),
            ..Default::default()
        };
        assert_eq!(count_tokens(&[], &config), 0);
    }
}
