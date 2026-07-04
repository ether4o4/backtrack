// Prevent a console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! CrossTrace desktop shell.
//!
//! This is a thin binding layer: every command locks the shared [`Store`] and
//! delegates to `crosstrace-core`. All ingest/correlation/search logic lives
//! in the core crate (and is unit-tested there); nothing of substance happens
//! here beyond marshalling arguments and errors across the IPC boundary.

use std::path::PathBuf;
use std::sync::Mutex;

use crosstrace_core::correlate::{Correlation, IdentityCluster, RecordDetail};
use crosstrace_core::ingest::import_path;
use crosstrace_core::search::{SearchFilters, SearchHit};
use crosstrace_core::stats::{DayCount, Stats};
use crosstrace_core::{model::EntityKind, Entity, ImportSummary, Source, Store};
use tauri::{Manager, State};

/// Shared application state: one open store behind a mutex. `rusqlite`'s
/// connection is `Send` but not `Sync`, so the mutex is what makes it usable
/// from Tauri's async command threads.
struct AppState {
    store: Mutex<Store>,
}

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[tauri::command]
fn import(state: State<AppState>, path: String) -> CmdResult<ImportSummary> {
    let mut store = state.store.lock().map_err(err)?;
    import_path(&mut store, &PathBuf::from(path)).map_err(err)
}

#[tauri::command]
fn search(
    state: State<AppState>,
    query: String,
    filters: Option<SearchFilters>,
) -> CmdResult<Vec<SearchHit>> {
    let store = state.store.lock().map_err(err)?;
    store
        .search(&query, &filters.unwrap_or_default())
        .map_err(err)
}

/// Recent records / chronological feed = search with an empty query.
#[tauri::command]
fn timeline(state: State<AppState>, filters: Option<SearchFilters>) -> CmdResult<Vec<SearchHit>> {
    let store = state.store.lock().map_err(err)?;
    store.search("", &filters.unwrap_or_default()).map_err(err)
}

#[tauri::command]
fn list_sources(state: State<AppState>) -> CmdResult<Vec<Source>> {
    let store = state.store.lock().map_err(err)?;
    store.list_sources().map_err(err)
}

#[tauri::command]
fn list_entities(state: State<AppState>, kind: Option<String>) -> CmdResult<Vec<Entity>> {
    let store = state.store.lock().map_err(err)?;
    let kind = kind.as_deref().and_then(EntityKind::from_str);
    store.list_entities(kind).map_err(err)
}

#[tauri::command]
fn correlation(state: State<AppState>, entity_id: i64) -> CmdResult<Option<Correlation>> {
    let store = state.store.lock().map_err(err)?;
    store.correlate(entity_id, 50).map_err(err)
}

#[tauri::command]
fn record_detail(state: State<AppState>, record_id: i64) -> CmdResult<Option<RecordDetail>> {
    let store = state.store.lock().map_err(err)?;
    store.record_detail(record_id, 30).map_err(err)
}

#[tauri::command]
fn identity_clusters(state: State<AppState>) -> CmdResult<Vec<IdentityCluster>> {
    let store = state.store.lock().map_err(err)?;
    store.identity_clusters(2).map_err(err)
}

#[tauri::command]
fn stats(state: State<AppState>) -> CmdResult<Stats> {
    let store = state.store.lock().map_err(err)?;
    store.stats().map_err(err)
}

#[tauri::command]
fn activity(state: State<AppState>) -> CmdResult<Vec<DayCount>> {
    let store = state.store.lock().map_err(err)?;
    store.activity_by_day().map_err(err)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Local, offline store under the OS app-data directory.
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("crosstrace.db");
            let store = Store::open(&db_path)
                .map_err(|e| format!("failed to open store at {db_path:?}: {e}"))?;
            app.manage(AppState {
                store: Mutex::new(store),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import,
            search,
            timeline,
            list_sources,
            list_entities,
            correlation,
            record_detail,
            identity_clusters,
            stats,
            activity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running CrossTrace");
}
