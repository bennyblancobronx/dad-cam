// Camera system Tauri commands (Phase 2: App DB)
// Camera profiles and devices now live in App DB (~/.dadcam/app.db),
// not the Library DB. This means cameras work even with no library open.

use serde::{Deserialize, Serialize};
use crate::db::app_db;
use crate::db::app_schema::{
    self, AppBundledProfile, AppUserProfile, AppCameraDevice,
    NewUserProfile, NewAppCameraDevice,
};

// ---------------------------------------------------------------------------
// Combined profile view (bundled + user, for frontend)
// ---------------------------------------------------------------------------

/// Unified camera profile returned to frontend.
/// Wraps both bundled and user profiles with a consistent shape.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraProfileView {
    pub profile_type: String,   // "bundled" or "user"
    pub profile_ref: String,    // slug (bundled) or uuid (user)
    pub name: String,
    pub version: i32,
    pub match_rules: String,
    pub transform_rules: String,
}

/// List all camera profiles (bundled + user) from App DB.
/// Does NOT require a library to be open.
#[tauri::command]
pub fn list_camera_profiles() -> Result<Vec<CameraProfileView>, String> {
    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    let bundled = app_schema::list_bundled_profiles(&conn).map_err(|e| e.to_string())?;
    let user = app_schema::list_user_profiles(&conn).map_err(|e| e.to_string())?;

    let mut views: Vec<CameraProfileView> = Vec::with_capacity(bundled.len() + user.len());

    for p in bundled {
        views.push(CameraProfileView {
            profile_type: "bundled".to_string(),
            profile_ref: p.slug,
            name: p.name,
            version: p.version,
            match_rules: p.match_rules,
            transform_rules: p.transform_rules,
        });
    }

    for p in user {
        views.push(CameraProfileView {
            profile_type: "user".to_string(),
            profile_ref: p.uuid,
            name: p.name,
            version: p.version,
            match_rules: p.match_rules,
            transform_rules: p.transform_rules,
        });
    }

    Ok(views)
}

// ---------------------------------------------------------------------------
// User profile CRUD
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserProfileParams {
    pub name: String,
    pub match_rules: Option<String>,
    pub transform_rules: Option<String>,
}

