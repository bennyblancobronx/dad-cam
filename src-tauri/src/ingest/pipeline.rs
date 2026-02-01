// Ingest job execution pipeline

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use rusqlite::Connection;
use tauri::AppHandle;

use crate::db::schema::{
    self,
    get_pending_ingest_files, update_ingest_file_status,
    update_job_status, update_job_progress,
    get_manifest_entries, get_pending_manifest_entries,
    update_ingest_session_status, update_ingest_session_finished,
    update_manifest_entry_result,
};
use crate::constants::ORIGINALS_FOLDER;
use crate::error::{DadCamError, Result};
use super::{IngestPayload, IngestResult, CameraBreakdown};

/// Run an ingest job (no progress events or cancellation -- for CLI / background runner).
pub fn run_ingest_job(conn: &Connection, job_id: i64, library_root: &Path) -> Result<IngestResult> {
    run_ingest_job_inner(conn, job_id, library_root, None, None)
}

/// Run an ingest job with progress events and cancellation support.
/// Called from the Tauri command (which has access to AppHandle).
pub fn run_ingest_job_with_progress(
    conn: &Connection,
    job_id: i64,
    library_root: &Path,
    app: &AppHandle,
    cancel_flag: &AtomicBool,
) -> Result<IngestResult> {
    run_ingest_job_inner(conn, job_id, library_root, Some(app), Some(cancel_flag))
}

