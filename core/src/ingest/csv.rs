//! Generic CSV/TSV parser.
//!
//! Every row becomes a record. The parser sniffs the header for common
//! semantic columns (a timestamp, a name/title, a body/message) so the row
//! lands in the timeline and reads well in lists; all columns are preserved
//! in `raw` and joined into the searchable body.

use crate::ingest::timeparse::parse_timestamp;
use crate::model::{EntityKind, NormalizedRecord, SourceKind};

pub fn parse(bytes: &[u8], platform: &str) -> crate::Result<Vec<NormalizedRecord>> {
    // Detect delimiter cheaply: tab if the header has more tabs than commas.
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let delim = if head.lines().next().map(|l| l.matches('\t').count() > l.matches(',').count()).unwrap_or(false) {
        b'\t'
    } else {
        b','
    };

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .has_headers(true)
        .from_reader(bytes);

    let headers: Vec<String> = rdr
        .headers()
        .map(|h| h.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let ts_col = find_col(&headers, &["timestamp", "date", "time", "datetime", "created", "sent"]);
    let name_col = find_col(&headers, &["name", "from", "sender", "contact", "author", "title"]);
    let body_col = find_col(&headers, &["message", "body", "text", "content", "snippet", "subject"]);
    let phone_col = find_col(&headers, &["phone", "number", "mobile", "tel"]);
    let email_col = find_col(&headers, &["email", "e-mail", "mail"]);

    let mut out = Vec::new();
    for result in rdr.records() {
        let row = match result {
            Ok(r) => r,
            Err(_) => continue, // skip malformed rows, keep going
        };
        let get = |idx: Option<usize>| idx.and_then(|i| row.get(i)).map(|s| s.to_string());

        // Build a JSON object of all named columns.
        let mut obj = serde_json::Map::new();
        for (i, field) in row.iter().enumerate() {
            let key = headers.get(i).cloned().unwrap_or_else(|| format!("col{i}"));
            obj.insert(key, serde_json::Value::String(field.to_string()));
        }

        let ts = get(ts_col).and_then(|s| parse_timestamp(&s));
        let title = get(name_col);
        let body = get(body_col).or_else(|| {
            // No dedicated body column: join all fields so the row is searchable.
            Some(
                row.iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | "),
            )
        });

        let mut rec = NormalizedRecord::new("row", platform)
            .with_time(ts)
            .with_title(title)
            .with_body(body)
            .with_raw(serde_json::Value::Object(obj));

        if let Some(phone) = get(phone_col) {
            if let Some(n) = crate::extract::normalize_phone(&phone) {
                rec.add_identifier(EntityKind::Phone, n);
            }
        }
        if let Some(email) = get(email_col) {
            rec.add_identifier(EntityKind::Email, crate::extract::normalize_email(&email));
        }
        if let Some(name) = get(name_col) {
            rec.add_identifier(EntityKind::Person, name);
        }
        out.push(rec);
    }
    Ok(out)
}

/// Case-insensitive header lookup: first header whose name contains any needle.
fn find_col(headers: &[String], needles: &[&str]) -> Option<usize> {
    headers.iter().position(|h| {
        let h = h.to_ascii_lowercase();
        needles.iter().any(|n| h.contains(n))
    })
}

pub const KIND: SourceKind = SourceKind::Csv;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows_and_columns() {
        let csv = "name,phone,message\nJohn,555-123-4567,hey there\nJane,,hi\n";
        let recs = parse(csv.as_bytes(), "contacts").unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].title.as_deref(), Some("John"));
        assert!(recs[0]
            .identifiers
            .iter()
            .any(|i| i.kind == EntityKind::Phone));
    }

    #[test]
    fn handles_tsv() {
        let tsv = "date\tfrom\ttext\n2024-03-01\tAlice\thello\n";
        let recs = parse(tsv.as_bytes(), "sms").unwrap();
        assert_eq!(recs.len(), 1);
        assert!(recs[0].timestamp.is_some());
    }
}
