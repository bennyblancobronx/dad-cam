// Camera system Tauri commands (Phase 5)

use tauri::State;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use super::DbState;
use crate::camera;
use crate::camera::devices::{self, CameraDevice, NewCameraDevice};
use crate::camera::matcher::{self, CameraMatchResult};

// --- list_camera_profiles ---

#[tauri::command]
pub fn list_camera_profiles(db: State<'_, DbState>) -> Result<Vec<camera::CameraProfile>, String> {
    let conn_guard = db.0.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("No library open")?;
    camera::get_all_profiles(conn).map_err(|e| e.to_string())
}

// --- list_camera_devices ---

#[tauri::command]
pub fn list_camera_devices(db: State<'_, DbState>) -> Result<Vec<CameraDevice>, String> {
    let conn_guard = db.0.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("No library open")?;
    devices::get_all_devices(conn).map_err(|e| e.to_string())
}

// --- register_camera_device ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceParams {
    pub profile_id: Option<i64>,
    pub serial_number: Option<String>,
    pub fleet_label: Option<String>,
    pub rental_notes: Option<String>,
    pub capture_usb: bool,
}

#[tauri::command]
pub fn register_camera_device(
    db: State<'_, DbState>,
    params: RegisterDeviceParams,
) -> Result<CameraDevice, String> {
    // License check: registration blocked when trial expired
    if !crate::licensing::is_allowed("camera_registration") {
        return Err("License required: camera registration is not available in trial-expired mode".to_string());
    }

    let conn_guard = db.0.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("No library open")?;

    // Capture USB fingerprints if requested (best-effort, never errors)
    let usb_fps = if params.capture_usb {
        devices::capture_usb_fingerprint().unwrap_or_default()
    } else {
        Vec::new()
    };

    let new_device = NewCameraDevice {
        profile_id: params.profile_id,
        serial_number: params.serial_number,
        fleet_label: params.fleet_label,
        usb_fingerprints: usb_fps,
        rental_notes: params.rental_notes,
    };

    let result = devices::insert_device(conn, &new_device).map_err(|e| e.to_string())?;

    // Sync devices to ~/.dadcam/custom_cameras.json (best-effort)
    if let Err(e) = devices::save_devices_to_json(conn) {
        eprintln!("Warning: Failed to save custom_cameras.json: {}", e);
    }

    Ok(result)
}

// --- match_camera ---

#[tauri::command]
pub fn match_camera(db: State<'_, DbState>, clip_id: i64) -> Result<CameraMatchResult, String> {
    let conn_guard = db.0.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("No library open")?;

    // Get stored clip data for matching
    let clip_data = conn.query_row(
        "SELECT codec, source_folder, width, height, fps, duration_ms,
                audio_codec, audio_channels, audio_sample_rate
         FROM clips WHERE id = ?1",
        [clip_id],
        |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i32>>(2)?,
                row.get::<_, Option<i32>>(3)?,
                row.get::<_, Option<f64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<i32>>(7)?,
                row.get::<_, Option<i32>>(8)?,
            ))
        },
    ).map_err(|e| e.to_string())?;

    // Try to get the original asset path for full metadata extraction
    let asset_path: Option<String> = conn.query_row(
        "SELECT a.path FROM assets a
         JOIN clip_assets ca ON ca.asset_id = a.id
         WHERE ca.clip_id = ?1 AND ca.role = 'primary'",
        [clip_id],
        |row| row.get(0),
    ).ok();

    // Try to extract full metadata from file; fall back to stored clip data
    let metadata = asset_path
        .as_ref()
        .and_then(|p| {
            let path = std::path::Path::new(p);
            if path.exists() {
                crate::metadata::extract_metadata(path).ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            // Build partial MediaMetadata from stored clip data
            crate::metadata::MediaMetadata {
                codec: clip_data.0.clone(),
                width: clip_data.2,
                height: clip_data.3,
                fps: clip_data.4,
                duration_ms: clip_data.5,
                audio_codec: clip_data.6.clone(),
                audio_channels: clip_data.7,
                audio_sample_rate: clip_data.8,
                media_type: "video".to_string(),
                ..Default::default()
            }
        });

    let result = matcher::match_camera(
        conn,
        &metadata,
        clip_data.1.as_deref(),
        None, // No USB fingerprints available for existing clips
    );

    Ok(result)
}

