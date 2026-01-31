// Re-extraction job: re-run exiftool + ffprobe on existing clips
// Triggered manually or by pipeline_version bump.
// Requires original file access (skips if file missing).

use rusqlite::Connection;
use std::path::Path;
use crate::error::Result;
use crate::constants::{METADATA_PIPELINE_VERSION, DADCAM_FOLDER, SIDECARS_FOLDER};

/// Re-extract metadata for all clips in a library that need it.
/// Returns count of clips re-extracted.
pub fn reextract_library(
    conn: &Connection,
    library_id: i64,
    library_root: &Path,
) -> Result<usize> {
    // Find clips that need re-extraction:
    // - metadata_status = 'extraction_failed' (retry)
    // - OR asset pipeline_version < current (outdated extraction)
    let mut stmt = conn.prepare(
        "SELECT c.id, a.path, a.id as asset_id, a.pipeline_version
         FROM clips c
         JOIN assets a ON c.original_asset_id = a.id
         WHERE c.library_id = ?1
         AND (c.metadata_status = 'extraction_failed'
              OR a.pipeline_version IS NULL
              OR a.pipeline_version < ?2)"
    )?;

    let clips: Vec<(i64, String, i64, Option<i32>)> = stmt.query_map(
        rusqlite::params![library_id, METADATA_PIPELINE_VERSION as i32],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?.filter_map(|r| r.ok()).collect();

    let mut reextracted = 0usize;

    for (clip_id, asset_path, asset_id, _old_version) in &clips {
        // Resolve actual file path
        let file_path = resolve_file_path(asset_path, library_root);
        let source_path = Path::new(&file_path);

        if !source_path.exists() {
            // File not accessible (reference mode, drive disconnected), skip
            continue;
        }

        match reextract_single_clip(conn, *clip_id, *asset_id, source_path, library_root) {
            Ok(()) => reextracted += 1,
            Err(e) => {
                eprintln!("Warning: reextract failed for clip {}: {}", clip_id, e);
            }
        }
    }

    Ok(reextracted)
}

/// Re-extract metadata for a single clip.
fn reextract_single_clip(
    conn: &Connection,
    clip_id: i64,
    asset_id: i64,
    source_path: &Path,
    library_root: &Path,
) -> Result<()> {
    // Set status to extracting (crash recovery marker)
    conn.execute(
        "UPDATE clips SET metadata_status = 'extracting' WHERE id = ?1",
        [clip_id],
    )?;

    // Run full extraction
    let full_result = crate::metadata::extract_metadata_full(source_path)?;

    // Update clip with new metadata
    let meta = &full_result.metadata;
    conn.execute(
        "UPDATE clips SET
            duration_ms = ?2, width = ?3, height = ?4, fps = ?5,
            codec = ?6, audio_codec = ?7, audio_channels = ?8,
            audio_sample_rate = ?9, metadata_status = ?10
         WHERE id = ?1",
        rusqlite::params![
            clip_id,
            meta.duration_ms, meta.width, meta.height, meta.fps,
            meta.codec, meta.audio_codec, meta.audio_channels,
            meta.audio_sample_rate,
            if full_result.ffprobe_success || full_result.exiftool_success {
                "extracted"
            } else {
                "extraction_failed"
            },
        ],
    )?;

    // Update asset pipeline_version
    conn.execute(
        "UPDATE assets SET pipeline_version = ?2 WHERE id = ?1",
        rusqlite::params![asset_id, METADATA_PIPELINE_VERSION as i32],
    )?;

    // Update sidecar with new raw dumps
    update_sidecar_raw_dumps(clip_id, library_root, &full_result)?;

    // Reset to extracted -> needs re-matching
    conn.execute(
        "UPDATE clips SET metadata_status = 'extracted' WHERE id = ?1 AND metadata_status = 'extracting'",
        [clip_id],
    )?;

    Ok(())
}

/// Update sidecar file with new raw dumps from re-extraction.
fn update_sidecar_raw_dumps(
    clip_id: i64,
    library_root: &Path,
    result: &crate::metadata::FullExtractionResult,
) -> Result<()> {
    let sidecar_path = library_root
        .join(DADCAM_FOLDER)
        .join(SIDECARS_FOLDER)
        .join(format!("{}.json", clip_id));

    // Read existing sidecar
    let existing_json = std::fs::read_to_string(&sidecar_path).unwrap_or_default();
    let mut sidecar: serde_json::Value = serde_json::from_str(&existing_json)
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    // Update raw dumps
    if let Some(obj) = sidecar.as_object_mut() {
        obj.insert("rawExifDump".to_string(), result.raw_exif_dump.clone());
        obj.insert("rawFfprobe".to_string(), result.raw_ffprobe_dump.clone());

        // Update extraction status
        let status = serde_json::json!({
            "status": if result.exiftool_success || result.ffprobe_success { "extracted" } else { "extraction_failed" },
            "exiftool": {
                "success": result.exiftool_success,
                "exitCode": result.exiftool_exit_code,
                "error": result.exiftool_error,
                "pipelineVersion": METADATA_PIPELINE_VERSION
            },
            "ffprobe": {
                "success": result.ffprobe_success,
                "exitCode": result.ffprobe_exit_code,
                "error": result.ffprobe_error,
                "pipelineVersion": METADATA_PIPELINE_VERSION
            },
            "extractedAt": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
        });
        obj.insert("extractionStatus".to_string(), status);
    }

    // Atomic write
    let sidecars_dir = library_root.join(DADCAM_FOLDER).join(SIDECARS_FOLDER);
    std::fs::create_dir_all(&sidecars_dir)?;
    let tmp_path = sidecars_dir.join(format!(".tmp_{}.json", clip_id));
    let final_path = sidecars_dir.join(format!("{}.json", clip_id));

    let json = serde_json::to_string_pretty(&sidecar)?;
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp_path, &final_path)?;

    Ok(())
}

/// Resolve file path -- handles ref: prefix for reference mode.
fn resolve_file_path(asset_path: &str, library_root: &Path) -> String {
    if let Some(ref_path) = asset_path.strip_prefix("ref:") {
        ref_path.to_string()
    } else {
        library_root.join(asset_path).to_string_lossy().to_string()
    }
}

/// Queue a reextract job for a library.
pub fn queue_reextract_job(conn: &Connection, library_id: i64) -> Result<i64> {
    crate::db::schema::insert_job(conn, &crate::db::schema::NewJob {
        job_type: "reextract".to_string(),
        library_id: Some(library_id),
        clip_id: None,
        asset_id: None,
        priority: 4,
        payload: "{}".to_string(),
    })
}
