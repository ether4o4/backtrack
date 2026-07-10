# CrossTrace architecture

CrossTrace follows one principle: **import once, normalize everything,
correlate automatically.** The system is layered so the engine is independent
of, and testable without, any UI.

```
             ┌────────────────────────────────────────────┐
  files/     │  ingest  →  detect  →  parse  →  normalize  │
  folders/   │                                     │       │
  zips  ─────▶                                      ▼       │
             │        extract identifiers   ┌──────────────┐│
             │                              │  SQLite store ││   core/  (pure Rust)
             │  search ◀── correlate ◀──────│  records      ││
             │  stats  ◀── cluster   ◀──────│  entities     ││
             └──────────────────────────────┴──────────────┴┘
                          ▲                        ▲
              IPC commands │                        │ CLI
             ┌─────────────┴──────┐        ┌────────┴─────────┐
             │  src-tauri (shell) │        │ crosstrace (bin) │
             └─────────┬──────────┘        └──────────────────┘
                       │
             ┌─────────▼──────────┐
             │ frontend (React)   │  three-pane dark UI
             └────────────────────┘
```

## Data model

Everything imported from any platform is normalized into a single store
(`core/src/schema.sql`):

- **sources** — one row per imported file or archive member (name, path,
  detected kind, platform, record count).
- **records** — the normalized unit: a message, call, contact, location fix,
  row, etc. Each has a coarse `kind`, a `platform`, an optional unix
  `timestamp`, a `title`, a searchable `body`, and the original `raw` payload
  preserved verbatim.
- **records_fts** — an FTS5 external-content index over title/body/platform,
  kept in sync by triggers. This is the full-text search backend.
- **entities** — de-duplicated identifiers, unique on `(kind, value)`. Kinds:
  person, username, phone, email, device_id, cookie, session_id, location, ip,
  file_hash, url.
- **record_entities** — the many-to-many link (with a role) between records
  and entities. This table is what the correlation engine walks.
- **audit_log** — append-only record of processing steps (privacy requirement).

De-duplicating entities on `(kind, value)` is what makes the interesting
queries cheap: the same phone number appearing in a thousand records is *one*
row referenced a thousand times, so "contact frequency", correlation, and
identity clustering are all simple joins.

## Ingest & parsing

`ingest::import_path` walks a dropped path:

- a **directory** is walked recursively;
- a **`.zip`** is read member-by-member *without extracting to disk*;
- a **file** is read directly.

Each file is auto-detected (`ingest::detect`) by extension first, then a
content sniff, with a platform hint drawn from the path (export folders are
named after their platform) and known payload markers. Detection dispatches to
a parser, each of which turns bytes into `NormalizedRecord`s:

| Parser | Handles |
|--------|---------|
| `csv` | Generic CSV/TSV; sniffs header for timestamp/name/body/phone/email columns |
| `json` | Generic arrays/objects **and** Facebook/Instagram `messages` exports |
| `vcard` | `.vcf` contacts (FN / TEL / EMAIL) |
| `sms_xml` | SMS Backup & Restore XML (`<sms>` / `<mms>`) |

Adding a format is a localized change: implement a `parse(bytes, platform) ->
Vec<NormalizedRecord>` function and add one arm to the dispatcher in
`ingest/mod.rs`.

## Identifier extraction

On insert, each record's title+body is scanned (`extract::scan_text`) for
high-confidence identifiers. Extraction favours **precision over recall** so
the correlation graph stays clean:

- Emails and URLs are matched first and masked out before later scans.
- IPv4 is octet-validated.
- Phone matching is guarded against false positives: a candidate is rejected if
  it is glued to another number, is a single-dot decimal (a GPS coordinate), or
  parses as a calendar date. Numbers are normalized, and matched across formats
  by their last 10 significant digits so a local `555-123-4567` resolves to a
  stored `+15551234567`.

## Correlation & identity clustering

Two records are related when they **share an entity**; two entities are related
when they **co-occur in a record**. `correlate` surfaces, for any entity, the
platforms it appears on, the entities it co-occurs with (ranked by shared
records), and a sample of its records.

`identity_clusters` groups entities into inferred identities using union-find
over the co-occurrence relation, restricted to strong identifier kinds
(person / username / phone / email / device_id). A **hub guard** prevents an
over-connected entity — a group chat, a mailing list, or the account owner —
from bridging unrelated people into one giant cluster: entities whose
co-occurrence degree exceeds a threshold never union through.

## Search

`search` interprets one query string the way a universal search box should:

1. if it looks like a phone/email/ip/hash, match the entity exactly (phones by
   significant digits);
2. substring-match entity values (usernames, device ids, locations…);
3. full-text search over record title/body via FTS5.

Results from all three merge, de-duplicated, into one ranked list. An empty
query returns the most recent records, which is the timeline feed. Structured
filters (platform, kind, time window) apply to every path.

## Frontend data layer

The React UI talks to a single `api` module. When running inside the Tauri
shell it dispatches every call to the core over IPC; in a plain browser it
falls back to a small in-browser demo engine (`frontend/src/demo.ts`) that
mirrors the same queries, so the interface is fully explorable without the
backend. The assistant (`frontend/src/assistant.ts`) is an **optional**
convenience: a deterministic natural-language-to-query translator that always
answers from, and cites, real records. It is off by default, toggled from the
top bar, and makes no network calls — every feature of the app is fully
reachable without it.
