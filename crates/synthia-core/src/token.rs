pub fn estimate_token_count(text: &str) -> usize {
    let mut ascii_count: usize = 0;
    let mut cjk_count: usize = 0;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk_count += 1;
        } else {
            ascii_count += ch.len_utf8();
        }
    }
    let text_tokens =
        (ascii_count as f64 / 4.0 + cjk_count as f64 / 1.5) as usize;
    let overhead = (text_tokens as f64 * 0.05) as usize;
    text_tokens + overhead
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}' |
        '\u{3400}'..='\u{4DBF}' |
        '\u{20000}'..='\u{2A6DF}' |
        '\u{2A700}'..='\u{2B73F}' |
        '\u{2B740}'..='\u{2B81F}' |
        '\u{2B820}'..='\u{2CEAF}' |
        '\u{F900}'..='\u{FAFF}' |
        '\u{2F800}'..='\u{2FA1F}' |
        '\u{3000}'..='\u{303F}' |
        '\u{3040}'..='\u{309F}' |
        '\u{30A0}'..='\u{30FF}' |
        '\u{AC00}'..='\u{D7AF}'
    )
}
