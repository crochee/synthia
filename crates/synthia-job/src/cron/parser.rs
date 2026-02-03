//! Cron expression parser
//!
//! This module implements a cron expression parser with configurable options.

use std::{collections::HashMap, sync::LazyLock};

use super::spec_trigger::SpecTrigger;
use crate::{
    JobError,
    Trigger,
    trigger::{every, run_at, run_once},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseOption(u32);

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

impl std::ops::BitOr for ParseOption {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        ParseOption(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for ParseOption {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        ParseOption(self.0 & rhs.0)
    }
}

const STAR_BIT: u64 = 1 << 63;
const DEFAULTS: [&str; 6] = ["0", "0", "0", "*", "*", "*"];
const PLACES: [ParseOption; 6] = [
    ParseOption::SECOND,
    ParseOption::MINUTE,
    ParseOption::HOUR,
    ParseOption::DOM,
    ParseOption::MONTH,
    ParseOption::DOW,
];

struct Bounds {
    min: u32,
    max: u32,
    names: HashMap<&'static str, u32>,
}

impl Bounds {
    fn new(min: u32, max: u32) -> Self {
        Self {
            min,
            max,
            names: HashMap::new(),
        }
    }

    fn with_names(min: u32, max: u32, names: &[(&'static str, u32)]) -> Self {
        let mut map = HashMap::new();
        for (k, v) in names {
            map.insert(*k, *v);
        }
        Self {
            min,
            max,
            names: map,
        }
    }
}

static SECONDS_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(0, 59));
static MINUTES_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(0, 59));
static HOURS_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(0, 23));
static DOM_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| Bounds::new(1, 31));
static MONTHS_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| {
    Bounds::with_names(
        1,
        12,
        &[
            ("jan", 1),
            ("feb", 2),
            ("mar", 3),
            ("apr", 4),
            ("may", 5),
            ("jun", 6),
            ("jul", 7),
            ("aug", 8),
            ("sep", 9),
            ("oct", 10),
            ("nov", 11),
            ("dec", 12),
        ],
    )
});
static DOW_BOUNDS: LazyLock<Bounds> = LazyLock::new(|| {
    Bounds::with_names(
        0,
        6,
        &[
            ("sun", 0),
            ("mon", 1),
            ("tue", 2),
            ("wed", 3),
            ("thu", 4),
            ("fri", 5),
            ("sat", 6),
        ],
    )
});

fn seconds_bounds() -> &'static Bounds {
    &SECONDS_BOUNDS
}
fn minutes_bounds() -> &'static Bounds {
    &MINUTES_BOUNDS
}
fn hours_bounds() -> &'static Bounds {
    &HOURS_BOUNDS
}
fn dom_bounds() -> &'static Bounds {
    &DOM_BOUNDS
}
fn months_bounds() -> &'static Bounds {
    &MONTHS_BOUNDS
}
fn dow_bounds() -> &'static Bounds {
    &DOW_BOUNDS
}

#[derive(Debug, Clone, Copy)]
pub struct Parser {
    options: ParseOption,
}

impl Parser {
    pub fn new(options: ParseOption) -> Self {
        let mut optionals = 0;
        if (options & ParseOption::DOW_OPTIONAL).0 > 0 {
            optionals += 1;
        }
        if (options & ParseOption::SECOND_OPTIONAL).0 > 0 {
            optionals += 1;
        }
        if optionals > 1 {
            panic!("multiple optionals may not be configured");
        }
        Self { options }
    }

