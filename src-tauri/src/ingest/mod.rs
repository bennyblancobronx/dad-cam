// Ingest pipeline module

pub mod discover;
pub mod copy;
pub mod sidecar;

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
};
use crate::hash::{compute_fast_hash, compute_size_duration_fingerprint};
use crate::metadata::{extract_metadata, detect_media_type, parse_folder_date};
use crate::constants::{HASH_FAST_SCHEME, ORIGINALS_FOLDER};
use crate::error::{DadCamError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestPayload {
    pub source_path: String,
    pub ingest_mode: String,
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
}

/// Create an ingest job for a source path
pub fn create_ingest_job(conn: &Connection, library_id: i64, source_path: &str, ingest_mode: &str) -> Result<i64> {
    let payload = IngestPayload {
        source_path: source_path.to_string(),
        ingest_mode: ingest_mode.to_string(),
    };

    let job = NewJob {
        job_type: "ingest".to_string(),
        library_id: Some(library_id),
        clip_id: None,
        asset_id: None,
        priority: 10,
        payload: serde_json::to_string(&payload)?,
    };

    let job_id = insert_job(conn, &job)?;

    // Discover files and create ingest_files records
    let files = discover::discover_media_files(Path::new(source_path))?;
    for file_path in &files {
        insert_ingest_file(conn, job_id, &file_path.to_string_lossy())?;
    }

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

    update_job_status(conn, job_id, "running")?;

    let mut result = IngestResult {
        total_files: 0,
        processed: 0,
        skipped: 0,
        failed: 0,
        clips_created: Vec::new(),
        camera_breakdown: Vec::new(),
    };
    let mut camera_counts: HashMap<String, usize> = HashMap::new();

    let pending_files = get_pending_ingest_files(conn, job_id)?;
    result.total_files = pending_files.len();
    let total = result.total_files as u64;
    let job_id_str = job_id.to_string();

    let originals_dir = library_root.join(ORIGINALS_FOLDER);

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

    // Build camera breakdown
    result.camera_breakdown = camera_counts
        .into_iter()
        .map(|(name, count)| CameraBreakdown { name, count })
        .collect();
    result.camera_breakdown.sort_by(|a, b| b.count.cmp(&a.count));

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
) -> Result<Option<(i64, Option<String>)>> {
    use crate::jobs::progress::{JobProgress, emit_progress_opt};

    // Track per-stage timestamps for sidecar
    let discovered_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Update status to copying
    update_ingest_file_status(conn, ingest_file_id, "copying", None)?;

    // Get file size
    let file_size = std::fs::metadata(source_path)
        .map_err(|e| DadCamError::Io(e))?
        .len() as i64;

    // Compute fast hash
    update_ingest_file_status(conn, ingest_file_id, "hashing", None)?;
    let file_name = source_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    emit_progress_opt(app, &JobProgress::new(job_id_str, "hashing", current, total)
        .with_message(format!("Hashing {}", file_name)));
    let hash_fast = compute_fast_hash(source_path)?;

    // Check for duplicates
    if let Some(_existing) = find_asset_by_hash(conn, library_id, &hash_fast)? {
        update_ingest_file_status(conn, ingest_file_id, "skipped", Some("Duplicate file"))?;
        return Ok(None);
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

    // Copy or reference the file
    let copied_at_start = chrono::Utc::now();
    let (dest_path, source_uri) = if ingest_mode == "copy" {
        let dest = copy::copy_file_to_library(source_path, originals_dir)?;
        (dest.to_string_lossy().to_string(), None)
    } else {
        // Reference mode: store original path
        let relative_path = format!("ref:{}", source_path.display());
        (relative_path, Some(source_path.to_string_lossy().to_string()))
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
    };
    let clip_id = insert_clip(conn, &clip)?;

    // Link clip to asset
    link_clip_asset(conn, clip_id, asset_id, "primary")?;

    // Unified camera matching: device + profile
    let camera_result = crate::camera::matcher::match_camera(
        conn,
        &metadata,
        source_folder.as_deref(),
        None, // USB fingerprints not available during ingest (would need to be passed from command)
    );
    let camera_profile_name = camera_result.profile_name.clone();
    if camera_result.confidence >= crate::constants::CAMERA_MATCH_MIN_CONFIDENCE {
        if let Some(profile_id) = camera_result.profile_id {
            update_clip_camera_profile(conn, clip_id, profile_id)?;
        }
        if let Some(device_id) = camera_result.device_id {
            crate::camera::devices::update_clip_camera_device(conn, clip_id, device_id)?;
        }
    }

    // Create fingerprint for relink
    let fingerprint = compute_size_duration_fingerprint(file_size, metadata.duration_ms);
    insert_fingerprint(conn, clip_id, "size_duration", &fingerprint)?;

    // Discover and ingest sidecars (THM, XML, XMP, SRT, etc.)
    let sidecars = discover::discover_sidecars(source_path);
    for sidecar_path in sidecars {
        if let Err(e) = ingest_sidecar(conn, library_id, clip_id, &sidecar_path, originals_dir, ingest_mode) {
            eprintln!("Warning: Failed to ingest sidecar {}: {}", sidecar_path.display(), e);
        }
    }

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

/// Queue jobs that should run after a clip is ingested:
/// - hash_full: Full file hash for verification (per contracts.md)
/// - preview jobs: thumb, proxy, sprite for UI display
fn queue_post_ingest_jobs(
    conn: &Connection,
    clip_id: i64,
    asset_id: i64,
    library_id: i64,
) -> Result<()> {
    // Queue hash_full job for verification (contracts.md: verification after copy)
    // Lower priority than preview jobs since verification can run in background
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

/// Ingest a sidecar file and link it to a clip
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
        let dest = copy::copy_file_to_library(sidecar_path, originals_dir)?;
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
