//! PDF parser: extracts the text layer from a PDF and turns it into records.
//!
//! Many exports and statements arrive as PDFs (call detail records, account
//! statements, chat transcripts saved to PDF). We pull the text and, if it
//! carries recognisable timestamps, split it into dated blocks the same way
//! the HTML parser does; otherwise the document becomes one searchable record.
//! Identifiers (names, numbers, emails) are extracted from the text on insert.
//!
//! Note: only the *text layer* is read. Scanned/image-only PDFs with no text
//! (i.e. needing OCR) yield nothing — that is surfaced as an empty result, not
//! an error.

use crate::ingest::timeparse::{parse_timestamp, TS_PATTERN};
use crate::model::{NormalizedRecord, SourceKind};
use once_cell::sync::Lazy;
use regex::Regex;

static TS_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(TS_PATTERN).unwrap());

pub fn parse(bytes: &[u8], platform: &str) -> crate::Result<Vec<NormalizedRecord>> {
    // pdf-extract can panic on malformed PDFs; contain it so one bad file
    // never aborts a whole import.
    let text = match std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(bytes)) {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => return Err(crate::Error::Parse(format!("pdf: {e}"))),
        Err(_) => return Err(crate::Error::Parse("pdf: could not be parsed".into())),
    };
    let text = normalize_ws(&text);
    if text.trim().is_empty() {
        // Image-only / no text layer — nothing to index.
        return Ok(vec![]);
    }

    let marks: Vec<(usize, i64)> = TS_LINE
        .find_iter(&text)
        .filter_map(|m| parse_timestamp(m.as_str()).map(|ts| (m.end(), ts)))
        .collect();

    if marks.len() < 2 {
        // Not clearly a dated log — keep the whole document as one record.
        return Ok(vec![NormalizedRecord::new("document", platform)
            .with_title(Some(first_line(&text)))
            .with_body(Some(truncate(&text, 40_000)))
            .with_raw(serde_json::json!({ "source": "pdf" }))]);
    }

    let mut records = Vec::new();
    for (i, &(pos, ts)) in marks.iter().enumerate() {
        let end = marks.get(i + 1).map(|n| n.0).unwrap_or(text.len());
        let block = text[pos..end].trim();
        if block.is_empty() {
            continue;
        }
        records.push(
            NormalizedRecord::new("entry", platform)
                .with_time(Some(ts))
                .with_title(Some(first_line(block)))
                .with_body(Some(truncate(block, 8_000)))
                .with_raw(serde_json::json!({ "source": "pdf" })),
        );
    }
    Ok(records)
}

fn normalize_ws(s: &str) -> String {
    static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t\x0b\x0c]+").unwrap());
    static BLANKS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());
    let a = WS.replace_all(s, " ");
    BLANKS.replace_all(&a, "\n\n").trim().to_string()
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

pub const KIND: SourceKind = SourceKind::Pdf;
