// In-browser demo engine.
//
// When CrossTrace runs as the desktop app, every query goes to the Rust core
// over Tauri IPC. When the same UI is opened in a plain browser (for design
// review, screenshots, or `npm run dev` without the shell), there is no
// backend — so this module provides a small, self-contained dataset and a
// faithful-enough reimplementation of the core queries. It is ONLY used as a
// fallback; the real engine is `crosstrace-core`.

import type {
  Correlation,
  DayCount,
  Entity,
  IdentityCluster,
  RecordDetail,
  RecordRow,
  SearchFilters,
  SearchHit,
  Source,
  Stats,
} from "./types";

interface DemoRecord extends RecordRow {
  entityIds: number[];
}

const T = (iso: string) => Math.floor(new Date(iso + "Z").getTime() / 1000);

// --- Seed entities -------------------------------------------------------
const entities: Entity[] = [
  { id: 1, kind: "person", value: "John Smith", display_name: null, record_count: 0 },
  { id: 2, kind: "phone", value: "+15551234567", display_name: null, record_count: 0 },
  { id: 3, kind: "email", value: "john.smith@example.com", display_name: null, record_count: 0 },
  { id: 4, kind: "username", value: "johnsmith88", display_name: null, record_count: 0 },
  { id: 5, kind: "person", value: "Jane Doe", display_name: null, record_count: 0 },
  { id: 6, kind: "phone", value: "+15559876543", display_name: null, record_count: 0 },
  { id: 7, kind: "email", value: "jane@example.com", display_name: null, record_count: 0 },
  { id: 8, kind: "location", value: "39.7392,-104.9903", display_name: "Denver, CO", record_count: 0 },
  { id: 9, kind: "ip", value: "10.0.0.5", display_name: null, record_count: 0 },
  { id: 10, kind: "device_id", value: "ABC123", display_name: null, record_count: 0 },
];

// --- Seed sources --------------------------------------------------------
const sources: Source[] = [
  { id: 1, name: "contacts.vcf", path: "/imports/contacts.vcf", kind: "vcard", platform: "contacts", record_count: 2, imported_at: T("2026-07-01T09:00:00") },
  { id: 2, name: "sms-backup.xml", path: "/imports/sms-backup.xml", kind: "sms_xml", platform: "sms", record_count: 3, imported_at: T("2026-07-01T09:00:05") },
  { id: 3, name: "john_smith.json", path: "/imports/facebook/messages/inbox/john_smith.json", kind: "json", platform: "facebook", record_count: 2, imported_at: T("2026-07-01T09:00:10") },
  { id: 4, name: "comments.json", path: "/imports/instagram/comments.json", kind: "json", platform: "instagram", record_count: 2, imported_at: T("2026-07-01T09:00:12") },
  { id: 5, name: "history.csv", path: "/imports/gps/history.csv", kind: "csv", platform: "location", record_count: 2, imported_at: T("2026-07-01T09:00:15") },
  { id: 6, name: "calls.csv", path: "/imports/calls.csv", kind: "csv", platform: "calls", record_count: 1, imported_at: T("2026-07-01T09:00:18") },
];

// --- Seed records --------------------------------------------------------
const records: DemoRecord[] = [
  { id: 1, source_id: 1, kind: "contact", platform: "contacts", timestamp: null, title: "John Smith", body: "+15551234567 · john.smith@example.com", entityIds: [1, 2, 3] },
  { id: 2, source_id: 1, kind: "contact", platform: "contacts", timestamp: null, title: "Jane Doe", body: "+15559876543 · jane@example.com", entityIds: [5, 6, 7] },
  { id: 3, source_id: 6, kind: "call", platform: "calls", timestamp: T("2024-03-09T11:50:00"), title: "John Smith", body: "outgoing call · 4m 12s", entityIds: [1, 2] },
  { id: 4, source_id: 2, kind: "sms", platform: "sms", timestamp: T("2024-03-09T16:00:00"), title: "John Smith", body: "lunch at noon?", entityIds: [1, 2] },
  { id: 5, source_id: 2, kind: "sms", platform: "sms", timestamp: T("2024-03-09T16:05:00"), title: "John Smith", body: "sure, see you at the cafe on 5th", entityIds: [1, 2] },
  { id: 6, source_id: 3, kind: "message", platform: "facebook", timestamp: T("2024-03-09T16:10:00"), title: "John Smith", body: "my new email is john.smith@example.com", entityIds: [1, 3] },
  { id: 7, source_id: 3, kind: "message", platform: "facebook", timestamp: T("2024-03-09T16:15:00"), title: "Me", body: "got it — logging in from 10.0.0.5 on device ABC123", entityIds: [9, 10] },
  { id: 8, source_id: 5, kind: "location", platform: "location", timestamp: T("2024-03-09T12:05:00"), title: "GPS fix", body: "Denver, CO · downtown", entityIds: [8] },
  { id: 9, source_id: 5, kind: "location", platform: "location", timestamp: T("2024-03-09T13:00:00"), title: "GPS fix", body: "Denver, CO · cafe on 5th", entityIds: [8] },
  { id: 10, source_id: 4, kind: "comment", platform: "instagram", timestamp: T("2024-03-09T16:16:00"), title: "johnsmith88", body: "posted a photo near Denver", entityIds: [4, 8] },
  { id: 11, source_id: 4, kind: "comment", platform: "instagram", timestamp: T("2024-03-09T16:33:00"), title: "janedoe", body: "nice!", entityIds: [5] },
  { id: 12, source_id: 2, kind: "sms", platform: "sms", timestamp: T("2024-03-10T05:53:00"), title: "Jane Doe", body: "call me at 555-111-2222", entityIds: [5, 6] },
];

