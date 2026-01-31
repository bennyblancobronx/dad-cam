// Device registration: EXIF dump, auto-profile, backflow scan (Steps 8, 10)

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::error::Result;
use crate::constants::VIDEO_EXTENSIONS;

// --- Step 8: Device Registration EXIF Dump + Auto-Profile (G11) ---

/// Discover 1-3 representative video files from a mounted volume.
/// Checks known camera directory structures first, then walks root (max depth 3).
pub fn discover_sample_files(mount_point: &Path) -> Vec<PathBuf> {
    let known_dirs = [
        "DCIM",
        "AVCHD/BDMV/STREAM",
        "PRIVATE/AVCHD/BDMV/STREAM",
        "PRIVATE/M4ROOT/CLIP",
    ];

    for dir in &known_dirs {
        let candidate = mount_point.join(dir);
        if candidate.is_dir() {
            let files = collect_video_files(&candidate, 1);
            if !files.is_empty() {
                return files.into_iter().take(3).collect();
            }
        }
    }

    // No known structure, walk root (max depth 3)
    collect_video_files(mount_point, 3)
        .into_iter()
        .take(3)
        .collect()
}

/// Collect video files from a directory, sorted by size descending (largest first).
fn collect_video_files(dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut files: Vec<(PathBuf, u64)> = Vec::new();
    collect_video_files_recursive(dir, 0, max_depth, &mut files);
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files.into_iter().map(|(p, _)| p).collect()
}

fn collect_video_files_recursive(
    dir: &Path,
    depth: usize,
    max_depth: usize,
    results: &mut Vec<(PathBuf, u64)>,
) {
    if depth > max_depth || results.len() >= 20 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map_or(false, |n| n.to_string_lossy().starts_with('.')) {
                continue;
            }
            collect_video_files_recursive(&path, depth + 1, max_depth, results);
        } else if is_video_extension(&path) {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size > 0 {
                results.push((path, size));
            }
        }
    }
}

fn is_video_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Result of probing a device's sample files for auto-profile matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceProbeResult {
    pub sample_file: String,
    pub metadata: crate::metadata::MediaMetadata,
    pub suggested_profile_type: Option<String>,
    pub suggested_profile_ref: Option<String>,
    pub confidence: f64,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub serial_number: Option<String>,
}

/// Probe sample files from a mount point to get metadata and suggest a profile.
/// Stores the full EXIF dump at ~/.dadcam/device_dumps/<uuid>.json.
pub fn probe_device_sample(
    mount_point: &Path,
    device_uuid: &str,
) -> Result<Option<DeviceProbeResult>> {
    let samples = discover_sample_files(mount_point);
    if samples.is_empty() {
        return Ok(None);
    }

    let sample = &samples[0];
    let full_result = crate::metadata::extract_metadata_full(sample)?;

    // Store raw dump
    store_device_dump(device_uuid, &full_result)?;

    let metadata = &full_result.metadata;
    let folder = sample.parent().and_then(|p| p.to_str());

    // Auto-match against profiles
    let (prof_type, prof_ref, confidence) = match crate::db::app_db::open_app_db_connection() {
        Ok(app_conn) => {
            if let Ok(user_profiles) = crate::db::app_schema::list_user_profiles(&app_conn) {
                if let Some(uuid) = crate::ingest::match_app_profile_rules(
                    &user_profiles, metadata, folder,
                ) {
                    (Some("user".to_string()), Some(uuid), 0.8)
                } else if let Ok(bundled) = crate::db::app_schema::list_bundled_profiles(&app_conn) {
                    if let Some(slug) = crate::ingest::match_bundled_profile_rules(
                        &bundled, metadata, folder,
                    ) {
                        (Some("bundled".to_string()), Some(slug), 0.8)
                    } else {
                        (None, None, 0.0)
                    }
                } else {
                    (None, None, 0.0)
                }
            } else {
                (None, None, 0.0)
            }
        }
        Err(_) => (None, None, 0.0),
    };

    Ok(Some(DeviceProbeResult {
        sample_file: sample.to_string_lossy().to_string(),
        metadata: metadata.clone(),
        suggested_profile_type: prof_type,
        suggested_profile_ref: prof_ref,
        confidence,
        camera_make: metadata.camera_make.clone(),
        camera_model: metadata.camera_model.clone(),
        serial_number: metadata.serial_number.clone(),
    }))
}

