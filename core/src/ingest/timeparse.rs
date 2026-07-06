//! Best-effort timestamp parsing across the many formats found in exports.
//!
//! Returns unix epoch seconds. Handles: unix seconds/millis, ISO-8601 /
//! RFC-3339, and a set of common human date formats. Ambiguous or unparseable
//! values return `None` rather than guessing wildly.

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Regex (compact, no verbose mode) matching the human date/time strings that
/// appear in HTML/PDF exports: "Mar 09, 2024 4:15:07 PM", "2024-03-09 16:15",
/// "09/03/2024, 16:15". Shared by the HTML and PDF parsers to split text into
/// dated blocks. `[\s,]` (not `[ ,]`) keeps the space separator intact.
pub const TS_PATTERN: &str = r"(?i)\b(?:(?:jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\s+\d{1,2},?\s+\d{4}(?:[\s,]+\d{1,2}:\d{2}(?::\d{2})?\s*(?:am|pm)?)?|\d{4}-\d{2}-\d{2}(?:[\st]\d{1,2}:\d{2}(?::\d{2})?)?|\d{1,2}/\d{1,2}/\d{4}(?:[\s,]+\d{1,2}:\d{2}(?::\d{2})?\s*(?:am|pm)?)?)\b";

pub fn parse_timestamp(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Pure integer: unix seconds or milliseconds (or micro/nano).
    if let Ok(n) = s.parse::<i64>() {
        return Some(normalize_epoch(n));
    }
    if let Ok(f) = s.parse::<f64>() {
        if f > 0.0 {
            return Some(normalize_epoch(f as i64));
        }
    }

    // RFC-3339 / ISO-8601 with timezone.
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp());
    }

    // Common explicit formats, interpreted as UTC when no zone is present.
    const NAIVE_FMTS: &[&str] = &[
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%m/%d/%Y %H:%M:%S",
        "%m/%d/%Y %I:%M %p",
        "%m/%d/%Y, %I:%M %p",
        "%d/%m/%Y %H:%M",
        "%d/%m/%Y, %H:%M",
        "%b %d, %Y %I:%M:%S %p",
        "%b %d, %Y %I:%M %p",
        "%b %d, %Y, %I:%M %p",
        "%B %d, %Y %I:%M:%S %p",
        "%B %d, %Y %I:%M %p",
        "%B %d, %Y",
    ];
    for fmt in NAIVE_FMTS {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(Utc.from_utc_datetime(&ndt).timestamp());
        }
    }

    // Date-only formats -> midnight UTC.
    const DATE_FMTS: &[&str] = &["%Y-%m-%d", "%m/%d/%Y", "%d/%m/%Y", "%Y/%m/%d"];
    for fmt in DATE_FMTS {
        if let Ok(nd) = NaiveDate::parse_from_str(s, fmt) {
            let ndt = nd.and_hms_opt(0, 0, 0)?;
            return Some(Utc.from_utc_datetime(&ndt).timestamp());
        }
    }

    None
}

/// Collapse epoch values given in ms/us/ns down to seconds. Uses magnitude:
/// anything past year ~2286 in seconds must be a finer unit.
fn normalize_epoch(n: i64) -> i64 {
    let abs = n.abs();
    if abs >= 1_000_000_000_000_000_000 {
        n / 1_000_000_000 // nanoseconds
    } else if abs >= 1_000_000_000_000_000 {
        n / 1_000_000 // microseconds
    } else if abs >= 1_000_000_000_000 {
        n / 1_000 // milliseconds
    } else {
        n // seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unix_seconds() {
        assert_eq!(parse_timestamp("1700000000"), Some(1700000000));
    }

    #[test]
    fn parses_unix_millis() {
        assert_eq!(parse_timestamp("1700000000000"), Some(1700000000));
    }

    #[test]
    fn parses_rfc3339() {
        assert_eq!(parse_timestamp("2023-11-14T22:13:20Z"), Some(1700000000));
    }

    #[test]
    fn parses_iso_naive() {
        let ts = parse_timestamp("2024-03-01 08:14:00").unwrap();
        assert_eq!(ts, Utc.with_ymd_and_hms(2024, 3, 1, 8, 14, 0).unwrap().timestamp());
    }

    #[test]
    fn parses_date_only() {
        let ts = parse_timestamp("2024-03-01").unwrap();
        assert_eq!(ts, Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap().timestamp());
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_timestamp("not a date"), None);
    }
}