    pub fn parse(&self, spec: &str) -> Result<Box<dyn Trigger>, JobError> {
        if spec.is_empty() {
            return Err(JobError::ParseError("empty spec string".to_string()));
        }

        let (spec, location) = self.extract_timezone(spec)?;

        if spec.starts_with('@') {
            if (self.options & ParseOption::DESCRIPTOR).0 == 0 {
                return Err(JobError::ParseError(format!(
                    "parser does not accept descriptors: {spec}"
                )));
            }
            return self.parse_descriptor(spec, location);
        }

        let fields: Vec<&str> = spec.split_whitespace().collect();
        let fields = normalize_fields(&fields, self.options)?;

        let second = get_field(fields[0], seconds_bounds())?;
        let minute = get_field(fields[1], minutes_bounds())?;
        let hour = get_field(fields[2], hours_bounds())?;
        let dayofmonth = get_field(fields[3], dom_bounds())?;
        let month = get_field(fields[4], months_bounds())?;
        let dayofweek = get_field(fields[5], dow_bounds())?;

        Ok(Box::new(SpecTrigger::new(super::spec_trigger::CronSpec {
            second,
            minute,
            hour,
            dom: dayofmonth,
            month,
            dow: dayofweek,
            location,
            spec: spec.to_string(),
        })))
    }

    fn extract_timezone(
        self,
        spec: &str,
    ) -> Result<(&str, Option<chrono_tz::Tz>), JobError> {
        if spec.starts_with("TZ=") || spec.starts_with("CRON_TZ=") {
            let eq_idx = spec.find('=').ok_or_else(|| {
                JobError::ParseError(
                    "invalid timezone specification".to_string(),
                )
            })?;
            let space_idx = spec.find(' ').ok_or_else(|| {
                JobError::ParseError(
                    "timezone specification must be followed by space"
                        .to_string(),
                )
            })?;

            let tz_str = &spec[eq_idx + 1..space_idx];
            let tz: chrono_tz::Tz = tz_str.parse().map_err(|e| {
                JobError::ParseError(format!(
                    "provided bad location {tz_str}: {e}"
                ))
            })?;

            let remaining = spec[space_idx..].trim_start();
            Ok((remaining, Some(tz)))
        } else {
            Ok((spec, None))
        }
    }

    fn parse_descriptor(
        self,
        descriptor: &str,
        location: Option<chrono_tz::Tz>,
    ) -> Result<Box<dyn Trigger>, JobError> {
        let seconds = seconds_bounds();
        let minutes = minutes_bounds();
        let hours = hours_bounds();
        let dom = dom_bounds();
        let months = months_bounds();
        let dow = dow_bounds();

        let make_trigger = |second, minute, hour, dom, month, dow| {
            Box::new(SpecTrigger::new(super::spec_trigger::CronSpec {
                second,
                minute,
                hour,
                dom,
                month,
                dow,
                location,
                spec: descriptor.to_string(),
            }))
        };

        match descriptor {
            "@yearly" | "@annually" => Ok(make_trigger(
                1u64 << seconds.min,
                1u64 << minutes.min,
                1u64 << hours.min,
                1u64 << (dom.min - 1),
                1u64 << (months.min - 1),
                all_bits(dow),
            )),
            "@monthly" => Ok(make_trigger(
                1u64 << seconds.min,
                1u64 << minutes.min,
                1u64 << hours.min,
                1u64 << (dom.min - 1),
                all_bits(months),
                all_bits(dow),
            )),
            "@weekly" => Ok(make_trigger(
                1u64 << seconds.min,
                1u64 << minutes.min,
                1u64 << hours.min,
                all_bits(dom),
                all_bits(months),
                1u64 << dow.min,
            )),
            "@daily" | "@midnight" => Ok(make_trigger(
                1u64 << seconds.min,
                1u64 << minutes.min,
                1u64 << hours.min,
                all_bits(dom),
                all_bits(months),
                all_bits(dow),
            )),
            "@hourly" => Ok(make_trigger(
                1u64 << seconds.min,
                1u64 << minutes.min,
                all_bits(hours),
                all_bits(dom),
                all_bits(months),
                all_bits(dow),
            )),
            _ => {
                if let Some(duration_str) = descriptor.strip_prefix("@every ") {
                    let duration = parse_duration(duration_str)?;
                    return Ok(Box::new(every(duration)));
                } else if let Some(at_str) = descriptor.strip_prefix("@at ") {
                    let at_ns: i64 = at_str.parse().map_err(|e| {
                        JobError::ParseError(format!(
                            "failed to parse UnixNano {descriptor}: {e}"
                        ))
                    })?;
                    return Ok(Box::new(run_at(at_ns)));
                } else if let Some(delay_str) =
                    descriptor.strip_prefix("@delay ")
                {
                    let duration = parse_duration(delay_str)?;
                    return Ok(Box::new(run_once(duration)));
                }

                Err(JobError::ParseError(format!(
                    "unrecognized descriptor: {descriptor}"
                )))
            }
        }
    }
}

