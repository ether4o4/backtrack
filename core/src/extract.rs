//! Identifier extraction: scans free text for the recurring identifiers that
//! let the correlation engine link records across platforms.
//!
//! Extraction is intentionally conservative — it favours precision over
//! recall so the correlation graph does not fill up with noise. Structured
//! identifiers supplied by parsers are always trusted; text scanning only
//! adds the high-confidence patterns below.

use crate::model::{EntityKind, Identifier};
use once_cell::sync::Lazy;
use regex::Regex;

static EMAIL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}\b").unwrap()
});

// URLs (http/https). Kept before phone/ip so we can avoid re-scanning them.
static URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bhttps?://[^\s<>()\[\]{}\x22']+").unwrap()
});

// IPv4 dotted quad with 0-255 octet validation.
static IPV4: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b((25[0-5]|2[0-4]\d|1?\d?\d)\.){3}(25[0-5]|2[0-4]\d|1?\d?\d)\b").unwrap()
});

// Phone numbers: optional +, groups of digits with spaces/dashes/dots/parens.
// Requires 7-15 digits total after normalization to avoid matching years etc.
static PHONE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\+?\d[\d\s().\-]{6,18}\d").unwrap()
});

// Hex file hashes: md5(32) / sha1(40) / sha256(64).
static HASH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b[a-f0-9]{32}\b|\b[a-f0-9]{40}\b|\b[a-f0-9]{64}\b").unwrap()
});

/// Normalize a phone number to `+<digits>` / `<digits>` for stable matching.
/// Returns `None` if the digit count is out of the plausible range.
pub fn normalize_phone(raw: &str) -> Option<String> {
    let plus = raw.trim_start().starts_with('+');
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 7 || digits.len() > 15 {
        return None;
    }
    Some(if plus { format!("+{digits}") } else { digits })
}

/// The "significant" digits of a phone number for cross-format matching: the
/// last 10 digits (national significant number for most regions), or all of
/// them if fewer. This lets a user's local `555-123-4567` match a stored
/// E.164 `+15551234567`, and vice-versa.
pub fn phone_significant(value: &str) -> String {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    let n = digits.len();
    if n > 10 {
        digits[n - 10..].to_string()
    } else {
        digits
    }
}

/// Decide whether a regex phone match is really a phone rather than a date,
/// coordinate, version string, or a number embedded in a larger one.
///
/// `text` is the full string, `start..end` the match span, `matched` the
/// matched slice. Returns the normalized phone, or `None` to reject.
fn accept_phone(text: &str, start: usize, end: usize, matched: &str) -> Option<String> {
    // Reject if glued to a digit or number-punctuation on either side — that
    // means we clipped a longer number (timestamp, decimal, version).
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    let is_boundary_bad = |c: Option<char>| {
        matches!(c, Some(ch) if ch.is_ascii_digit() || ch == '.' || ch == ':' || ch == '/')
    };
    if is_boundary_bad(before) || is_boundary_bad(after) {
        return None;
    }
    // A single interior dot means a decimal (e.g. a GPS coordinate). Real
    // dotted phone numbers use two or more separators (555.123.4567).
    if matched.matches('.').count() == 1 {
        return None;
    }
    // Dashed values that parse as a calendar date are dates, not phones.
    if matched.contains('-')
        && crate::ingest::timeparse::parse_timestamp(matched.trim()).is_some()
    {
        return None;
    }
    normalize_phone(matched)
}

