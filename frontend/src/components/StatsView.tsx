import type { Bucket, DayCount, Stats } from "../types";
import { fmtDate, fmtDay, platformColor } from "../util";

export function StatsView(props: { stats: Stats; activity: DayCount[] }) {
  const { stats, activity } = props;
  const maxDay = Math.max(1, ...activity.map((d) => d.count));

  return (
    <div className="stats">
      <div className="stat-cards">
        <Card n={stats.records} label="records" />
        <Card n={stats.entities} label="entities" />
        <Card n={stats.sources} label="sources" />
      </div>

      <div style={{ color: "var(--text-faint)", fontSize: 12, marginBottom: 16 }}>
        Data spans {fmtDate(stats.earliest)} → {fmtDate(stats.latest)}
      </div>

      <Bars title="By platform" data={stats.by_platform} colored />
      <Bars title="By record type" data={stats.by_kind} />
      <Bars title="By entity kind" data={stats.by_entity_kind} />

      <div className="section-h" style={{ marginTop: 22 }}>Activity heatmap</div>
      <div className="heatmap">
        {activity.map((d) => (
          <span
            key={d.day}
            className="heat-cell"
            title={`${fmtDay(d.day)}: ${d.count}`}
            style={{
              background: `rgba(91,140,255,${0.15 + 0.85 * (d.count / maxDay)})`,
            }}
          />
        ))}
        {activity.length === 0 && <span style={{ color: "var(--text-faint)", fontSize: 12 }}>No timestamped records.</span>}
      </div>
    </div>
  );
}

function Card(props: { n: number; label: string }) {
  return (
    <div className="stat-card">
      <div className="n">{props.n.toLocaleString()}</div>
      <div className="l">{props.label}</div>
    </div>
  );
}

function Bars(props: { title: string; data: Bucket[]; colored?: boolean }) {
  const max = Math.max(1, ...props.data.map((b) => b.count));
  return (
    <div style={{ marginBottom: 18 }}>
      <div className="section-h">{props.title}</div>
      <div className="bars">
        {props.data.map((b) => (
          <div key={b.key} className="bar-row">
            <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{b.key}</span>
            <div className="bar-track">
              <div
                className="bar-fill"
                style={{
                  width: `${(b.count / max) * 100}%`,
                  background: props.colored ? platformColor(b.key) : "var(--accent)",
                }}
              />
            </div>
            <span className="num">{b.count}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
