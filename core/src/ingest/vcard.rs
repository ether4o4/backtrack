//! vCard (.vcf) contacts parser.
//!
//! Splits on BEGIN/END:VCARD and pulls FN (full name), TEL, EMAIL. Each card
//! becomes one "contact" record with the name as title and phones/emails as
//! structured identifiers so contacts correlate with messages and calls.

use crate::model::{EntityKind, NormalizedRecord, SourceKind};

pub fn parse(bytes: &[u8], platform: &str) -> crate::Result<Vec<NormalizedRecord>> {
    let text = String::from_utf8_lossy(bytes);
    let mut out = Vec::new();
    let mut cur: Option<Card> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("BEGIN:VCARD") {
            cur = Some(Card::default());
            continue;
        }
        if upper.starts_with("END:VCARD") {
            if let Some(card) = cur.take() {
                if let Some(rec) = card.into_record(platform) {
                    out.push(rec);
                }
            }
            continue;
        }
        let Some(card) = cur.as_mut() else { continue };

        // Property name is up to the first ':' (params separated by ';').
        let Some((prop_full, value)) = line.split_once(':') else {
            continue;
        };
        let prop = prop_full.split(';').next().unwrap_or("").to_ascii_uppercase();
        let value = value.trim().to_string();
        match prop.as_str() {
            "FN" => card.name = Some(value),
            "N" if card.name.is_none() => {
                // N is structured "Last;First;...": build a display name.
                let parts: Vec<&str> = value.split(';').collect();
                let name = format!(
                    "{} {}",
                    parts.get(1).unwrap_or(&""),
                    parts.first().unwrap_or(&"")
                )
                .trim()
                .to_string();
                if !name.is_empty() {
                    card.name = Some(name);
                }
            }
            "TEL" => {
                if let Some(n) = crate::extract::normalize_phone(&value) {
                    card.phones.push(n);
                }
            }
            "EMAIL" => card.emails.push(crate::extract::normalize_email(&value)),
            _ => {}
        }
    }
    Ok(out)
}

#[derive(Default)]
struct Card {
    name: Option<String>,
    phones: Vec<String>,
    emails: Vec<String>,
}

impl Card {
    fn into_record(self, platform: &str) -> Option<NormalizedRecord> {
        if self.name.is_none() && self.phones.is_empty() && self.emails.is_empty() {
            return None;
        }
        let body = {
            let mut parts = Vec::new();
            if !self.phones.is_empty() {
                parts.push(self.phones.join(", "));
            }
            if !self.emails.is_empty() {
                parts.push(self.emails.join(", "));
            }
            parts.join(" · ")
        };
        let raw = serde_json::json!({
            "name": self.name,
            "phones": self.phones,
            "emails": self.emails,
        });
        let mut rec = NormalizedRecord::new("contact", platform)
            .with_title(self.name.clone())
            .with_body(Some(body))
            .with_raw(raw);
        if let Some(name) = &self.name {
            rec.add_identifier(EntityKind::Person, name.clone());
        }
        for p in self.phones {
            rec.add_identifier(EntityKind::Phone, p);
        }
        for e in self.emails {
            rec.add_identifier(EntityKind::Email, e);
        }
        Some(rec)
    }
}

pub const KIND: SourceKind = SourceKind::VCard;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_card() {
        let vcf = "BEGIN:VCARD\nVERSION:3.0\nFN:John Smith\nTEL;CELL:+1 555 123 4567\nEMAIL:john@example.com\nEND:VCARD\n";
        let recs = parse(vcf.as_bytes(), "contacts").unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].title.as_deref(), Some("John Smith"));
        assert!(recs[0].identifiers.iter().any(|i| i.kind == EntityKind::Phone));
        assert!(recs[0].identifiers.iter().any(|i| i.kind == EntityKind::Email));
    }

    #[test]
    fn parses_multiple_cards() {
        let vcf = "BEGIN:VCARD\nFN:A\nEND:VCARD\nBEGIN:VCARD\nFN:B\nEND:VCARD\n";
        let recs = parse(vcf.as_bytes(), "contacts").unwrap();
        assert_eq!(recs.len(), 2);
    }
}