#[tauri::command]
pub fn create_user_camera_profile(params: CreateUserProfileParams) -> Result<AppUserProfile, String> {
    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    let profile = app_schema::create_user_profile(&conn, &NewUserProfile {
        name: params.name,
        match_rules: params.match_rules,
        transform_rules: params.transform_rules,
    }).map_err(|e| e.to_string())?;
    Ok(profile)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserProfileParams {
    pub uuid: String,
    pub name: Option<String>,
    pub match_rules: Option<String>,
    pub transform_rules: Option<String>,
}

#[tauri::command]
pub fn update_user_camera_profile(params: UpdateUserProfileParams) -> Result<(), String> {
    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    app_schema::update_user_profile(
        &conn,
        &params.uuid,
        params.name.as_deref(),
        params.match_rules.as_deref(),
        params.transform_rules.as_deref(),
    ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_user_camera_profile(uuid: String) -> Result<(), String> {
    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    app_schema::delete_user_profile(&conn, &uuid).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Camera devices (App DB)
// ---------------------------------------------------------------------------

/// List all camera devices from App DB.
/// Does NOT require a library to be open.
#[tauri::command]
pub fn list_camera_devices() -> Result<Vec<AppCameraDevice>, String> {
    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    app_schema::list_camera_devices_app(&conn).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceParams {
    pub profile_type: Option<String>,
    pub profile_ref: Option<String>,
    pub serial_number: Option<String>,
    pub fleet_label: Option<String>,
    pub rental_notes: Option<String>,
    pub capture_usb: bool,
}

/// Register a new camera device in App DB.
#[tauri::command]
pub fn register_camera_device(params: RegisterDeviceParams) -> Result<AppCameraDevice, String> {
    // License check
    if !crate::licensing::is_allowed("camera_registration") {
        return Err("License required: camera registration is not available in trial-expired mode".to_string());
    }

    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    // Capture USB fingerprints if requested (best-effort)
    let usb_fps = if params.capture_usb {
        crate::camera::devices::capture_usb_fingerprint().unwrap_or_default()
    } else {
        Vec::new()
    };

    let device = app_schema::create_camera_device(&conn, &NewAppCameraDevice {
        profile_type: params.profile_type,
        profile_ref: params.profile_ref,
        serial_number: params.serial_number,
        fleet_label: params.fleet_label,
        usb_fingerprints: usb_fps,
        rental_notes: params.rental_notes,
    }).map_err(|e| e.to_string())?;

    Ok(device)
}

// ---------------------------------------------------------------------------
// Camera matching (uses App DB profiles + Library DB clip data)
// ---------------------------------------------------------------------------

/// Match result with stable references
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraMatchResult {
    pub device_uuid: Option<String>,
    pub profile_type: Option<String>,
    pub profile_ref: Option<String>,
    pub profile_name: Option<String>,
    pub device_label: Option<String>,
    pub confidence: f64,
    pub reason: String,
}

/// Match a clip against camera profiles using App DB (spec section 7.2 priority order).
/// 1. Registered device match (USB fingerprint / serial -> device UUID -> assigned profile)
/// 2. User profiles rules engine (App DB user_profiles.match_rules)
/// 3. Bundled profiles rules engine (App DB bundled_profiles.match_rules)
/// 4. Generic fallback (none)
#[tauri::command]
pub fn match_camera(
    db: tauri::State<'_, super::DbState>,
    clip_id: i64,
) -> Result<CameraMatchResult, String> {
    let lib_conn = db.connect()?;
    let app_conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    // Get stored clip data for matching
    let clip_data = lib_conn.query_row(
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
    let asset_path: Option<String> = lib_conn.query_row(
        "SELECT a.path FROM assets a
         JOIN clip_assets ca ON ca.asset_id = a.id
         WHERE ca.clip_id = ?1 AND ca.role = 'primary'",
        [clip_id],
        |row| row.get(0),
    ).ok();

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

    let source_folder = clip_data.1.as_deref();

    // Priority 1: Registered device by serial number (from metadata)
    if let Some(ref serial) = metadata.serial_number {
        if let Ok(Some(device)) = app_schema::find_device_by_serial_app(&app_conn, serial) {
            if device.profile_type != "none" && !device.profile_ref.is_empty() {
                let profile_name = resolve_profile_name(&app_conn, &device.profile_type, &device.profile_ref);
                return Ok(CameraMatchResult {
                    device_uuid: Some(device.uuid),
                    profile_type: Some(device.profile_type),
                    profile_ref: Some(device.profile_ref),
                    profile_name,
                    device_label: device.fleet_label,
                    confidence: 0.95,
                    reason: "Serial number match to registered device".to_string(),
                });
            }
            // Device found but no assigned profile -- continue matching, keep device_uuid
            let (ptype, pref, pname, confidence, reason) = match_profile_from_app_db(
                &app_conn, &metadata, source_folder,
            );
            return Ok(CameraMatchResult {
                device_uuid: Some(device.uuid),
                profile_type: Some(ptype),
                profile_ref: Some(pref),
                profile_name: pname,
                device_label: device.fleet_label,
                confidence,
                reason,
            });
        }
    }

    // Priority 2-4: Profile matching from App DB (user > bundled > fallback)
    let (ptype, pref, pname, confidence, reason) = match_profile_from_app_db(
        &app_conn, &metadata, source_folder,
    );

    Ok(CameraMatchResult {
        device_uuid: None,
        profile_type: Some(ptype),
        profile_ref: Some(pref),
        profile_name: pname,
        device_label: None,
        confidence,
        reason,
    })
}

/// Match profile from App DB using spec 7.2 priority: user > bundled > fallback.
/// Returns (profile_type, profile_ref, profile_name, confidence, reason).
fn match_profile_from_app_db(
    app_conn: &rusqlite::Connection,
    metadata: &crate::metadata::MediaMetadata,
    source_folder: Option<&str>,
) -> (String, String, Option<String>, f64, String) {
    use crate::ingest::{match_app_profile_rules, match_bundled_profile_rules};

    // Priority 2: User profiles
    if let Ok(user_profiles) = app_schema::list_user_profiles(app_conn) {
        if let Some(uuid) = match_app_profile_rules(&user_profiles, metadata, source_folder) {
            let name = user_profiles.iter().find(|p| p.uuid == uuid).map(|p| p.name.clone());
            return ("user".to_string(), uuid, name, 0.80, "User profile match".to_string());
        }
    }

    // Priority 3: Bundled profiles
    if let Ok(bundled) = app_schema::list_bundled_profiles(app_conn) {
        if let Some(slug) = match_bundled_profile_rules(&bundled, metadata, source_folder) {
            let name = bundled.iter().find(|p| p.slug == slug).map(|p| p.name.clone());
            return ("bundled".to_string(), slug, name, 0.80, "Bundled profile match".to_string());
        }
    }

    // Priority 4: Generic fallback
    ("none".to_string(), String::new(), None, 0.0, "No camera match (generic fallback)".to_string())
}

/// Look up profile display name from App DB by type+ref.
fn resolve_profile_name(
    app_conn: &rusqlite::Connection,
    profile_type: &str,
    profile_ref: &str,
) -> Option<String> {
    match profile_type {
        "bundled" => {
            app_schema::list_bundled_profiles(app_conn).ok()
                .and_then(|ps| ps.into_iter().find(|p| p.slug == profile_ref).map(|p| p.name))
        }
        "user" => {
            app_schema::get_user_profile(app_conn, profile_ref).ok()
                .flatten()
                .map(|p| p.name)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Import / Export (App DB)
// ---------------------------------------------------------------------------

/// Import result
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCameraDbResult {
    pub profiles_imported: u32,
    pub devices_imported: u32,
}

/// Combined JSON format for import/export
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CameraExportFormat {
    #[serde(default)]
    bundled_profiles: Vec<AppBundledProfile>,
    #[serde(default)]
    user_profiles: Vec<AppUserProfile>,
    #[serde(default)]
    devices: Vec<AppCameraDevice>,
}

#[tauri::command]
pub fn import_camera_db(json_path: String) -> Result<ImportCameraDbResult, String> {
    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    let path = std::path::Path::new(&json_path);
    if !path.exists() {
        return Err(format!("File not found: {}", json_path));
    }

    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut profiles_imported = 0u32;
    let mut devices_imported = 0u32;

    if let Ok(combined) = serde_json::from_str::<CameraExportFormat>(&content) {
        // Import user profiles by UUID (skip duplicates)
        for profile in &combined.user_profiles {
            if app_schema::get_user_profile(&conn, &profile.uuid).map_err(|e| e.to_string())?.is_none() {
                app_schema::create_user_profile(&conn, &NewUserProfile {
                    name: profile.name.clone(),
                    match_rules: Some(profile.match_rules.clone()),
                    transform_rules: Some(profile.transform_rules.clone()),
                }).map_err(|e| e.to_string())?;
                profiles_imported += 1;
            }
        }

        // Import devices by UUID (skip duplicates)
        for device in &combined.devices {
            if app_schema::get_camera_device_by_uuid(&conn, &device.uuid).map_err(|e| e.to_string())?.is_none() {
                app_schema::upsert_camera_device(&conn, device).map_err(|e| e.to_string())?;
                devices_imported += 1;
            }
        }
    }

    Ok(ImportCameraDbResult {
        profiles_imported,
        devices_imported,
    })
}

/// Export result
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCameraDbResult {
    pub bundled_profiles_count: usize,
    pub user_profiles_count: usize,
    pub devices_count: usize,
}

#[tauri::command]
pub fn export_camera_db(output_path: String) -> Result<ExportCameraDbResult, String> {
    let conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    let bundled = app_schema::list_bundled_profiles(&conn).map_err(|e| e.to_string())?;
    let user = app_schema::list_user_profiles(&conn).map_err(|e| e.to_string())?;
    let devices = app_schema::list_camera_devices_app(&conn).map_err(|e| e.to_string())?;

    let export = CameraExportFormat {
        bundled_profiles: bundled.clone(),
        user_profiles: user.clone(),
        devices: devices.clone(),
    };

    let json = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
    std::fs::write(&output_path, json).map_err(|e| e.to_string())?;

    Ok(ExportCameraDbResult {
        bundled_profiles_count: bundled.len(),
        user_profiles_count: user.len(),
        devices_count: devices.len(),
    })
}
