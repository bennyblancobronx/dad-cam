// Ingest pipeline module

pub mod discover;
pub mod copy;
pub mod sidecar;
pub mod audit;

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::db::schema::{
    self, NewAsset, NewClip, NewJob,
    insert_job, insert_ingest_file, get_pending_ingest_files, update_ingest_file_status,
    update_ingest_file_complete, insert_asset, insert_clip, link_clip_asset, insert_fingerprint,
    find_asset_by_hash, update_job_status, update_job_progress,
    get_or_create_volume, link_asset_volume, update_clip_camera_profile,
    update_clip_camera_refs,
    // New imports for gold-standard verification
    NewIngestSession, insert_ingest_session, update_ingest_session_status,
    update_ingest_session_manifest_hash, update_ingest_session_rescan,
    update_ingest_session_finished,
    NewManifestEntry, insert_manifest_entry, get_manifest_entries,
    get_pending_manifest_entries,
    update_manifest_entry_result, update_manifest_entry_hash_fast,
    update_asset_hash_full, update_asset_verified_with_method,
};
use crate::hash::{compute_fast_hash, compute_full_hash, compute_full_hash_from_bytes, compute_size_duration_fingerprint};
use crate::metadata::{extract_metadata, parse_folder_date};
use crate::constants::{HASH_FAST_SCHEME, ORIGINALS_FOLDER};
use crate::error::{DadCamError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestPayload {
    pub source_path: String,
    pub ingest_mode: String,
    #[serde(default)]
    pub session_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraBreakdown {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    pub total_files: usize,
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub clips_created: Vec<i64>,
    pub camera_breakdown: Vec<CameraBreakdown>,
    pub session_id: Option<i64>,
    /// Number of sidecar files discovered and processed (sidecar-importplan 12.7)
    pub sidecar_count: usize,
    /// Number of sidecar files that failed verification (sidecar-importplan 12.7)
    pub sidecar_failed: usize,
}

/// Create an ingest job for a source path.
/// Now also creates an IngestSession + ManifestEntries for gold-standard verification.
pub fn create_ingest_job(conn: &Connection, library_id: i64, source_path: &str, ingest_mode: &str) -> Result<i64> {
    // Discover files
    let files = discover::discover_media_files(Path::new(source_path))?;

    // Create job first (session references job)
    let payload_initial = IngestPayload {
        source_path: source_path.to_string(),
        ingest_mode: ingest_mode.to_string(),
        session_id: None,
    };
    let job = NewJob {
        job_type: "ingest".to_string(),
        library_id: Some(library_id),
        clip_id: None,
        asset_id: None,
        priority: 10,
        payload: serde_json::to_string(&payload_initial)?,
    };
    let job_id = insert_job(conn, &job)?;

    // Create ingest_files records (legacy table, kept for backward compat)
    for file_path in &files {
        insert_ingest_file(conn, job_id, &file_path.to_string_lossy())?;
    }

    // Create IngestSession
    let volume_info = if !files.is_empty() {
        discover::get_volume_info(Path::new(source_path))
    } else {
        discover::VolumeInfo { serial: None, label: None, mount_point: None }
    };

    let session = NewIngestSession {
        job_id,
        source_root: source_path.to_string(),
        device_serial: volume_info.serial.clone(),
        device_label: volume_info.label.clone(),
        device_mount_point: volume_info.mount_point.clone(),
        device_capacity_bytes: None, // Could be populated if we stat the mount point
    };
    let session_id = insert_ingest_session(conn, &session)?;

    // Create manifest entries for each discovered file (media + sidecars)
    let source_root = Path::new(source_path);
    let mut manifest_data = Vec::new(); // for computing manifest_hash

    // Helper: stat a file and return (size_bytes, mtime_string)
    fn stat_for_manifest(file_path: &Path) -> (i64, Option<String>) {
        let meta = std::fs::metadata(file_path).ok();
        let size_bytes = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
        let mtime = meta.as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            });
        (size_bytes, mtime)
    }

    // Phase 1: Insert media entries (these get lower IDs, processed first)
    // Track media_path -> manifest_entry_id for sidecar parent linking
    let mut media_entry_ids: std::collections::HashMap<std::path::PathBuf, i64> = std::collections::HashMap::new();

    for file_path in &files {
        let relative_path = file_path
            .strip_prefix(source_root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        let (size_bytes, mtime) = stat_for_manifest(file_path);

        let entry_id = insert_manifest_entry(conn, &NewManifestEntry {
            session_id,
            relative_path: relative_path.clone(),
            size_bytes,
            mtime: mtime.clone(),
            entry_type: "media".to_string(),
            parent_entry_id: None,
        })?;

        media_entry_ids.insert(file_path.clone(), entry_id);

        manifest_data.push(format!("{}|{}|{}", relative_path, size_bytes, mtime.as_deref().unwrap_or("")));
    }

    // Phase 2: Discover sidecars and insert as manifest entries
    let (paired_sidecars, orphan_sidecars) = discover::discover_all_sidecars(source_root, &files);

    // Insert paired sidecars (linked to parent media entry)
    for (media_path, sidecar_paths) in &paired_sidecars {
        let parent_entry_id = media_entry_ids.get(media_path).copied();
        for sidecar_path in sidecar_paths {
            let relative_path = sidecar_path
                .strip_prefix(source_root)
                .unwrap_or(sidecar_path)
                .to_string_lossy()
                .to_string();

            let (size_bytes, mtime) = stat_for_manifest(sidecar_path);

            insert_manifest_entry(conn, &NewManifestEntry {
                session_id,
                relative_path: relative_path.clone(),
                size_bytes,
                mtime: mtime.clone(),
                entry_type: "sidecar".to_string(),
                parent_entry_id,
            })?;

            manifest_data.push(format!("{}|{}|{}", relative_path, size_bytes, mtime.as_deref().unwrap_or("")));
        }
    }

    // Insert orphan sidecars (no parent media)
    for sidecar_path in &orphan_sidecars {
        let relative_path = sidecar_path
            .strip_prefix(source_root)
            .unwrap_or(sidecar_path)
            .to_string_lossy()
            .to_string();

        let (size_bytes, mtime) = stat_for_manifest(sidecar_path);

        insert_manifest_entry(conn, &NewManifestEntry {
            session_id,
            relative_path: relative_path.clone(),
            size_bytes,
            mtime: mtime.clone(),
            entry_type: "sidecar".to_string(),
            parent_entry_id: None,
        })?;

        manifest_data.push(format!("{}|{}|{}", relative_path, size_bytes, mtime.as_deref().unwrap_or("")));
    }

    // Compute manifest_hash = BLAKE3 of sorted manifest entries (media + sidecars)
    manifest_data.sort();
    let manifest_blob = manifest_data.join("\n");
    let manifest_hash = compute_full_hash_from_bytes(manifest_blob.as_bytes());
    update_ingest_session_manifest_hash(conn, session_id, &manifest_hash)?;

    // Update job payload with session_id
    let payload_final = IngestPayload {
        source_path: source_path.to_string(),
        ingest_mode: ingest_mode.to_string(),
        session_id: Some(session_id),
    };
    conn.execute(
        "UPDATE jobs SET payload = ?1 WHERE id = ?2",
        rusqlite::params![serde_json::to_string(&payload_final)?, job_id],
    )?;

    Ok(job_id)
}

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
        match process_single_file(
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
                eprintln!("Failed to process {}: {}", source_path.display(), e);
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

            match process_sidecar_entry(
                conn, library_id, sidecar_entry, source_root, &originals_dir, &payload.ingest_mode,
            ) {
                Ok(()) => {
                    result.processed += 1;
                }
                Err(e) => {
                    result.failed += 1;
                    result.sidecar_failed += 1;
                    eprintln!("Failed to process sidecar {}: {}", sidecar_entry.relative_path, e);
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
        if let Err(e) = run_rescan(conn, sid, source_root) {
            eprintln!("Rescan failed for session {}: {}", sid, e);
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

/// Process a single file through the ingest pipeline.
/// Returns (clip_id, camera_profile_name) on success.
fn process_single_file(
    conn: &Connection,
    library_id: i64,
    ingest_file_id: i64,
    source_path: &Path,
    originals_dir: &Path,
    ingest_mode: &str,
    library_root: &Path,
    app: Option<&AppHandle>,
    job_id_str: &str,
    current: u64,
    total: u64,
    manifest_entry: Option<&schema::ManifestEntry>,
) -> Result<Option<(i64, Option<String>)>> {
    use crate::jobs::progress::{JobProgress, emit_progress_opt};

    // Track per-stage timestamps for sidecar
    let discovered_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Step 1: Change detection against manifest baseline
    if let Some(entry) = manifest_entry {
        let current_meta = std::fs::metadata(source_path).ok();
        let current_size = current_meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
        let current_mtime = current_meta.as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            });

        // Compare size and mtime to manifest
        if current_size != entry.size_bytes
            || (entry.mtime.is_some() && current_mtime != entry.mtime)
        {
            // File changed since manifest was built -- block safe-to-wipe
            update_manifest_entry_result(
                conn, entry.id, "changed", None, None,
                Some("CHANGED_SINCE_MANIFEST"),
                Some(&format!("Size or mtime changed: manifest={}/{:?} current={}/{:?}",
                    entry.size_bytes, entry.mtime, current_size, current_mtime)),
            )?;
            update_ingest_file_status(conn, ingest_file_id, "skipped", Some("File changed since discovery"))?;
            return Ok(None);
        }
    }

    // Update status to copying
    update_ingest_file_status(conn, ingest_file_id, "copying", None)?;

    // Get file size
    let file_size = std::fs::metadata(source_path)
        .map_err(|e| DadCamError::Io(e))?
        .len() as i64;

    // Step 2: Compute fast hash for dedup candidate lookup
    update_ingest_file_status(conn, ingest_file_id, "hashing", None)?;
    let file_name = source_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    emit_progress_opt(app, &JobProgress::new(job_id_str, "hashing", current, total)
        .with_message(format!("Hashing {}", file_name)));
    let hash_fast = compute_fast_hash(source_path)?;

    // Update manifest entry with fast hash
    if let Some(entry) = manifest_entry {
        let _ = update_manifest_entry_hash_fast(conn, entry.id, &hash_fast);
    }

    // Step 2b: Check for duplicates with verified dedup
    if let Some(existing) = find_asset_by_hash(conn, library_id, &hash_fast)? {
        // Duplicate candidate found via fast hash
        if let Some(ref existing_full_hash) = existing.hash_full {
            // Existing asset has full hash -- we can verify dedup
            // Compute full source hash to compare
            let source_full_hash = compute_full_hash(source_path)?;
            if source_full_hash == *existing_full_hash {
                // Dedup verified: same file, no need to copy
                if let Some(entry) = manifest_entry {
                    update_manifest_entry_result(
                        conn, entry.id, "dedup_verified",
                        Some(&source_full_hash), Some(existing.id),
                        None, None,
                    )?;
                }
                update_ingest_file_status(conn, ingest_file_id, "skipped", Some("Dedup verified"))?;
                return Ok(None);
            }
            // Full hash mismatch: fast hash collision, treat as unique -- fall through to copy
        }
        // No full hash on existing asset: can't prove match, treat as unique -- fall through to copy
    }

    // Get volume info for the source path (for relink support)
    let volume_info = discover::get_volume_info(source_path);
    let volume_id = if volume_info.serial.is_some() || volume_info.label.is_some() {
        Some(get_or_create_volume(
            conn,
            volume_info.serial.as_deref(),
            volume_info.label.as_deref(),
            volume_info.mount_point.as_deref(),
        )?)
    } else {
        None
    };

    // Step 3: Copy or reference the file
    let copied_at_start = chrono::Utc::now();
    let (dest_path, source_uri, source_hash) = if ingest_mode == "copy" {
        let (dest, hash) = copy::copy_file_to_library(source_path, originals_dir)?;
        (dest.to_string_lossy().to_string(), None, Some(hash))
    } else {
        // Reference mode: store original path
        let relative_path = format!("ref:{}", source_path.display());
        (relative_path, Some(source_path.to_string_lossy().to_string()), None)
    };

    // Extract metadata
    update_ingest_file_status(conn, ingest_file_id, "metadata", None)?;
    emit_progress_opt(app, &JobProgress::new(job_id_str, "metadata", current, total)
        .with_message(format!("Extracting metadata from {}", file_name)));
    let metadata = extract_metadata(source_path)?;

    // Determine recorded_at with timestamp precedence
    let (recorded_at, timestamp_source) = determine_timestamp(source_path, &metadata)?;
    // Clone for sidecar use (originals move into NewClip)
    let recorded_at_sidecar = recorded_at.clone();
    let timestamp_source_sidecar = timestamp_source.clone();

    // Get source folder for event grouping
    let source_folder = source_path
        .parent()
        .map(|p| p.to_string_lossy().to_string());

    // Create asset record
    let asset = NewAsset {
        library_id,
        asset_type: "original".to_string(),
        path: dest_path.clone(),
        source_uri,
        size_bytes: file_size,
        hash_fast: Some(hash_fast),
        hash_fast_scheme: Some(HASH_FAST_SCHEME.to_string()),
    };
    let asset_id = insert_asset(conn, &asset)?;

    // Step 4: Store full hash and mark verified (if we have it from copy)
    if let Some(ref hash) = source_hash {
        update_asset_hash_full(conn, asset_id, hash)?;
        update_asset_verified_with_method(conn, asset_id, "copy_readback")?;
    }

    // Update manifest entry with result
    if let Some(entry) = manifest_entry {
        update_manifest_entry_result(
            conn, entry.id, "copied_verified",
            source_hash.as_deref(), Some(asset_id),
            None, None,
        )?;
    }

    // Link asset to volume (for relink support)
    if let Some(vid) = volume_id {
        link_asset_volume(conn, asset_id, vid)?;
    }

    // Create clip record
    let title = source_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    let clip = NewClip {
        library_id,
        original_asset_id: asset_id,
        camera_profile_id: None, // Will be set after clip creation via camera matching
        media_type: metadata.media_type.clone(),
        title,
        duration_ms: metadata.duration_ms,
        width: metadata.width,
        height: metadata.height,
        fps: metadata.fps,
        codec: metadata.codec.clone(),
        audio_codec: metadata.audio_codec.clone(),
        audio_channels: metadata.audio_channels,
        audio_sample_rate: metadata.audio_sample_rate,
        recorded_at,
        recorded_at_offset_minutes: None,
        recorded_at_is_estimated: timestamp_source.as_deref() == Some("filesystem"),
        timestamp_source,
        source_folder: source_folder.clone(),
        // Stable camera refs set after matching below
        camera_profile_type: None,
        camera_profile_ref: None,
        camera_device_uuid: None,
    };
    let clip_id = insert_clip(conn, &clip)?;

    // Link clip to asset
    link_clip_asset(conn, clip_id, asset_id, "primary")?;

    // Unified camera matching: device + profile (legacy Library DB matcher)
    let camera_result = crate::camera::matcher::match_camera(
        conn,
        &metadata,
        source_folder.as_deref(),
        None, // USB fingerprints not available during ingest (would need to be passed from command)
    );
    let camera_profile_name = camera_result.profile_name.clone();
    if camera_result.confidence >= crate::constants::CAMERA_MATCH_MIN_CONFIDENCE {
        // Legacy integer refs (kept for backward compat)
        if let Some(profile_id) = camera_result.profile_id {
            update_clip_camera_profile(conn, clip_id, profile_id)?;
        }
        if let Some(device_id) = camera_result.device_id {
            crate::camera::devices::update_clip_camera_device(conn, clip_id, device_id)?;
        }
    }

    // Stable camera refs via App DB priority order (spec 7.2):
    // device > user profile > bundled profile > legacy name > fallback
    let (stable_type, stable_ref, device_uuid) = resolve_stable_camera_refs(
        conn,
        camera_result.profile_id,
        camera_result.device_id,
        None, // USB fingerprints not available during file ingest
        &metadata,
        source_folder.as_deref(),
    );
    update_clip_camera_refs(
        conn,
        clip_id,
        stable_type.as_deref(),
        stable_ref.as_deref(),
        device_uuid.as_deref(),
    )?;

    // Create fingerprint for relink
    let fingerprint = compute_size_duration_fingerprint(file_size, metadata.duration_ms);
    insert_fingerprint(conn, clip_id, "size_duration", &fingerprint)?;

    // NOTE: Sidecar files (THM, XML, XMP, SRT, etc.) are now first-class manifest entries
    // discovered during manifest building (sidecar-importplan.md 12.3). They are processed
    // by the main ingest loop via get_pending_manifest_entries(), not here.
    // The old warning-only discover+ingest_sidecar loop was removed per 12.4.

    // Write sidecar JSON to .dadcam/sidecars/
    emit_progress_opt(app, &JobProgress::new(job_id_str, "indexing", current, total)
        .with_message(format!("Indexing {}", file_name)));
    let copied_at = copied_at_start.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let indexed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let sidecar_data = sidecar::SidecarData {
        original_file_path: source_path.to_string_lossy().to_string(),
        file_hash_blake3: asset.hash_fast.as_ref().cloned(),
        metadata_snapshot: sidecar::MetadataSnapshot {
            media_type: metadata.media_type.clone(),
            duration_ms: metadata.duration_ms,
            width: metadata.width,
            height: metadata.height,
            fps: metadata.fps,
            codec: metadata.codec.clone(),
            audio_codec: metadata.audio_codec.clone(),
            audio_channels: metadata.audio_channels,
            audio_sample_rate: metadata.audio_sample_rate,
            camera_make: metadata.camera_make.clone(),
            camera_model: metadata.camera_model.clone(),
            recorded_at: recorded_at_sidecar,
            timestamp_source: timestamp_source_sidecar,
        },
        camera_match: sidecar::CameraMatchSnapshot {
            device_id: camera_result.device_id,
            profile_id: camera_result.profile_id,
            confidence: camera_result.confidence,
            reason: camera_result.reason.clone(),
            profile_type: stable_type.clone(),
            profile_ref: stable_ref.clone(),
            device_uuid: device_uuid.clone(),
        },
        ingest_timestamps: sidecar::IngestTimestamps {
            discovered_at,
            copied_at,
            indexed_at,
        },
        derived_asset_paths: sidecar::expected_derived_paths(library_root, clip_id),
        rental_audit: None,
    };
    if let Err(e) = sidecar::write_sidecar(library_root, clip_id, &sidecar_data) {
        eprintln!("Warning: Failed to write sidecar for clip {}: {}", clip_id, e);
    }

    // Mark ingest file as complete
    update_ingest_file_complete(conn, ingest_file_id, &dest_path, asset_id, clip_id)?;

    // Queue background jobs for the new clip
    queue_post_ingest_jobs(conn, clip_id, asset_id, library_id)?;

    Ok(Some((clip_id, camera_profile_name)))
}

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
    let rescan_files = discover::discover_all_eligible_files(source_root)?;
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

/// Queue jobs that should run after a clip is ingested:
/// - hash_full: Full file hash for verification (per contracts.md)
/// - preview jobs: thumb, proxy, sprite for UI display
fn queue_post_ingest_jobs(
    conn: &Connection,
    clip_id: i64,
    asset_id: i64,
    library_id: i64,
) -> Result<()> {
    // Queue hash_full job for secondary verification (contracts.md)
    // The copy already verified via read-back, this is belt-and-suspenders
    insert_job(conn, &NewJob {
        job_type: "hash_full".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: Some(asset_id),
        priority: 2, // Low priority - background verification
        payload: "{}".to_string(),
    })?;

    // Queue preview generation jobs (Phase 2)
    // Thumbnails first (highest priority) - needed for UI
    insert_job(conn, &NewJob {
        job_type: "thumb".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 8, // High priority
        payload: "{}".to_string(),
    })?;

    // Proxy for playback
    insert_job(conn, &NewJob {
        job_type: "proxy".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 5, // Medium priority
        payload: "{}".to_string(),
    })?;

    // Sprite for hover scrubbing
    insert_job(conn, &NewJob {
        job_type: "sprite".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 3, // Lower priority
        payload: "{}".to_string(),
    })?;

    Ok(())
}

/// Process a single sidecar manifest entry through the gold-standard pipeline.
/// Same copy+verify algorithm as media files (sidecar-importplan section 4.2).
/// Does NOT create clips -- creates sidecar asset and links to parent clip.
fn process_sidecar_entry(
    conn: &Connection,
    library_id: i64,
    entry: &schema::ManifestEntry,
    source_root: &Path,
    originals_dir: &Path,
    ingest_mode: &str,
) -> Result<()> {
    let source_path = source_root.join(&entry.relative_path);

    // Step 1: Re-stat and compare to manifest baseline (change detection)
    let current_meta = std::fs::metadata(&source_path).map_err(|e| {
        DadCamError::Ingest(format!(
            "Sidecar file disappeared: {} ({})", entry.relative_path, e
        ))
    })?;
    let current_size = current_meta.len() as i64;
    let current_mtime = current_meta.modified().ok().map(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    });

    if current_size != entry.size_bytes
        || (entry.mtime.is_some() && current_mtime != entry.mtime)
    {
        update_manifest_entry_result(
            conn, entry.id, "changed", None, None,
            Some("CHANGED_SINCE_MANIFEST"),
            Some(&format!("Sidecar changed: manifest={}/{:?} current={}/{:?}",
                entry.size_bytes, entry.mtime, current_size, current_mtime)),
        )?;
        return Err(DadCamError::Ingest(format!(
            "Sidecar file changed since manifest: {}", entry.relative_path
        )));
    }

    // Step 2: Compute fast hash for dedup candidate lookup
    let hash_fast = compute_fast_hash(&source_path)?;
    let _ = update_manifest_entry_hash_fast(conn, entry.id, &hash_fast);

    // Step 2b: Check for dedup against existing assets
    if let Some(existing) = find_asset_by_hash(conn, library_id, &hash_fast)? {
        if let Some(ref existing_full_hash) = existing.hash_full {
            let source_full_hash = compute_full_hash(&source_path)?;
            if source_full_hash == *existing_full_hash {
                // Dedup verified
                update_manifest_entry_result(
                    conn, entry.id, "dedup_verified",
                    Some(&source_full_hash), Some(existing.id),
                    None, None,
                )?;
                // Still link to parent clip if available
                link_sidecar_to_parent_clip(conn, entry, existing.id)?;
                return Ok(());
            }
        }
    }

    // Step 3: Copy with verification (same algorithm as media files)
    let (dest_path, source_uri, source_hash) = if ingest_mode == "copy" {
        let (dest, hash) = copy::copy_file_to_library(&source_path, originals_dir)?;
        (dest.to_string_lossy().to_string(), None, Some(hash))
    } else {
        let relative_path = format!("ref:{}", source_path.display());
        (relative_path, Some(source_path.to_string_lossy().to_string()), None)
    };

    // Step 4: Create sidecar asset record
    let asset = NewAsset {
        library_id,
        asset_type: "sidecar".to_string(),
        path: dest_path,
        source_uri,
        size_bytes: current_size,
        hash_fast: Some(hash_fast),
        hash_fast_scheme: Some(HASH_FAST_SCHEME.to_string()),
    };
    let asset_id = insert_asset(conn, &asset)?;

    // Step 5: Store full hash and mark verified
    if let Some(ref hash) = source_hash {
        update_asset_hash_full(conn, asset_id, hash)?;
        update_asset_verified_with_method(conn, asset_id, "copy_readback")?;
    }

    // Step 6: Update manifest entry
    update_manifest_entry_result(
        conn, entry.id, "copied_verified",
        source_hash.as_deref(), Some(asset_id),
        None, None,
    )?;

    // Step 7: Link sidecar asset to parent clip
    link_sidecar_to_parent_clip(conn, entry, asset_id)?;

    Ok(())
}

/// Link a sidecar asset to its parent media file's clip.
/// Uses parent_entry_id -> parent manifest entry -> asset_id -> clip lookup.
/// Orphan sidecars (parent_entry_id = NULL) are not linked to any clip.
fn link_sidecar_to_parent_clip(
    conn: &Connection,
    sidecar_entry: &schema::ManifestEntry,
    sidecar_asset_id: i64,
) -> Result<()> {
    let parent_id = match sidecar_entry.parent_entry_id {
        Some(id) => id,
        None => return Ok(()), // Orphan sidecar, no clip to link
    };

    // Get parent manifest entry's asset_id
    let parent_asset_id: Option<i64> = conn.query_row(
        "SELECT asset_id FROM ingest_manifest_entries WHERE id = ?1",
        rusqlite::params![parent_id],
        |row| row.get(0),
    ).unwrap_or(None);

    if let Some(asset_id) = parent_asset_id {
        if let Some(clip) = schema::get_clip_by_asset(conn, asset_id)? {
            link_clip_asset(conn, clip.id, sidecar_asset_id, "sidecar")?;
        }
    }

    Ok(())
}

/// DEPRECATED: Legacy sidecar ingest (pre-sidecar-importplan).
/// Kept for backward compatibility with pre-Migration-10 sessions.
/// New sessions use process_sidecar_entry() which follows the gold-standard pipeline.
#[allow(dead_code)]
fn ingest_sidecar(
    conn: &Connection,
    library_id: i64,
    clip_id: i64,
    sidecar_path: &Path,
    originals_dir: &Path,
    ingest_mode: &str,
) -> Result<i64> {
    let file_size = std::fs::metadata(sidecar_path)
        .map_err(|e| DadCamError::Io(e))?
        .len() as i64;

    // Copy or reference the sidecar
    let (dest_path, source_uri) = if ingest_mode == "copy" {
        let (dest, _hash) = copy::copy_file_to_library(sidecar_path, originals_dir)?;
        (dest.to_string_lossy().to_string(), None)
    } else {
        let relative_path = format!("ref:{}", sidecar_path.display());
        (relative_path, Some(sidecar_path.to_string_lossy().to_string()))
    };

    // Create sidecar asset record
    let asset = NewAsset {
        library_id,
        asset_type: "sidecar".to_string(),
        path: dest_path,
        source_uri,
        size_bytes: file_size,
        hash_fast: None, // Sidecars don't need dedup hashing
        hash_fast_scheme: None,
    };
    let asset_id = insert_asset(conn, &asset)?;

    // Link sidecar to clip with role="sidecar"
    link_clip_asset(conn, clip_id, asset_id, "sidecar")?;

    Ok(asset_id)
}

/// Resolve camera match to stable refs using App DB priority order (spec section 7.2):
/// 1. Registered device match (USB fingerprint -> device UUID -> assigned profile if set)
/// 2. User profiles rules engine (match_rules from App DB user_profiles)
/// 3. Bundled profiles rules engine (match_rules from App DB bundled_profiles)
/// 4. Generic fallback (none)
///
/// Also resolves legacy library-local profile_id by name for backward compat.
/// Returns (profile_type, profile_ref, device_uuid).
fn resolve_stable_camera_refs(
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
    legacy_device_id: Option<i64>,
    usb_fingerprints: Option<&[String]>,
    metadata: &crate::metadata::MediaMetadata,
    source_folder: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let app_conn = match crate::db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(_) => return resolve_stable_refs_fallback(lib_conn, legacy_profile_id, legacy_device_id),
    };

    // Priority 1: Registered device by USB fingerprint
    if let Some(fps) = usb_fingerprints {
        for fp in fps {
            if let Ok(Some(device)) = crate::db::app_schema::find_device_by_usb_fingerprint_app(&app_conn, fp) {
                if device.profile_type != "none" && !device.profile_ref.is_empty() {
                    return (
                        Some(device.profile_type.clone()),
                        Some(device.profile_ref.clone()),
                        Some(device.uuid),
                    );
                }
                // Device found but no assigned profile -- continue matching, keep device_uuid
                let device_uuid = Some(device.uuid.clone());
                let (ptype, pref) = resolve_profile_from_app_db(
                    &app_conn, metadata, source_folder, lib_conn, legacy_profile_id,
                );
                return (Some(ptype), Some(pref), device_uuid);
            }
        }
    }

    // Priority 1b: Device by serial number
    if let Some(ref serial) = metadata.serial_number {
        if let Ok(Some(device)) = crate::db::app_schema::find_device_by_serial_app(&app_conn, serial) {
            if device.profile_type != "none" && !device.profile_ref.is_empty() {
                return (
                    Some(device.profile_type.clone()),
                    Some(device.profile_ref.clone()),
                    Some(device.uuid),
                );
            }
            let device_uuid = Some(device.uuid.clone());
            let (ptype, pref) = resolve_profile_from_app_db(
                &app_conn, metadata, source_folder, lib_conn, legacy_profile_id,
            );
            return (Some(ptype), Some(pref), device_uuid);
        }
    }

    // No device match -- resolve profile from App DB, device from legacy
    let (ptype, pref) = resolve_profile_from_app_db(
        &app_conn, metadata, source_folder, lib_conn, legacy_profile_id,
    );

    // Resolve legacy device_id to UUID if available
    let device_uuid = legacy_device_id.and_then(|did| {
        lib_conn.query_row(
            "SELECT uuid FROM camera_devices WHERE id = ?1",
            [did],
            |row| row.get::<_, String>(0),
        ).ok()
    });

    (Some(ptype), Some(pref), device_uuid)
}

/// Resolve profile using App DB priority: user profiles > bundled profiles > legacy name > fallback.
fn resolve_profile_from_app_db(
    app_conn: &Connection,
    metadata: &crate::metadata::MediaMetadata,
    source_folder: Option<&str>,
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
) -> (String, String) {
    // Priority 2: User profiles match_rules
    if let Ok(user_profiles) = crate::db::app_schema::list_user_profiles(app_conn) {
        if let Some(matched) = match_app_profile_rules(&user_profiles, metadata, source_folder) {
            return ("user".to_string(), matched);
        }
    }

    // Priority 3: Bundled profiles match_rules
    if let Ok(bundled) = crate::db::app_schema::list_bundled_profiles(app_conn) {
        if let Some(matched) = match_bundled_profile_rules(&bundled, metadata, source_folder) {
            return ("bundled".to_string(), matched);
        }

        // Fallback: resolve legacy profile_id by name against bundled/user
        if let Some(pid) = legacy_profile_id {
            if let Ok(name) = lib_conn.query_row(
                "SELECT name FROM camera_profiles WHERE id = ?1",
                [pid],
                |row| row.get::<_, String>(0),
            ) {
                if let Some(bp) = bundled.iter().find(|b| {
                    b.name.eq_ignore_ascii_case(&name) || b.slug.eq_ignore_ascii_case(&name)
                }) {
                    return ("bundled".to_string(), bp.slug.clone());
                }
                if let Ok(ups) = crate::db::app_schema::list_user_profiles(app_conn) {
                    if let Some(up) = ups.iter().find(|u| u.name.eq_ignore_ascii_case(&name)) {
                        return ("user".to_string(), up.uuid.clone());
                    }
                }
            }
        }
    }

    // Priority 4: Generic fallback
    ("none".to_string(), String::new())
}

/// Match metadata against App DB user profiles' match_rules.
/// Returns the UUID of the best matching user profile, if any.
/// Tie-break per spec 7.4: (1) higher version, (2) higher specificity, (3) profile_ref ascending.
pub(crate) fn match_app_profile_rules(
    profiles: &[crate::db::app_schema::AppUserProfile],
    metadata: &crate::metadata::MediaMetadata,
    source_folder: Option<&str>,
) -> Option<String> {
    let mut best: Option<(i32, f64, &str)> = None; // (version, score, ref)

    for profile in profiles {
        let rules: serde_json::Value = serde_json::from_str(&profile.match_rules).unwrap_or_default();
        let score = score_match_rules(&rules, metadata, source_folder);
        if score > 0.0 {
            let is_better = best.map_or(true, |(bv, bs, br)| {
                profile.version > bv
                    || (profile.version == bv && score > bs)
                    || (profile.version == bv && (score - bs).abs() < f64::EPSILON && profile.uuid.as_str() < br)
            });
            if is_better {
                best = Some((profile.version, score, &profile.uuid));
            }
        }
    }

    best.map(|(_, _, uuid)| uuid.to_string())
}

/// Match metadata against App DB bundled profiles' match_rules.
/// Returns the slug of the best matching bundled profile, if any.
/// Tie-break per spec 7.4: (1) higher version, (2) higher specificity, (3) profile_ref ascending.
pub(crate) fn match_bundled_profile_rules(
    profiles: &[crate::db::app_schema::AppBundledProfile],
    metadata: &crate::metadata::MediaMetadata,
    source_folder: Option<&str>,
) -> Option<String> {
    let mut best: Option<(i32, f64, &str)> = None; // (version, score, ref)

    for profile in profiles {
        let rules: serde_json::Value = serde_json::from_str(&profile.match_rules).unwrap_or_default();
        let score = score_match_rules(&rules, metadata, source_folder);
        if score > 0.0 {
            let is_better = best.map_or(true, |(bv, bs, br)| {
                profile.version > bv
                    || (profile.version == bv && score > bs)
                    || (profile.version == bv && (score - bs).abs() < f64::EPSILON && profile.slug.as_str() < br)
            });
            if is_better {
                best = Some((profile.version, score, &profile.slug));
            }
        }
    }

    best.map(|(_, _, slug)| slug.to_string())
}

/// Score how well a match_rules JSON object matches the given metadata.
/// Keys are ANDed; within a key, arrays are ORed; strings are case-insensitive (spec 7.3).
/// Returns 0.0 if any specified key fails to match.
/// Score uses Appendix A specificity weights:
///   +5 make+model, +3 folderPattern, +3 codec+container,
///   +2 resolution constraints, +1 frameRate
pub(crate) fn score_match_rules(
    rules: &serde_json::Value,
    metadata: &crate::metadata::MediaMetadata,
    source_folder: Option<&str>,
) -> f64 {
    let obj = match rules.as_object() {
        Some(o) if !o.is_empty() => o,
        _ => return 0.0,
    };

    let mut total_keys = 0usize;
    let mut matched_keys = 0usize;
    let mut specificity = 0.0f64;

    // make (+5 when combined with model, tracked below)
    let make_matched = if let Some(makes) = obj.get("make").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref cam_make) = metadata.camera_make {
            if makes.iter().any(|m| {
                m.as_str().map_or(false, |s| cam_make.to_lowercase().contains(&s.to_lowercase()))
            }) {
                matched_keys += 1;
                true
            } else { false }
        } else { false }
    } else { false };

    // model (+5 when combined with make)
    let model_matched = if let Some(models) = obj.get("model").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref cam_model) = metadata.camera_model {
            if models.iter().any(|m| {
                m.as_str().map_or(false, |s| cam_model.to_lowercase().contains(&s.to_lowercase()))
            }) {
                matched_keys += 1;
                true
            } else { false }
        } else { false }
    } else { false };

    // Award make+model specificity
    if make_matched && model_matched {
        specificity += 5.0;
    } else if make_matched || model_matched {
        specificity += 2.0;
    }

    // codec (+3 when combined with container, tracked below)
    let codec_matched = if let Some(codecs) = obj.get("codec").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref codec) = metadata.codec {
            if codecs.iter().any(|c| {
                c.as_str().map_or(false, |s| codec.eq_ignore_ascii_case(s))
            }) {
                matched_keys += 1;
                true
            } else { false }
        } else { false }
    } else { false };

    // container (+3 when combined with codec)
    let container_matched = if let Some(containers) = obj.get("container").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref container) = metadata.container {
            let parts: Vec<&str> = container.split(',').map(|s| s.trim()).collect();
            if containers.iter().any(|c| {
                c.as_str().map_or(false, |s| parts.iter().any(|p| p.eq_ignore_ascii_case(s)))
            }) {
                matched_keys += 1;
                true
            } else { false }
        } else { false }
    } else { false };

    // Award codec+container specificity
    if codec_matched && container_matched {
        specificity += 3.0;
    } else if codec_matched || container_matched {
        specificity += 1.5;
    }

    // folderPattern (+3)
    if let Some(pattern) = obj.get("folderPattern").and_then(|v| v.as_str()) {
        total_keys += 1;
        if let Some(folder) = source_folder {
            if let Ok(re) = regex::RegexBuilder::new(pattern).case_insensitive(true).build() {
                if re.is_match(folder) {
                    matched_keys += 1;
                    specificity += 3.0;
                }
            }
        }
    }

    // Resolution constraints (+2 collectively): minWidth, maxWidth, minHeight, maxHeight
    let has_resolution_rule = obj.contains_key("minWidth") || obj.contains_key("maxWidth")
        || obj.contains_key("minHeight") || obj.contains_key("maxHeight");
    if has_resolution_rule {
        total_keys += 1;
        let w = metadata.width.unwrap_or(0);
        let h = metadata.height.unwrap_or(0);
        let mut res_ok = true;
        if let Some(min_w) = obj.get("minWidth").and_then(|v| v.as_i64()) {
            if (w as i64) < min_w { res_ok = false; }
        }
        if let Some(max_w) = obj.get("maxWidth").and_then(|v| v.as_i64()) {
            if (w as i64) > max_w { res_ok = false; }
        }
        if let Some(min_h) = obj.get("minHeight").and_then(|v| v.as_i64()) {
            if (h as i64) < min_h { res_ok = false; }
        }
        if let Some(max_h) = obj.get("maxHeight").and_then(|v| v.as_i64()) {
            if (h as i64) > max_h { res_ok = false; }
        }
        if res_ok {
            matched_keys += 1;
            specificity += 2.0;
        }
    }

    // frameRate (+1, tolerance +/- 0.01 per spec)
    if let Some(rates) = obj.get("frameRate").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(fps) = metadata.fps {
            if rates.iter().any(|r| {
                r.as_f64().map_or(false, |expected| (fps - expected).abs() <= 0.01)
            }) {
                matched_keys += 1;
                specificity += 1.0;
            }
        }
    }

    if total_keys == 0 {
        return 0.0;
    }

    // All keys must match (AND semantics per spec 7.3)
    if matched_keys == total_keys {
        specificity
    } else {
        0.0
    }
}

