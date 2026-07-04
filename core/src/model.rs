//! Core data model shared across ingest, storage, correlation and search.
//!
//! Everything imported from any platform is normalized into a single
//! `Record` shape. Identifiers found inside records (phones, emails,
//! usernames, ...) become `Entity` rows, and the many-to-many link between
//! them is what the correlation engine walks.

use serde::{Deserialize, Serialize};

/// The kind of an imported source (a file or an archive member).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Csv,
    Json,
    VCard,
    SmsXml,
    Html,
    Text,
    Unknown,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Csv => "csv",
            SourceKind::Json => "json",
            SourceKind::VCard => "vcard",
            SourceKind::SmsXml => "sms_xml",
            SourceKind::Html => "html",
            SourceKind::Text => "text",
            SourceKind::Unknown => "unknown",
        }
    }
}

/// A normalized record: one message, call, contact, event, row, etc.
///
/// This is the parser output. It has no database id yet — `db` assigns one on
/// insert. `raw` preserves the original structured payload so nothing is lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedRecord {
    /// Coarse type of the record, e.g. "message", "call", "contact", "row".
    pub kind: String,
    /// Platform/source label, e.g. "facebook", "sms", "generic-csv".
    pub platform: String,
    /// Unix epoch seconds, if the record carries a timestamp.
    pub timestamp: Option<i64>,
    /// Short human label for lists (sender, contact name, subject line...).
    pub title: Option<String>,
    /// Full searchable text body.
    pub body: Option<String>,
    /// Original structured payload, preserved verbatim.
    pub raw: serde_json::Value,
    /// Identifiers explicitly attached by the parser (structured fields).
    /// The extractor will additionally scan `title`/`body` for more.
    pub identifiers: Vec<Identifier>,
}

impl NormalizedRecord {
    pub fn new(kind: impl Into<String>, platform: impl Into<String>) -> Self {
        NormalizedRecord {
            kind: kind.into(),
            platform: platform.into(),
            timestamp: None,
            title: None,
            body: None,
            raw: serde_json::Value::Null,
            identifiers: Vec::new(),
        }
    }

    pub fn with_time(mut self, ts: Option<i64>) -> Self {
        self.timestamp = ts;
        self
    }
    pub fn with_title(mut self, t: Option<String>) -> Self {
        self.title = t;
        self
    }
    pub fn with_body(mut self, b: Option<String>) -> Self {
        self.body = b;
        self
    }
    pub fn with_raw(mut self, r: serde_json::Value) -> Self {
        self.raw = r;
        self
    }
    pub fn add_identifier(&mut self, kind: EntityKind, value: impl Into<String>) {
        let value = value.into();
        if value.trim().is_empty() {
            return;
        }
        self.identifiers.push(Identifier { kind, value });
    }
}

/// A typed identifier attached to a record by a parser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identifier {
    pub kind: EntityKind,
    pub value: String,
}

/// The kind of a normalized entity. These are the things the correlation
/// engine clusters people around.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Person,
    Username,
    Phone,
    Email,
    DeviceId,
    Cookie,
    SessionId,
    Location,
    Ip,
    FileHash,
    Url,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityKind::Person => "person",
            EntityKind::Username => "username",
            EntityKind::Phone => "phone",
            EntityKind::Email => "email",
            EntityKind::DeviceId => "device_id",
            EntityKind::Cookie => "cookie",
            EntityKind::SessionId => "session_id",
            EntityKind::Location => "location",
            EntityKind::Ip => "ip",
            EntityKind::FileHash => "file_hash",
            EntityKind::Url => "url",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "person" => EntityKind::Person,
            "username" => EntityKind::Username,
            "phone" => EntityKind::Phone,
            "email" => EntityKind::Email,
            "device_id" => EntityKind::DeviceId,
            "cookie" => EntityKind::Cookie,
            "session_id" => EntityKind::SessionId,
            "location" => EntityKind::Location,
            "ip" => EntityKind::Ip,
            "file_hash" => EntityKind::FileHash,
            "url" => EntityKind::Url,
            _ => return None,
        })
    }
}

/// A stored source (row in `sources`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub platform: String,
    pub record_count: i64,
    pub imported_at: i64,
}

/// A stored record as returned to the UI (row in `records`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: i64,
    pub source_id: i64,
    pub kind: String,
    pub platform: String,
    pub timestamp: Option<i64>,
    pub title: Option<String>,
    pub body: Option<String>,
}

/// A stored entity (row in `entities`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: i64,
    pub kind: String,
    pub value: String,
    pub display_name: Option<String>,
    pub record_count: i64,
}

/// Summary returned after an import run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportSummary {
    pub sources_added: usize,
    pub records_added: usize,
    pub entities_added: usize,
    pub files_skipped: usize,
    pub errors: Vec<String>,
}
