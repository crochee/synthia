//! The `@every` / `@delay` duration string parser.
//!
//! Accepts shorthand like `1h30m`, `45s`, `2d12h`. Each
//! unit-suffix updates a running total-seconds counter; the
//! final value must be positive and well-formed (no trailing
//! number without a suffix).

use std::time::Duration;

use crate::{Error, Result};

pub(super) fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::Parse("empty duration".to_string()));
    }

    let mut total_secs = 0i64;
    let mut current_num = String::new();

    for c in s.chars() {
        match c {
            '0'..='9' => {
                current_num.push(c);
            }
            's' | 'S' => {
                let num: i64 = current_num.parse().map_err(|_| {
                    Error::Parse(format!("invalid duration: {s}"))
                })?;
                total_secs += num;
                current_num.clear();
            }
            'm' | 'M' => {
                let num: i64 = current_num.parse().map_err(|_| {
                    Error::Parse(format!("invalid duration: {s}"))
                })?;
                total_secs += num * 60;
                current_num.clear();
            }
            'h' | 'H' => {
                let num: i64 = current_num.parse().map_err(|_| {
                    Error::Parse(format!("invalid duration: {s}"))
                })?;
                total_secs += num * 3600;
                current_num.clear();
            }
            'd' | 'D' => {
                let num: i64 = current_num.parse().map_err(|_| {
                    Error::Parse(format!("invalid duration: {s}"))
                })?;
                total_secs += num * 86400;
                current_num.clear();
            }
            ' ' | '\t' => {}
            _ => {
                return Err(Error::Parse(format!(
                    "invalid character in duration: {c}"
                )));
            }
        }
    }

    if !current_num.is_empty() {
        return Err(Error::Parse(format!("trailing number in duration: {s}")));
    }

    if total_secs <= 0 {
        return Err(Error::Parse("duration must be positive".to_string()));
    }

    Ok(Duration::from_secs(total_secs as u64))
}
