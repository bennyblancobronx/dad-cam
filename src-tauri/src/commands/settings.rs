// Dad Cam - App Settings Commands
// Persistent app-level settings via Tauri Store plugin

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use rusqlite::Connection;

/// App settings schema version
const SETTINGS_VERSION: u32 = 1;
const SETTINGS_FILE: &str = "settings.json";
const MAX_RECENT_LIBRARIES: usize = 10;

/// App mode: personal (single library, auto-open) or pro (multi-library)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Personal,
    Pro,
}

impl Default for AppMode {
    fn default() -> Self {
        AppMode::Personal
    }
}

/// Recent library entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentLibrary {
    pub path: String,
    pub name: String,
    pub last_opened: String,
    pub clip_count: i64,
    pub thumbnail_path: Option<String>,
}

/// App settings structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub version: u32,
    pub mode: AppMode,
    pub last_library_path: Option<String>,
    pub recent_libraries: Vec<RecentLibrary>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            mode: AppMode::Personal,
            last_library_path: None,
            recent_libraries: Vec::new(),
        }
    }
}

/// Get app settings from store
#[tauri::command]
pub fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;

    // Try to read each field, falling back to defaults if missing/corrupt
    let version = store
        .get("version")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(SETTINGS_VERSION);

    let mode = store
        .get("mode")
        .map(|v| {
            if v.as_str() == Some("pro") {
                AppMode::Pro
            } else {
                AppMode::Personal
            }
        })
        .unwrap_or_default();

    let last_library_path = store
        .get("lastLibraryPath")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let recent_libraries = store
        .get("recentLibraries")
        .and_then(|v| serde_json::from_value::<Vec<RecentLibrary>>(v.clone()).ok())
        .unwrap_or_default();

    Ok(AppSettings {
        version,
        mode,
        last_library_path,
        recent_libraries,
    })
}

/// Save app settings to store
#[tauri::command]
pub fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;

    store.set("version", serde_json::json!(settings.version));
    store.set(
        "mode",
        serde_json::json!(match settings.mode {
            AppMode::Personal => "personal",
            AppMode::Pro => "pro",
        }),
    );
    store.set(
        "lastLibraryPath",
        match &settings.last_library_path {
            Some(p) => serde_json::json!(p),
            None => serde_json::Value::Null,
        },
    );
    store.set(
        "recentLibraries",
        serde_json::to_value(&settings.recent_libraries).map_err(|e| e.to_string())?,
    );

    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// Get current mode
#[tauri::command]
pub fn get_mode(app: AppHandle) -> Result<String, String> {
    let settings = get_app_settings(app)?;
    Ok(match settings.mode {
        AppMode::Personal => "personal".to_string(),
        AppMode::Pro => "pro".to_string(),
    })
}

/// Set mode
#[tauri::command]
pub fn set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let mut settings = get_app_settings(app.clone())?;
    settings.mode = match mode.as_str() {
        "pro" => AppMode::Pro,
        _ => AppMode::Personal,
    };
    save_app_settings(app, settings)
}

/// Get the thumbnail path for the first clip in a library (for library card display)
fn get_library_thumbnail(library_path: &str) -> Option<String> {
    let db_path = Path::new(library_path).join(".dadcam").join("dadcam.db");
    if !db_path.exists() {
        return None;
    }

    let conn = Connection::open(&db_path).ok()?;

    // Get the first clip with a thumbnail, ordered by recorded_at desc (most recent first)
    // This gives the library card a representative recent thumbnail
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

    // If we got a relative path, make it absolute
    if let Some(thumb_path) = result {
        let full_path = Path::new(library_path).join(&thumb_path);
        if full_path.exists() {
            return Some(full_path.to_string_lossy().to_string());
        }
    }

    None
}

/// Add or update a recent library entry
#[tauri::command]
pub fn add_recent_library(
    app: AppHandle,
    path: String,
    name: String,
    clip_count: i64,
) -> Result<(), String> {
    let mut settings = get_app_settings(app.clone())?;

    // Get current timestamp
    let now = chrono::Utc::now().to_rfc3339();

    // Try to get library thumbnail (first clip's thumbnail)
    let thumbnail_path = get_library_thumbnail(&path);

    // Remove existing entry with same path (if any)
    settings.recent_libraries.retain(|lib| lib.path != path);

    // Add new entry at the front
    settings.recent_libraries.insert(
        0,
        RecentLibrary {
            path: path.clone(),
            name,
            last_opened: now,
            clip_count,
            thumbnail_path,
        },
    );

    // Trim to max size
    settings.recent_libraries.truncate(MAX_RECENT_LIBRARIES);

    // Update last library path
    settings.last_library_path = Some(path);

    save_app_settings(app, settings)
}

/// Remove a library from recent list
#[tauri::command]
pub fn remove_recent_library(app: AppHandle, path: String) -> Result<(), String> {
    let mut settings = get_app_settings(app.clone())?;

    settings.recent_libraries.retain(|lib| lib.path != path);

    // Clear last library path if it was the removed one
    if settings.last_library_path.as_ref() == Some(&path) {
        settings.last_library_path = settings.recent_libraries.first().map(|lib| lib.path.clone());
    }

    save_app_settings(app, settings)
}

/// Get recent libraries list
#[tauri::command]
pub fn get_recent_libraries(app: AppHandle) -> Result<Vec<RecentLibrary>, String> {
    let settings = get_app_settings(app)?;
    Ok(settings.recent_libraries)
}

/// Check if a library path exists (for validation)
#[tauri::command]
pub fn validate_library_path(path: String) -> Result<bool, String> {
    let path = std::path::Path::new(&path);
    let db_path = path.join(".dadcam").join("dadcam.db");
    Ok(db_path.exists())
}
