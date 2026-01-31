// Rescan gate and wipe workflow for gold-standard verification

use std::collections::HashMap;
use std::path::Path;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::schema::{
    self, get_manifest_entries, update_ingest_session_rescan,
};
use crate::hash::compute_full_hash_from_bytes;
use crate::error::{DadCamError, Result};

/// Run rescan gate after all files are processed (importplan section 4.4).
/// Re-walks the source directory and compares to the original manifest.
/// Sets safe_to_wipe_at only if everything matches.
pub fn run_rescan(conn: &Connection, session_id: i64, source_root: &Path) -> Result<()> {
    let manifest_entries = get_manifest_entries(conn, session_id)?;
    if manifest_entries.is_empty() {
        return Ok(());
    }

    // Device ejection detection: if source root is gone, fail the rescan explicitly
    if !source_root.exists() {
        eprintln!("Rescan failed: source '{}' is no longer accessible (device disconnected?)", source_root.display());
        update_ingest_session_rescan(conn, session_id, "", false)?;
        return Err(DadCamError::Ingest(format!(
            "Source device disconnected: '{}' is no longer accessible. Session is NOT safe to wipe.",
            source_root.display()
        )));
    }

    // Build set of manifest relative paths with their sizes
    let manifest_set: HashMap<String, i64> = manifest_entries.iter()
        .map(|e| (e.relative_path.clone(), e.size_bytes))
        .collect();

    // Re-walk source for ALL eligible files (media + sidecars) per sidecar-importplan 12.5
    let rescan_files = super::discover::discover_all_eligible_files(source_root)?;
    let mut rescan_data = Vec::new();
    let mut rescan_set: HashMap<String, i64> = HashMap::new();

    for file_path in &rescan_files {
        let relative_path = file_path
            .strip_prefix(source_root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();
        let size = std::fs::metadata(file_path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);
        let mtime = std::fs::metadata(file_path).ok()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            });
        rescan_data.push(format!("{}|{}|{}", relative_path, size, mtime.as_deref().unwrap_or("")));
        rescan_set.insert(relative_path, size);
    }

    // Compute rescan hash
    rescan_data.sort();
    let rescan_blob = rescan_data.join("\n");
    let rescan_hash = compute_full_hash_from_bytes(rescan_blob.as_bytes());

    // Compare: every manifest entry must still exist with same size
    let mut all_match = true;
    for (path, manifest_size) in &manifest_set {
        match rescan_set.get(path) {
            Some(rescan_size) if *rescan_size == *manifest_size => {}
            _ => {
                all_match = false;
                eprintln!("Rescan mismatch: manifest entry '{}' missing or changed", path);
            }
        }
    }

    // Check for new files that weren't in the manifest
    for path in rescan_set.keys() {
        if !manifest_set.contains_key(path) {
            all_match = false;
            eprintln!("Rescan mismatch: new file '{}' found on source", path);
        }
    }

    // Check all manifest entries are verified
    let all_verified = manifest_entries.iter().all(|e| {
        e.result == "copied_verified" || e.result == "dedup_verified"
    });

    let safe = all_match && all_verified;
    update_ingest_session_rescan(conn, session_id, &rescan_hash, safe)?;

    Ok(())
}

/// Wipe (delete) source files from a device after SAFE TO WIPE is confirmed.
/// Importplan section 5: only allowed when safe_to_wipe_at is set.
/// Returns a WipeReport with per-file outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WipeReport {
    pub session_id: i64,
    pub source_root: String,
    pub total_files: usize,
    pub deleted: usize,
    pub failed: usize,
    pub entries: Vec<WipeReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WipeReportEntry {
    pub relative_path: String,
    pub success: bool,
    pub error: Option<String>,
}

pub fn wipe_source_files(conn: &Connection, session_id: i64) -> Result<WipeReport> {
    // Hard gate: require SAFE TO WIPE state
    let session = schema::get_ingest_session(conn, session_id)?
        .ok_or_else(|| DadCamError::NotFound(format!("Ingest session {} not found", session_id)))?;

    if session.safe_to_wipe_at.is_none() {
        return Err(DadCamError::Ingest(
            "Cannot wipe: session is not SAFE TO WIPE. All files must be verified and rescan must match.".to_string()
        ));
    }

    let source_root = Path::new(&session.source_root);
    let manifest_entries = schema::get_manifest_entries(conn, session_id)?;

    let mut report = WipeReport {
        session_id,
        source_root: session.source_root.clone(),
        total_files: manifest_entries.len(),
        deleted: 0,
        failed: 0,
        entries: Vec::with_capacity(manifest_entries.len()),
    };

    // Delete in deterministic order (sorted by relative_path -- manifest entries are already sorted)
    for entry in &manifest_entries {
        let full_path = source_root.join(&entry.relative_path);
        match std::fs::remove_file(&full_path) {
            Ok(()) => {
                report.deleted += 1;
                report.entries.push(WipeReportEntry {
                    relative_path: entry.relative_path.clone(),
                    success: true,
                    error: None,
                });
            }
            Err(e) => {
                report.failed += 1;
                report.entries.push(WipeReportEntry {
                    relative_path: entry.relative_path.clone(),
                    success: false,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    Ok(report)
}