fn standard_parser() -> Parser {
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

pub fn parse_standard(spec: &str) -> Result<Box<dyn Trigger>, JobError> {
    standard_parser().parse(spec)
}

fn normalize_fields<'a>(
    fields: &[&'a str],
    options: ParseOption,
) -> Result<Vec<&'a str>, JobError> {
    let mut options = options;
    let mut optionals = 0;

    if (options & ParseOption::SECOND_OPTIONAL).0 > 0 {
        options = options | ParseOption::SECOND;
        optionals += 1;
    }
    if (options & ParseOption::DOW_OPTIONAL).0 > 0 {
        options = options | ParseOption::DOW;
        optionals += 1;
    }
    if optionals > 1 {
        return Err(JobError::ParseError(
            "multiple optionals may not be configured".to_string(),
        ));
    }

    let mut max = 0;
    for place in &PLACES {
        if (options & *place).0 > 0 {
            max += 1;
        }
    }
    let min = max - optionals;

    let count = fields.len();
    if count < min || count > max {
        return Err(JobError::ParseError(format!(
            "expected {min} to {max} fields, found {count}: {fields:?}"
        )));
    }

    let mut fields: Vec<&str> = fields.to_vec();
    if min < max && fields.len() == min {
        if (options & ParseOption::DOW_OPTIONAL).0 > 0 {
            fields.push(DEFAULTS[5]);
        } else if (options & ParseOption::SECOND_OPTIONAL).0 > 0 {
            fields.insert(0, DEFAULTS[0]);
        } else {
            return Err(JobError::ParseError(
                "unknown optional field".to_string(),
            ));
        }
    }

    let mut expanded_fields = DEFAULTS.to_vec();
    let mut n = 0;
    for (i, place) in PLACES.iter().enumerate() {
        if (options & *place).0 > 0 {
            expanded_fields[i] = fields[n];
            n += 1;
        }
    }

    Ok(expanded_fields)
}

fn get_field(field: &str, bounds: &Bounds) -> Result<u64, JobError> {
    let mut bits = 0u64;
    for expr in field.split(',') {
        let bit = get_range(expr, bounds)?;
        bits |= bit;
    }
    Ok(bits)
}

