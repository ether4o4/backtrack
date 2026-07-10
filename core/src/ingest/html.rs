//! HTML parser for platform exports that ship as HTML rather than JSON —
//! notably Facebook, Instagram and Snapchat "download your data" archives.
//!
//! There is no single HTML schema across platforms, so this takes a robust,
//! schema-agnostic approach: strip scripts/styles/tags, decode the common
//! entities, and split the visible text into message-like blocks on the
//! timestamps the exports print next to each entry. Each block becomes a
//! record; identifiers (names, phones, emails) are extracted from its text.

use crate::ingest::timeparse::segment_by_timestamps;
use crate::model::{NormalizedRecord, SourceKind};
use once_cell::sync::Lazy;
use regex::Regex;

pub fn parse(bytes: &[u8], platform: &str) -> crate::Result<Vec<NormalizedRecord>> {
    let raw = String::from_utf8_lossy(bytes);
    let text = to_text(&raw);
    if text.trim().is_empty() {
        return Ok(vec![]);
    }

    // Split into dated blocks (timestamp may lead or trail its message,
    // depending on the exporting platform — see segment_by_timestamps). If
    // there isn't enough structure to segment, the whole document becomes one
    // searchable record.
    let Some(blocks) = segment_by_timestamps(&text) else {
        return Ok(vec![NormalizedRecord::new("document", platform)
            .with_title(Some(first_line(&text)))
            .with_body(Some(truncate(&text, 20_000)))
            .with_raw(serde_json::json!({ "source": "html" }))]);
    };

    Ok(blocks
        .into_iter()
        .map(|(ts, block)| {
            NormalizedRecord::new("message", platform)
                .with_time(Some(ts))
                .with_title(Some(first_line(&block)))
                .with_body(Some(truncate(&block, 8_000)))
                .with_raw(serde_json::json!({ "source": "html" }))
        })
        .collect())
}

/// Strip HTML down to readable text: drop script/style bodies, replace tags
/// with spaces (so words don't run together), decode common entities, and
/// collapse whitespace.
fn to_text(html: &str) -> String {
    // Rust's regex engine has no backreferences, so match each element type
    // explicitly rather than with a \1 back-reference.
    static SCRIPT: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<script[^>]*>.*?</\s*script\s*>").unwrap());
    static STYLE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<style[^>]*>.*?</\s*style\s*>").unwrap());
    static TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<[^>]+>").unwrap());
    static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t\x0b\x0c\r]+").unwrap());
    static BLANKS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

    let without_scripts = SCRIPT.replace_all(html, " ");
    let no_scripts = STYLE.replace_all(&without_scripts, " ");
    // Turn block-ish tags into newlines so messages stay on separate lines.
    let blocked = no_scripts
        .replace("</div>", "\n")
        .replace("</p>", "\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</li>", "\n")
        .replace("</tr>", "\n");
    let no_tags = TAG.replace_all(&blocked, " ");
    let decoded = decode_entities(&no_tags);
    let spaced = WS.replace_all(&decoded, " ");
    BLANKS.replace_all(&spaced, "\n\n").trim().to_string()
}

fn decode_entities(s: &str) -> String {
    let mut out = s
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    // Numeric entities like &#128512; -> best effort char.
    static NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#(\d{1,7});").unwrap());
    out = NUM
        .replace_all(&out, |c: &regex::Captures| {
            c[1].parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_default()
        })
        .into_owned();
    out
}

fn first_line(s: &str) -> String {
    let line = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    truncate(line, 120)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

pub const KIND: SourceKind = SourceKind::Html;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntityKind;

    #[test]
    fn splits_messages_on_timestamps() {
        let html = r#"<html><body>
            <div class="msg">John Smith</div>
            <div class="ts">Mar 09, 2024 4:15:07 PM</div>
            <div class="text">lunch at noon? call 555-123-4567</div>
            <div class="ts">Mar 09, 2024 4:20:00 PM</div>
            <div class="text">see you then, john@example.com</div>
        </body></html>"#;
        let recs = parse(html.as_bytes(), "facebook").unwrap();
        assert!(recs.len() >= 2, "expected message blocks, got {}", recs.len());
        assert!(recs[0].timestamp.is_some());
        // Entities are extracted from the block text on insert; here we just
        // confirm the text survived tag-stripping.
        assert!(recs[0].body.as_deref().unwrap().contains("lunch at noon"));
        assert!(recs.iter().any(|r| r.body.as_deref().unwrap_or("").contains("john@example.com")));
        // sanity: no angle brackets left over
        assert!(!recs[0].body.as_deref().unwrap().contains('<'));
        let _ = EntityKind::Email;
    }

    #[test]
    fn whole_document_when_no_timestamps() {
        let html = "<html><body><p>Reach me at jane@example.com</p></body></html>";
        let recs = parse(html.as_bytes(), "unknown").unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].kind, "document");
        assert!(recs[0].body.as_deref().unwrap().contains("jane@example.com"));
    }

    #[test]
    fn decodes_entities_and_strips_scripts() {
        let html = "<html><head><style>.x{color:red}</style></head><body>\
            <script>alert('x')</script><p>Tom &amp; Jerry &lt;3</p></body></html>";
        let recs = parse(html.as_bytes(), "unknown").unwrap();
        let body = recs[0].body.as_deref().unwrap();
        assert!(body.contains("Tom & Jerry <3"));
        assert!(!body.contains("alert"));
        assert!(!body.contains("color:red"));
    }
}
