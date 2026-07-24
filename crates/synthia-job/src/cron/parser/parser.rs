//! The main [`Parser`] struct + its 3 methods.
//!
//! [`Parser::parse`] is the public entry point: it strips an
//! optional `TZ=...` / `CRON_TZ=...` prefix via
//! [`Parser::extract_timezone`], then either delegates to
//! [`Parser::parse_descriptor`] (for `@yearly` / `@every` /
//! `@at` / `@delay`) or to the 6-field standard parser.

use super::{
    super::spec_trigger::{CronSpec, SpecTrigger},
    bounds::{
        dom_bounds,
        dow_bounds,
        hours_bounds,
        minutes_bounds,
        months_bounds,
        seconds_bounds,
    },
    duration::parse_duration,
    field::{all_bits, get_field, normalize_fields},
    option::ParseOption,
};
use crate::{
    Error,
    Result,
    Trigger,
    trigger::{every, run_at, run_once},
};

/// Cron expression parser.
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

    pub fn parse(&self, spec: &str) -> Result<Box<dyn Trigger>> {
        if spec.is_empty() {
            return Err(Error::Parse("empty spec string".to_string()));
        }

        let (spec, location) = self.extract_timezone(spec)?;

        if spec.starts_with('@') {
            if (self.options & ParseOption::DESCRIPTOR).0 == 0 {
                return Err(Error::Parse(format!(
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

        Ok(Box::new(SpecTrigger::new(CronSpec {
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

    /// Strip an optional `TZ=...` / `CRON_TZ=...` prefix from
    /// `spec`. Returns `(remaining_spec, parsed_tz)`.
    fn extract_timezone(
        self,
        spec: &str,
    ) -> Result<(&str, Option<chrono_tz::Tz>)> {
        if spec.starts_with("TZ=") || spec.starts_with("CRON_TZ=") {
            let eq_idx = spec.find('=').ok_or_else(|| {
                Error::Parse("invalid timezone specification".to_string())
            })?;
            let space_idx = spec.find(' ').ok_or_else(|| {
                Error::Parse(
                    "timezone specification must be followed by space"
                        .to_string(),
                )
            })?;

            let tz_str = &spec[eq_idx + 1..space_idx];
            let tz: chrono_tz::Tz = tz_str.parse().map_err(|e| {
                Error::Parse(format!("provided bad location {tz_str}: {e}"))
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
    ) -> Result<Box<dyn Trigger>> {
        let seconds = seconds_bounds();
        let minutes = minutes_bounds();
        let hours = hours_bounds();
        let dom = dom_bounds();
        let months = months_bounds();
        let dow = dow_bounds();

        let make_trigger = |second, minute, hour, dom, month, dow| {
            Box::new(SpecTrigger::new(CronSpec {
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
                        Error::Parse(format!(
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

                Err(Error::Parse(format!(
                    "unrecognized descriptor: {descriptor}"
                )))
            }
        }
    }
}
