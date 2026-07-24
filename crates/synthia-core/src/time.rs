use chrono::{DateTime, Utc};
use thiserror::Error;

#[derive(Error, Debug)]
#[error("Failed to parse timestamp: {0}")]
pub struct TimestampParseError(String);

pub fn format_timestamp_utc(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

pub fn parse_timestamp(s: &str) -> Result<DateTime<Utc>, TimestampParseError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| TimestampParseError(e.to_string()))
}

pub fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use chrono::Datelike;

    use super::*;

    #[test]
    fn test_format_timestamp_utc() {
        let dt = DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let formatted = format_timestamp_utc(&dt);
        assert!(formatted.contains("2024-01-15"));
        assert!(formatted.contains("10:30:00"));
    }

    #[test]
    fn test_parse_timestamp_valid() {
        let input = "2024-01-15T10:30:00Z";
        let parsed = parse_timestamp(input).unwrap();
        assert_eq!(parsed.year(), 2024);
        assert_eq!(parsed.month(), 1);
        assert_eq!(parsed.day(), 15);
    }

    #[test]
    fn test_parse_timestamp_invalid() {
        assert!(parse_timestamp("not-a-date").is_err());
    }

    #[test]
    fn test_roundtrip_format_and_parse() {
        let original = Utc::now();
        let formatted = format_timestamp_utc(&original);
        let parsed = parse_timestamp(&formatted).unwrap();
        let diff = parsed - original;
        assert!(diff.num_seconds() < 1);
    }

    #[test]
    fn test_now_utc() {
        let now = now_utc();
        let actual = Utc::now();
        let diff = actual - now;
        assert!(diff.num_seconds() < 1);
    }
}
