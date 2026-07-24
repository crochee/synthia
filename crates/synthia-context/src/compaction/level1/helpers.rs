use synthia_provider::Message;

use crate::traits::estimate_message_tokens;

pub(crate) fn estimate_tokens(messages: &[Message]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

pub(crate) fn estimate_token_count(s: &str) -> usize {
    s.chars().count().div_ceil(4)
}

pub(crate) fn first_n_lines(s: &str, n: usize) -> String {
    s.lines().take(n).collect::<Vec<_>>().join("\n")
}
