//! Pure parsing of a single cron field.
//!
//! [`get_field`] dispatches a comma-separated list to
//! [`get_range`], which handles the `start` / `start-end` /
//! `start/step` / `start-end/step` / `*` / `?` shapes. The
//! returned `u64` is a 64-bit bitmap (one bit per slot) with the
//! high bit set to flag a `*` (so the SpecTrigger can
//! distinguish explicit-zero from wildcard).

use std::collections::HashMap;

use super::{
    bounds::Bounds,
    option::{DEFAULTS, PLACES, ParseOption, STAR_BIT},
};
use crate::{Error, Result};

/// Normalize the input field list to a canonical 6-element
/// `[second, minute, hour, dom, month, dow]` layout, defaulting
/// missing slots from [`DEFAULTS`].
pub(super) fn normalize_fields<'a>(
    fields: &[&'a str],
    options: ParseOption,
) -> Result<Vec<&'a str>> {
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
        return Err(Error::Parse(
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
        return Err(Error::Parse(format!(
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
            return Err(Error::Parse("unknown optional field".to_string()));
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

/// `field` is a comma-separated list of range expressions; the
/// result ORs all of them into a 64-bit bitmap.
pub(super) fn get_field(field: &str, bounds: &Bounds) -> Result<u64> {
    let mut bits = 0u64;
    for expr in field.split(',') {
        let bit = get_range(expr, bounds)?;
        bits |= bit;
    }
    Ok(bits)
}

/// Parse one range expression (e.g. `*`, `?`, `5`, `1-10`,
/// `1-30/2`) into its bitmap.
pub(super) fn get_range(expr: &str, bounds: &Bounds) -> Result<u64> {
    let range_and_step: Vec<&str> = expr.split('/').collect();
    let low_and_high: Vec<&str> = range_and_step[0].split('-').collect();
    let _single_digit = low_and_high.len() == 1;

    let (start, end, mut extra) = if low_and_high[0] == "*"
        || low_and_high[0] == "?"
    {
        (bounds.min, bounds.max, STAR_BIT)
    } else {
        let start = parse_int_or_name(low_and_high[0], &bounds.names)?;
        let end = match low_and_high.len() {
            1 => start,
            2 => parse_int_or_name(low_and_high[1], &bounds.names)?,
            _ => {
                return Err(Error::Parse(format!("too many hyphens: {expr}")));
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
            return Err(Error::Parse(format!("too many slashes: {expr}")));
        }
    };

    if start < bounds.min {
        return Err(Error::Parse(format!(
            "beginning of range ({}) below minimum ({}): {}",
            start, bounds.min, expr
        )));
    }
    if end > bounds.max {
        return Err(Error::Parse(format!(
            "end of range ({}) above maximum ({}): {}",
            end, bounds.max, expr
        )));
    }
    if start > end {
        return Err(Error::Parse(format!(
            "beginning of range ({start}) beyond end of range ({end}): {expr}"
        )));
    }
    if step == 0 {
        return Err(Error::Parse(format!(
            "step of range should be a positive number: {expr}"
        )));
    }

    Ok(get_bits(start, end, step) | extra)
}

/// Try the name table first (case-insensitive), fall back to a
/// numeric parse.
pub(super) fn parse_int_or_name(
    expr: &str,
    names: &HashMap<&'static str, u32>,
) -> Result<u32> {
    let expr_lower = expr.to_lowercase();
    if let Some(&named_int) =
        names.get(expr_lower.as_str()).or_else(|| names.get(expr))
    {
        return Ok(named_int);
    }
    must_parse_int(expr)
}

pub(super) fn must_parse_int(expr: &str) -> Result<u32> {
    let num: i32 = expr.parse().map_err(|_| {
        Error::Parse(format!("failed to parse int from {expr}"))
    })?;

    if num < 0 {
        return Err(Error::Parse(format!(
            "negative number ({num}) not allowed: {expr}"
        )));
    }

    Ok(num as u32)
}

/// Unpack a `[min, max]` stepped range into a 64-bit bitmap.
/// `step == 1` uses a single mask; otherwise it iterates.
pub(super) fn get_bits(min: u32, max: u32, step: u32) -> u64 {
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

pub(super) fn all_bits(bounds: &Bounds) -> u64 {
    get_bits(bounds.min, bounds.max, 1) | STAR_BIT
}
