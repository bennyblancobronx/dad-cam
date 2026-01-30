// Dad Cam - App Settings Commands
// Persistent app-level settings via App DB (spec section 6.3)
// Settings are stored in the app_settings KV table at ~/.dadcam/app.db.
// The Tauri Store (settings.json) is migrated once at startup (lib.rs) and
// no longer used as the primary store.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;
use rusqlite::{Connection, params};

use crate::db;
use crate::constants;

/// App settings schema version
const SETTINGS_VERSION: u32 = 2;
const MAX_RECENT_PROJECTS: usize = 10;

/// App mode: simple (single project, auto-open) or advanced (multi-project)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Simple,
    Advanced,
}

impl Default for AppMode {
    fn default() -> Self {
        AppMode::Simple
    }
}

/// Recent project entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub last_opened: String,
    pub clip_count: i64,
    pub thumbnail_path: Option<String>,
}

/// Feature flags (Advanced mode shows toggles; Simple mode uses defaults)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    pub screen_grabs: bool,
    pub face_detection: bool,
    pub best_clips: bool,
    pub cameras_tab: bool,
}

impl FeatureFlags {
    pub fn defaults_for_mode(mode: &AppMode) -> Self {
        match mode {
            AppMode::Simple => Self {
                screen_grabs: true,
                face_detection: false,
                best_clips: true,
                cameras_tab: false,
            },
            AppMode::Advanced => Self {
                screen_grabs: true,
                face_detection: true,
                best_clips: true,
                cameras_tab: true,
            },
        }
    }
}

/// Score weights for the scoring engine (configurable via dev menu)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreWeights {
    pub scene: f64,
    pub audio: f64,
    pub sharpness: f64,
    pub motion: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            scene: 0.25,
            audio: 0.25,
            sharpness: 0.25,
            motion: 0.25,
        }
    }
}

/// Dev menu settings (formulas, watermark overrides)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevMenuSettings {
    pub title_start_seconds: f64,
    pub jl_blend_ms: u32,
    pub score_weights: ScoreWeights,
    pub watermark_text: Option<String>,
}

impl Default for DevMenuSettings {
    fn default() -> Self {
        Self {
            title_start_seconds: 5.0,
            jl_blend_ms: 500,
            score_weights: ScoreWeights::default(),
            watermark_text: None,
        }
    }
}

/// Cached license state summary (non-secret -- raw key stays in keychain)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStateCache {
    pub license_type: String,
    pub is_active: bool,
    pub days_remaining: Option<i32>,
}

/// App settings structure (v2)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub version: u32,
    pub mode: AppMode,
    pub first_run_completed: bool,
    pub theme: String,
    pub default_project_path: Option<String>,
    pub recent_projects: Vec<RecentProject>,
    pub feature_flags: FeatureFlags,
    pub dev_menu: DevMenuSettings,
    pub license_state_cache: Option<LicenseStateCache>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            mode: AppMode::Simple,
            first_run_completed: false,
            theme: "light".to_string(),
            default_project_path: None,
            recent_projects: Vec::new(),
            feature_flags: FeatureFlags::defaults_for_mode(&AppMode::Simple),
            dev_menu: DevMenuSettings::default(),
            license_state_cache: None,
        }
    }
}

