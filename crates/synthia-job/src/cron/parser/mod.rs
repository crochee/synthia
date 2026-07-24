//! Cron expression parser.
//!
//! This module implements a cron expression parser with
//! configurable [`option::ParseOption`] bitflags. It mirrors the
//! behavior of the Go `robfig/cron` parser (5- or 6-field spec,
//! `*`/`?`/`-`/`,`/`/`/` operators, descriptor shortcuts
//! `@yearly` / `@every` / `@at` / `@delay`, optional seconds,
//! optional DOW).
//!
//! # Module Layout
//!
//! - [`option`]: [`option::ParseOption`] bitflags (9 constants +
//!   `BitOr` / `BitAnd` impls) + the package-level `STAR_BIT` /
//!   `DEFAULTS` / `PLACES` constants.
//! - [`bounds`]: [`bounds::Bounds`] struct + the 6 lazy statics
//!   `SECONDS_BOUNDS` / `MINUTES_BOUNDS` / `HOURS_BOUNDS` /
//!   `DOM_BOUNDS` / `MONTHS_BOUNDS` / `DOW_BOUNDS` + 6 accessor
//!   functions.
//! - [`field`]: Pure parsing of a single field
//!   ([`field::get_field`] → [`field::get_range`] →
//!   [`field::parse_int_or_name`] /
//!   [`field::must_parse_int`]) + the bit-unpacker
//!   [`field::get_bits`] / [`field::all_bits`] +
//!   [`field::normalize_fields`] which dispatches a
//!   pre-`PLACES` list to the canonical 6-slot layout.
//! - [`parser`]: The main [`parser::Parser`] struct + its 3
//!   methods ([`parser::Parser::new`],
//!   [`parser::Parser::parse`],
//!   [`parser::Parser::extract_timezone`],
//!   [`parser::Parser::parse_descriptor`]).
//! - [`duration`]: The `@every` / `@delay` duration string parser
//!   ([`duration::parse_duration`]).
//! - [`standard`]: [`standard::standard_parser`] +
//!   [`standard::parse_standard`] (the canonical
//!   "second-optional + descriptor-enabled" preset).

mod bounds;
mod duration;
mod field;
mod option;
#[allow(clippy::module_inception)]
mod parser;
mod standard;

pub use option::ParseOption;
pub use parser::Parser;
pub use standard::parse_standard;
