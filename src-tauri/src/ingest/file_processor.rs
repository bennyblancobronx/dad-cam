// Single-file processing through the ingest pipeline

use std::path::Path;
use rusqlite::Connection;
use tauri::AppHandle;

use crate::db::schema::{
    self, NewAsset, NewClip,
    update_ingest_file_status, update_ingest_file_complete,
    insert_asset, insert_clip, link_clip_asset, insert_fingerprint,
    find_asset_by_hash, insert_job,
    get_or_create_volume, link_asset_volume,
    update_clip_camera_profile, update_clip_camera_refs,
    update_manifest_entry_result, update_manifest_entry_hash_fast,
    update_asset_hash_full, update_asset_verified_with_method,
};
use crate::hash::{compute_fast_hash, compute_full_hash, compute_size_duration_fingerprint};
use crate::metadata::parse_folder_date;
use crate::constants::HASH_FAST_SCHEME;
use crate::error::{DadCamError, Result};

/// Process a single file through the ingest pipeline.
/// Returns (clip_id, camera_profile_name) on success.
pub(crate) fn process_single_file(
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

        if current_size != entry.size_bytes
            || (entry.mtime.is_some() && current_mtime != entry.mtime)
        {
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

    update_ingest_file_status(conn, ingest_file_id, "copying", None)?;

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

    if let Some(entry) = manifest_entry {
        let _ = update_manifest_entry_hash_fast(conn, entry.id, &hash_fast);
    }

    // Step 2b: Check for duplicates with verified dedup
    if let Some(existing) = find_asset_by_hash(conn, library_id, &hash_fast)? {
        if let Some(ref existing_full_hash) = existing.hash_full {
            let source_full_hash = compute_full_hash(source_path)?;
            if source_full_hash == *existing_full_hash {
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
        }
    }

    // Get volume info for relink support
    let volume_info = super::discover::get_volume_info(source_path);
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
        let (dest, hash) = super::copy::copy_file_to_library(source_path, originals_dir)?;
        (dest.to_string_lossy().to_string(), None, Some(hash))
    } else {
        let relative_path = format!("ref:{}", source_path.display());
        (relative_path, Some(source_path.to_string_lossy().to_string()), None)
    };

    // Extract full metadata with raw dumps (Layer 0: gold-standard capture)
    update_ingest_file_status(conn, ingest_file_id, "metadata", None)?;
    emit_progress_opt(app, &JobProgress::new(job_id_str, "metadata", current, total)
        .with_message(format!("Extracting metadata from {}", file_name)));
    let full_result = crate::metadata::extract_metadata_full(source_path)?;
    let metadata = full_result.metadata.clone();

    // Determine recorded_at with timestamp precedence
    let (recorded_at, timestamp_source) = determine_timestamp(source_path, &metadata)?;
    let recorded_at_sidecar = recorded_at.clone();
    let timestamp_source_sidecar = timestamp_source.clone();

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

    if let Some(entry) = manifest_entry {
        update_manifest_entry_result(
            conn, entry.id, "copied_verified",
            source_hash.as_deref(), Some(asset_id),
            None, None,
        )?;
    }

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
        camera_profile_id: None,
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
        camera_profile_type: None,
        camera_profile_ref: None,
        camera_device_uuid: None,
    };
    let clip_id = insert_clip(conn, &clip)?;
    link_clip_asset(conn, clip_id, asset_id, "primary")?;

    // Legacy camera matching (kept for backward compat)
    let camera_result = crate::camera::matcher::match_camera(
        conn,
        &metadata,
        source_folder.as_deref(),
        None,
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

    // Stable camera refs via App DB priority order (spec 7.2) with full audit trail
    let matching_result = super::matching::resolve_stable_camera_refs_with_audit(
        conn,
        camera_result.profile_id,
        camera_result.device_id,
        None,
        &metadata,
        source_folder.as_deref(),
    );
    let stable_type = Some(matching_result.profile_type.clone());
    let stable_ref = Some(matching_result.profile_ref.clone());
    let device_uuid = matching_result.device_uuid.clone();
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

    // Build extraction status (G7)
    let extraction_status = build_extraction_status(&full_result);

    // Build extended metadata
    let extended_metadata = build_extended_metadata(&full_result);

    // Build match audit trail (Layer 5)
    let match_audit = super::matching::build_match_audit(
        &matching_result,
        &metadata,
        source_folder.as_deref(),
        &full_result.ffprobe_extended,
        &full_result.exif_extended,
    );

    // Write sidecar JSON to .dadcam/sidecars/
    emit_progress_opt(app, &JobProgress::new(job_id_str, "indexing", current, total)
        .with_message(format!("Indexing {}", file_name)));
    let copied_at = copied_at_start.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let indexed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let sidecar_data = super::sidecar::SidecarData {
        original_file_path: source_path.to_string_lossy().to_string(),
        file_hash_blake3: asset.hash_fast.as_ref().cloned(),
        raw_exif_dump: Some(full_result.raw_exif_dump),
        raw_ffprobe: Some(full_result.raw_ffprobe_dump),
        extraction_status: Some(extraction_status),
        metadata_snapshot: super::sidecar::MetadataSnapshot {
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
        extended_metadata: Some(extended_metadata),
        camera_match: super::sidecar::CameraMatchSnapshot {
            device_id: camera_result.device_id,
            profile_id: camera_result.profile_id,
            confidence: matching_result.confidence,
            reason: camera_result.reason.clone(),
            profile_type: stable_type.clone(),
            profile_ref: stable_ref.clone(),
            device_uuid: device_uuid.clone(),
        },
        match_audit: Some(match_audit),
        ingest_timestamps: super::sidecar::IngestTimestamps {
            discovered_at,
            copied_at,
            indexed_at,
        },
        derived_asset_paths: super::sidecar::expected_derived_paths(library_root, clip_id),
        rental_audit: None,
    };
    if let Err(e) = super::sidecar::write_sidecar(library_root, clip_id, &sidecar_data) {
        eprintln!("Warning: Failed to write sidecar for clip {}: {}", clip_id, e);
    }

    update_ingest_file_complete(conn, ingest_file_id, &dest_path, asset_id, clip_id)?;

    // Queue background jobs for the new clip
    queue_post_ingest_jobs(conn, clip_id, asset_id, library_id)?;

    Ok(Some((clip_id, camera_profile_name)))
}

/// Build extraction status from full result (G7).
fn build_extraction_status(
    result: &crate::metadata::FullExtractionResult,
) -> super::sidecar::ExtractionStatus {
    let status = if result.exiftool_success && result.ffprobe_success {
        "extracted"
    } else if result.exiftool_success || result.ffprobe_success {
        "extracted" // partial data is still extracted (G2)
    } else {
        "extraction_failed"
    };

    super::sidecar::ExtractionStatus {
        status: status.to_string(),
        exiftool: super::sidecar::ToolExtractionStatus {
            success: result.exiftool_success,
            exit_code: result.exiftool_exit_code,
            error: result.exiftool_error.clone(),
            pipeline_version: crate::constants::PIPELINE_VERSION,
        },
        ffprobe: super::sidecar::ToolExtractionStatus {
            success: result.ffprobe_success,
            exit_code: result.ffprobe_exit_code,
            error: result.ffprobe_error.clone(),
            pipeline_version: crate::constants::PIPELINE_VERSION,
        },
        extracted_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    }
}

/// Build extended metadata from full extraction result.
fn build_extended_metadata(
    result: &crate::metadata::FullExtractionResult,
) -> super::sidecar::ExtendedMetadata {
    let exif = &result.exif_extended;
    let ff = &result.ffprobe_extended;

    super::sidecar::ExtendedMetadata {
        sensor_type: exif.sensor_type.clone(),
        focal_length: exif.focal_length,
        focal_length_35mm: exif.focal_length_35mm,
        scale_factor: exif.scale_factor,
        native_width: exif.native_width,
        native_height: exif.native_height,
        bits_per_sample: exif.bits_per_sample,
        exif_color_space: exif.color_space.clone(),
        white_balance: exif.white_balance.clone(),
        lens_model: exif.lens_model.clone(),
        lens_id: exif.lens_id.clone(),
        megapixels: exif.megapixels,
        rotation: exif.rotation,
        compressor_id: exif.compressor_id.clone(),
        field_order: ff.field_order.clone(),
        bits_per_raw_sample: ff.bits_per_raw_sample.clone(),
        color_space: ff.color_space.clone(),
        color_primaries: ff.color_primaries.clone(),
        color_transfer: ff.color_transfer.clone(),
        display_aspect_ratio: ff.display_aspect_ratio.clone(),
        sample_aspect_ratio: ff.sample_aspect_ratio.clone(),
        codec_profile: ff.codec_profile.clone(),
        codec_level: ff.codec_level,
    }
}

/// Queue post-ingest jobs: hash_full verification + preview generation.
pub(crate) fn queue_post_ingest_jobs(
    conn: &Connection, clip_id: i64, asset_id: i64, library_id: i64,
) -> Result<()> {
    use crate::db::schema::NewJob;
    for (job_type, aid, priority) in [
        ("hash_full", Some(asset_id), 2),
        ("thumb",     None,           8),
        ("proxy",     None,           5),
        ("sprite",    None,           3),
    ] {
        insert_job(conn, &NewJob {
            job_type: job_type.to_string(),
            library_id: Some(library_id),
            clip_id: Some(clip_id),
            asset_id: aid,
            priority,
            payload: "{}".to_string(),
        })?;
    }
    Ok(())
}

/// Determine timestamp using precedence rules
pub(crate) fn determine_timestamp(
    path: &Path, metadata: &crate::metadata::MediaMetadata,
) -> Result<(Option<String>, Option<String>)> {
    if let Some(ref recorded_at) = metadata.recorded_at {
        return Ok((Some(recorded_at.clone()), Some("metadata".to_string())));
    }

    if let Some(parent) = path.parent() {
        if let Some(folder_name) = parent.file_name().and_then(|n| n.to_str()) {
            if let Some(date) = parse_folder_date(folder_name) {
                return Ok((Some(date), Some("folder".to_string())));
            }
        }
    }

    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            let datetime: chrono::DateTime<chrono::Utc> = modified.into();
            return Ok((
                Some(datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                Some("filesystem".to_string()),
            ));
        }
    }

    Ok((None, None))
}