/// Unified ingest job implementation.
/// When app and cancel_flag are None, runs without progress events or cancellation.
fn run_ingest_job_inner(
    conn: &Connection,
    job_id: i64,
    library_root: &Path,
    app: Option<&AppHandle>,
    cancel_flag: Option<&AtomicBool>,
) -> Result<IngestResult> {
    use crate::jobs::progress::{JobProgress, emit_progress_opt};
    use crate::jobs::is_cancelled;

    let job = schema::get_job(conn, job_id)?
        .ok_or_else(|| DadCamError::JobNotFound(job_id))?;

    let payload: IngestPayload = serde_json::from_str(&job.payload)?;
    let library_id = job.library_id.ok_or_else(|| DadCamError::Other("Job has no library".to_string()))?;
    let session_id = payload.session_id;

    // Update session status to ingesting
    if let Some(sid) = session_id {
        let _ = update_ingest_session_status(conn, sid, "ingesting");
    }

    update_job_status(conn, job_id, "running")?;

    let mut result = IngestResult {
        total_files: 0,
        processed: 0,
        skipped: 0,
        failed: 0,
        clips_created: Vec::new(),
        camera_breakdown: Vec::new(),
        session_id,
        sidecar_count: 0,
        sidecar_failed: 0,
    };
    let mut camera_counts: HashMap<String, usize> = HashMap::new();

    // Get manifest entries for this session (if available) for change detection
    let manifest_entries = session_id
        .and_then(|sid| get_manifest_entries(conn, sid).ok())
        .unwrap_or_default();
    let manifest_map: HashMap<String, schema::ManifestEntry> = manifest_entries
        .into_iter()
        .map(|e| (e.relative_path.clone(), e))
        .collect();

    let pending_files = get_pending_ingest_files(conn, job_id)?;
    result.total_files = pending_files.len();
    let total = result.total_files as u64;
    let job_id_str = job_id.to_string();

    let originals_dir = library_root.join(ORIGINALS_FOLDER);
    let source_root = Path::new(&payload.source_path);

    for (idx, ingest_file) in pending_files.iter().enumerate() {
        // Check cancel flag between files (when available)
        if let Some(flag) = cancel_flag {
            if is_cancelled(flag) {
                emit_progress_opt(app, &JobProgress::new(&job_id_str, "cancelled", idx as u64, total)
                    .cancelled()
                    .with_message("Import cancelled by user"));
                update_job_status(conn, job_id, "cancelled")?;
                return Ok(result);
            }
        }

        let source_path = Path::new(&ingest_file.source_path);

        // Device ejection detection: check source root is still accessible
        if !source_root.exists() {
            // Source disappeared (device ejected / unmounted)
            if let Some(sid) = session_id {
                let _ = update_ingest_session_status(conn, sid, "failed");
                let _ = update_ingest_session_finished(conn, sid);
            }
            emit_progress_opt(app, &JobProgress::new(&job_id_str, "failed", idx as u64, total)
                .error("Source device disconnected during import. Remaining files were not verified. Do NOT wipe the device.".to_string()));
            update_job_status(conn, job_id, "failed")?;

            // Mark all remaining pending manifest entries as failed
            if let Some(sid) = session_id {
                let remaining = get_manifest_entries(conn, sid).unwrap_or_default();
                for entry in &remaining {
                    if entry.result == "pending" || entry.result == "copying" {
                        let _ = update_manifest_entry_result(
                            conn, entry.id, "failed", None, None,
                            Some("DEVICE_DISCONNECTED"),
                            Some("Source device was disconnected during import"),
                        );
                    }
                }
            }

            return Ok(result);
        }

        // Emit per-file progress (when app handle available)
        let current = (idx + 1) as u64;
        let file_name = source_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        emit_progress_opt(app, &JobProgress::new(&job_id_str, "copying", current, total)
            .with_message(format!("Copying {}", file_name)));

        // Update DB progress
        let progress = ((idx + 1) * 100 / result.total_files.max(1)) as i32;
        update_job_progress(conn, job_id, progress)?;

        // Find corresponding manifest entry for change detection
        let relative_path = source_path
            .strip_prefix(source_root)
            .unwrap_or(source_path)
            .to_string_lossy()
            .to_string();
        let manifest_entry = manifest_map.get(&relative_path);

        // Process file
        match super::file_processor::process_single_file(
            conn,
            library_id,
            ingest_file.id,
            source_path,
            &originals_dir,
            &payload.ingest_mode,
            library_root,
            app,
            &job_id_str,
            current,
            total,
            manifest_entry,
        ) {
            Ok(Some((clip_id, camera_name))) => {
                result.processed += 1;
                result.clips_created.push(clip_id);
                let name = camera_name.unwrap_or_else(|| "Unknown".to_string());
                *camera_counts.entry(name).or_insert(0) += 1;
            }
            Ok(None) => {
                result.skipped += 1;
            }
            Err(e) => {
                result.failed += 1;
                update_ingest_file_status(conn, ingest_file.id, "failed", Some(&e.to_string()))?;
                log::error!("Failed to process {}: {}", source_path.display(), e);
            }
        }
    }

    // Process sidecar manifest entries (sidecar-importplan section 12.4)
    // Sidecars are in manifest_entries but not legacy ingest_files table.
    // Processed after all media files so parent clips exist for linking.
    if let Some(sid) = session_id {
        let sidecar_entries: Vec<_> = get_pending_manifest_entries(conn, sid)?
            .into_iter()
            .filter(|e| e.entry_type == "sidecar")
            .collect();

        let sidecar_total = sidecar_entries.len();
        for (sidx, sidecar_entry) in sidecar_entries.iter().enumerate() {
            // Check cancel flag
            if let Some(flag) = cancel_flag {
                if is_cancelled(flag) {
                    emit_progress_opt(app, &JobProgress::new(&job_id_str, "cancelled", (sidx) as u64, sidecar_total as u64)
                        .cancelled()
                        .with_message("Import cancelled by user"));
                    update_job_status(conn, job_id, "cancelled")?;
                    return Ok(result);
                }
            }

            // Emit progress
            emit_progress_opt(app, &JobProgress::new(&job_id_str, "copying_sidecars", (sidx + 1) as u64, sidecar_total as u64)
                .with_message(format!("Copying sidecar {}/{}", sidx + 1, sidecar_total)));

            match super::sidecar_processor::process_sidecar_entry(
                conn, library_id, sidecar_entry, source_root, &originals_dir, &payload.ingest_mode,
            ) {
                Ok(()) => {
                    result.processed += 1;
                }
                Err(e) => {
                    result.failed += 1;
                    result.sidecar_failed += 1;
                    log::error!("Failed to process sidecar {}: {}", sidecar_entry.relative_path, e);
                    // Mark manifest entry as failed (blocks SAFE TO WIPE)
                    let _ = update_manifest_entry_result(
                        conn, sidecar_entry.id, "failed", None, None,
                        Some("SIDECAR_COPY_FAILED"),
                        Some(&e.to_string()),
                    );
                }
            }
        }

        result.total_files += sidecar_total;
        result.sidecar_count = sidecar_total;
    }

    // Build camera breakdown
    result.camera_breakdown = camera_counts
        .into_iter()
        .map(|(name, count)| CameraBreakdown { name, count })
        .collect();
    result.camera_breakdown.sort_by(|a, b| b.count.cmp(&a.count));

    // Update session status to verifying, then run rescan
    if let Some(sid) = session_id {
        let _ = update_ingest_session_status(conn, sid, "rescanning");
        // Run rescan gate
        if let Err(e) = super::verification::run_rescan(conn, sid, source_root) {
            log::warn!("Rescan failed for session {}: {}", sid, e);
        }
        let _ = update_ingest_session_status(conn, sid, "complete");
        let _ = update_ingest_session_finished(conn, sid);
    }

    let final_status = if result.failed > 0 && result.processed == 0 {
        "failed"
    } else {
        "completed"
    };
    update_job_status(conn, job_id, final_status)?;

    Ok(result)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