/// Store full extraction dump for a device at ~/.dadcam/device_dumps/<uuid>.json.
fn store_device_dump(
    device_uuid: &str,
    result: &crate::metadata::FullExtractionResult,
) -> Result<()> {
    let dump_dir = match directories::BaseDirs::new() {
        Some(dirs) => dirs.home_dir().join(".dadcam").join("device_dumps"),
        None => return Ok(()),
    };
    std::fs::create_dir_all(&dump_dir)?;

    let dump = serde_json::json!({
        "deviceUuid": device_uuid,
        "probedAt": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        "rawExifDump": result.raw_exif_dump,
        "rawFfprobe": result.raw_ffprobe_dump,
        "metadata": {
            "make": result.metadata.camera_make,
            "model": result.metadata.camera_model,
            "serial": result.metadata.serial_number,
            "codec": result.metadata.codec,
            "container": result.metadata.container,
            "width": result.metadata.width,
            "height": result.metadata.height,
            "fps": result.metadata.fps,
        },
        "exiftoolSuccess": result.exiftool_success,
        "ffprobeSuccess": result.ffprobe_success,
    });

    let tmp_path = dump_dir.join(format!(".tmp_{}.json", device_uuid));
    let final_path = dump_dir.join(format!("{}.json", device_uuid));
    let json = serde_json::to_string_pretty(&dump)?;
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

// --- Step 10: Backflow -- Re-match Existing Clips on Device Registration ---

/// Result of backflow scan after device registration (G12).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackflowResult {
    pub device_uuid: String,
    /// Clip IDs auto-assigned by serial or USB match (applied immediately).
    pub auto_assigned: Vec<i64>,
    /// Clips matched by make+model only (NOT applied, suggestion for user).
    pub suggested: Vec<SuggestedMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedMatch {
    pub clip_id: i64,
    pub confidence: f64,
    pub match_method: String,
}

/// After device registration, scan unassigned clips for matches.
/// Auto-assigns serial/USB matches. Returns model matches as suggestions only (G12).
pub fn backflow_scan_for_device(
    lib_conn: &Connection,
    device: &crate::db::app_schema::AppCameraDevice,
) -> Result<BackflowResult> {
    let mut auto_assigned = Vec::new();
    let mut suggested = Vec::new();

    // B1: Find all clips without a device assignment
    let mut stmt = lib_conn.prepare(
        "SELECT id, camera_make, camera_model, serial_number
         FROM clips
         WHERE camera_device_uuid IS NULL OR camera_device_uuid = ''"
    )?;
    let clips: Vec<(i64, Option<String>, Option<String>, Option<String>)> = stmt.query_map(
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?.filter_map(|r| r.ok()).collect();

    for (clip_id, clip_make, clip_model, clip_serial) in &clips {
        // B2: Match by serial number (confidence 0.95)
        if let (Some(dev_serial), Some(c_serial)) = (&device.serial_number, clip_serial) {
            if !dev_serial.is_empty() && dev_serial == c_serial {
                auto_assigned.push(*clip_id);
                continue;
            }
        }

        // B3: Match by make + model (confidence 0.6, suggestion only)
        if device.profile_type != "none" && !device.profile_ref.is_empty() {
            if let Some(dev_mm) = get_device_make_model(device) {
                if let (Some(cm), Some(cmod)) = (clip_make, clip_model) {
                    if matches_make_model(&dev_mm, cm, cmod) {
                        suggested.push(SuggestedMatch {
                            clip_id: *clip_id,
                            confidence: 0.6,
                            match_method: "make_model".to_string(),
                        });
                    }
                }
            }
        }
    }

    // B4: Check ingest sessions for USB fingerprint matches
    if !device.usb_fingerprints.is_empty() {
        let usb_matched = match_by_ingest_session_usb(lib_conn, &clips, &device.usb_fingerprints)?;
        for clip_id in usb_matched {
            if !auto_assigned.contains(&clip_id) {
                auto_assigned.push(clip_id);
            }
        }
    }

    // B5: Apply auto-assigned matches in a transaction
    if !auto_assigned.is_empty() {
        let tx = lib_conn.unchecked_transaction()?;
        for clip_id in &auto_assigned {
            tx.execute(
                "UPDATE clips SET camera_device_uuid = ?1 WHERE id = ?2",
                params![device.uuid, clip_id],
            )?;
            // If device has profile and clip is generic-fallback, upgrade profile too
            if device.profile_type != "none" && !device.profile_ref.is_empty() {
                let rows = tx.execute(
                    "UPDATE clips SET camera_profile_type = ?1, camera_profile_ref = ?2
                     WHERE id = ?3 AND (camera_profile_ref = 'generic-fallback' OR camera_profile_ref IS NULL)",
                    params![device.profile_type, device.profile_ref, clip_id],
                )?;
                // Invalidate proxy if profile actually changed (G13)
                if rows > 0 {
                    tx.execute(
                        "UPDATE assets SET pipeline_version = 0
                         WHERE id IN (SELECT asset_id FROM clip_assets WHERE clip_id = ?1 AND role = 'proxy')",
                        [clip_id],
                    )?;
                }
            }
        }
        tx.commit()?;
    }

    Ok(BackflowResult {
        device_uuid: device.uuid.clone(),
        auto_assigned,
        suggested,
    })
}

/// Try to get make/model from device's stored dump.
fn get_device_make_model(
    device: &crate::db::app_schema::AppCameraDevice,
) -> Option<(String, String)> {
    let dump_dir = directories::BaseDirs::new()?
        .home_dir()
        .join(".dadcam")
        .join("device_dumps");
    let dump_path = dump_dir.join(format!("{}.json", device.uuid));
    let content = std::fs::read_to_string(&dump_path).ok()?;
    let dump: serde_json::Value = serde_json::from_str(&content).ok()?;
    let make = dump.get("metadata")?.get("make")?.as_str()?.to_string();
    let model = dump.get("metadata")?.get("model")?.as_str()?.to_string();
    Some((make, model))
}

fn matches_make_model(device_mm: &(String, String), clip_make: &str, clip_model: &str) -> bool {
    let (dev_make, dev_model) = device_mm;
    clip_make.to_lowercase().contains(&dev_make.to_lowercase())
        && clip_model.to_lowercase().contains(&dev_model.to_lowercase())
}

/// Match clips by ingest session USB fingerprints (B4).
fn match_by_ingest_session_usb(
    conn: &Connection,
    clips: &[(i64, Option<String>, Option<String>, Option<String>)],
    device_fps: &[String],
) -> Result<Vec<i64>> {
    let mut matched = Vec::new();

    for fp in device_fps {
        let escaped_fp = fp.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let pattern = format!("%{}%", escaped_fp);

        let mut stmt = conn.prepare(
            "SELECT DISTINCT c.id FROM clips c
             JOIN ingest_files f ON c.id = f.clip_id
             JOIN ingest_manifest_entries me ON f.source_path = me.relative_path
             JOIN ingest_sessions s ON me.session_id = s.id
             WHERE s.device_serial LIKE ?1 ESCAPE '\\'"
        )?;

        let ids: Vec<i64> = stmt.query_map([&pattern], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for id in ids {
            if clips.iter().any(|(cid, _, _, _)| *cid == id) && !matched.contains(&id) {
                matched.push(id);
            }
        }
    }

    Ok(matched)
}
