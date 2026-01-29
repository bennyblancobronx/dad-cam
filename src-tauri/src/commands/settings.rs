// Dad Cam - App Settings Commands
// Persistent app-level settings via Tauri Store plugin

use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use rusqlite::Connection;

/// App settings schema version
const SETTINGS_VERSION: u32 = 2;
const SETTINGS_FILE: &str = "settings.json";
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

/// Get app settings from store, with v1->v2 migration
#[tauri::command]
pub fn get_app_settings(app: AppHandle) -> Result<AppSettings, String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;

    // Check stored version
    let version = store
        .get("version")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0);

    // v1->v2 migration: old settings used "personal"/"pro" and "recentLibraries"
    if version < 2 {
        // Read v1 fields
        let old_mode_str = store
            .get("mode")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "personal".to_string());

        let mode = match old_mode_str.as_str() {
            "pro" | "advanced" => AppMode::Advanced,
            _ => AppMode::Simple,
        };

        let default_project_path = store
            .get("lastLibraryPath")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        // Migrate recentLibraries -> recentProjects
        let recent_projects: Vec<RecentProject> = store
            .get("recentLibraries")
            .and_then(|v| serde_json::from_value::<Vec<RecentProject>>(v.clone()).ok())
            .unwrap_or_default();

        // Existing users skip the wizard
        let first_run_completed = store.get("version").is_some();

        let feature_flags = FeatureFlags::defaults_for_mode(&mode);

        let settings = AppSettings {
            version: SETTINGS_VERSION,
            mode,
            first_run_completed,
            theme: "light".to_string(),
            default_project_path,
            recent_projects,
            feature_flags,
            dev_menu: DevMenuSettings::default(),
            license_state_cache: None,
        };

        // Persist the migrated settings immediately
        save_app_settings_inner(&store, &settings)?;

        return Ok(settings);
    }

    // v2 load: read each field with fallbacks
    let mode = store
        .get("mode")
        .map(|v| {
            if v.as_str() == Some("advanced") {
                AppMode::Advanced
            } else {
                AppMode::Simple
            }
        })
        .unwrap_or_default();

    let first_run_completed = store
        .get("firstRunCompleted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let theme = store
        .get("theme")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "light".to_string());

    let default_project_path = store
        .get("defaultProjectPath")
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    let recent_projects = store
        .get("recentProjects")
        .and_then(|v| serde_json::from_value::<Vec<RecentProject>>(v.clone()).ok())
        .unwrap_or_default();

    let feature_flags = store
        .get("featureFlags")
        .and_then(|v| serde_json::from_value::<FeatureFlags>(v.clone()).ok())
        .unwrap_or_else(|| FeatureFlags::defaults_for_mode(&mode));

    let dev_menu = store
        .get("devMenu")
        .and_then(|v| serde_json::from_value::<DevMenuSettings>(v.clone()).ok())
        .unwrap_or_default();

    let license_state_cache = store
        .get("licenseStateCache")
        .and_then(|v| serde_json::from_value::<LicenseStateCache>(v.clone()).ok());

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

/// Internal save helper (takes store reference)
fn save_app_settings_inner(
    store: &tauri_plugin_store::Store<tauri::Wry>,
    settings: &AppSettings,
) -> Result<(), String> {
    store.set("version", serde_json::json!(settings.version));
    store.set(
        "mode",
        serde_json::json!(match settings.mode {
            AppMode::Simple => "simple",
            AppMode::Advanced => "advanced",
        }),
    );
    store.set("firstRunCompleted", serde_json::json!(settings.first_run_completed));
    store.set("theme", serde_json::json!(settings.theme));
    store.set(
        "defaultProjectPath",
        match &settings.default_project_path {
            Some(p) => serde_json::json!(p),
            None => serde_json::Value::Null,
        },
    );
    store.set(
        "recentProjects",
        serde_json::to_value(&settings.recent_projects).map_err(|e| e.to_string())?,
    );
    store.set(
        "featureFlags",
        serde_json::to_value(&settings.feature_flags).map_err(|e| e.to_string())?,
    );
    store.set(
        "devMenu",
        serde_json::to_value(&settings.dev_menu).map_err(|e| e.to_string())?,
    );
    store.set(
        "licenseStateCache",
        match &settings.license_state_cache {
            Some(c) => serde_json::to_value(c).map_err(|e| e.to_string())?,
            None => serde_json::Value::Null,
        },
    );

    // Clean up old v1 keys if present
    store.delete("lastLibraryPath");
    store.delete("recentLibraries");

    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// Save app settings to store
#[tauri::command]
pub fn save_app_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    save_app_settings_inner(&store, &settings)
}

/// Get current mode
#[tauri::command]
pub fn get_mode(app: AppHandle) -> Result<String, String> {
    let settings = get_app_settings(app)?;
    Ok(match settings.mode {
        AppMode::Simple => "simple".to_string(),
        AppMode::Advanced => "advanced".to_string(),
    })
}

/// Set mode
#[tauri::command]
pub fn set_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let mut settings = get_app_settings(app.clone())?;
    let new_mode = match mode.as_str() {
        "advanced" => AppMode::Advanced,
        _ => AppMode::Simple,
    };
    // Update feature flags when mode changes
    settings.feature_flags = FeatureFlags::defaults_for_mode(&new_mode);
    settings.mode = new_mode;
    save_app_settings(app, settings)
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

/// Add or update a recent project entry
#[tauri::command]
pub fn add_recent_library(
    app: AppHandle,
    path: String,
    name: String,
    clip_count: i64,
) -> Result<(), String> {
    let mut settings = get_app_settings(app.clone())?;

    let now = chrono::Utc::now().to_rfc3339();
    let thumbnail_path = get_library_thumbnail(&path);

    // Remove existing entry with same path
    settings.recent_projects.retain(|p| p.path != path);

    // Add at front
    settings.recent_projects.insert(
        0,
        RecentProject {
            path: path.clone(),
            name,
            last_opened: now,
            clip_count,
            thumbnail_path,
        },
    );

    settings.recent_projects.truncate(MAX_RECENT_PROJECTS);
    settings.default_project_path = Some(path);

    save_app_settings(app, settings)
}

/// Remove a project from recent list
#[tauri::command]
pub fn remove_recent_library(app: AppHandle, path: String) -> Result<(), String> {
    let mut settings = get_app_settings(app.clone())?;

    settings.recent_projects.retain(|p| p.path != path);

    if settings.default_project_path.as_ref() == Some(&path) {
        settings.default_project_path = settings.recent_projects.first().map(|p| p.path.clone());
    }

    save_app_settings(app, settings)
}

/// Get recent projects list
#[tauri::command]
pub fn get_recent_libraries(app: AppHandle) -> Result<Vec<RecentProject>, String> {
    let settings = get_app_settings(app)?;
    Ok(settings.recent_projects)
}

/// Check if a library path exists (for validation)
#[tauri::command]
pub fn validate_library_path(path: String) -> Result<bool, String> {
    let path = std::path::Path::new(&path);
    let db_path = path.join(".dadcam").join("dadcam.db");
    Ok(db_path.exists())
}
