//! Storage layer: opens the SQLite store, applies the schema, and persists
//! normalized records + de-duplicated entities with their links.
//!
//! Entities are de-duplicated on `(kind, value)` so the same phone number
//! appearing in a thousand records is one row referenced a thousand times —
//! this is what makes correlation and "contact frequency" cheap.

use crate::extract;
use crate::model::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

pub struct Store {
    pub(crate) conn: Connection,
}

impl Store {
    /// Open (or create) the store at `path`. Use ":memory:" for tests.
    pub fn open(path: impl AsRef<Path>) -> crate::Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_conn(conn)
    }

    pub fn open_in_memory() -> crate::Result<Self> {
        Self::from_conn(Connection::open_in_memory()?)
    }

    fn from_conn(conn: Connection) -> crate::Result<Self> {
        conn.execute_batch(include_str!("schema.sql"))?;
        Ok(Store { conn })
    }

    /// Record a processing step in the audit log.
    pub fn audit(&self, action: &str, detail: &str) -> crate::Result<()> {
        self.conn.execute(
            "INSERT INTO audit_log(at, action, detail) VALUES (?1, ?2, ?3)",
            params![now(), action, detail],
        )?;
        Ok(())
    }

    /// Create a source row and return its id.
    pub fn add_source(
        &self,
        name: &str,
        path: &str,
        kind: SourceKind,
        platform: &str,
    ) -> crate::Result<i64> {
        self.conn.execute(
            "INSERT INTO sources(name, path, kind, platform, imported_at) VALUES (?1,?2,?3,?4,?5)",
            params![name, path, kind.as_str(), platform, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Insert one normalized record under `source_id`, extract identifiers
    /// from its text, and link every identifier as a de-duplicated entity.
    /// Returns the number of *newly created* entity rows.
    pub fn insert_record(
        &mut self,
        source_id: i64,
        rec: &NormalizedRecord,
    ) -> crate::Result<usize> {
        let raw = serde_json::to_string(&rec.raw).unwrap_or_else(|_| "null".into());
        self.conn.execute(
            "INSERT INTO records(source_id, kind, platform, timestamp, title, body, raw)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                source_id,
                rec.kind,
                rec.platform,
                rec.timestamp,
                rec.title,
                rec.body,
                raw
            ],
        )?;
        let record_id = self.conn.last_insert_rowid();

        // Collect identifiers: those the parser attached, plus any scanned
        // from title+body. De-duplicate within this record.
        let mut seen: HashMap<(String, String), &'static str> = HashMap::new();
        for id in &rec.identifiers {
            seen.entry((id.kind.as_str().to_string(), id.value.clone()))
                .or_insert("tagged");
        }
        let mut text = String::new();
        if let Some(t) = &rec.title {
            text.push_str(t);
            text.push('\n');
        }
        if let Some(b) = &rec.body {
            text.push_str(b);
        }
        for id in extract::scan_text(&text) {
            seen.entry((id.kind.as_str().to_string(), id.value))
                .or_insert("mentioned");
        }

        let mut new_entities = 0;
        for ((kind, value), role) in seen {
            let (entity_id, created) = self.upsert_entity(&kind, &value)?;
            if created {
                new_entities += 1;
            }
            self.conn.execute(
                "INSERT OR IGNORE INTO record_entities(record_id, entity_id, role)
                 VALUES (?1,?2,?3)",
                params![record_id, entity_id, role],
            )?;
        }

        Ok(new_entities)
    }

    /// Insert-or-fetch an entity. Returns `(id, created)`.
    fn upsert_entity(&self, kind: &str, value: &str) -> crate::Result<(i64, bool)> {
        if let Some(id) = self
            .conn
            .query_row(
                "SELECT id FROM entities WHERE kind=?1 AND value=?2",
                params![kind, value],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
        {
            return Ok((id, false));
        }
        self.conn.execute(
            "INSERT INTO entities(kind, value) VALUES (?1,?2)",
            params![kind, value],
        )?;
        Ok((self.conn.last_insert_rowid(), true))
    }

    /// Update the cached record count on a source after import.
    pub fn refresh_source_count(&self, source_id: i64) -> crate::Result<()> {
        self.conn.execute(
            "UPDATE sources SET record_count =
                (SELECT COUNT(*) FROM records WHERE source_id=?1) WHERE id=?1",
            params![source_id],
        )?;
        Ok(())
    }

    pub fn list_sources(&self) -> crate::Result<Vec<Source>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, kind, platform, record_count, imported_at
             FROM sources ORDER BY imported_at DESC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Source {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    path: r.get(2)?,
                    kind: r.get(3)?,
                    platform: r.get(4)?,
                    record_count: r.get(5)?,
                    imported_at: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List entities, optionally filtered by kind, ordered by how many records
    /// reference them (contact frequency).
    pub fn list_entities(&self, kind: Option<EntityKind>) -> crate::Result<Vec<Entity>> {
        let sql = "SELECT e.id, e.kind, e.value, e.display_name,
                          COUNT(re.record_id) AS n
                   FROM entities e
                   LEFT JOIN record_entities re ON re.entity_id = e.id
                   WHERE (?1 IS NULL OR e.kind = ?1)
                   GROUP BY e.id
                   ORDER BY n DESC, e.value ASC";
        let mut stmt = self.conn.prepare(sql)?;
        let kind_s = kind.map(|k| k.as_str());
        let rows = stmt
            .query_map(params![kind_s], |r| {
                Ok(Entity {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    value: r.get(2)?,
                    display_name: r.get(3)?,
                    record_count: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_record(&self, id: i64) -> crate::Result<Option<Record>> {
        let rec = self
            .conn
            .query_row(
                "SELECT id, source_id, kind, platform, timestamp, title, body
                 FROM records WHERE id=?1",
                params![id],
                map_record,
            )
            .optional()?;
        Ok(rec)
    }

    /// Total record count (for stats / progress).
    pub fn record_count(&self) -> crate::Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM records", [], |r| r.get(0))?)
    }
}

pub(crate) fn map_record(r: &rusqlite::Row) -> rusqlite::Result<Record> {
    Ok(Record {
        id: r.get(0)?,
        source_id: r.get(1)?,
        kind: r.get(2)?,
        platform: r.get(3)?,
        timestamp: r.get(4)?,
        title: r.get(5)?,
        body: r.get(6)?,
    })
}

/// Current unix time in seconds. Isolated so tests can reason about it.
pub fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