/// Get app settings from App DB
#[tauri::command]
pub fn get_app_settings(_app: AppHandle) -> Result<AppSettings, String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    // UI mode
    let mode_str = db::app_schema::get_ui_mode(&conn).map_err(|e| e.to_string())?;
    let mode = match mode_str.as_str() {
        "advanced" => AppMode::Advanced,
        _ => AppMode::Simple,
    };

    // First run completed
    let first_run_completed = db::app_schema::get_setting(&conn, "first_run_completed")
        .map_err(|e| e.to_string())?
        .map(|v| v == "true")
        .unwrap_or(false);

    // Theme
    let theme = db::app_schema::get_setting(&conn, "theme")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "light".to_string());

    // Default project path (resolve from UUID -> registry path)
    let default_project_path = db::app_schema::get_simple_default_library_uuid(&conn)
        .ok()
        .flatten()
        .and_then(|uuid| {
            db::app_schema::get_library_by_uuid(&conn, &uuid)
                .ok()
                .flatten()
                .map(|lib| lib.path)
        });

    // Recent projects from libraries registry
    let registry = db::app_schema::list_recent_libraries(&conn)
        .map_err(|e| e.to_string())?;
    let recent_projects: Vec<RecentProject> = registry.iter()
        .take(MAX_RECENT_PROJECTS)
        .map(|lib| {
            let thumbnail_path = get_library_thumbnail(&lib.path);
            let clip_count = get_library_clip_count(&lib.path);
            RecentProject {
                path: lib.path.clone(),
                name: lib.label.clone().unwrap_or_else(|| {
                    Path::new(&lib.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled".to_string())
                }),
                last_opened: lib.last_opened_at.clone().unwrap_or_default(),
                clip_count,
                thumbnail_path,
            }
        })
        .collect();

    // Feature flags
    let feature_flags = db::app_schema::get_setting(&conn, "features")
        .map_err(|e| e.to_string())?
        .and_then(|s| serde_json::from_str::<FeatureFlags>(&s).ok())
        .unwrap_or_else(|| FeatureFlags::defaults_for_mode(&mode));

    // Dev menu
    let dev_menu = db::app_schema::get_setting(&conn, "dev_menu")
        .map_err(|e| e.to_string())?
        .and_then(|s| serde_json::from_str::<DevMenuSettings>(&s).ok())
        .unwrap_or_default();

    // License state cache
    let license_state_cache = db::app_schema::get_setting(&conn, "license_state_cache")
        .map_err(|e| e.to_string())?
        .and_then(|s| serde_json::from_str::<LicenseStateCache>(&s).ok());

    Ok(AppSettings {
        version: SETTINGS_VERSION,
        mode,
        first_run_completed,
        theme,
        default_project_path,
        recent_projects,
        feature_flags,
        dev_menu,
        license_state_cache,
    })
}

/// Save app settings to App DB
#[tauri::command]
pub fn save_app_settings(_app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    // UI mode
    let mode_str = match settings.mode {
        AppMode::Simple => "simple",
        AppMode::Advanced => "advanced",
    };
    db::app_schema::set_ui_mode(&conn, mode_str).map_err(|e| e.to_string())?;

    // First run completed
    db::app_schema::set_setting(&conn, "first_run_completed", &settings.first_run_completed.to_string())
        .map_err(|e| e.to_string())?;

    // Theme
    db::app_schema::set_setting(&conn, "theme", &settings.theme)
        .map_err(|e| e.to_string())?;

    // Feature flags
    let ff_json = serde_json::to_string(&settings.feature_flags).map_err(|e| e.to_string())?;
    db::app_schema::set_features(&conn, &ff_json).map_err(|e| e.to_string())?;

    // Dev menu
    let dm_json = serde_json::to_string(&settings.dev_menu).map_err(|e| e.to_string())?;
    db::app_schema::set_setting(&conn, "dev_menu", &dm_json).map_err(|e| e.to_string())?;

    // Title offset (individual key per spec)
    db::app_schema::set_title_offset(&conn, settings.dev_menu.title_start_seconds)
        .map_err(|e| e.to_string())?;

    // License state cache
    match &settings.license_state_cache {
        Some(lsc) => {
            let lsc_json = serde_json::to_string(lsc).map_err(|e| e.to_string())?;
            db::app_schema::set_setting(&conn, "license_state_cache", &lsc_json)
                .map_err(|e| e.to_string())?;
        }
        None => {
            let _ = db::app_schema::delete_setting(&conn, "license_state_cache");
        }
    }

    // Default project path -> resolve to UUID and store
    if let Some(ref path) = settings.default_project_path {
        let lib_db_path = Path::new(path)
            .join(constants::DADCAM_FOLDER)
            .join(constants::DB_FILENAME);
        if lib_db_path.exists() {
            if let Ok(lib_conn) = db::open_db(&lib_db_path) {
                if let Ok(uuid) = db::app_schema::get_or_create_library_uuid(&lib_conn) {
                    let _ = db::app_schema::set_simple_default_library_uuid(&conn, &uuid);
                }
            }
        }
    } else {
        let _ = db::app_schema::set_setting(&conn, "simple_default_library_uuid", "");
    }

    // Note: recentProjects are derived from the libraries registry table,
    // not stored as a separate KV setting. They are read-only in this context.

    Ok(())
}

/// Get current mode
#[tauri::command]
pub fn get_mode(_app: AppHandle) -> Result<String, String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    db::app_schema::get_ui_mode(&conn).map_err(|e| e.to_string())
}

/// Set mode
#[tauri::command]
pub fn set_mode(_app: AppHandle, mode: String) -> Result<(), String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    let new_mode = match mode.as_str() {
        "advanced" => AppMode::Advanced,
        _ => AppMode::Simple,
    };

    // Update mode
    db::app_schema::set_ui_mode(&conn, &mode).map_err(|e| e.to_string())?;

    // Update feature flags to match mode
    let feature_flags = FeatureFlags::defaults_for_mode(&new_mode);
    let ff_json = serde_json::to_string(&feature_flags).map_err(|e| e.to_string())?;
    db::app_schema::set_features(&conn, &ff_json).map_err(|e| e.to_string())?;

    Ok(())
}

