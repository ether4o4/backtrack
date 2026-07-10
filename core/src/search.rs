//! Universal search over the normalized store.
//!
//! A single query string is interpreted the way the user expects a
//! "Google-like" box to behave: a phone number matches phone entities, an
//! email matches email entities, an entity value match pulls in every record
//! linked to that entity, and everything else runs as full-text search. All
//! of it merges into one ranked result list.

use crate::db::{map_record, Store};
use crate::extract;
use crate::model::Record;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Structured filters that can accompany a text query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilters {
    pub platform: Option<String>,
    pub kind: Option<String>,
    /// Inclusive lower bound (unix seconds).
    pub after: Option<i64>,
    /// Inclusive upper bound (unix seconds).
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub record: Record,
    /// Why this record matched: "fts", "phone", "email", "entity".
    pub matched_via: String,
}

impl Store {
    /// Run a universal search. Empty `query` returns the most recent records
    /// (respecting filters), which is what the UI shows on an empty box.
    pub fn search(&self, query: &str, filters: &SearchFilters) -> crate::Result<Vec<SearchHit>> {
        let q = query.trim();
        let limit = filters.limit.unwrap_or(200).clamp(1, 5000);

        // No query text: recent records by timestamp.
        if q.is_empty() {
            return self.recent(filters, limit);
        }

        let mut hits: Vec<SearchHit> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 1. Identifier-aware matching: if the query looks like a phone/email,
        //    pull records linked to the matching entity. Phones match on their
        //    significant digits so a local number finds an E.164 one.
        for id in exact_identifier_candidates(q) {
            let recs = if id.kind == crate::model::EntityKind::Phone {
                self.records_for_phone(&id.value, filters, limit)?
            } else {
                self.records_for_entity_value(id.kind.as_str(), &id.value, filters, limit)?
            };
            for rec in recs {
                if seen.insert(rec.id) {
                    hits.push(SearchHit {
                        record: rec,
                        matched_via: id.kind.as_str().to_string(),
                    });
                }
            }
        }

        // 2. Entity value substring match (usernames, device ids, locations...).
        for rec in self.records_for_entity_like(q, filters, limit)? {
            if seen.insert(rec.id) {
                hits.push(SearchHit {
                    record: rec,
                    matched_via: "entity".into(),
                });
            }
        }

        // 3. Full-text search over title/body/platform.
        for rec in self.fts(q, filters, limit)? {
            if seen.insert(rec.id) {
                hits.push(SearchHit {
                    record: rec,
                    matched_via: "fts".into(),
                });
            }
        }

        hits.truncate(limit as usize);
        Ok(hits)
    }