/// Lower-case + trim an email for stable matching.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// Scan a blob of text and return the identifiers found in it.
///
/// De-duplication and cross-record linking happen later in the DB layer; this
/// just yields typed candidates. Emails and URLs are masked out before phone
/// scanning so an email's digits are not misread as a phone number.
pub fn scan_text(text: &str) -> Vec<Identifier> {
    let mut out = Vec::new();
    let mut masked = text.to_string();

    for m in EMAIL.find_iter(text) {
        out.push(Identifier {
            kind: EntityKind::Email,
            value: normalize_email(m.as_str()),
        });
    }
    for m in URL.find_iter(text) {
        out.push(Identifier {
            kind: EntityKind::Url,
            value: m.as_str().trim_end_matches(['.', ',', ')']).to_string(),
        });
    }
    // Mask emails and URLs so their characters do not leak into later scans.
    for r in EMAIL.find_iter(text).chain(URL.find_iter(text)) {
        let blank: String = std::iter::repeat(' ').take(r.end() - r.start()).collect();
        masked.replace_range(r.start()..r.end(), &blank);
    }

    for m in HASH.find_iter(&masked) {
        out.push(Identifier {
            kind: EntityKind::FileHash,
            value: m.as_str().to_ascii_lowercase(),
        });
    }
    // Mask hashes before IP/phone (a 32-hex string won't collide, but keeps
    // the scans disjoint and cheap).
    let mut masked2 = masked.clone();
    for r in HASH.find_iter(&masked) {
        let blank: String = std::iter::repeat(' ').take(r.end() - r.start()).collect();
        masked2.replace_range(r.start()..r.end(), &blank);
    }

    for m in IPV4.find_iter(&masked2) {
        out.push(Identifier {
            kind: EntityKind::Ip,
            value: m.as_str().to_string(),
        });
    }
    // Mask IPs so their dotted digits are not re-read as phone numbers.
    let mut masked3 = masked2.clone();
    for r in IPV4.find_iter(&masked2) {
        let blank: String = std::iter::repeat(' ').take(r.end() - r.start()).collect();
        masked3.replace_range(r.start()..r.end(), &blank);
    }

    for m in PHONE.find_iter(&masked3) {
        // Validate against the ORIGINAL text so boundary checks see the real
        // neighbouring characters (masking only blanks earlier match kinds).
        if let Some(norm) = accept_phone(text, m.start(), m.end(), &text[m.start()..m.end()]) {
            out.push(Identifier {
                kind: EntityKind::Phone,
                value: norm,
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_email() {
        let ids = scan_text("reach me at John.Doe@Example.com please");
        assert!(ids
            .iter()
            .any(|i| i.kind == EntityKind::Email && i.value == "john.doe@example.com"));
    }

    #[test]
    fn extracts_phone_normalized() {
        let ids = scan_text("call +1 (555) 123-4567 tonight");
        assert!(ids
            .iter()
            .any(|i| i.kind == EntityKind::Phone && i.value == "+15551234567"));
    }

    #[test]
    fn email_digits_not_read_as_phone() {
        let ids = scan_text("user1234567@mail.com");
        assert!(ids.iter().all(|i| i.kind != EntityKind::Phone));
    }

    #[test]
    fn validates_ipv4_octets() {
        let ids = scan_text("server 192.168.1.42 and bogus 999.1.1.1");
        let ips: Vec<_> = ids
            .iter()
            .filter(|i| i.kind == EntityKind::Ip)
            .map(|i| i.value.as_str())
            .collect();
        assert!(ips.contains(&"192.168.1.42"));
        assert!(!ips.contains(&"999.1.1.1"));
    }

    #[test]
    fn extracts_sha256_hash() {
        let h = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let ids = scan_text(&format!("hash: {h}"));
        assert!(ids
            .iter()
            .any(|i| i.kind == EntityKind::FileHash && i.value == h));
    }

    #[test]
    fn rejects_too_short_phone() {
        assert_eq!(normalize_phone("12345"), None);
    }

    #[test]
    fn gps_coordinate_not_read_as_phone() {
        let ids = scan_text("2024-03-09 12:05:00 | 39.7392 | -104.9903 | downtown");
        assert!(
            ids.iter().all(|i| i.kind != EntityKind::Phone),
            "coords/timestamps must not become phones: {ids:?}"
        );
    }

    #[test]
    fn iso_date_not_read_as_phone() {
        let ids = scan_text("event on 2024-03-09 was logged");
        assert!(ids.iter().all(|i| i.kind != EntityKind::Phone));
    }

    #[test]
    fn phone_significant_matches_across_formats() {
        assert_eq!(phone_significant("+15551234567"), "5551234567");
        assert_eq!(phone_significant("5551234567"), "5551234567");
        assert_eq!(
            phone_significant("+15551234567"),
            phone_significant("555-123-4567".chars().filter(|c| c.is_ascii_digit()).collect::<String>().as_str())
        );
    }
}
