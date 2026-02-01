// Re-matching job: re-evaluate camera profiles for existing clips
// Triggered when bundled_profiles.json changes or manually from settings.
// Uses stored inputSignature from sidecar matchAudit -- no file access needed.

use rusqlite::Connection;
use crate::error::Result;
use crate::constants::{DADCAM_FOLDER, SIDECARS_FOLDER};

/// Re-match all generic-fallback clips in a library against current profiles.
/// Returns count of clips upgraded to a better profile.
pub fn rematch_library(conn: &Connection, library_id: i64, library_root: &std::path::Path) -> Result<usize> {
    // Find clips that are generic-fallback or have older matcher version
    let mut stmt = conn.prepare(
        "SELECT id, camera_profile_ref, source_folder FROM clips
         WHERE library_id = ?1
         AND (camera_profile_ref = 'generic-fallback' OR camera_profile_ref IS NULL)"
    )?;

    let clips: Vec<(i64, Option<String>, Option<String>)> = stmt.query_map(
        [library_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?.filter_map(|r| r.ok()).collect();

    let mut upgraded = 0usize;

    for (clip_id, _old_ref, source_folder) in &clips {
        match rematch_single_clip(conn, *clip_id, source_folder.as_deref(), library_root) {
            Ok(true) => upgraded += 1,
            Ok(false) => {}
            Err(e) => {
                log::warn!("rematch failed for clip {}: {}", clip_id, e);
            }
        }
    }

    Ok(upgraded)
}

/// Re-match a single clip using its sidecar inputSignature.
/// Returns true if profile was upgraded.
fn rematch_single_clip(
    conn: &Connection,
    clip_id: i64,
    source_folder: Option<&str>,
    library_root: &std::path::Path,
) -> Result<bool> {
    // Read sidecar to get inputSignature
    let sidecar_path = library_root
        .join(DADCAM_FOLDER)
        .join(SIDECARS_FOLDER)
        .join(format!("{}.json", clip_id));

    let sidecar_json = match std::fs::read_to_string(&sidecar_path) {
        Ok(s) => s,
        Err(_) => return Ok(false), // No sidecar, skip
    };

    let sidecar: serde_json::Value = match serde_json::from_str(&sidecar_json) {
        Ok(v) => v,
        Err(_) => return Ok(false), // Corrupt sidecar, skip
    };

    // Extract inputSignature from matchAudit
    let input_sig = match sidecar.get("matchAudit").and_then(|a| a.get("inputSignature")) {
        Some(sig) => sig,
        None => return Ok(false), // No audit trail, skip
    };

    // Rebuild MediaMetadata from inputSignature
    let metadata = crate::metadata::MediaMetadata {
        camera_make: input_sig.get("make").and_then(|v| v.as_str()).map(|s| s.to_string()),
        camera_model: input_sig.get("model").and_then(|v| v.as_str()).map(|s| s.to_string()),
        serial_number: input_sig.get("serial").and_then(|v| v.as_str()).map(|s| s.to_string()),
        codec: input_sig.get("codec").and_then(|v| v.as_str()).map(|s| s.to_string()),
        container: input_sig.get("container").and_then(|v| v.as_str()).map(|s| s.to_string()),
        width: input_sig.get("width").and_then(|v| v.as_i64()).map(|v| v as i32),
        height: input_sig.get("height").and_then(|v| v.as_i64()).map(|v| v as i32),
        fps: input_sig.get("fps").and_then(|v| v.as_f64()),
        media_type: "video".to_string(),
        ..Default::default()
    };

    let folder = input_sig.get("folderPath")
        .and_then(|v| v.as_str())
        .or(source_folder);

    // Run matching with audit
    let result = crate::ingest::resolve_stable_camera_refs_with_audit(
        conn, None, None, None, &metadata, folder,
    );

    // Only upgrade if new profile is NOT generic-fallback
    if result.profile_ref == "generic-fallback" {
        return Ok(false);
    }

    // Update clip
    crate::db::schema::update_clip_camera_refs(
        conn,
        clip_id,
        Some(&result.profile_type),
        Some(&result.profile_ref),
        result.device_uuid.as_deref(),
    )?;

    // Invalidate proxy if profile changed (G13)
    invalidate_clip_proxy(conn, clip_id)?;

    Ok(true)
}

/// Mark a clip's proxy as stale by setting pipeline_version = 0 (G13).
fn invalidate_clip_proxy(conn: &Connection, clip_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE assets SET pipeline_version = 0
         WHERE id IN (SELECT asset_id FROM clip_assets WHERE clip_id = ?1 AND role = 'proxy')",
        [clip_id],
    )?;
    Ok(())
}

/// Queue a rematch job for a library.
pub fn queue_rematch_job(conn: &Connection, library_id: i64) -> Result<i64> {
    crate::db::schema::insert_job(conn, &crate::db::schema::NewJob {
        job_type: "rematch".to_string(),
        library_id: Some(library_id),
        clip_id: None,
        asset_id: None,
        priority: 3,
        payload: "{}".to_string(),
    })
}
