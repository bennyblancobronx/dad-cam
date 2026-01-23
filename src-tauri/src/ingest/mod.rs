// Ingest pipeline module

pub mod discover;
pub mod copy;

use std::path::Path;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

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
pub struct IngestResult {
    pub total_files: usize,
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub clips_created: Vec<i64>,
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

/// Run an ingest job
pub fn run_ingest_job(conn: &Connection, job_id: i64, library_root: &Path) -> Result<IngestResult> {
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
    };

    // Get pending files
    let pending_files = get_pending_ingest_files(conn, job_id)?;
    result.total_files = pending_files.len();

    let originals_dir = library_root.join(ORIGINALS_FOLDER);

    for (idx, ingest_file) in pending_files.iter().enumerate() {
        let source_path = Path::new(&ingest_file.source_path);

        // Update progress
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
        ) {
            Ok(Some(clip_id)) => {
                result.processed += 1;
                result.clips_created.push(clip_id);
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

    let final_status = if result.failed > 0 && result.processed == 0 {
        "failed"
    } else {
        "completed"
    };
    update_job_status(conn, job_id, final_status)?;

    Ok(result)
}

/// Process a single file through the ingest pipeline
fn process_single_file(
    conn: &Connection,
    library_id: i64,
    ingest_file_id: i64,
    source_path: &Path,
    originals_dir: &Path,
    ingest_mode: &str,
) -> Result<Option<i64>> {
    // Update status to copying
    update_ingest_file_status(conn, ingest_file_id, "copying", None)?;

    // Get file size
    let file_size = std::fs::metadata(source_path)
        .map_err(|e| DadCamError::Io(e))?
        .len() as i64;

    // Compute fast hash
    update_ingest_file_status(conn, ingest_file_id, "hashing", None)?;
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
    let metadata = extract_metadata(source_path)?;

    // Determine recorded_at with timestamp precedence
    let (recorded_at, timestamp_source) = determine_timestamp(source_path, &metadata)?;

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

    // Match camera profile and update clip
    if let Ok(Some(camera_match)) = crate::camera::match_camera_profile(conn, &metadata, source_folder.as_deref()) {
        if camera_match.confidence >= 0.5 {
            update_clip_camera_profile(conn, clip_id, camera_match.profile_id)?;
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

    // Mark ingest file as complete
    update_ingest_file_complete(conn, ingest_file_id, &dest_path, asset_id, clip_id)?;

    Ok(Some(clip_id))
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