// --- import_camera_db ---

/// Import result containing counts for both profiles and devices
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCameraDbResult {
    pub profiles_imported: u32,
    pub devices_imported: u32,
}

/// Combined JSON format matching what export_camera_db produces:
/// { "profiles": [...], "devices": [...] }
#[derive(Debug, Deserialize)]
struct CameraExportFormat {
    #[serde(default)]
    profiles: Vec<camera::CameraProfile>,
    #[serde(default)]
    devices: Vec<CameraDevice>,
}

#[tauri::command]
pub fn import_camera_db(db: State<'_, DbState>, json_path: String) -> Result<ImportCameraDbResult, String> {
    let conn_guard = db.0.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("No library open")?;

    let path = std::path::Path::new(&json_path);
    if !path.exists() {
        return Err(format!("File not found: {}", json_path));
    }

    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut profiles_imported = 0u32;
    let mut devices_imported = 0u32;

    // Try the combined export format first (has profiles + devices keys)
    if let Ok(combined) = serde_json::from_str::<CameraExportFormat>(&content) {
        // Import profiles by name (skip duplicates)
        for profile in &combined.profiles {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM camera_profiles WHERE name = ?1)",
                [&profile.name],
                |row| row.get(0),
            ).map_err(|e| e.to_string())?;

            if !exists {
                camera::insert_profile(conn, profile).map_err(|e| e.to_string())?;
                profiles_imported += 1;
            }
        }

        // Import devices by UUID (skip duplicates)
        for device in &combined.devices {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM camera_devices WHERE uuid = ?1)",
                [&device.uuid],
                |row| row.get(0),
            ).unwrap_or(true);

            if !exists {
                let fps_json = serde_json::to_string(&device.usb_fingerprints)
                    .unwrap_or_else(|_| "[]".to_string());
                match conn.execute(
                    "INSERT INTO camera_devices (uuid, profile_id, serial_number, fleet_label, usb_fingerprints, rental_notes)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        device.uuid,
                        device.profile_id,
                        device.serial_number,
                        device.fleet_label,
                        fps_json,
                        device.rental_notes,
                    ],
                ) {
                    Ok(_) => devices_imported += 1,
                    Err(e) => eprintln!("Warning: Failed to import device {}: {}", device.uuid, e),
                }
            }
        }
    } else {
        // Plain array format (legacy profiles-only JSON like canonical.json)
        profiles_imported = camera::bundled::load_bundled_profiles(conn, path)
            .map_err(|e| e.to_string())?;
    }

    Ok(ImportCameraDbResult {
        profiles_imported,
        devices_imported,
    })
}

// --- export_camera_db ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCameraDbResult {
    pub profiles_count: usize,
    pub devices_count: usize,
}

#[tauri::command]
pub fn export_camera_db(db: State<'_, DbState>, output_path: String) -> Result<ExportCameraDbResult, String> {
    let conn_guard = db.0.lock().map_err(|e| e.to_string())?;
    let conn = conn_guard.as_ref().ok_or("No library open")?;

    let profiles = camera::get_all_profiles(conn).map_err(|e| e.to_string())?;
    let devices = devices::get_all_devices(conn).map_err(|e| e.to_string())?;

    #[derive(Serialize)]
    struct CameraExport {
        profiles: Vec<camera::CameraProfile>,
        devices: Vec<CameraDevice>,
    }

    let export = CameraExport {
        profiles: profiles.clone(),
        devices: devices.clone(),
    };

    let json = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
    std::fs::write(&output_path, json).map_err(|e| e.to_string())?;

    Ok(ExportCameraDbResult {
        profiles_count: profiles.len(),
        devices_count: devices.len(),
    })
}
