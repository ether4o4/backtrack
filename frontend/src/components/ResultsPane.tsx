import type {
  DayCount,
  Entity,
  IdentityCluster,
  SearchHit,
  Stats,
} from "../types";
import { entityIcon, fmtTime, platformColor } from "../util";
import type { View } from "./SourcesPane";
import { StatsView } from "./StatsView";
import { RelationshipGraph } from "./Graph";

export function ResultsPane(props: {
  view: View;
  busy: boolean;
  query: string;
  results: SearchHit[];
  entities: Entity[];
  clusters: IdentityCluster[];
  stats: Stats | null;
  activity: DayCount[];
  selectedRecord: number | null;
  onOpenRecord: (id: number) => void;
  onOpenEntity: (id: number) => void;
}) {
  const { view } = props;

  const head = (() => {
    switch (view) {
      case "timeline": return ["Timeline", `${props.results.length} events, newest first`];
      case "search": return [props.query ? `Results for “${props.query}”` : "Search", `${props.results.length} records`];
      case "entities": return ["Entities", `${props.entities.length} identifiers, by frequency`];
      case "clusters": return ["Inferred identities", `${props.clusters.length} clusters`];
      case "graph": return ["Relationship graph", "entities linked by shared records"];
      case "stats": return ["Overview", "import statistics"];
    }
  })();

  return (
    <div className="pane center">
      <div className="results-head">
        <h2>{head[0]}</h2>
        <span className="sub">{head[1]}</span>
        <span className="spacer" />
        {props.busy && <span className="spin">…</span>}
      </div>

      {view === "stats" ? (
        props.stats && <StatsView stats={props.stats} activity={props.activity} />
      ) : view === "graph" ? (
        <RelationshipGraph entities={props.entities} clusters={props.clusters} onOpenEntity={props.onOpenEntity} />
      ) : view === "entities" ? (
        <EntityList entities={props.entities} onOpenEntity={props.onOpenEntity} />
      ) : view === "clusters" ? (
        <ClusterList clusters={props.clusters} onOpenEntity={props.onOpenEntity} />
      ) : (
        <RecordList
          records={props.results}
          timeline={view === "timeline"}
          selected={props.selectedRecord}
          onOpen={props.onOpenRecord}
        />
      )}
    </div>
  );
}

function RecordList(props: {
  records: SearchHit[];
  timeline: boolean;
  selected: number | null;
  onOpen: (id: number) => void;
}) {
  if (props.records.length === 0) {
    return (
      <div className="empty">
        <div className="big" aria-hidden>⌕</div>
        Nothing here yet. Import data or adjust your search.
      </div>
    );
  }
  return (
    <div className={props.timeline ? "timeline" : ""}>
      {props.records.map((r) => (
        <div
          key={r.id}
          className={`record ${props.selected === r.id ? "sel" : ""}`}
          onClick={() => props.onOpen(r.id)}
        >
          <div className="rail">
            <span className="tick" style={{ background: platformColor(r.platform) }} />
            <span className="time">{r.timestamp ? fmtTime(r.timestamp).split(",")[1]?.trim() : ""}</span>
          </div>
          <div className="body-wrap">
            <div className="meta">
              <span className="who">{r.title ?? "(untitled)"}</span>
              <span className="pill" style={{ background: platformColor(r.platform) }}>{r.platform}</span>
              <span className="pill ghost">{r.kind}</span>
              {r.matched_via && r.matched_via !== "recent" && (
                <span className="via">via {r.matched_via}</span>
              )}
              <span className="via" style={{ marginLeft: r.matched_via && r.matched_via !== "recent" ? 6 : "auto" }}>
                {fmtTime(r.timestamp)}
              </span>
            </div>
            <div className="text">{r.body ?? ""}</div>
          </div>
        </div>
      ))}
    </div>
  );
}

function EntityList(props: { entities: Entity[]; onOpenEntity: (id: number) => void }) {
  if (props.entities.length === 0)
    return <div className="empty">No entities of this kind.</div>;
  return (
    <div>
      {props.entities.map((e) => (
        <div key={e.id} className="entity-row" onClick={() => props.onOpenEntity(e.id)}>
          <span className="entity-glyph" aria-hidden>{entityIcon(e.kind)}</span>
          <div style={{ minWidth: 0 }}>
            <div className="val">{e.display_name ?? e.value}</div>
            <div className="kind">{e.kind.replace("_", " ")}{e.display_name ? ` · ${e.value}` : ""}</div>
          </div>
          <div className="freq">
            <b>{e.record_count}</b>
            <span>records</span>
          </div>
        </div>
      ))}
    </div>
  );
}

function ClusterList(props: { clusters: IdentityCluster[]; onOpenEntity: (id: number) => void }) {
  if (props.clusters.length === 0)
    return <div className="empty">No multi-identifier identities inferred yet.</div>;
  return (
    <div>
      {props.clusters.map((c, i) => (
        <div key={i} className="cluster">
          <div className="clabel">
            <span aria-hidden>⧉</span> {c.label}
            <span className="via" style={{ marginLeft: "auto" }}>{c.record_count} records</span>
          </div>
          <div className="members">
            {c.members.map((m) => (
              <span key={m.id} className="mtag" onClick={() => props.onOpenEntity(m.id)}>
                <span aria-hidden>{entityIcon(m.kind)}</span>
                {m.display_name ?? m.value}
              </span>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
