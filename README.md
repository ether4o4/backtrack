# CrossTrace

**Import once, normalize everything, correlate automatically.**

CrossTrace is an offline-first analytics desktop app that lets you import your
own exported data from many platforms and cross-reference it to reveal
timelines, relationships, recurring identifiers, and patterns — all in one
fast, searchable interface. Nothing leaves your machine.

> CrossTrace is designed for exploring **your own** exported data (Facebook,
> Instagram, SMS backups, contacts, call logs, GPS history, and so on). It runs
> fully offline against a local store.

![Timeline view](docs/screenshots/timeline.png)

## What it does

- **Import** folders, files, or ZIPs (read in place, no extraction). Platform
  and file type are auto-detected.
- **Normalize** every source into one local SQLite store with full-text search.
- **Extract** identifiers — people, phone numbers, emails, usernames, device
  IDs, IPs, locations, file hashes — and de-duplicate them into entities.
- **Correlate** automatically: records that share an entity are linked, and
  entities that co-occur are clustered into inferred identities.
- **Explore** through a three-pane dark UI: a chronological cross-platform
  timeline, universal Google-like search, an entity list by frequency, a
  relationship graph, and an import overview with charts and an activity
  heatmap.
- **Ask** (optional) a built-in assistant natural-language questions; it
  translates them into queries over your data and **cites the exact records** —
  it never invents information. The assistant is off by default and can be
  toggled from the top bar; every feature above works fully without it, and it
  makes no network calls (it is a local, deterministic query translator).

## Architecture

CrossTrace is split so the engine can be tested in isolation from the GUI:

| Layer | Path | Role |
|-------|------|------|
| **Core engine** | [`core/`](core) | Pure Rust. Ingest, parsing, entity extraction, storage, search, correlation, clustering, stats. No GUI deps — fully unit-tested. |
| **Desktop shell** | [`src-tauri/`](src-tauri) | Thin Tauri (v2) layer exposing the core over IPC commands. |
| **Frontend** | [`frontend/`](frontend) | React + TypeScript + Vite three-pane UI. |
| **CLI** | [`core/src/bin/crosstrace.rs`](core/src/bin/crosstrace.rs) | A command-line front end over the same engine (runs without the GUI). |

See [`docs/architecture.md`](docs/architecture.md) for the data model and the
correlation strategy.

## Quick start

### Run the engine from the command line

```bash
cd core
cargo run --bin crosstrace -- crosstrace.db import /path/to/your/exports
cargo run --bin crosstrace -- crosstrace.db search "555-123-4567"
cargo run --bin crosstrace -- crosstrace.db timeline
cargo run --bin crosstrace -- crosstrace.db clusters
cargo run --bin crosstrace -- crosstrace.db stats
```

### Run the tests

```bash
cd core && cargo test
```

### Run the desktop app

Requires the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
(a C toolchain and, on Linux, `webkit2gtk`).

```bash
cd frontend && npm install
cd ../src-tauri && cargo tauri dev      # or: npm --prefix ../frontend run tauri dev
```

### Build the Android app

The same Rust engine and React UI run on Android via Tauri v2 mobile. The UI is
responsive — the three panes collapse into a single-pane layout with a bottom
tab bar on phones, and import uses the Android file picker (content URIs are
read to bytes and handed to the engine, so no filesystem path is needed).

A **debug-signed APK** is built by the `Build Android APK` GitHub Actions
workflow (this repo has no Android SDK locally, so it builds on CI). Every push
to `main` re-publishes it to a rolling **`nightly`** release — direct download,
no zip to unpack:

[![Download APK](https://img.shields.io/badge/Download-CrossTrace%20APK-3b82f6?logo=android&logoColor=white&style=for-the-badge)](https://github.com/ether4o4/backtrack/releases/download/nightly/backtrack-debug.apk)

**[Download the latest APK](https://github.com/ether4o4/backtrack/releases/download/nightly/backtrack-debug.apk)**

> Debug-signed. Enable *Install unknown apps* for your browser or file manager,
> then open the APK.

You can also grab the APK from the workflow run's **Artifacts** if you need a
specific build. Locally, with the Android SDK + NDK installed:

```bash
cd src-tauri
cargo tauri android init
cargo tauri android build --apk --debug
```

### Preview the UI in a browser (no backend)

The frontend ships with a small bundled demo dataset and an in-browser engine,
so you can explore the interface without building the desktop shell:

```bash
cd frontend && npm install && npm run dev
```

## Supported formats (MVP)

CSV / TSV · JSON (generic, Facebook/Instagram `{participants, messages}` exports,
and conversation-map exports like Snapchat's contact-keyed message lists) ·
HTML (Facebook/Instagram/Snapchat "download your data" archives — text layer
only, split into dated entries where timestamps are present) · PDF (text-layer
extraction; scanned/image-only PDFs yield nothing) · vCard (`.vcf`) contacts ·
SMS Backup & Restore XML · ZIP archives of any of the above.

Known limitation: some Facebook HTML exports contain mojibake from a
long-standing double-UTF-8-encoding bug on Facebook's side (e.g. "don't"
appears as "donâ€™t"). No automatic repair is attempted, since a naive fix
risks corrupting genuinely non-English text — affected text passes through
as-is rather than being silently "fixed" incorrectly.

The parser layer is a plugin surface — new formats implement one function and
register in the ingest dispatcher (see the architecture doc).

## Privacy

- Offline by default; no network calls.
- Local store under your OS app-data directory.
- Every processing step is written to an append-only audit log.

## License

MIT
