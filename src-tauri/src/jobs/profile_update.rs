// Step 11: Backflow on profile add/update
// When bundled_profiles.json changes or user creates/edits a profile:
// - Re-match generic-fallback clips using stored inputSignature
// - Re-check devices with profile_type='none' against new profiles
// - Invalidate proxies if profile changed (G13)

use rusqlite::Connection;
use crate::error::Result;
use crate::constants::{DADCAM_FOLDER, SIDECARS_FOLDER};

/// Re-match generic-fallback clips in a library after profile changes.
/// Uses stored inputSignature from sidecar matchAudit (no file access needed).
/// Returns count of clips upgraded.
pub fn rematch_on_profile_change(
    conn: &Connection,
    library_id: i64,
    library_root: &std::path::Path,
) -> Result<usize> {
    // Find all generic-fallback clips in this library
    let mut stmt = conn.prepare(
        "SELECT id, source_folder FROM clips
         WHERE library_id = ?1
         AND (camera_profile_ref = 'generic-fallback' OR camera_profile_ref IS NULL)"
    )?;

    let clips: Vec<(i64, Option<String>)> = stmt.query_map(
        [library_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?.filter_map(|r| r.ok()).collect();

    let mut upgraded = 0usize;

    for (clip_id, source_folder) in &clips {
        match rematch_clip_from_sidecar(conn, *clip_id, source_folder.as_deref(), library_root) {
            Ok(true) => upgraded += 1,
            Ok(false) => {}
            Err(e) => {
                log::warn!("profile-change rematch failed for clip {}: {}", clip_id, e);
            }
        }
    }

    Ok(upgraded)
}

/// Re-match a single clip using its sidecar inputSignature.
/// Returns true if the clip was upgraded from generic-fallback.
fn rematch_clip_from_sidecar(
    conn: &Connection,
    clip_id: i64,
    source_folder: Option<&str>,
    library_root: &std::path::Path,
) -> Result<bool> {
    let sidecar_path = library_root
        .join(DADCAM_FOLDER)
        .join(SIDECARS_FOLDER)
        .join(format!("{}.json", clip_id));

    let sidecar_json = match std::fs::read_to_string(&sidecar_path) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };

    let sidecar: serde_json::Value = match serde_json::from_str(&sidecar_json) {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };

    let input_sig = match sidecar.get("matchAudit").and_then(|a| a.get("inputSignature")) {
        Some(sig) => sig,
        None => return Ok(false),
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

    // Invalidate proxy (G13)
    conn.execute(
        "UPDATE assets SET pipeline_version = 0
         WHERE id IN (SELECT asset_id FROM clip_assets WHERE clip_id = ?1 AND role = 'proxy')",
        [clip_id],
    )?;

    Ok(true)
}

/// Re-check devices with profile_type='none' against current profiles.
/// If a device's stored EXIF dump now matches a profile, suggest assignment.
/// Returns list of (device_uuid, suggested_profile_type, suggested_profile_ref).
pub fn check_unassigned_devices() -> Vec<(String, String, String)> {
    let mut suggestions = Vec::new();

    let app_conn = match crate::db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(_) => return suggestions,
    };

    let devices = match crate::db::app_schema::list_camera_devices_app(&app_conn) {
        Ok(d) => d,
        Err(_) => return suggestions,
    };

    let dump_dir = match directories::BaseDirs::new() {
        Some(dirs) => dirs.home_dir().join(".dadcam").join("device_dumps"),
        None => return suggestions,
    };

    for device in &devices {
        if device.profile_type != "none" {
            continue; // Already has a profile
        }

        // Read stored dump
        let dump_path = dump_dir.join(format!("{}.json", device.uuid));
        let content = match std::fs::read_to_string(&dump_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let dump: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let meta_obj = match dump.get("metadata") {
            Some(m) => m,
            None => continue,
        };

        let metadata = crate::metadata::MediaMetadata {
            camera_make: meta_obj.get("make").and_then(|v| v.as_str()).map(|s| s.to_string()),
            camera_model: meta_obj.get("model").and_then(|v| v.as_str()).map(|s| s.to_string()),
            serial_number: meta_obj.get("serial").and_then(|v| v.as_str()).map(|s| s.to_string()),
            codec: meta_obj.get("codec").and_then(|v| v.as_str()).map(|s| s.to_string()),
            container: meta_obj.get("container").and_then(|v| v.as_str()).map(|s| s.to_string()),
            width: meta_obj.get("width").and_then(|v| v.as_i64()).map(|v| v as i32),
            height: meta_obj.get("height").and_then(|v| v.as_i64()).map(|v| v as i32),
            fps: meta_obj.get("fps").and_then(|v| v.as_f64()),
            media_type: "video".to_string(),
            ..Default::default()
        };

        // Try matching against profiles
        if let Ok(user_profiles) = crate::db::app_schema::list_user_profiles(&app_conn) {
            if let Some(uuid) = crate::ingest::match_app_profile_rules(
                &user_profiles, &metadata, None,
            ) {
                suggestions.push((device.uuid.clone(), "user".to_string(), uuid));
                continue;
            }
        }
        if let Ok(bundled) = crate::db::app_schema::list_bundled_profiles(&app_conn) {
            if let Some(slug) = crate::ingest::match_bundled_profile_rules(
                &bundled, &metadata, None,
            ) {
                suggestions.push((device.uuid.clone(), "bundled".to_string(), slug));
            }
        }
    }

    suggestions
}

/// Queue a profile-change rematch job for a library.
pub fn queue_profile_rematch_job(conn: &Connection, library_id: i64) -> Result<i64> {
    crate::db::schema::insert_job(conn, &crate::db::schema::NewJob {
        job_type: "rematch".to_string(),
        library_id: Some(library_id),
        clip_id: None,
        asset_id: None,
        priority: 3,
        payload: r#"{"trigger":"profile_change"}"#.to_string(),
    })
}
