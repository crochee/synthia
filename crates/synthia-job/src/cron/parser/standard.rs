//! The canonical "second-optional + descriptor-enabled" parser
//! preset ([`standard_parser`]) + its [`parse_standard`]
//! shortcut.

use super::{option::ParseOption, parser::Parser};
use crate::{Result, Trigger};

pub(super) fn standard_parser() -> Parser {
    Parser::new(
        ParseOption::SECOND_OPTIONAL
            | ParseOption::MINUTE
            | ParseOption::HOUR
            | ParseOption::DOM
            | ParseOption::MONTH
            | ParseOption::DOW
            | ParseOption::DESCRIPTOR,
    )
}

pub fn parse_standard(spec: &str) -> Result<Box<dyn Trigger>> {
    standard_parser().parse(spec)
}
