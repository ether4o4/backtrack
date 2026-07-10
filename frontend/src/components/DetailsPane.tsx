import { useState } from "react";
import type { Correlation, RecordDetail } from "../types";
import { entityIcon, fmtTime, platformColor } from "../util";
import { ask, SUGGESTED_PROMPTS, type AiAnswer } from "../assistant";

export function DetailsPane(props: {
  detail: RecordDetail | null;
  correlation: Correlation | null;
  assistantEnabled: boolean;
  onOpenRecord: (id: number) => void;
  onOpenEntity: (id: number) => void;
  onSearch: (q: string) => void;
}) {
  return (
    <div className="pane right">
      {props.correlation ? (
        <CorrelationView c={props.correlation} onOpenEntity={props.onOpenEntity} onSearch={props.onSearch} />
      ) : props.detail ? (
        <DetailView d={props.detail} onOpenRecord={props.onOpenRecord} onOpenEntity={props.onOpenEntity} />
      ) : (
        <div className="detail-empty">
          Select a record or entity to see its connections{props.assistantEnabled ? ", or ask the assistant below" : ""}.
        </div>
      )}

      {/* The assistant is an optional convenience. Everything the app does is
          reachable without it — it is off unless explicitly enabled. */}
      {props.assistantEnabled && (
        <>
          <div style={{ borderTop: "1px solid var(--border)", marginTop: "auto" }} />
          <Assistant onOpenRecord={props.onOpenRecord} />
        </>
      )}
    </div>
  );
}

function DetailView(props: {
  d: RecordDetail;
  onOpenRecord: (id: number) => void;
  onOpenEntity: (id: number) => void;
}) {
  const { d } = props;
  const r = d.record;
  return (
    <div className="detail">
      <h3>{r.title ?? "(untitled record)"}</h3>
      <div className="dsub">
        <span className="pill" style={{ background: platformColor(r.platform) }}>{r.platform}</span>{" "}
        {r.kind} · {fmtTime(r.timestamp)}
      </div>

      {r.body && (
        <div style={{ background: "var(--bg-2)", borderRadius: 8, padding: 10, fontSize: 13, lineHeight: 1.5, marginBottom: 8 }}>
          {r.body}
        </div>
      )}

      <div className="section-h">Entities in this record</div>
      {d.entities.length === 0 && <div style={{ color: "var(--text-faint)", fontSize: 12 }}>None extracted.</div>}
      {d.entities.map((e) => (
        <div key={e.id + e.role} className="ent-link" onClick={() => props.onOpenEntity(e.id)}>
          <span className="g" aria-hidden>{entityIcon(e.kind)}</span>
          <span>{e.display_name ?? e.value}</span>
          <span className="r">{e.kind.replace("_", " ")}</span>
        </div>
      ))}

      {d.related_records.length > 0 && (
        <>
          <div className="section-h">Connected records ({d.related_records.length})</div>
          <div className="related">
            {d.related_records.slice(0, 12).map((rr) => (
              <div key={rr.id} className="rrec" onClick={() => props.onOpenRecord(rr.id)}>
                <span className="pill" style={{ background: platformColor(rr.platform), fontSize: 9 }}>{rr.platform}</span>{" "}
                {rr.title ?? rr.body?.slice(0, 40)}
              </div>
            ))}
          </div>
        </>
      )}

      <div className="section-h">Raw record</div>
      <pre className="raw">{JSON.stringify(d.raw, null, 2)}</pre>
    </div>
  );
}

function CorrelationView(props: {
  c: Correlation;
  onOpenEntity: (id: number) => void;
  onSearch: (q: string) => void;
}) {
  const { c } = props;
  return (
    <div className="detail">
      <h3>
        <span aria-hidden>{entityIcon(c.entity.kind)}</span> {c.entity.display_name ?? c.entity.value}
      </h3>
      <div className="dsub">
        {c.entity.kind.replace("_", " ")} · appears in {c.total_records} records
        <span
          style={{ color: "var(--accent)", cursor: "pointer", marginLeft: 8 }}
          onClick={() => props.onSearch(c.entity.value)}
        >
          search ↗
        </span>
      </div>

      <div className="section-h">Seen on</div>
      <div className="members">
        {c.platforms.map((p) => (
          <span key={p} className="mtag">
            <span className="dot" style={{ background: platformColor(p) }} /> {p}
          </span>
        ))}
      </div>

      <div className="section-h">Connected to</div>
      {c.related_entities.length === 0 && <div style={{ color: "var(--text-faint)", fontSize: 12 }}>No co-occurring entities.</div>}
      {c.related_entities.map((re) => (
        <div key={re.entity.id} className="ent-link" onClick={() => props.onOpenEntity(re.entity.id)}>
          <span className="g" aria-hidden>{entityIcon(re.entity.kind)}</span>
          <span>{re.entity.display_name ?? re.entity.value}</span>
          <span className="r">shared ×{re.shared_records}</span>
        </div>
      ))}
    </div>
  );
}

function Assistant(props: { onOpenRecord: (id: number) => void }) {
  const [q, setQ] = useState("");
  const [ans, setAns] = useState<AiAnswer | null>(null);
  const [busy, setBusy] = useState(false);

  async function run(question: string) {
    if (!question.trim()) return;
    setBusy(true);
    try {
      setAns(await ask(question));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="detail">
      <div className="section-h" style={{ marginTop: 4 }}>◆ Assistant</div>
      <div className="assistant">
        <div className="prompts">
          {SUGGESTED_PROMPTS.map((p) => (
            <button key={p} className="chip" onClick={() => { setQ(p); run(p); }}>{p}</button>
          ))}
        </div>
        <div className="ask">
          <input
            value={q}
            placeholder="Ask about your data…"
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && run(q)}
          />
          <button onClick={() => run(q)}>{busy ? "…" : "Ask"}</button>
        </div>

        {ans && (
          <div className="ai-answer">
            <div>{ans.text}</div>
            {ans.citations.length > 0 && (
              <div style={{ marginTop: 8 }}>
                {ans.citations.map((r) => (
                  <span key={r.id} className="cite" title={r.body ?? ""} onClick={() => props.onOpenRecord(r.id)}>
                    #{r.id} {r.platform}
                  </span>
                ))}
              </div>
            )}
            <div className="interp">interpreted as → {ans.interpretation}</div>
          </div>
        )}
      </div>
    </div>
  );
}