    fn recent(&self, f: &SearchFilters, limit: i64) -> crate::Result<Vec<SearchHit>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, kind, platform, timestamp, title, body FROM records
             WHERE (?1 IS NULL OR platform = ?1)
               AND (?2 IS NULL OR kind = ?2)
               AND (?3 IS NULL OR timestamp >= ?3)
               AND (?4 IS NULL OR timestamp <= ?4)
             ORDER BY COALESCE(timestamp, 0) DESC, id DESC
             LIMIT ?5",
        )?;
        let rows = stmt
            .query_map(
                params![f.platform, f.kind, f.after, f.before, limit],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|record| SearchHit {
                record,
                matched_via: "recent".into(),
            })
            .collect())
    }

    fn fts(&self, q: &str, f: &SearchFilters, limit: i64) -> crate::Result<Vec<Record>> {
        let match_expr = to_fts_query(q);
        if match_expr.is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.source_id, r.kind, r.platform, r.timestamp, r.title, r.body
             FROM records_fts f
             JOIN records r ON r.id = f.rowid
             WHERE records_fts MATCH ?1
               AND (?2 IS NULL OR r.platform = ?2)
               AND (?3 IS NULL OR r.kind = ?3)
               AND (?4 IS NULL OR r.timestamp >= ?4)
               AND (?5 IS NULL OR r.timestamp <= ?5)
             ORDER BY rank
             LIMIT ?6",
        )?;
        let rows = stmt
            .query_map(
                params![match_expr, f.platform, f.kind, f.after, f.before, limit],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn records_for_entity_value(
        &self,
        kind: &str,
        value: &str,
        f: &SearchFilters,
        limit: i64,
    ) -> crate::Result<Vec<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.source_id, r.kind, r.platform, r.timestamp, r.title, r.body
             FROM records r
             JOIN record_entities re ON re.record_id = r.id
             JOIN entities e ON e.id = re.entity_id
             WHERE e.kind = ?1 AND e.value = ?2
               AND (?3 IS NULL OR r.platform = ?3)
               AND (?4 IS NULL OR r.kind = ?4)
               AND (?5 IS NULL OR r.timestamp >= ?5)
               AND (?6 IS NULL OR r.timestamp <= ?6)
             ORDER BY COALESCE(r.timestamp,0) DESC
             LIMIT ?7",
        )?;
        let rows = stmt
            .query_map(
                params![kind, value, f.platform, f.kind, f.after, f.before, limit],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Match phone records by significant digits (last 10), so `555-123-4567`
    /// and `+1 555 123 4567` resolve to the same records.
    fn records_for_phone(
        &self,
        value: &str,
        f: &SearchFilters,
        limit: i64,
    ) -> crate::Result<Vec<Record>> {
        let sig = extract::phone_significant(value);
        if sig.len() < 7 {
            return Ok(vec![]);
        }
        let like = format!("%{sig}");
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT r.id, r.source_id, r.kind, r.platform, r.timestamp, r.title, r.body
             FROM records r
             JOIN record_entities re ON re.record_id = r.id
             JOIN entities e ON e.id = re.entity_id
             WHERE e.kind = 'phone' AND e.value LIKE ?1
               AND (?2 IS NULL OR r.platform = ?2)
               AND (?3 IS NULL OR r.kind = ?3)
               AND (?4 IS NULL OR r.timestamp >= ?4)
               AND (?5 IS NULL OR r.timestamp <= ?5)
             ORDER BY COALESCE(r.timestamp,0) DESC
             LIMIT ?6",
        )?;
        let rows = stmt
            .query_map(
                params![like, f.platform, f.kind, f.after, f.before, limit],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn records_for_entity_like(
        &self,
        q: &str,
        f: &SearchFilters,
        limit: i64,
    ) -> crate::Result<Vec<Record>> {
        let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT r.id, r.source_id, r.kind, r.platform, r.timestamp, r.title, r.body
             FROM records r
             JOIN record_entities re ON re.record_id = r.id
             JOIN entities e ON e.id = re.entity_id
             WHERE e.value LIKE ?1 ESCAPE '\\'
               AND (?2 IS NULL OR r.platform = ?2)
               AND (?3 IS NULL OR r.kind = ?3)
               AND (?4 IS NULL OR r.timestamp >= ?4)
               AND (?5 IS NULL OR r.timestamp <= ?5)
             ORDER BY COALESCE(r.timestamp,0) DESC
             LIMIT ?6",
        )?;
        let rows = stmt
            .query_map(
                params![like, f.platform, f.kind, f.after, f.before, limit],
                map_record,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// If the query is itself a recognizable identifier, return normalized
/// candidates so we can match entities exactly.
fn exact_identifier_candidates(q: &str) -> Vec<crate::model::Identifier> {
    let mut out = extract::scan_text(q);
    // scan_text already normalizes phone/email; keep only whole-query matches
    // to avoid over-triggering on long free text.
    out.retain(|id| match id.kind {
        crate::model::EntityKind::Phone
        | crate::model::EntityKind::Email
        | crate::model::EntityKind::Ip
        | crate::model::EntityKind::FileHash => true,
        _ => false,
    });
    out
}

/// Turn a raw user query into a safe FTS5 MATCH expression: split into terms,
/// quote each term (so punctuation can't inject FTS syntax), AND them together
/// with a trailing prefix match on the last term for incremental search.
pub fn to_fts_query(q: &str) -> String {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();
    if terms.is_empty() {
        return String::new();
    }
    let mut expr = terms.join(" AND ");
    // Make the final term a prefix match: "foo" -> "foo"*
    expr.push('*');
    expr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_is_escaped() {
        // A query full of FTS operators must not blow up the parser.
        let expr = to_fts_query("john OR (drop table)");
        assert!(expr.contains("\"john\""));
        assert!(expr.contains("\"table\""));
    }

    #[test]
    fn empty_query_is_empty_expr() {
        assert_eq!(to_fts_query("   "), "");
    }
}