// Backfill entity record_count.
for (const e of entities) {
  e.record_count = records.filter((r) => r.entityIds.includes(e.id)).length;
}

const entityById = (id: number) => entities.find((e) => e.id === id)!;
const digits = (s: string) => s.replace(/\D/g, "");

function passesFilters(r: DemoRecord, f: SearchFilters): boolean {
  if (f.platform && r.platform !== f.platform) return false;
  if (f.kind && r.kind !== f.kind) return false;
  if (f.after != null && (r.timestamp ?? 0) < f.after) return false;
  if (f.before != null && (r.timestamp ?? 0) > f.before) return false;
  return true;
}

function toHit(r: DemoRecord, via: string): SearchHit {
  const { entityIds, ...row } = r;
  void entityIds;
  return { ...row, matched_via: via };
}

export const demo = {
  async import(): Promise<never> {
    throw new Error(
      "Import needs the desktop app. You are viewing the in-browser demo dataset."
    );
  },

  async list_sources(): Promise<Source[]> {
    return [...sources].sort((a, b) => b.imported_at - a.imported_at);
  },

  async list_entities(kind?: string | null): Promise<Entity[]> {
    return entities
      .filter((e) => !kind || e.kind === kind)
      .sort((a, b) => b.record_count - a.record_count);
  },

  async search(query: string, filters: SearchFilters): Promise<SearchHit[]> {
    const q = query.trim();
    const limit = filters.limit ?? 200;
    if (!q) return this.timeline(filters);

    const hits: SearchHit[] = [];
    const seen = new Set<number>();
    const push = (r: DemoRecord, via: string) => {
      if (!passesFilters(r, filters) || seen.has(r.id)) return;
      seen.add(r.id);
      hits.push(toHit(r, via));
    };

    // Phone match by significant digits.
    const qd = digits(q);
    if (qd.length >= 7) {
      const sig = qd.slice(-10);
      for (const e of entities) {
        if (e.kind === "phone" && digits(e.value).endsWith(sig)) {
          records.filter((r) => r.entityIds.includes(e.id)).forEach((r) => push(r, "phone"));
        }
      }
    }
    // Entity value substring (usernames, emails, device ids, locations...).
    const ql = q.toLowerCase();
    for (const e of entities) {
      if (e.value.toLowerCase().includes(ql) || (e.display_name ?? "").toLowerCase().includes(ql)) {
        records.filter((r) => r.entityIds.includes(e.id)).forEach((r) => push(r, "entity"));
      }
    }
    // Full-text over title/body.
    for (const r of records) {
      const hay = `${r.title ?? ""} ${r.body ?? ""}`.toLowerCase();
      if (ql.split(/\s+/).every((t) => hay.includes(t))) push(r, "fts");
    }
    return hits.slice(0, limit);
  },

  async timeline(filters: SearchFilters): Promise<SearchHit[]> {
    return records
      .filter((r) => passesFilters(r, filters))
      .sort((a, b) => (b.timestamp ?? 0) - (a.timestamp ?? 0))
      .slice(0, filters.limit ?? 200)
      .map((r) => toHit(r, "recent"));
  },

  async record_detail(id: number): Promise<RecordDetail | null> {
    const r = records.find((x) => x.id === id);
    if (!r) return null;
    const { entityIds, ...row } = r;
    const ents = entityIds.map((eid) => ({ ...entityById(eid), role: "mentioned" }));
    const related = records
      .filter((x) => x.id !== id && x.entityIds.some((e) => entityIds.includes(e)))
      .sort((a, b) => (b.timestamp ?? 0) - (a.timestamp ?? 0))
      .map(({ entityIds: _e, ...rr }) => rr);
    return {
      record: row,
      raw: { platform: r.platform, kind: r.kind, title: r.title, body: r.body },
      entities: ents,
      related_records: related,
    };
  },

  async correlation(entityId: number): Promise<Correlation | null> {
    const e = entities.find((x) => x.id === entityId);
    if (!e) return null;
    const recs = records.filter((r) => r.entityIds.includes(entityId));
    const platforms = [...new Set(recs.map((r) => r.platform))].sort();
    const relatedMap = new Map<number, number>();
    for (const r of recs) {
      for (const oid of r.entityIds) {
        if (oid !== entityId) relatedMap.set(oid, (relatedMap.get(oid) ?? 0) + 1);
      }
    }
    const related_entities = [...relatedMap.entries()]
      .map(([id, shared]) => ({ entity: entityById(id), shared_records: shared }))
      .sort((a, b) => b.shared_records - a.shared_records);
    return {
      entity: e,
      platforms,
      related_entities,
      records: recs.map(({ entityIds: _e, ...rr }) => rr).sort((a, b) => (b.timestamp ?? 0) - (a.timestamp ?? 0)),
      total_records: recs.length,
    };
  },

  async identity_clusters(): Promise<IdentityCluster[]> {
    // Union-find over strong identifiers that co-occur in a record.
    const strong = new Set(["person", "username", "phone", "email", "device_id"]);
    const parent = new Map<number, number>();
    const ids = entities.filter((e) => strong.has(e.kind)).map((e) => e.id);
    ids.forEach((i) => parent.set(i, i));
    const find = (x: number): number => {
      while (parent.get(x)! !== x) x = parent.get(x)!;
      return x;
    };
    const union = (a: number, b: number) => {
      if (!parent.has(a) || !parent.has(b)) return;
      parent.set(find(a), find(b));
    };
    for (const r of records) {
      const strongIds = r.entityIds.filter((id) => strong.has(entityById(id).kind));
      for (let i = 0; i < strongIds.length; i++)
        for (let j = i + 1; j < strongIds.length; j++) union(strongIds[i], strongIds[j]);
    }
    const groups = new Map<number, Entity[]>();
    for (const id of ids) {
      const root = find(id);
      if (!groups.has(root)) groups.set(root, []);
      groups.get(root)!.push(entityById(id));
    }
    const clusters: IdentityCluster[] = [];
    for (const members of groups.values()) {
      if (members.length < 2) continue;
      const recSet = new Set<number>();
      records.forEach((r) => {
        if (r.entityIds.some((e) => members.some((m) => m.id === e))) recSet.add(r.id);
      });
      const label =
        members.find((m) => m.kind === "person")?.value ??
        members.find((m) => m.kind === "username")?.value ??
        members[0].value;
      clusters.push({ label, members, record_count: recSet.size });
    }
    return clusters.sort((a, b) => b.record_count - a.record_count);
  },

  async stats(): Promise<Stats> {
    const count = (arr: string[]) => {
      const m = new Map<string, number>();
      arr.forEach((k) => m.set(k, (m.get(k) ?? 0) + 1));
      return [...m.entries()].map(([key, c]) => ({ key, count: c })).sort((a, b) => b.count - a.count);
    };
    const ts = records.map((r) => r.timestamp).filter((t): t is number => t != null);
    return {
      sources: sources.length,
      records: records.length,
      entities: entities.length,
      by_platform: count(records.map((r) => r.platform)),
      by_kind: count(records.map((r) => r.kind)),
      by_entity_kind: count(entities.map((e) => e.kind)),
      earliest: ts.length ? Math.min(...ts) : null,
      latest: ts.length ? Math.max(...ts) : null,
    };
  },

  async activity(): Promise<DayCount[]> {
    const m = new Map<number, number>();
    for (const r of records) {
      if (r.timestamp == null) continue;
      const day = Math.floor(r.timestamp / 86400) * 86400;
      m.set(day, (m.get(day) ?? 0) + 1);
    }
    return [...m.entries()].map(([day, count]) => ({ day, count })).sort((a, b) => a.day - b.day);
  },
};
