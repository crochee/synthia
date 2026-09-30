//! [`Tokenizer`] — pluggable text → token vector.
//!
//! [`CjkTokenizer`] is the default. It lowercases Latin words,
//! emits one token per CJK ideograph / kana / Hangul syllable, and
//! emits a **CJK bigram** for every pair of consecutive CJK
//! characters (so "中文" produces `["中","文","中文"]`). The
//! bigram form raises recall on Chinese queries without pulling a
//! Chinese-segmentation dependency.

#[cfg(test)]
use std::collections::HashSet;

pub trait Tokenizer: Send + Sync + 'static {
    fn tokenize(&self, text: &str) -> Vec<String>;
}

pub struct CjkTokenizer;

impl Tokenizer for CjkTokenizer {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut buf = String::new();
        let mut prev_cjk: Option<char> = None;
        for ch in text.to_lowercase().chars() {
            if is_cjk(ch) {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
                tokens.push(ch.to_string());
                if let Some(prev) = prev_cjk {
                    let mut bg = String::with_capacity(8);
                    bg.push(prev);
                    bg.push(ch);
                    tokens.push(bg);
                }
                prev_cjk = Some(ch);
            } else if ch.is_alphanumeric() {
                buf.push(ch);
                prev_cjk = None;
            } else {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
                prev_cjk = None;
            }
        }
        if !buf.is_empty() {
            tokens.push(buf);
        }
        tokens
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF      // CJK ideographs
            | 0x3400..=0x4DBF    // CJK Ext A
            | 0x3040..=0x30FF    // Hiragana + Katakana
            | 0xAC00..=0xD7AF    // Hangul syllables
    )
}

/// Test-only helper: unique set of tokens produced by the tokenizer.
#[cfg(test)]
fn unique_tokens(tk: &dyn Tokenizer, text: &str) -> HashSet<String> {
    tk.tokenize(text).into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_emits_unigrams_and_bigrams() {
        let tk = CjkTokenizer;
        let owned = tk.tokenize("中文");
        let tokens: Vec<&str> = owned.iter().map(String::as_str).collect();
        assert!(tokens.contains(&"中"));
        assert!(tokens.contains(&"文"));
        assert!(tokens.contains(&"中文"));
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn cjk_lowercases_latin_words() {
        let tokens = CjkTokenizer.tokenize("Hello WORLD");
        assert_eq!(tokens, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn cjk_splits_on_punctuation() {
        let tokens = CjkTokenizer.tokenize("hello, world!");
        assert_eq!(tokens, vec!["hello".to_string(), "world".to_string()]);
    }

    #[test]
    fn cjk_drops_whitespace() {
        let tokens = CjkTokenizer.tokenize("  spaced   out  ");
        assert_eq!(tokens, vec!["spaced".to_string(), "out".to_string()]);
    }

    #[test]
    fn cjk_kana_is_recognised() {
        // "あいう" — three Hiragana, two bigrams.
        let tokens = CjkTokenizer.tokenize("あいう");
        assert!(tokens.iter().any(|t| t == "あ"));
        assert!(tokens.iter().any(|t| t == "あい"));
        assert!(tokens.iter().any(|t| t == "いう"));
    }

    #[test]
    fn unique_tokens_helper_dedupes() {
        let s = unique_tokens(&CjkTokenizer, "中 中文");
        assert!(s.contains("中"));
        assert!(s.contains("中文"));
    }
}
