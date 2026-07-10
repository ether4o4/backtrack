// Types mirroring the serde shapes returned by crosstrace-core over IPC.

export interface Source {
  id: number;
  name: string;
  path: string;
  kind: string;
  platform: string;
  record_count: number;
  imported_at: number;
}

export interface RecordRow {
  id: number;
  source_id: number;
  kind: string;
  platform: string;
  timestamp: number | null;
  title: string | null;
  body: string | null;
}

export interface SearchHit extends RecordRow {
  matched_via: string;
}

export interface Entity {
  id: number;
  kind: string;
  value: string;
  display_name: string | null;
  record_count: number;
}

export interface EntityRef extends Entity {
  role: string;
}

export interface RecordDetail {
  record: RecordRow;
  raw: unknown;
  entities: EntityRef[];
  related_records: RecordRow[];
}

export interface RelatedEntity {
  entity: Entity;
  shared_records: number;
}

export interface Correlation {
  entity: Entity;
  platforms: string[];
  related_entities: RelatedEntity[];
  records: RecordRow[];
  total_records: number;
}

export interface IdentityCluster {
  label: string;
  members: Entity[];
  record_count: number;
}

export interface Bucket {
  key: string;
  count: number;
}

export interface Stats {
  sources: number;
  records: number;
  entities: number;
  by_platform: Bucket[];
  by_kind: Bucket[];
  by_entity_kind: Bucket[];
  earliest: number | null;
  latest: number | null;
}

export interface DayCount {
  day: number;
  count: number;
}

export interface ImportSummary {
  sources_added: number;
  records_added: number;
  entities_added: number;
  files_skipped: number;
  errors: string[];
}

export interface SearchFilters {
  platform?: string | null;
  kind?: string | null;
  after?: number | null;
  before?: number | null;
  limit?: number | null;
}