/// Fallback when App DB is unavailable: resolve from legacy library DB refs only.
fn resolve_stable_refs_fallback(
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
    legacy_device_id: Option<i64>,
) -> (Option<String>, Option<String>, Option<String>) {
    let device_uuid = legacy_device_id.and_then(|did| {
        lib_conn.query_row(
            "SELECT uuid FROM camera_devices WHERE id = ?1",
            [did],
            |row| row.get::<_, String>(0),
        ).ok()
    });
    let _pid = legacy_profile_id; // Cannot resolve without App DB
    (Some("none".to_string()), Some(String::new()), device_uuid)
}

/// Determine timestamp using precedence rules
fn determine_timestamp(path: &Path, metadata: &crate::metadata::MediaMetadata) -> Result<(Option<String>, Option<String>)> {
    // 1. Try embedded metadata
    if let Some(ref recorded_at) = metadata.recorded_at {
        return Ok((Some(recorded_at.clone()), Some("metadata".to_string())));
    }

    // 2. Try folder name parsing
    if let Some(parent) = path.parent() {
        if let Some(folder_name) = parent.file_name().and_then(|n| n.to_str()) {
            if let Some(date) = parse_folder_date(folder_name) {
                return Ok((Some(date), Some("folder".to_string())));
            }
        }
    }

    // 3. Fall back to filesystem modified date
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            let datetime: chrono::DateTime<chrono::Utc> = modified.into();
            return Ok((Some(datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()), Some("filesystem".to_string())));
        }
    }

    Ok((None, None))
}

