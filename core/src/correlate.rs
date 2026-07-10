//! Correlation engine: given an entity, find everything connected to it.
//!
//! Two records are related when they share an entity; two entities are
//! related when they co-occur in the same record. Walking these links yields
//! the "identity cluster" the design shows — the platforms, identifiers,
//! shared photos and locations that hang off one person.

use crate::db::Store;
use crate::model::{Entity, Record};
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// Everything the correlation engine surfaces for one entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correlation {
    pub entity: Entity,
    /// Distinct platforms this entity shows up on.
    pub platforms: Vec<String>,
    /// Other entities that co-occur with it, strongest link first.
    pub related_entities: Vec<RelatedEntity>,
    /// A sample of records referencing this entity (most recent first).
    pub records: Vec<Record>,
    pub total_records: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedEntity {
    pub entity: Entity,
    /// How many records the two entities share.
    pub shared_records: i64,
}

/// An entity as referenced by a record, with the role it plays there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityRef {
    pub role: String,
    #[serde(flatten)]
    pub entity: Entity,
}

/// Full detail for one record: the record, its preserved raw payload, the
/// entities it references, and other records connected through those entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordDetail {
    pub record: Record,
    pub raw: serde_json::Value,
    pub entities: Vec<EntityRef>,
    pub related_records: Vec<Record>,
}

/// A merged identity: a group of entities inferred to be the same person
/// because they are transitively linked through shared records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityCluster {
    pub label: String,
    pub members: Vec<Entity>,
    pub record_count: i64,
}

impl Store {
    /// Build the correlation view for a single entity.
    pub fn correlate(&self, entity_id: i64, sample: i64) -> crate::Result<Option<Correlation>> {
        let entity = self.entity_by_id(entity_id)?;
        let Some(entity) = entity else {
            return Ok(None);
        };

        let platforms = self.platforms_for_entity(entity_id)?;
        let related = self.related_entities(entity_id, 25)?;
        let records = self.entity_records(entity_id, sample.clamp(1, 1000))?;
        let total = self.entity_record_count(entity_id)?;

        Ok(Some(Correlation {
            entity,
            platforms,
            related_entities: related,
            records,
            total_records: total,
        }))
    }