fn get_range(expr: &str, bounds: &Bounds) -> Result<u64, JobError> {
    let range_and_step: Vec<&str> = expr.split('/').collect();
    let low_and_high: Vec<&str> = range_and_step[0].split('-').collect();
    let _single_digit = low_and_high.len() == 1;

    let (start, end, mut extra) =
        if low_and_high[0] == "*" || low_and_high[0] == "?" {
            (bounds.min, bounds.max, STAR_BIT)
        } else {
            let start = parse_int_or_name(low_and_high[0], &bounds.names)?;
            let end = match low_and_high.len() {
                1 => start,
                2 => parse_int_or_name(low_and_high[1], &bounds.names)?,
                _ => {
                    return Err(JobError::ParseError(format!(
                        "too many hyphens: {expr}"
                    )));
                }
            };
            (start, end, 0)
        };

    let step = match range_and_step.len() {
        1 => 1,
        2 => {
            let step: u32 = must_parse_int(range_and_step[1])?;
            if step > 1 {
                extra = 0;
            }
            step
        }
        _ => {
            return Err(JobError::ParseError(format!(
                "too many slashes: {expr}"
            )));
        }
    };

    if start < bounds.min {
        return Err(JobError::ParseError(format!(
            "beginning of range ({}) below minimum ({}): {}",
            start, bounds.min, expr
        )));
    }
    if end > bounds.max {
        return Err(JobError::ParseError(format!(
            "end of range ({}) above maximum ({}): {}",
            end, bounds.max, expr
        )));
    }
    if start > end {
        return Err(JobError::ParseError(format!(
            "beginning of range ({start}) beyond end of range ({end}): {expr}"
        )));
    }
    if step == 0 {
        return Err(JobError::ParseError(format!(
            "step of range should be a positive number: {expr}"
        )));
    }

    Ok(get_bits(start, end, step) | extra)
}

fn parse_int_or_name(
    expr: &str,
    names: &HashMap<&'static str, u32>,
) -> Result<u32, JobError> {
    let expr_lower = expr.to_lowercase();
    if let Some(&named_int) =
        names.get(expr_lower.as_str()).or_else(|| names.get(expr))
    {
        return Ok(named_int);
    }
    must_parse_int(expr)
}

fn must_parse_int(expr: &str) -> Result<u32, JobError> {
    let num: i32 = expr.parse().map_err(|_| {
        JobError::ParseError(format!("failed to parse int from {expr}"))
    })?;

    if num < 0 {
        return Err(JobError::ParseError(format!(
            "negative number ({num}) not allowed: {expr}"
        )));
    }

    Ok(num as u32)
}

fn get_bits(min: u32, max: u32, step: u32) -> u64 {
    if step == 1 {
        if max >= 63 {
            let mut bits = 0u64;
            for i in min..=max {
                if i < 64 {
                    bits |= 1u64 << i;
                }
            }
            bits
        } else {
            ((1u64 << (max + 1)) - 1) & !((1u64 << min) - 1)
        }
    } else {
        let mut bits = 0u64;
        let mut i = min;
        while i <= max {
            if i < 64 {
                bits |= 1u64 << i;
            }
            i += step;
        }
        bits
    }
}

fn all_bits(bounds: &Bounds) -> u64 {
    get_bits(bounds.min, bounds.max, 1) | STAR_BIT
}

fn parse_duration(s: &str) -> Result<std::time::Duration, JobError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(JobError::ParseError("empty duration".to_string()));
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
                    JobError::ParseError(format!("invalid duration: {s}"))
                })?;
                total_secs += num;
                current_num.clear();
            }
            'm' | 'M' => {
                let num: i64 = current_num.parse().map_err(|_| {
                    JobError::ParseError(format!("invalid duration: {s}"))
                })?;
                total_secs += num * 60;
                current_num.clear();
            }
            'h' | 'H' => {
                let num: i64 = current_num.parse().map_err(|_| {
                    JobError::ParseError(format!("invalid duration: {s}"))
                })?;
                total_secs += num * 3600;
                current_num.clear();
            }
            'd' | 'D' => {
                let num: i64 = current_num.parse().map_err(|_| {
                    JobError::ParseError(format!("invalid duration: {s}"))
                })?;
                total_secs += num * 86400;
                current_num.clear();
            }
            ' ' | '\t' => {}
            _ => {
                return Err(JobError::ParseError(format!(
                    "invalid character in duration: {c}"
                )));
            }
        }
    }

    if !current_num.is_empty() {
        return Err(JobError::ParseError(format!(
            "trailing number in duration: {s}"
        )));
    }

    if total_secs <= 0 {
        return Err(JobError::ParseError(
            "duration must be positive".to_string(),
        ));
    }

    Ok(std::time::Duration::from_secs(total_secs as u64))
}