// --- Section 10 tests (importplan.md) ---

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::TempDir;

    /// Set up an in-memory DB with all migrations applied and a library record.
    /// Returns (conn, library_id).
    fn setup_test_db() -> (Connection, i64) {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::db::migrations::run_migrations(&conn).unwrap();
        let lib_id = schema::insert_library(&conn, "/test/lib", "TestLib", "copy").unwrap();
        (conn, lib_id)
    }

    /// Create a source directory with N video files of known content.
    /// Returns (source_dir, Vec<(filename, content_bytes)>).
    fn create_source_files(dir: &Path, files: &[(&str, &[u8])]) {
        std::fs::create_dir_all(dir).unwrap();
        for (name, content) in files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(content).unwrap();
        }
    }

    // ---------------------------------------------------------------
    // Test 1: Integrity -- corrupt dest after copy, read-back must fail
    // ---------------------------------------------------------------
    #[test]
    fn test_readback_detects_corruption() {
        // copy_with_verify writes to temp, reads it back, compares hashes.
        // We can't corrupt *during* copy_with_verify (it's atomic inside the fn),
        // so instead we test copy_file_to_library with a valid file to confirm
        // the happy path, then directly call the internal verify mechanism:
        // write a temp file, corrupt it, and confirm hash mismatch is detected.

        let tmp = TempDir::new().unwrap();
        let source_dir = tmp.path().join("source");
        let originals_dir = tmp.path().join("originals");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&originals_dir).unwrap();

        // Create a source file with known content
        let content = b"This is test video content for integrity check. Repeating data to exceed trivial size.";
        let source_file = source_dir.join("test_clip.mp4");
        {
            let mut f = std::fs::File::create(&source_file).unwrap();
            f.write_all(content).unwrap();
        }

        // Happy path: copy succeeds and returns a valid hash
        let (rel_path, source_hash) = copy::copy_file_to_library(&source_file, &originals_dir).unwrap();
        assert!(source_hash.starts_with("blake3:full:"), "Hash should be blake3:full: prefixed");

        // Verify the dest file matches
        let dest_full = originals_dir.parent().unwrap_or(&originals_dir).join(&rel_path);
        assert!(dest_full.exists(), "Dest file should exist after copy");
        let dest_hash = crate::hash::compute_full_hash(&dest_full).unwrap();
        assert_eq!(source_hash, dest_hash, "Source and dest hash should match after copy");

        // Now corrupt the dest file (flip a byte) and verify hash no longer matches
        {
            let mut bytes = std::fs::read(&dest_full).unwrap();
            assert!(!bytes.is_empty());
            bytes[0] ^= 0xFF; // flip first byte
            std::fs::write(&dest_full, &bytes).unwrap();
        }
        let corrupted_hash = crate::hash::compute_full_hash(&dest_full).unwrap();
        assert_ne!(source_hash, corrupted_hash, "Corrupted file must produce different hash");
        assert_ne!(dest_hash, corrupted_hash, "Corrupted file must not match original dest hash");

        // Verify via verify_hash reports mismatch
        let matches = crate::hash::verify_hash(&dest_full, &source_hash).unwrap();
        assert!(!matches, "verify_hash must report mismatch on corrupted file");
    }

    // ---------------------------------------------------------------
    // Test 2: Crash safety -- only temp file exists mid-copy;
    //         no final file until verification passes
    // ---------------------------------------------------------------
    #[test]
    fn test_crash_safety_temp_file_pattern() {
        // Verify that the copy function uses a temp file prefix and that
        // on failure, the final path does not exist.

        let tmp = TempDir::new().unwrap();
        let originals_dir = tmp.path().join("originals");
        std::fs::create_dir_all(&originals_dir).unwrap();

        // Create a source file
        let source_file = tmp.path().join("source_clip.mp4");
        {
            let mut f = std::fs::File::create(&source_file).unwrap();
            f.write_all(b"crash safety test content padding data").unwrap();
        }

        // Copy should succeed -- verify no temp files remain
        let (rel_path, _hash) = copy::copy_file_to_library(&source_file, &originals_dir).unwrap();
        let dest_full = originals_dir.parent().unwrap_or(&originals_dir).join(&rel_path);
        assert!(dest_full.exists(), "Final file should exist after successful copy");

        // Verify no temp files left behind in the destination directory
        let dest_parent = dest_full.parent().unwrap();
        for entry in std::fs::read_dir(dest_parent).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(
                !name.starts_with(crate::constants::TEMP_FILE_PREFIX),
                "No temp files should remain after successful copy, found: {}",
                name
            );
        }

        // Now test that copying a nonexistent source fails and leaves no final file
        let missing_source = tmp.path().join("does_not_exist.mp4");
        let result = copy::copy_file_to_library(&missing_source, &originals_dir);
        assert!(result.is_err(), "Copying nonexistent file should fail");

        // The dest directory should have exactly 1 file (the successful copy)
        let file_count = std::fs::read_dir(dest_parent)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().is_file())
            .count();
        assert_eq!(file_count, 1, "Only the one successful copy should exist -- no orphan files");
    }

    // ---------------------------------------------------------------
    // Test 3: Dedup correctness -- two files that share first/last MB
    //         + size cause fast-hash collision; full hash resolves
    // ---------------------------------------------------------------
    #[test]
    fn test_dedup_fast_hash_collision_resolved_by_full_hash() {
        let (conn, library_id) = setup_test_db();
        let tmp = TempDir::new().unwrap();

        // Create two files with identical first 1MB, last 1MB, and size,
        // but different middle content.
        // For fast_hash (first_last_size_v1): first 1MB + last 1MB + size.
        // If file <= 2MB, entire content is hashed. So make files > 2MB.
        let size = 2 * 1024 * 1024 + 1024; // 2MB + 1KB
        let mut content_a = vec![0xAAu8; size];
        let mut content_b = vec![0xAAu8; size];
        // Differ only in the middle (outside first/last 1MB windows)
        let mid = size / 2;
        content_a[mid] = 0x01;
        content_b[mid] = 0x02;

        let file_a = tmp.path().join("file_a.mp4");
        let file_b = tmp.path().join("file_b.mp4");
        std::fs::write(&file_a, &content_a).unwrap();
        std::fs::write(&file_b, &content_b).unwrap();

        // Verify fast hashes collide
        let hash_fast_a = crate::hash::compute_fast_hash(&file_a).unwrap();
        let hash_fast_b = crate::hash::compute_fast_hash(&file_b).unwrap();
        assert_eq!(hash_fast_a, hash_fast_b, "Fast hashes should collide for files differing only in middle");

        // Verify full hashes differ
        let hash_full_a = crate::hash::compute_full_hash(&file_a).unwrap();
        let hash_full_b = crate::hash::compute_full_hash(&file_b).unwrap();
        assert_ne!(hash_full_a, hash_full_b, "Full hashes must differ for different file content");

        // Insert file_a as an existing asset with hash_fast and hash_full
        let asset_a_id = schema::insert_asset(&conn, &schema::NewAsset {
            library_id,
            asset_type: "original".to_string(),
            path: "originals/file_a.mp4".to_string(),
            source_uri: None,
            size_bytes: size as i64,
            hash_fast: Some(hash_fast_a.clone()),
            hash_fast_scheme: Some("first_last_size_v1".to_string()),
        }).unwrap();
        schema::update_asset_hash_full(&conn, asset_a_id, &hash_full_a).unwrap();

        // Now simulate dedup check for file_b:
        // find_asset_by_hash should find asset_a (fast hash match)
        let candidate = schema::find_asset_by_hash(&conn, library_id, &hash_fast_b).unwrap();
        assert!(candidate.is_some(), "Should find candidate via fast hash");
        let candidate = candidate.unwrap();
        assert_eq!(candidate.id, asset_a_id);

        // But full hash comparison must reject the dedup
        let source_full = crate::hash::compute_full_hash(&file_b).unwrap();
        assert_ne!(
            source_full,
            candidate.hash_full.unwrap(),
            "Full hash mismatch must prevent dedup -- these are different files"
        );
    }

    // ---------------------------------------------------------------
    // Test 4: Completeness -- new file added after manifest must block
    //         SAFE TO WIPE via rescan diff
    // ---------------------------------------------------------------
    #[test]
    fn test_new_file_after_manifest_blocks_safe_to_wipe() {
        let (conn, _library_id) = setup_test_db();
        let tmp = TempDir::new().unwrap();
        let source_dir = tmp.path().join("sd_card");
        std::fs::create_dir_all(&source_dir).unwrap();

        // Create initial files that form the manifest baseline
        create_source_files(&source_dir, &[
            ("DCIM/clip001.mp4", b"video content 001"),
            ("DCIM/clip002.mp4", b"video content 002"),
        ]);

        // Create a fake job to hang the session off of (library_id from setup_test_db)
        let job_id = schema::insert_job(&conn, &schema::NewJob {
            job_type: "ingest".to_string(),
            library_id: Some(_library_id),
            clip_id: None,
            asset_id: None,
            priority: 10,
            payload: "{}".to_string(),
        }).unwrap();

        // Create session
        let session_id = schema::insert_ingest_session(&conn, &schema::NewIngestSession {
            job_id,
            source_root: source_dir.to_string_lossy().to_string(),
            device_serial: None,
            device_label: None,
            device_mount_point: None,
            device_capacity_bytes: None,
        }).unwrap();

        // Build manifest from the 2 files
        let files = discover::discover_media_files(&source_dir).unwrap();
        assert_eq!(files.len(), 2, "Should discover exactly 2 files");

        for file_path in &files {
            let relative = file_path.strip_prefix(&source_dir).unwrap().to_string_lossy().to_string();
            let meta = std::fs::metadata(file_path).unwrap();
            schema::insert_manifest_entry(&conn, &schema::NewManifestEntry {
                session_id,
                relative_path: relative,
                size_bytes: meta.len() as i64,
                mtime: None,
                entry_type: "media".to_string(),
                parent_entry_id: None,
            }).unwrap();
        }

        // Create a dummy asset so manifest entries can reference it
        let dummy_asset_id = schema::insert_asset(&conn, &schema::NewAsset {
            library_id: _library_id,
            asset_type: "original".to_string(),
            path: "originals/dummy.mp4".to_string(),
            source_uri: None,
            size_bytes: 100,
            hash_fast: None,
            hash_fast_scheme: None,
        }).unwrap();

        // Mark all manifest entries as copied_verified (simulate successful ingest)
        let entries = schema::get_manifest_entries(&conn, session_id).unwrap();
        assert_eq!(entries.len(), 2);
        for entry in &entries {
            schema::update_manifest_entry_result(
                &conn, entry.id, "copied_verified", Some("blake3:full:abc123"), Some(dummy_asset_id),
                None, None,
            ).unwrap();
        }

        // Run rescan BEFORE adding new file -- should be safe
        run_rescan(&conn, session_id, &source_dir).unwrap();
        let session = schema::get_ingest_session(&conn, session_id).unwrap().unwrap();
        assert!(
            session.safe_to_wipe_at.is_some(),
            "Should be SAFE TO WIPE when manifest matches rescan exactly"
        );

        // Now add a NEW file to the source (simulating camera still writing)
        create_source_files(&source_dir, &[
            ("DCIM/clip003.mp4", b"video content 003 -- new file added after manifest"),
        ]);

        // Clear safe_to_wipe by resetting session (simulate re-check)
        conn.execute(
            "UPDATE ingest_sessions SET safe_to_wipe_at = NULL, rescan_hash = NULL WHERE id = ?1",
            rusqlite::params![session_id],
        ).unwrap();

        // Run rescan AFTER adding new file -- must NOT be safe
        run_rescan(&conn, session_id, &source_dir).unwrap();
        let session = schema::get_ingest_session(&conn, session_id).unwrap().unwrap();
        assert!(
            session.safe_to_wipe_at.is_none(),
            "Must NOT be SAFE TO WIPE when new file appears on source after manifest"
        );
    }
}
