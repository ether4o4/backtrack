//! `crosstrace` — a thin command-line front end over the core engine.
//!
//! It exists so the ingest/correlate/search pipeline is runnable and
//! demonstrable without the desktop GUI. The Tauri shell calls the same
//! library functions this binary does.
//!
//! Usage:
//!   crosstrace <db> import <path>          Import a file/folder/zip
//!   crosstrace <db> search <query...>      Universal search
//!   crosstrace <db> timeline [limit]       Recent records, chronological
//!   crosstrace <db> entities [kind]        List entities by frequency
//!   crosstrace <db> correlate <entity_id>  Show an entity's connections
//!   crosstrace <db> clusters               Inferred identity clusters
//!   crosstrace <db> stats                  Store statistics

use crosstrace_core::ingest::import_path;
use crosstrace_core::search::SearchFilters;
use crosstrace_core::{model::EntityKind, Store};
use std::path::Path;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        usage();
        exit(2);
    }
    let db = &args[1];
    let cmd = &args[2];
    let rest = &args[3..];

    let result = run(db, cmd, rest);
    if let Err(e) = result {
        eprintln!("error: {e}");
        exit(1);
    }
}

fn run(db: &str, cmd: &str, rest: &[String]) -> crosstrace_core::Result<()> {
    let mut store = Store::open(db)?;
    match cmd {
        "import" => {
            let path = rest.first().map(String::as_str).unwrap_or(".");
            let s = import_path(&mut store, Path::new(path))?;
            println!(
                "imported {} sources, {} records, {} new entities ({} files skipped)",
                s.sources_added, s.records_added, s.entities_added, s.files_skipped
            );
            for err in s.errors.iter().take(10) {
                eprintln!("  warn: {err}");
            }
        }
        "search" => {
            let q = rest.join(" ");
            let hits = store.search(&q, &SearchFilters::default())?;
            println!("{} results for {:?}", hits.len(), q);
            for h in hits.iter().take(50) {
                println!(
                    "  #{:<5} [{:<9}] {:<10} {}  {}",
                    h.record.id,
                    h.matched_via,
                    h.record.platform,
                    fmt_time(h.record.timestamp),
                    line(&h.record.title, &h.record.body)
                );
            }
        }
        "timeline" => {
            let limit = rest.first().and_then(|s| s.parse().ok()).unwrap_or(50);
            let hits = store.search(
                "",
                &SearchFilters {
                    limit: Some(limit),
                    ..Default::default()
                },
            )?;
            for h in hits {
                println!(
                    "  {}  [{:<10}] {}",
                    fmt_time(h.record.timestamp),
                    h.record.platform,
                    line(&h.record.title, &h.record.body)
                );
            }
        }
        "entities" => {
            let kind = rest.first().and_then(|s| EntityKind::from_str(s));
            for e in store.list_entities(kind)? {
                println!("  #{:<5} {:<10} {:<32} ×{}", e.id, e.kind, e.value, e.record_count);
            }
        }
        "correlate" => {
            let id: i64 = rest
                .first()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| crosstrace_core::Error::Parse("need entity_id".into()))?;
            match store.correlate(id, 20)? {
                None => println!("no such entity"),
                Some(c) => {
                    println!("{} ({}) — {} records", c.entity.value, c.entity.kind, c.total_records);
                    println!("  platforms: {}", c.platforms.join(", "));
                    println!("  related:");
                    for r in &c.related_entities {
                        println!("    {:<10} {:<28} shared×{}", r.entity.kind, r.entity.value, r.shared_records);
                    }
                }
            }
        }
        "clusters" => {
            for c in store.identity_clusters(2)? {
                println!("● {} — {} records, {} identifiers", c.label, c.record_count, c.members.len());
                for m in &c.members {
                    println!("    {:<10} {}", m.kind, m.value);
                }
            }
        }
        "stats" => {
            let s = store.stats()?;
            println!("sources: {}  records: {}  entities: {}", s.sources, s.records, s.entities);
            println!("span: {} .. {}", fmt_time(s.earliest), fmt_time(s.latest));
            println!("by platform:");
            for b in &s.by_platform {
                println!("  {:<12} {}", b.key, b.count);
            }
            println!("by entity kind:");
            for b in &s.by_entity_kind {
                println!("  {:<12} {}", b.key, b.count);
            }
        }
        other => {
            eprintln!("unknown command: {other}");
            usage();
            exit(2);
        }
    }
    Ok(())
}

fn fmt_time(ts: Option<i64>) -> String {
    match ts {
        None => "unknown-time        ".to_string(),
        Some(t) => {
            use chrono::{TimeZone, Utc};
            Utc.timestamp_opt(t, 0)
                .single()
                .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| t.to_string())
        }
    }
}

fn line(title: &Option<String>, body: &Option<String>) -> String {
    let t = title.as_deref().unwrap_or("");
    let b = body.as_deref().unwrap_or("");
    let joined = if t.is_empty() { b.to_string() } else { format!("{t}: {b}") };
    let one: String = joined.chars().filter(|c| *c != '\n').take(80).collect();
    one
}

fn usage() {
    eprintln!(
        "crosstrace <db> <command> [args]\n\
         commands: import <path> | search <query> | timeline [n] |\n\
                   entities [kind] | correlate <entity_id> | clusters | stats"
    );
}