    /// Full detail for a single record, for the details pane.
    pub fn record_detail(&self, record_id: i64, related: i64) -> crate::Result<Option<RecordDetail>> {
        let Some(record) = self.get_record(record_id)? else {
            return Ok(None);
        };
        // Preserved raw payload.
        let raw: serde_json::Value = {
            use rusqlite::OptionalExtension;
            self.conn
                .query_row(
                    "SELECT raw FROM records WHERE id=?1",
                    params![record_id],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null)
        };

        // Entities referenced by this record.
        let mut stmt = self.conn.prepare(
            "SELECT re.role, e.id, e.kind, e.value, e.display_name,
                    (SELECT COUNT(*) FROM record_entities WHERE entity_id=e.id)
             FROM record_entities re JOIN entities e ON e.id = re.entity_id
             WHERE re.record_id = ?1 ORDER BY e.kind",
        )?;
        let entities = stmt
            .query_map(params![record_id], |r| {
                Ok(EntityRef {
                    role: r.get(0)?,
                    entity: Entity {
                        id: r.get(1)?,
                        kind: r.get(2)?,
                        value: r.get(3)?,
                        display_name: r.get(4)?,
                        record_count: r.get(5)?,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Other records sharing any of this record's entities.
        let mut rstmt = self.conn.prepare(
            "SELECT DISTINCT r.id, r.source_id, r.kind, r.platform, r.timestamp, r.title, r.body
             FROM records r
             JOIN record_entities re ON re.record_id = r.id
             WHERE re.entity_id IN (SELECT entity_id FROM record_entities WHERE record_id = ?1)
               AND r.id != ?1
             ORDER BY COALESCE(r.timestamp,0) DESC
             LIMIT ?2",
        )?;
        let related_records = rstmt
            .query_map(params![record_id, related.clamp(1, 500)], crate::db::map_record)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(RecordDetail {
            record,
            raw,
            entities,
            related_records,
        }))
    }

    pub fn entity_by_id(&self, id: i64) -> crate::Result<Option<Entity>> {
        use rusqlite::OptionalExtension;
        let e = self
            .conn
            .query_row(
                "SELECT e.id, e.kind, e.value, e.display_name,
                        (SELECT COUNT(*) FROM record_entities WHERE entity_id=e.id)
                 FROM entities e WHERE e.id=?1",
                params![id],
                |r| {
                    Ok(Entity {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        value: r.get(2)?,
                        display_name: r.get(3)?,
                        record_count: r.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(e)
    }

    fn platforms_for_entity(&self, entity_id: i64) -> crate::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT r.platform FROM records r
             JOIN record_entities re ON re.record_id = r.id
             WHERE re.entity_id = ?1 ORDER BY r.platform",
        )?;
        let rows = stmt
            .query_map(params![entity_id], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn related_entities(&self, entity_id: i64, limit: i64) -> crate::Result<Vec<RelatedEntity>> {
        // Entities sharing at least one record with the target, ranked by the
        // number of shared records.
        let mut stmt = self.conn.prepare(
            "SELECT e.id, e.kind, e.value, e.display_name, COUNT(*) AS shared
             FROM record_entities a
             JOIN record_entities b ON a.record_id = b.record_id AND b.entity_id != a.entity_id
             JOIN entities e ON e.id = b.entity_id
             WHERE a.entity_id = ?1
             GROUP BY b.entity_id
             ORDER BY shared DESC, e.value ASC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![entity_id, limit], |r| {
                Ok(RelatedEntity {
                    entity: Entity {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        value: r.get(2)?,
                        display_name: r.get(3)?,
                        record_count: 0,
                    },
                    shared_records: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn entity_records(&self, entity_id: i64, limit: i64) -> crate::Result<Vec<Record>> {
        let mut stmt = self.conn.prepare(
            "SELECT r.id, r.source_id, r.kind, r.platform, r.timestamp, r.title, r.body
             FROM records r
             JOIN record_entities re ON re.record_id = r.id
             WHERE re.entity_id = ?1
             ORDER BY COALESCE(r.timestamp,0) DESC
             LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![entity_id, limit], crate::db::map_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn entity_record_count(&self, entity_id: i64) -> crate::Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM record_entities WHERE entity_id=?1",
            params![entity_id],
            |r| r.get(0),
        )?)
    }

    /// Cluster entities into inferred identities using union-find over the
    /// "co-occur in a record" relation, restricted to strong identifier kinds
    /// (phone/email/username/person/device) so weak signals like a shared URL
    /// don't merge unrelated people.
    pub fn identity_clusters(&self, min_size: usize) -> crate::Result<Vec<IdentityCluster>> {
        self.identity_clusters_opts(min_size, 50)
    }

    /// As [`Self::identity_clusters`] but with an explicit hub-degree limit;
    /// entities co-occurring with more than `hub_limit` others never bridge a
    /// union. Exposed for testing the guard.
    pub fn identity_clusters_opts(
        &self,
        min_size: usize,
        hub_limit: i64,
    ) -> crate::Result<Vec<IdentityCluster>> {
        let strong = "('person','username','phone','email','device_id')";
        // Load candidate entities.
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, kind, value, display_name FROM entities WHERE kind IN {strong}"
        ))?;
        let entities: Vec<Entity> = stmt
            .query_map([], |r| {
                Ok(Entity {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    value: r.get(2)?,
                    display_name: r.get(3)?,
                    record_count: 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut uf = UnionFind::new(&entities);

        // Collect co-occurrence edges between strong identifiers.
        let mut edges_stmt = self.conn.prepare(&format!(
            "SELECT a.entity_id, b.entity_id
             FROM record_entities a
             JOIN record_entities b ON a.record_id = b.record_id AND a.entity_id < b.entity_id
             JOIN entities ea ON ea.id = a.entity_id AND ea.kind IN {strong}
             JOIN entities eb ON eb.id = b.entity_id AND eb.kind IN {strong}"
        ))?;
        let mut edge_rows = edges_stmt.query([])?;
        let mut edges: Vec<(i64, i64)> = Vec::new();
        let mut degree: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        while let Some(row) = edge_rows.next()? {
            let a: i64 = row.get(0)?;
            let b: i64 = row.get(1)?;
            edges.push((a, b));
            *degree.entry(a).or_default() += 1;
            *degree.entry(b).or_default() += 1;
        }

        // Hub guard: an entity co-occurring with a huge number of others (a
        // group chat, a mailing list, or the account owner) is not evidence
        // that those others are the same person. Refuse to union THROUGH such
        // hubs so they don't collapse the whole graph into one cluster.
        for (a, b) in edges {
            if degree.get(&a).copied().unwrap_or(0) > hub_limit
                || degree.get(&b).copied().unwrap_or(0) > hub_limit
            {
                continue;
            }
            uf.union(a, b);
        }

        let mut clusters = uf.groups(&entities);
        // Count records per cluster and label it.
        let mut out = Vec::new();
        for members in clusters.drain(..) {
            if members.len() < min_size.max(1) {
                continue;
            }
            let ids: Vec<String> = members.iter().map(|e| e.id.to_string()).collect();
            let count: i64 = self.conn.query_row(
                &format!(
                    "SELECT COUNT(DISTINCT record_id) FROM record_entities
                     WHERE entity_id IN ({})",
                    ids.join(",")
                ),
                [],
                |r| r.get(0),
            )?;
            let label = cluster_label(&members);
            out.push(IdentityCluster {
                label,
                members,
                record_count: count,
            });
        }
        out.sort_by(|a, b| b.record_count.cmp(&a.record_count));
        Ok(out)
    }
}

/// Pick a human label for a cluster: prefer a person name, else a username,
/// else the highest-signal identifier.
fn cluster_label(members: &[Entity]) -> String {
    let pick = |kind: &str| members.iter().find(|e| e.kind == kind);
    if let Some(p) = pick("person") {
        return p.display_name.clone().unwrap_or_else(|| p.value.clone());
    }
    if let Some(u) = pick("username") {
        return u.value.clone();
    }
    if let Some(e) = pick("email") {
        return e.value.clone();
    }
    if let Some(p) = pick("phone") {
        return p.value.clone();
    }
    members
        .first()
        .map(|e| e.value.clone())
        .unwrap_or_else(|| "unknown".into())
}

/// Minimal union-find keyed by entity id.
struct UnionFind {
    parent: std::collections::HashMap<i64, i64>,
}

impl UnionFind {
    fn new(entities: &[Entity]) -> Self {
        let parent = entities.iter().map(|e| (e.id, e.id)).collect();
        UnionFind { parent }
    }
    fn find(&mut self, x: i64) -> i64 {
        let mut root = x;
        while self.parent[&root] != root {
            root = self.parent[&root];
        }
        // Path compression.
        let mut cur = x;
        while self.parent[&cur] != root {
            let next = self.parent[&cur];
            self.parent.insert(cur, root);
            cur = next;
        }
        root
    }
    fn union(&mut self, a: i64, b: i64) {
        if !self.parent.contains_key(&a) || !self.parent.contains_key(&b) {
            return;
        }
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }
    fn groups(&mut self, entities: &[Entity]) -> Vec<Vec<Entity>> {
        let mut map: std::collections::HashMap<i64, Vec<Entity>> = std::collections::HashMap::new();
        for e in entities {
            let root = self.find(e.id);
            map.entry(root).or_default().push(e.clone());
        }
        map.into_values().collect()
    }
}