/// Get the thumbnail path for the first clip in a library (for library card display)
fn get_library_thumbnail(library_path: &str) -> Option<String> {
    let db_path = Path::new(library_path).join(".dadcam").join("dadcam.db");
    if !db_path.exists() {
        return None;
    }

    let conn = Connection::open(&db_path).ok()?;

    let result: Option<String> = conn
        .query_row(
            "SELECT a.path FROM clips c
             JOIN clip_assets ca ON ca.clip_id = c.id
             JOIN assets a ON a.id = ca.asset_id
             WHERE ca.role = 'thumb'
             ORDER BY c.recorded_at DESC
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(thumb_path) = result {
        let full_path = Path::new(library_path).join(&thumb_path);
        if full_path.exists() {
            return Some(full_path.to_string_lossy().to_string());
        }
    }

    None
}

/// Get the clip count for a library (for library card display)
fn get_library_clip_count(library_path: &str) -> i64 {
    let db_path = Path::new(library_path).join(".dadcam").join("dadcam.db");
    if !db_path.exists() {
        return 0;
    }
    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    conn.query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))
        .unwrap_or(0)
}

/// Add or update a recent project entry (upserts into App DB registry)
#[tauri::command]
pub fn add_recent_library(
    _app: AppHandle,
    path: String,
    name: String,
    _clip_count: i64,
) -> Result<(), String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    // Open library DB to get UUID
    let lib_db_path = Path::new(&path)
        .join(constants::DADCAM_FOLDER)
        .join(constants::DB_FILENAME);

    if !lib_db_path.exists() {
        return Err(format!("Library database not found at {}", path));
    }

    let lib_conn = db::open_db(&lib_db_path).map_err(|e| e.to_string())?;
    let uuid = db::app_schema::get_or_create_library_uuid(&lib_conn)
        .map_err(|e| e.to_string())?;

    // Upsert into registry and mark as opened
    db::app_schema::upsert_library(&conn, &uuid, &path, Some(&name))
        .map_err(|e| e.to_string())?;
    db::app_schema::mark_opened(&conn, &uuid)
        .map_err(|e| e.to_string())?;

    // Update default project path
    let _ = db::app_schema::set_simple_default_library_uuid(&conn, &uuid);

    Ok(())
}

/// Remove a project from recent list (deletes from App DB registry)
#[tauri::command]
pub fn remove_recent_library(_app: AppHandle, path: String) -> Result<(), String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;

    // Match both raw and canonicalized paths
    let canonical = std::fs::canonicalize(&path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.clone());

    conn.execute(
        "DELETE FROM libraries WHERE path = ?1 OR path = ?2",
        params![path, canonical],
    ).map_err(|e| e.to_string())?;

    // If the deleted library was the default, pick the next most recent
    if let Ok(Some(default_uuid)) = db::app_schema::get_simple_default_library_uuid(&conn) {
        if let Ok(None) = db::app_schema::get_library_by_uuid(&conn, &default_uuid) {
            let recents = db::app_schema::list_recent_libraries(&conn).unwrap_or_default();
            if let Some(first) = recents.first() {
                let _ = db::app_schema::set_simple_default_library_uuid(&conn, &first.library_uuid);
            } else {
                let _ = db::app_schema::set_setting(&conn, "simple_default_library_uuid", "");
            }
        }
    }

    Ok(())
}

/// Get recent projects list (from App DB registry)
#[tauri::command]
pub fn get_recent_libraries(_app: AppHandle) -> Result<Vec<RecentProject>, String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    let registry = db::app_schema::list_recent_libraries(&conn)
        .map_err(|e| e.to_string())?;

    Ok(registry.iter()
        .take(MAX_RECENT_PROJECTS)
        .map(|lib| {
            let thumbnail_path = get_library_thumbnail(&lib.path);
            let clip_count = get_library_clip_count(&lib.path);
            RecentProject {
                path: lib.path.clone(),
                name: lib.label.clone().unwrap_or_else(|| {
                    Path::new(&lib.path)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled".to_string())
                }),
                last_opened: lib.last_opened_at.clone().unwrap_or_default(),
                clip_count,
                thumbnail_path,
            }
        })
        .collect())
}

/// Check if a library path exists (for validation)
#[tauri::command]
pub fn validate_library_path(path: String) -> Result<bool, String> {
    let path = std::path::Path::new(&path);
    let db_path = path.join(".dadcam").join("dadcam.db");
    Ok(db_path.exists())
}
