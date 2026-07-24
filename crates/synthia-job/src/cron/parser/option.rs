//! [`ParseOption`] bitflags + the package-level constants used
//! across all parser stages.
//!
//! `ParseOption` mirrors robfig/cron's bitfield layout
//! (1 / 2 / 4 / 8 / 16 / 32 / 64 / 128 / 256) so callers can
//! construct presets with the same `|` / `&` operators the Go
//! version exposes.

use std::ops::{BitAnd, BitOr};

/// Single bit per parser stage. `DESCRIPTOR` (256) gates
/// `@yearly` / `@every` / `@at` / `@delay` parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOption(pub(super) u32);

impl ParseOption {
    pub const DESCRIPTOR: ParseOption = ParseOption(256);
    pub const DOM: ParseOption = ParseOption(16);
    pub const DOW: ParseOption = ParseOption(64);
    pub const DOW_OPTIONAL: ParseOption = ParseOption(128);
    pub const HOUR: ParseOption = ParseOption(8);
    pub const MINUTE: ParseOption = ParseOption(4);
    pub const MONTH: ParseOption = ParseOption(32);
    pub const SECOND: ParseOption = ParseOption(1);
    pub const SECOND_OPTIONAL: ParseOption = ParseOption(2);
}

impl BitOr for ParseOption {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        ParseOption(self.0 | rhs.0)
    }
}

impl BitAnd for ParseOption {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        ParseOption(self.0 & rhs.0)
    }
}

/// `1 << 63`. Marks a slot as `*` (or `?`) so the SpecTrigger
/// can distinguish "explicitly 0 bits" from "wildcard".
pub(super) const STAR_BIT: u64 = 1 << 63;

/// Default placeholders for `normalize_fields` (second, minute,
/// hour, dom, month, dow).
pub(super) const DEFAULTS: [&str; 6] = ["0", "0", "0", "*", "*", "*"];

/// Order in which the 6 standard cron places are interpreted.
pub(super) const PLACES: [ParseOption; 6] = [
    ParseOption::SECOND,
    ParseOption::MINUTE,
    ParseOption::HOUR,
    ParseOption::DOM,
    ParseOption::MONTH,
    ParseOption::DOW,
];
