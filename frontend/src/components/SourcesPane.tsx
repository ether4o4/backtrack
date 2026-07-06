import { useState } from "react";
import type { Entity, Source } from "../types";
import { ENTITY_KINDS, entityIcon, platformColor } from "../util";

export type View = "timeline" | "search" | "entities" | "clusters" | "graph" | "stats";

const NAV: { id: View; label: string; glyph: string }[] = [
  { id: "timeline", label: "Timeline", glyph: "≡" },
  { id: "search", label: "Search", glyph: "⌕" },
  { id: "entities", label: "Entities", glyph: "◉" },
  { id: "clusters", label: "Identities", glyph: "⧉" },
  { id: "graph", label: "Relationships", glyph: "⁂" },
  { id: "stats", label: "Overview", glyph: "▤" },
];

export function SourcesPane(props: {
  view: View;
  setView: (v: View) => void;
  sources: Source[];
  entities: Entity[];
  platform: string | null;
  setPlatform: (p: string | null) => void;
  entityKind: string | null;
  setEntityKind: (k: string | null) => void;
  onImport: (path: string) => Promise<unknown>;
  onPickImport: () => Promise<unknown>;
  mode: "desktop" | "demo";
}) {
  const [drag, setDrag] = useState(false);
  const [pathInput, setPathInput] = useState("");
  const [msg, setMsg] = useState<string | null>(null);

  const platforms = Array.from(new Set(props.sources.map((s) => s.platform)));

  async function importPath(path: string) {
    if (!path.trim()) return;
    setMsg("Importing…");
    try {
      await props.onImport(path.trim());
      setMsg(null);
      setPathInput("");
    } catch (e) {
      setMsg(String(e));
    }
  }

  return (
    <div className="pane left">
      <div className="pane-section">
        <div className="nav">
          {NAV.map((n) => (
            <button
              key={n.id}
              className={props.view === n.id ? "active" : ""}
              onClick={() => props.setView(n.id)}
            >
              <span className="k" aria-hidden>{n.glyph}</span>
              {n.label}
            </button>
          ))}
        </div>
      </div>

      <div className="pane-section">
        <div className="pane-title">Import</div>
        <div
          className={`dropzone ${drag ? "drag" : ""}`}
          onDragOver={(e) => {
            e.preventDefault();
            setDrag(true);
          }}
          onDragLeave={() => setDrag(false)}
          onDrop={(e) => {
            e.preventDefault();
            setDrag(false);
            // In the browser we can't read absolute paths; the desktop shell
            // wires real OS drag-drop via Tauri's window drag-drop event.
            const f = e.dataTransfer.files?.[0];
            if (f) importPath((f as unknown as { path?: string }).path ?? f.name);
          }}
        >
          Drag & drop folders, ZIPs, or files
        </div>
        <div style={{ display: "flex", gap: 6, marginTop: 8 }}>
          <input
            className="chip"
            style={{ flex: 1, background: "var(--bg-3)" }}
            placeholder="or paste a path…"
            value={pathInput}
            onChange={(e) => setPathInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && importPath(pathInput)}
          />
        </div>
        <button className="import-btn" style={{ marginTop: 8 }} onClick={() => props.onPickImport()}>
          + Choose files to import
        </button>
        {pathInput.trim() && (
          <button
            className="chip"
            style={{ width: "100%", marginTop: 6, padding: 8 }}
            onClick={() => importPath(pathInput)}
          >
            Import from pasted path
          </button>
        )}
        {props.mode === "demo" && (
          <div style={{ color: "var(--text-faint)", fontSize: 11, marginTop: 7, lineHeight: 1.4 }}>
            Viewing bundled demo data. Run the desktop app to import your own exports.
          </div>
        )}
        {msg && <div style={{ color: "var(--danger)", fontSize: 11, marginTop: 6 }}>{msg}</div>}
      </div>

      <div className="pane-section">
        <div className="pane-title">Sources</div>
        <div className="source-item" onClick={() => props.setPlatform(null)}>
          <span className="dot" style={{ background: "#666" }} />
          <span className="name" style={{ fontWeight: props.platform === null ? 700 : 400 }}>
            All platforms
          </span>
          <span className="n">{props.sources.reduce((a, s) => a + s.record_count, 0)}</span>
        </div>
        {platforms.map((p) => {
          const recs = props.sources.filter((s) => s.platform === p).reduce((a, s) => a + s.record_count, 0);
          return (
            <div
              key={p}
              className="source-item"
              onClick={() => props.setPlatform(props.platform === p ? null : p)}
            >
              <span className="dot" style={{ background: platformColor(p) }} />
              <span className="name" style={{ fontWeight: props.platform === p ? 700 : 400 }}>{p}</span>
              <span className="n">{recs}</span>
            </div>
          );
        })}
      </div>

      <div className="pane-section">
        <div className="pane-title">Filter entities</div>
        <div className="chip-row">
          <button
            className={`chip ${props.entityKind === null ? "active" : ""}`}
            onClick={() => props.setEntityKind(null)}
          >
            all
          </button>
          {ENTITY_KINDS.map((k) => (
            <button
              key={k}
              className={`chip ${props.entityKind === k ? "active" : ""}`}
              onClick={() => props.setEntityKind(props.entityKind === k ? null : k)}
            >
              <span aria-hidden>{entityIcon(k)}</span> {k.replace("_", " ")}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
