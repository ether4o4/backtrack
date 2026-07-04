//! Generic + platform-aware JSON parser.
//!
//! Handles three shapes:
//!   1. Facebook/Instagram message exports: `{ participants, messages: [...] }`.
//!   2. A top-level array of objects -> one record per element.
//!   3. Any other object -> a single record.
//!
//! Field names are matched loosely (case-insensitive, common synonyms) so we
//! extract a timestamp/sender/text without a per-platform schema for each.

use crate::ingest::timeparse::parse_timestamp;
use crate::model::{EntityKind, NormalizedRecord, SourceKind};
use serde_json::Value;

pub fn parse(bytes: &[u8], platform: &str) -> crate::Result<Vec<NormalizedRecord>> {
    let root: Value = serde_json::from_slice(bytes)
        .map_err(|e| crate::Error::Parse(format!("json: {e}")))?;

    // Shape 1: message-export object with a `messages` array.
    if let Some(msgs) = root.get("messages").and_then(|m| m.as_array()) {
        let convo = participants(&root);
        return Ok(msgs
            .iter()
            .map(|m| message_record(m, platform, convo.as_deref()))
            .collect());
    }

    // Shape 2: top-level array.
    if let Some(arr) = root.as_array() {
        return Ok(arr
            .iter()
            .map(|v| generic_record(v, platform))
            .collect());
    }

    // Shape 3: single object.
    Ok(vec![generic_record(&root, platform)])
}

fn participants(root: &Value) -> Option<String> {
    let names: Vec<String> = root
        .get("participants")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        None
    } else {
        Some(names.join(", "))
    }
}

fn message_record(m: &Value, platform: &str, convo: Option<&str>) -> NormalizedRecord {
    let sender = str_field(m, &["sender_name", "sender", "from", "author"]);
    let text = str_field(m, &["content", "text", "message", "body"]);
    let ts = num_field(m, &["timestamp_ms", "timestamp", "date", "time", "created_at"])
        .or_else(|| str_field(m, &["timestamp", "date", "time", "created_at"]).and_then(|s| parse_timestamp(&s)));

    // Keep the conversation label as context in `raw` (useful in the details
    // pane) but do NOT turn the participant list into a Person entity — a
    // concatenated "A, B, C" is not a real person, and a shared participant
    // would act as a hub that merges unrelated identities during clustering.
    let raw = if let (Value::Object(mut o), Some(c)) = (m.clone(), convo) {
        o.insert("conversation".into(), Value::String(c.to_string()));
        Value::Object(o)
    } else {
        m.clone()
    };

    let mut rec = NormalizedRecord::new("message", platform)
        .with_time(ts)
        .with_title(sender.clone())
        .with_body(text)
        .with_raw(raw);

    if let Some(s) = sender {
        rec.add_identifier(EntityKind::Person, s);
    }
    rec
}

fn generic_record(v: &Value, platform: &str) -> NormalizedRecord {
    let title = str_field(v, &["name", "title", "sender", "from", "subject", "author"]);
    let body = str_field(v, &["content", "text", "message", "body", "description", "snippet"])
        .or_else(|| Some(compact(v)));
    let ts = num_field(v, &["timestamp_ms", "timestamp", "time", "date", "created_at", "epoch"])
        .or_else(|| str_field(v, &["timestamp", "time", "date", "created_at"]).and_then(|s| parse_timestamp(&s)));

    let mut rec = NormalizedRecord::new("row", platform)
        .with_time(ts)
        .with_title(title)
        .with_body(body)
        .with_raw(v.clone());

    // Structured identifier fields.
    if let Some(p) = str_field(v, &["phone", "phone_number", "number"]) {
        if let Some(n) = crate::extract::normalize_phone(&p) {
            rec.add_identifier(EntityKind::Phone, n);
        }
    }
    if let Some(e) = str_field(v, &["email", "email_address"]) {
        rec.add_identifier(EntityKind::Email, crate::extract::normalize_email(&e));
    }
    if let Some(u) = str_field(v, &["username", "handle", "screen_name"]) {
        rec.add_identifier(EntityKind::Username, u);
    }
    if let Some(d) = str_field(v, &["device_id", "device", "deviceid"]) {
        rec.add_identifier(EntityKind::DeviceId, d);
    }
    if let (Some(lat), Some(lon)) = (
        num_field(v, &["latitude", "lat"]),
        num_field(v, &["longitude", "lon", "lng"]),
    ) {
        rec.add_identifier(EntityKind::Location, format!("{lat},{lon}"));
    }
    rec
}

/// Case-insensitive string field lookup over the given synonym list.
fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    let obj = v.as_object()?;
    for (k, val) in obj {
        let kl = k.to_ascii_lowercase();
        if keys.iter().any(|want| kl == *want) {
            if let Some(s) = val.as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

/// Numeric field lookup returning a normalized epoch (seconds) when the field
/// looks like a timestamp, or the raw integer otherwise.
fn num_field(v: &Value, keys: &[&str]) -> Option<i64> {
    let obj = v.as_object()?;
    for (k, val) in obj {
        let kl = k.to_ascii_lowercase();
        if keys.iter().any(|want| kl == *want) {
            if let Some(n) = val.as_i64() {
                // Reuse timeparse's epoch normalization for ms/us fields.
                return Some(parse_timestamp(&n.to_string()).unwrap_or(n));
            }
            if let Some(f) = val.as_f64() {
                return Some(parse_timestamp(&(f as i64).to_string()).unwrap_or(f as i64));
            }
        }
    }
    None
}

/// A compact one-line rendering of an object for the searchable body when no
/// obvious text field exists.
fn compact(v: &Value) -> String {
    match v {
        Value::Object(o) => o
            .iter()
            .filter_map(|(k, val)| match val {
                Value::String(s) if !s.is_empty() => Some(format!("{k}: {s}")),
                Value::Number(n) => Some(format!("{k}: {n}")),
                Value::Bool(b) => Some(format!("{k}: {b}")),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" | "),
        other => other.to_string(),
    }
}

pub const KIND: SourceKind = SourceKind::Json;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_facebook_style_messages() {
        let j = br#"{
            "participants":[{"name":"John Smith"},{"name":"Me"}],
            "messages":[
                {"sender_name":"John Smith","timestamp_ms":1700000000000,"content":"hey"},
                {"sender_name":"Me","timestamp_ms":1700000100000,"content":"hi"}
            ]
        }"#;
        let recs = parse(j, "facebook").unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].kind, "message");
        assert_eq!(recs[0].timestamp, Some(1700000000));
        assert!(recs[0]
            .identifiers
            .iter()
            .any(|i| i.kind == EntityKind::Person && i.value == "John Smith"));
    }

    #[test]
    fn parses_array_of_objects() {
        let j = br#"[{"email":"a@b.com","name":"A"},{"phone":"555-123-4567","name":"B"}]"#;
        let recs = parse(j, "contacts").unwrap();
        assert_eq!(recs.len(), 2);
        assert!(recs[0].identifiers.iter().any(|i| i.kind == EntityKind::Email));
        assert!(recs[1].identifiers.iter().any(|i| i.kind == EntityKind::Phone));
    }

    #[test]
    fn parses_gps_lat_lon() {
        let j = br#"[{"latitude":39.7392,"longitude":-104.9903,"timestamp":1700000000}]"#;
        let recs = parse(j, "location").unwrap();
        assert!(recs[0]
            .identifiers
            .iter()
            .any(|i| i.kind == EntityKind::Location));
    }
}
