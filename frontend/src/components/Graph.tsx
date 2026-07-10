import type { Entity, IdentityCluster } from "../types";
import { entityIcon, platformColor } from "../util";

// A deterministic relationship graph: each inferred identity cluster is drawn
// as a star — a central identity node with its member identifiers orbiting it,
// connected by edges. Layout is computed geometrically (no physics sim) so it
// renders identically every time and needs no animation loop.

const KIND_COLOR: Record<string, string> = {
  person: "#5b8cff",
  phone: "#34c759",
  email: "#5e5ce6",
  username: "#c13584",
  device_id: "#ff9f0a",
  location: "#ff375f",
  ip: "#af52de",
};

export function RelationshipGraph(props: {
  entities: Entity[];
  clusters?: IdentityCluster[];
  onOpenEntity: (id: number) => void;
}) {
  const clusters = props.clusters ?? [];
  if (clusters.length === 0) {
    return (
      <div className="empty">
        <div className="big" aria-hidden>⁂</div>
        No linked identities to graph yet. Import more overlapping sources.
      </div>
    );
  }

  const W = 720;
  const perRow = Math.min(3, clusters.length);
  const cellW = W / perRow;
  const cellH = 250;
  const rows = Math.ceil(clusters.length / perRow);
  const H = rows * cellH;

  return (
    <div className="graph-wrap">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Relationship graph">
        {clusters.map((c, i) => {
          const cx = (i % perRow) * cellW + cellW / 2;
          const cy = Math.floor(i / perRow) * cellH + cellH / 2;
          const radius = 78;
          const members = c.members.slice(0, 10);
          return (
            <g key={i}>
              {members.map((m, j) => {
                const a = (j / members.length) * Math.PI * 2 - Math.PI / 2;
                const x = cx + Math.cos(a) * radius;
                const y = cy + Math.sin(a) * radius;
                return <line key={`e${m.id}`} className="edge" x1={cx} y1={cy} x2={x} y2={y} />;
              })}
              {/* center */}
              <circle cx={cx} cy={cy} r={16} fill="#1b1e26" stroke="#5b8cff" strokeWidth={1.6} />
              <text x={cx} y={cy + 30} textAnchor="middle" className="node-label" style={{ fontWeight: 700 }}>
                {trunc(c.label, 16)}
              </text>
              {members.map((m, j) => {
                const a = (j / members.length) * Math.PI * 2 - Math.PI / 2;
                const x = cx + Math.cos(a) * radius;
                const y = cy + Math.sin(a) * radius;
                return (
                  <g key={m.id} style={{ cursor: "pointer" }} onClick={() => props.onOpenEntity(m.id)}>
                    <circle cx={x} cy={y} r={9} fill={KIND_COLOR[m.kind] ?? platformColor(m.kind)} />
                    <title>{`${m.kind}: ${m.value}`}</title>
                    <text x={x} y={y - 13} textAnchor="middle" className="node-label">
                      {entityIcon(m.kind)} {trunc(m.display_name ?? m.value, 14)}
                    </text>
                  </g>
                );
              })}
            </g>
          );
        })}
      </svg>
    </div>
  );
}

function trunc(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}
