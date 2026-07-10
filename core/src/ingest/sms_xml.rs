//! SMS Backup & Restore XML parser (the de-facto Android SMS/MMS backup
//! format). Each `<sms>` element becomes a message record. `date` is unix
//! milliseconds; `type` 1 = received, 2 = sent. The counterpart's number
//! (`address`) and contact name (`contact_name`) become identifiers.

use crate::model::{EntityKind, NormalizedRecord, SourceKind};
use quick_xml::events::Event;
use quick_xml::Reader;

pub fn parse(bytes: &[u8], platform: &str) -> crate::Result<Vec<NormalizedRecord>> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) => {
                let name = e.name();
                let tag = name.as_ref();
                if tag == b"sms" {
                    out.push(sms_from_attrs(&e, platform));
                }
                // MMS text parts also carry an `address`; treat the element
                // itself as a lightweight record.
                if tag == b"mms" {
                    out.push(mms_from_attrs(&e, platform));
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(crate::Error::Parse(format!("sms xml: {e}"))),
            _ => {}
        }
        buf.clear();
    }
    Ok(out)
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.as_ref() == key {
            Some(String::from_utf8_lossy(&a.value).into_owned())
        } else {
            None
        }
    })
}

fn sms_from_attrs(e: &quick_xml::events::BytesStart, platform: &str) -> NormalizedRecord {
    let address = attr(e, b"address");
    let body = attr(e, b"body");
    let contact = attr(e, b"contact_name").filter(|c| c != "(Unknown)" && c != "null");
    let ts = attr(e, b"date")
        .and_then(|d| d.parse::<i64>().ok())
        .map(|ms| ms / 1000);
    let direction = match attr(e, b"type").as_deref() {
        Some("1") => "received",
        Some("2") => "sent",
        _ => "message",
    };

    let title = contact.clone().or_else(|| address.clone());
    let mut raw = serde_json::Map::new();
    for (k, v) in [
        ("address", &address),
        ("contact_name", &contact),
        ("direction", &Some(direction.to_string())),
    ] {
        if let Some(v) = v {
            raw.insert(k.to_string(), serde_json::Value::String(v.clone()));
        }
    }

    let mut rec = NormalizedRecord::new("sms", platform)
        .with_time(ts)
        .with_title(title)
        .with_body(body)
        .with_raw(serde_json::Value::Object(raw));

    if let Some(addr) = address {
        if let Some(n) = crate::extract::normalize_phone(&addr) {
            rec.add_identifier(EntityKind::Phone, n);
        }
    }
    if let Some(c) = contact {
        rec.add_identifier(EntityKind::Person, c);
    }
    rec
}

fn mms_from_attrs(e: &quick_xml::events::BytesStart, platform: &str) -> NormalizedRecord {
    let address = attr(e, b"address");
    let contact = attr(e, b"contact_name").filter(|c| c != "(Unknown)" && c != "null");
    let ts = attr(e, b"date")
        .and_then(|d| d.parse::<i64>().ok())
        .map(|ms| ms / 1000);
    let title = contact.clone().or_else(|| address.clone());

    let mut rec = NormalizedRecord::new("mms", platform)
        .with_time(ts)
        .with_title(title)
        .with_body(Some("[MMS]".to_string()));
    if let Some(addr) = address {
        if let Some(n) = crate::extract::normalize_phone(&addr) {
            rec.add_identifier(EntityKind::Phone, n);
        }
    }
    if let Some(c) = contact {
        rec.add_identifier(EntityKind::Person, c);
    }
    rec
}

pub const KIND: SourceKind = SourceKind::SmsXml;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sms_elements() {
        let xml = r#"<?xml version="1.0"?>
        <smses count="2">
          <sms address="+15551234567" contact_name="John Smith" date="1700000000000" type="1" body="hey there" />
          <sms address="5559876543" contact_name="(Unknown)" date="1700000100000" type="2" body="reply" />
        </smses>"#;
        let recs = parse(xml.as_bytes(), "sms").unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].timestamp, Some(1700000000));
        assert_eq!(recs[0].title.as_deref(), Some("John Smith"));
        assert!(recs[0].identifiers.iter().any(|i| i.kind == EntityKind::Phone));
        // "(Unknown)" contact name is dropped.
        assert_eq!(recs[1].title.as_deref(), Some("5559876543"));
    }
}
