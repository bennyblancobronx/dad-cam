// Audit export for ingest sessions (importplan section 9)
//
// Exports per session:
// - session.json: device fingerprint, start/end, safe_to_wipe_at
// - manifest.jsonl: baseline entries
// - results.jsonl: per-file hashes, method, timestamps, errors
// - rescan.jsonl: rescan snapshot
// - rescan_diff.json: differences (must be empty for SAFE)

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::schema::{
    get_ingest_session, get_manifest_entries,
};
use crate::ingest::discover;
use crate::error::{DadCamError, Result};

#[derive(Serialize)]
struct SessionExport {
    session_id: i64,
    job_id: i64,
    source_root: String,
    device_serial: Option<String>,
    device_label: Option<String>,
    device_mount_point: Option<String>,
    device_capacity_bytes: Option<i64>,
    status: String,
    manifest_hash: Option<String>,
    rescan_hash: Option<String>,
    safe_to_wipe_at: Option<String>,
    started_at: String,
    finished_at: Option<String>,
}

#[derive(Serialize)]
struct ManifestExportEntry {
    relative_path: String,
    size_bytes: i64,
    mtime: Option<String>,
}

#[derive(Serialize)]
struct ResultExportEntry {
    relative_path: String,
    size_bytes: i64,
    result: String,
    hash_fast: Option<String>,
    hash_source_full: Option<String>,
    asset_id: Option<i64>,
    error_code: Option<String>,
    error_detail: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
struct RescanEntry {
    relative_path: String,
    size_bytes: i64,
    mtime: Option<String>,
}

#[derive(Serialize)]
struct RescanDiff {
    missing_from_source: Vec<String>,
    new_on_source: Vec<String>,
    size_changed: Vec<String>,
}

/// Export audit report for an ingest session.
/// Returns the path to the output directory.
pub fn export_audit_report(conn: &Connection, session_id: i64, output_dir: &Path) -> Result<PathBuf> {
    let session = get_ingest_session(conn, session_id)?
        .ok_or_else(|| DadCamError::NotFound(format!("Ingest session {} not found", session_id)))?;

    let entries = get_manifest_entries(conn, session_id)?;

    // Create output directory
    let session_dir = output_dir.join(format!("audit_session_{}", session_id));
    fs::create_dir_all(&session_dir)?;

    // 1. session.json
    let session_export = SessionExport {
        session_id: session.id,
        job_id: session.job_id,
        source_root: session.source_root.clone(),
        device_serial: session.device_serial.clone(),
        device_label: session.device_label.clone(),
        device_mount_point: session.device_mount_point.clone(),
        device_capacity_bytes: session.device_capacity_bytes,
        status: session.status.clone(),
        manifest_hash: session.manifest_hash.clone(),
        rescan_hash: session.rescan_hash.clone(),
        safe_to_wipe_at: session.safe_to_wipe_at.clone(),
        started_at: session.started_at.clone(),
        finished_at: session.finished_at.clone(),
    };
    let session_json = serde_json::to_string_pretty(&session_export)?;
    fs::write(session_dir.join("session.json"), session_json)?;

    // 2. manifest.jsonl -- baseline entries
    {
        let mut manifest_file = fs::File::create(session_dir.join("manifest.jsonl"))?;
        for entry in &entries {
            let line = serde_json::to_string(&ManifestExportEntry {
                relative_path: entry.relative_path.clone(),
                size_bytes: entry.size_bytes,
                mtime: entry.mtime.clone(),
            })?;
            writeln!(manifest_file, "{}", line)?;
        }
    }

    // 3. results.jsonl -- per-file results
    {
        let mut results_file = fs::File::create(session_dir.join("results.jsonl"))?;
        for entry in &entries {
            let line = serde_json::to_string(&ResultExportEntry {
                relative_path: entry.relative_path.clone(),
                size_bytes: entry.size_bytes,
                result: entry.result.clone(),
                hash_fast: entry.hash_fast.clone(),
                hash_source_full: entry.hash_source_full.clone(),
                asset_id: entry.asset_id,
                error_code: entry.error_code.clone(),
                error_detail: entry.error_detail.clone(),
                created_at: entry.created_at.clone(),
                updated_at: entry.updated_at.clone(),
            })?;
            writeln!(results_file, "{}", line)?;
        }
    }

    // 4. rescan.jsonl -- current state of source (if source is still accessible)
    let source_root = Path::new(&session.source_root);
    let mut rescan_map: HashMap<String, i64> = HashMap::new();
    {
        let mut rescan_file = fs::File::create(session_dir.join("rescan.jsonl"))?;
        if source_root.exists() {
            if let Ok(files) = discover::discover_media_files(source_root) {
                for file_path in &files {
                    let relative = file_path
                        .strip_prefix(source_root)
                        .unwrap_or(file_path)
                        .to_string_lossy()
                        .to_string();
                    let meta = fs::metadata(file_path).ok();
                    let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
                    let mtime = meta.as_ref()
                        .and_then(|m| m.modified().ok())
                        .map(|t| {
                            let dt: chrono::DateTime<chrono::Utc> = t.into();
                            dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
                        });

                    let line = serde_json::to_string(&RescanEntry {
                        relative_path: relative.clone(),
                        size_bytes: size,
                        mtime,
                    })?;
                    writeln!(rescan_file, "{}", line)?;
                    rescan_map.insert(relative, size);
                }
            }
        }
    }

    // 5. rescan_diff.json -- differences between manifest and rescan
    let manifest_map: HashMap<String, i64> = entries.iter()
        .map(|e| (e.relative_path.clone(), e.size_bytes))
        .collect();

    let mut missing_from_source = Vec::new();
    let mut new_on_source = Vec::new();
    let mut size_changed = Vec::new();

    for (path, manifest_size) in &manifest_map {
        match rescan_map.get(path) {
            Some(rescan_size) if *rescan_size != *manifest_size => {
                size_changed.push(path.clone());
            }
            None => {
                missing_from_source.push(path.clone());
            }
            _ => {}
        }
    }
    for path in rescan_map.keys() {
        if !manifest_map.contains_key(path) {
            new_on_source.push(path.clone());
        }
    }

    let diff = RescanDiff {
        missing_from_source,
        new_on_source,
        size_changed,
    };
    let diff_json = serde_json::to_string_pretty(&diff)?;
    fs::write(session_dir.join("rescan_diff.json"), diff_json)?;

    Ok(session_dir)
}

/// Export wipe report after wipe_source_files() completes (importplan section 9).
/// Writes wipe_report.json to the audit output directory.
pub fn export_wipe_report(report: &crate::ingest::WipeReport, output_dir: &Path) -> Result<PathBuf> {
    let session_dir = output_dir.join(format!("audit_session_{}", report.session_id));
    fs::create_dir_all(&session_dir)?;

    let json = serde_json::to_string_pretty(report)?;
    let report_path = session_dir.join("wipe_report.json");
    fs::write(&report_path, json)?;

    Ok(report_path)
}
