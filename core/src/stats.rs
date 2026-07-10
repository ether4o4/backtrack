//! Aggregate statistics for the dashboard: import counts, per-platform and
//! per-kind breakdowns, the overall time span, and a daily activity histogram
//! that powers the calendar / heatmap views.

use crate::db::Store;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub sources: i64,
    pub records: i64,
    pub entities: i64,
    pub by_platform: Vec<Bucket>,
    pub by_kind: Vec<Bucket>,
    pub by_entity_kind: Vec<Bucket>,
    pub earliest: Option<i64>,
    pub latest: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bucket {
    pub key: String,
    pub count: i64,
}

/// One day of activity for the heatmap: `day` is unix seconds at UTC midnight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayCount {
    pub day: i64,
    pub count: i64,
}

impl Store {
    pub fn stats(&self) -> crate::Result<Stats> {
        Ok(Stats {
            sources: self.scalar("SELECT COUNT(*) FROM sources")?,
            records: self.scalar("SELECT COUNT(*) FROM records")?,
            entities: self.scalar("SELECT COUNT(*) FROM entities")?,
            by_platform: self.buckets(
                "SELECT platform, COUNT(*) FROM records GROUP BY platform ORDER BY 2 DESC",
            )?,
            by_kind: self
                .buckets("SELECT kind, COUNT(*) FROM records GROUP BY kind ORDER BY 2 DESC")?,
            by_entity_kind: self.buckets(
                "SELECT kind, COUNT(*) FROM entities GROUP BY kind ORDER BY 2 DESC",
            )?,
            earliest: self.opt_scalar("SELECT MIN(timestamp) FROM records WHERE timestamp IS NOT NULL")?,
            latest: self.opt_scalar("SELECT MAX(timestamp) FROM records WHERE timestamp IS NOT NULL")?,
        })
    }

    /// Daily activity counts across the whole store, for the heatmap/calendar.
    pub fn activity_by_day(&self) -> crate::Result<Vec<DayCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT CAST(timestamp/86400 AS INTEGER)*86400 AS day, COUNT(*)
             FROM records WHERE timestamp IS NOT NULL
             GROUP BY day ORDER BY day",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DayCount {
                    day: r.get(0)?,
                    count: r.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn scalar(&self, sql: &str) -> crate::Result<i64> {
        Ok(self.conn.query_row(sql, [], |r| r.get(0))?)
    }
    fn opt_scalar(&self, sql: &str) -> crate::Result<Option<i64>> {
        Ok(self.conn.query_row(sql, [], |r| r.get(0))?)
    }
    fn buckets(&self, sql: &str) -> crate::Result<Vec<Bucket>> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Bucket {
                    key: r.get(0)?,
                    count: r.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
