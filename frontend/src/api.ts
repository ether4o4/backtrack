// Unified data access. Detects whether we are running inside the Tauri
// desktop shell; if so, every call is dispatched to the Rust core over IPC.
// Otherwise it transparently falls back to the in-browser demo engine so the
// UI is fully explorable without the backend.

import type {
  Correlation,
  DayCount,
  Entity,
  IdentityCluster,
  ImportSummary,
  RecordDetail,
  SearchFilters,
  SearchHit,
  Source,
  Stats,
} from "./types";
import { demo } from "./demo";

// Tauri v2 injects `__TAURI_INTERNALS__` on the window.
export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // Imported lazily so the module also loads in a plain browser where the
  // Tauri API's internals are absent.
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const api = {
  mode(): "desktop" | "demo" {
    return isTauri() ? "desktop" : "demo";
  },

  importPath(path: string): Promise<ImportSummary> {
    return isTauri() ? invoke("import", { path }) : demo.import();
  },

  /**
   * Cross-platform import that works on both desktop and Android: opens the
   * OS file picker, reads each chosen file into bytes, and hands the bytes to
   * the backend. On Android the picker returns content URIs (not filesystem
   * paths), so reading to bytes here is what makes mobile import possible.
   * Returns null if the user cancelled.
   */
  async importViaPicker(): Promise<ImportSummary[] | null> {
    if (!isTauri()) return demo.import();
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { readFile } = await import("@tauri-apps/plugin-fs");
    const selection = await open({
      multiple: true,
      title: "Choose export files to import",
    });
    if (!selection) return null;
    const paths = Array.isArray(selection) ? selection : [selection];
    const summaries: ImportSummary[] = [];
    for (const path of paths) {
      const bytes = await readFile(path);
      const name = path.split(/[\\/]/).pop() || path;
      const summary = await invoke<ImportSummary>("import_blob", {
        name,
        bytes: Array.from(bytes),
      });
      summaries.push(summary);
    }
    return summaries;
  },
  search(query: string, filters: SearchFilters = {}): Promise<SearchHit[]> {
    return isTauri() ? invoke("search", { query, filters }) : demo.search(query, filters);
  },
  timeline(filters: SearchFilters = {}): Promise<SearchHit[]> {
    return isTauri() ? invoke("timeline", { filters }) : demo.timeline(filters);
  },
  listSources(): Promise<Source[]> {
    return isTauri() ? invoke("list_sources") : demo.list_sources();
  },
  listEntities(kind?: string | null): Promise<Entity[]> {
    return isTauri() ? invoke("list_entities", { kind: kind ?? null }) : demo.list_entities(kind);
  },
  correlation(entityId: number): Promise<Correlation | null> {
    return isTauri() ? invoke("correlation", { entityId }) : demo.correlation(entityId);
  },
  recordDetail(recordId: number): Promise<RecordDetail | null> {
    return isTauri() ? invoke("record_detail", { recordId }) : demo.record_detail(recordId);
  },
  identityClusters(): Promise<IdentityCluster[]> {
    return isTauri() ? invoke("identity_clusters") : demo.identity_clusters();
  },
  stats(): Promise<Stats> {
    return isTauri() ? invoke("stats") : demo.stats();
  },
  activity(): Promise<DayCount[]> {
    return isTauri() ? invoke("activity") : demo.activity();
  },
};
