//! Auto-detection of platform and file type.
//!
//! Detection is by extension first, then a light content sniff to catch
//! mislabelled files (e.g. an SMS Backup & Restore XML, or a Facebook export
//! JSON). Platform hints come from the path (export folders are named after
//! the platform) and from known payload shapes.

use crate::model::SourceKind;
use std::path::Path;

/// A detection result: what kind of file and which platform it came from.
#[derive(Debug, Clone)]
pub struct Detection {
    pub kind: SourceKind,
    pub platform: String,
}

/// Detect from a filename plus an optional leading chunk of its content.
pub fn detect(path: &Path, head: &str) -> Detection {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let platform = platform_hint(path, head);

    let kind = match ext.as_str() {
        "csv" | "tsv" => SourceKind::Csv,
        "json" => SourceKind::Json,
        "vcf" | "vcard" => SourceKind::VCard,
        "xml" => {
            if head.contains("<smses") || head.contains("<sms ") || head.contains("<mms ") {
                SourceKind::SmsXml
            } else {
                SourceKind::Unknown
            }
        }
        "html" | "htm" => SourceKind::Html,
        "txt" | "log" => SourceKind::Text,
        _ => sniff(head),
    };

    Detection { kind, platform }
}

/// Content sniff when the extension is missing or unknown.
fn sniff(head: &str) -> SourceKind {
    let t = head.trim_start();
    if t.starts_with('{') || t.starts_with('[') {
        SourceKind::Json
    } else if t.starts_with("BEGIN:VCARD") {
        SourceKind::VCard
    } else if t.contains("<smses") || t.contains("<sms ") {
        SourceKind::SmsXml
    } else if t.starts_with("<!DOCTYPE html") || t.starts_with("<html") {
        SourceKind::Html
    } else if t.contains(',') && t.lines().next().map(|l| l.contains(',')).unwrap_or(false) {
        SourceKind::Csv
    } else {
        SourceKind::Unknown
    }
}

/// Guess the originating platform from path segments and payload markers.
fn platform_hint(path: &Path, head: &str) -> String {
    let p = path.to_string_lossy().to_ascii_lowercase();
    let known = [
        ("facebook", "facebook"),
        ("instagram", "instagram"),
        ("snapchat", "snapchat"),
        ("whatsapp", "whatsapp"),
        ("telegram", "telegram"),
        ("discord", "discord"),
        ("messages", "sms"),
        ("sms", "sms"),
        ("call", "calls"),
        ("contacts", "contacts"),
        ("browser", "browser"),
        ("history", "browser"),
        ("location", "location"),
        ("gps", "location"),
        ("email", "email"),
        ("mail", "email"),
    ];
    for (needle, label) in known {
        if p.contains(needle) {
            return label.to_string();
        }
    }
    // Payload markers.
    let h = head.to_ascii_lowercase();
    if h.contains("\"sender_name\"") && h.contains("\"messages\"") {
        return "facebook".into();
    }
    "unknown".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_csv_by_ext() {
        let d = detect(&PathBuf::from("contacts.csv"), "name,phone\n");
        assert_eq!(d.kind, SourceKind::Csv);
        assert_eq!(d.platform, "contacts");
    }

    #[test]
    fn detects_sms_xml_by_content() {
        let d = detect(
            &PathBuf::from("backup.xml"),
            "<?xml version=\"1.0\"?><smses count=\"2\">",
        );
        assert_eq!(d.kind, SourceKind::SmsXml);
    }

    #[test]
    fn sniffs_json_without_ext() {
        let d = detect(&PathBuf::from("export"), "{\"a\":1}");
        assert_eq!(d.kind, SourceKind::Json);
    }

    #[test]
    fn platform_from_path() {
        let d = detect(&PathBuf::from("instagram/messages/inbox.json"), "[]");
        assert_eq!(d.platform, "instagram");
    }
}
