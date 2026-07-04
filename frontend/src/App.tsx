import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import type {
  Correlation,
  Entity,
  IdentityCluster,
  RecordDetail,
  SearchHit,
  Source,
  Stats,
  DayCount,
} from "./types";
import { SourcesPane, type View } from "./components/SourcesPane";
import { ResultsPane } from "./components/ResultsPane";
import { DetailsPane } from "./components/DetailsPane";

export function App() {
  const mode = api.mode();

  // Query + filter state.
  const [query, setQuery] = useState("");
  const [platform, setPlatform] = useState<string | null>(null);
  const [entityKind, setEntityKind] = useState<string | null>(null);
  const [view, setView] = useState<View>("timeline");

  // Loaded data.
  const [sources, setSources] = useState<Source[]>([]);
  const [results, setResults] = useState<SearchHit[]>([]);
  const [entities, setEntities] = useState<Entity[]>([]);
  const [clusters, setClusters] = useState<IdentityCluster[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [activity, setActivity] = useState<DayCount[]>([]);

  // Selection / right pane.
  const [detail, setDetail] = useState<RecordDetail | null>(null);
  const [correlation, setCorrelation] = useState<Correlation | null>(null);
  const [selectedRecord, setSelectedRecord] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  const filters = useMemo(() => ({ platform, limit: 300 }), [platform]);

  // ---- Loaders ---------------------------------------------------------
  const reloadSidebar = useCallback(async () => {
    const [s, e] = await Promise.all([api.listSources(), api.listEntities(null)]);
    setSources(s);
    setEntities(e);
  }, []);

  const runQuery = useCallback(async () => {
    setBusy(true);
    try {
      if (query.trim()) {
        setResults(await api.search(query, filters));
      } else {
        setResults(await api.timeline(filters));
      }
    } finally {
      setBusy(false);
    }
  }, [query, filters]);

  const loadView = useCallback(
    async (v: View) => {
      setBusy(true);
      try {
        if (v === "entities") setEntities(await api.listEntities(entityKind));
        else if (v === "clusters" || v === "graph") setClusters(await api.identityClusters());
        else if (v === "stats") {
          setStats(await api.stats());
          setActivity(await api.activity());
        } else await runQuery();
      } finally {
        setBusy(false);
      }
    },
    [entityKind, runQuery]
  );

  useEffect(() => {
    reloadSidebar();
  }, [reloadSidebar]);

  useEffect(() => {
    loadView(view);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view, query, platform, entityKind]);

  // ⌘K / Ctrl-K focuses search.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // ---- Selection -------------------------------------------------------
  const openRecord = useCallback(async (id: number) => {
    setSelectedRecord(id);
    setCorrelation(null);
    setDetail(await api.recordDetail(id));
  }, []);

  const openEntity = useCallback(async (id: number) => {
    setDetail(null);
    setSelectedRecord(null);
    setCorrelation(await api.correlation(id));
  }, []);

  // From the assistant / details "search this" affordances.
  const searchFor = useCallback((q: string) => {
    setQuery(q);
    setView("search");
    setPlatform(null);
  }, []);

  // ---- Import (desktop only) ------------------------------------------
  const doImport = useCallback(
    async (path: string) => {
      setBusy(true);
      try {
        const s = await api.importPath(path);
        await reloadSidebar();
        await loadView(view);
        return s;
      } finally {
        setBusy(false);
      }
    },
    [reloadSidebar, loadView, view]
  );

  return (
    <div className="app">
      <TopBar
        mode={mode}
        query={query}
        setQuery={setQuery}
        onEnter={() => setView(query.trim() ? "search" : "timeline")}
        inputRef={searchRef}
      />

      <div className="panes">
        <SourcesPane
          view={view}
          setView={setView}
          sources={sources}
          entities={entities}
          platform={platform}
          setPlatform={setPlatform}
          entityKind={entityKind}
          setEntityKind={setEntityKind}
          onImport={doImport}
          mode={mode}
        />

        <ResultsPane
          view={view}
          busy={busy}
          query={query}
          results={results}
          entities={entities}
          clusters={clusters}
          stats={stats}
          activity={activity}
          selectedRecord={selectedRecord}
          onOpenRecord={openRecord}
          onOpenEntity={openEntity}
        />

        <DetailsPane
          detail={detail}
          correlation={correlation}
          onOpenRecord={openRecord}
          onOpenEntity={openEntity}
          onSearch={searchFor}
        />
      </div>

      <StatusBar mode={mode} sources={sources.length} results={results.length} busy={busy} />
    </div>
  );
}

function TopBar(props: {
  mode: "desktop" | "demo";
  query: string;
  setQuery: (s: string) => void;
  onEnter: () => void;
  inputRef: React.RefObject<HTMLInputElement>;
}) {
  return (
    <div className="topbar">
      <div className="brand">
        <span className="logo" aria-hidden>
          <Logo />
        </span>
        CrossTrace <small>· cross-reference your own data</small>
      </div>
      <div className="searchbar">
        <span className="ico" aria-hidden>⌕</span>
        <input
          ref={props.inputRef}
          value={props.query}
          placeholder="Search anything — a name, number, email, place, date, device…"
          onChange={(e) => props.setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && props.onEnter()}
        />
        <kbd>⌘K</kbd>
      </div>
      <span className={`mode-badge ${props.mode}`}>
        {props.mode === "desktop" ? "● Local engine" : "◐ Demo data"}
      </span>
    </div>
  );
}

function StatusBar(props: { mode: string; sources: number; results: number; busy: boolean }) {
  return (
    <div className="statusbar">
      <span className="g">● Offline</span>
      <span>Local encrypted store</span>
      <span>{props.sources} sources</span>
      <span>{props.results} shown</span>
      <span style={{ marginLeft: "auto" }}>
        {props.busy ? <span className="spin">indexing…</span> : "ready"}
      </span>
    </div>
  );
}

function Logo() {
  return (
    <svg width="22" height="22" viewBox="0 0 22 22" fill="none">
      <circle cx="11" cy="11" r="9" stroke="#5b8cff" strokeWidth="1.6" />
      <circle cx="11" cy="11" r="2.2" fill="#7c5cff" />
      <path d="M11 2v4M11 16v4M2 11h4M16 11h4" stroke="#5b8cff" strokeWidth="1.4" strokeLinecap="round" />
      <path d="M4.6 4.6l2.8 2.8M14.6 14.6l2.8 2.8M17.4 4.6l-2.8 2.8M7.4 14.6l-2.8 2.8" stroke="#3a4666" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}
