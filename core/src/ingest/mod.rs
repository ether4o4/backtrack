//! Ingest orchestration: walk a dropped path (file, directory, or ZIP),
//! detect each file, dispatch to the right parser, and persist normalized
//! records. ZIPs are read in place without extracting to disk.

pub mod csv;
pub mod detect;
pub mod html;
pub mod json;
pub mod pdf;
pub mod sms_xml;
pub mod timeparse;
pub mod vcard;

use crate::db::Store;
use crate::model::{ImportSummary, NormalizedRecord, SourceKind};
use detect::Detection;
use std::io::Read;
use std::path::Path;
use walkdir::WalkDir;

/// Files larger than this are streamed/limited; parsers here load into memory,
/// so we cap a single member to keep peak RSS bounded. (Timelines of millions
/// of records still work — this only bounds one file at a time.)
const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Import anything at `path` into the store. Directories are walked
/// recursively; `.zip` files are read as archives.
pub fn import_path(store: &mut Store, path: &Path) -> crate::Result<ImportSummary> {
    let mut summary = ImportSummary::default();
    store.audit("import_start", &path.to_string_lossy())?;

    if path.is_dir() {
        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                import_file(store, entry.path(), &mut summary);
            }
        }
    } else if is_zip(path) {
        import_zip(store, path, &mut summary);
    } else if path.is_file() {
        import_file(store, path, &mut summary);
    } else {
        summary
            .errors
            .push(format!("path not found: {}", path.display()));
    }

    store.audit(
        "import_done",
        &format!(
            "{} records, {} entities, {} skipped",
            summary.records_added, summary.entities_added, summary.files_skipped
        ),
    )?;
    Ok(summary)
}

fn is_zip(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

/// Read every member of a ZIP archive without extracting to disk.
fn import_zip(store: &mut Store, path: &Path, summary: &mut ImportSummary) {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            summary.errors.push(format!("open {}: {e}", path.display()));
            return;
        }
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(e) => {
            summary.errors.push(format!("zip {}: {e}", path.display()));
            return;
        }
    };

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => {
                summary.errors.push(format!("zip entry {i}: {e}"));
                continue;
            }
        };
        if !entry.is_file() || entry.size() > MAX_FILE_BYTES {
            summary.files_skipped += 1;
            continue;
        }
        let name = entry.name().to_string();
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        if entry.read_to_end(&mut bytes).is_err() {
            summary.files_skipped += 1;
            continue;
        }
        // Virtual path: "<zip>!/<member>" so the source is traceable.
        let virt = format!("{}!/{}", path.display(), name);
        ingest_bytes(store, Path::new(&name), &virt, &bytes, summary);
    }
}

fn import_file(store: &mut Store, path: &Path, summary: &mut ImportSummary) {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => {
            summary.files_skipped += 1;
            return;
        }
    };
    if meta.len() > MAX_FILE_BYTES {
        summary.files_skipped += 1;
        summary
            .errors
            .push(format!("skipped large file: {}", path.display()));
        return;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            summary.files_skipped += 1;
            summary.errors.push(format!("read {}: {e}", path.display()));
            return;
        }
    };
    let path_str = path.to_string_lossy().to_string();
    ingest_bytes(store, path, &path_str, &bytes, summary);
}

/// Detect + parse + persist a single in-memory file.
fn ingest_bytes(
    store: &mut Store,
    name_path: &Path,
    full_path: &str,
    bytes: &[u8],
    summary: &mut ImportSummary,
) {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let det = detect::detect(name_path, &head);

    let records = match parse_with(&det, bytes) {
        Some(Ok(r)) => r,
        Some(Err(e)) => {
            summary.errors.push(format!("{full_path}: {e}"));
            summary.files_skipped += 1;
            return;
        }
        None => {
            // Unknown/unsupported type — skip quietly.
            summary.files_skipped += 1;
            return;
        }
    };
    if records.is_empty() {
        summary.files_skipped += 1;
        return;
    }

    let name = name_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| full_path.to_string());
    persist(store, &name, full_path, &det, records, summary);
}

/// Dispatch to the parser for a detected kind. `None` means unsupported.
fn parse_with(det: &Detection, bytes: &[u8]) -> Option<crate::Result<Vec<NormalizedRecord>>> {
    let p = det.platform.as_str();
    Some(match det.kind {
        SourceKind::Csv => csv::parse(bytes, p),
        SourceKind::Json => json::parse(bytes, p),
        SourceKind::VCard => vcard::parse(bytes, p),
        SourceKind::SmsXml => sms_xml::parse(bytes, p),
        SourceKind::Html => html::parse(bytes, p),
        SourceKind::Pdf => pdf::parse(bytes, p),
        SourceKind::Text | SourceKind::Unknown => return None,
    })
}

fn persist(
    store: &mut Store,
    name: &str,
    path: &str,
    det: &Detection,
    records: Vec<NormalizedRecord>,
    summary: &mut ImportSummary,
) {
    let source_id = match store.add_source(name, path, det.kind, &det.platform) {
        Ok(id) => id,
        Err(e) => {
            summary.errors.push(format!("{path}: {e}"));
            return;
        }
    };
    for rec in &records {
        match store.insert_record(source_id, rec) {
            Ok(new_entities) => {
                summary.records_added += 1;
                summary.entities_added += new_entities;
            }
            Err(e) => summary.errors.push(format!("{path}: {e}")),
        }
    }
    let _ = store.refresh_source_count(source_id);
    summary.sources_added += 1;
}

/// Import raw bytes directly (used by tests and by callers that already hold
/// the content, e.g. a browser drop of an in-memory blob).
pub fn import_bytes(
    store: &mut Store,
    name: &str,
    bytes: &[u8],
) -> crate::Result<ImportSummary> {
    let mut summary = ImportSummary::default();
    ingest_bytes(store, Path::new(name), name, bytes, &mut summary);
    Ok(summary)
}
