// Dad Cam - Phase 3 Library Commands
// Commands for library open/close/create operations

use std::path::PathBuf;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::db::{get_db_path, init_library_folders, ensure_library_db_initialized};
use crate::db::schema;
use crate::db::app_schema;
use crate::db::app_db;
use crate::camera;
use crate::constants;
use super::DbState;

/// Library info returned to frontend
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryResponse {
    pub id: i64,
    pub root_path: String,
    pub name: String,
    pub ingest_mode: String,
    pub created_at: String,
    pub clip_count: i64,
    pub library_uuid: String,
}

/// Open an existing library
#[tauri::command]
pub fn open_library(state: State<DbState>, path: String) -> Result<LibraryResponse, String> {
    let library_path = PathBuf::from(&path);
    let db_path = get_db_path(&library_path);

    if !db_path.exists() {
        return Err(format!("No library found at {}", path));
    }

    // Open library DB, run migrations, create UUID, and backfill stable camera refs (spec 3.5)
    let (conn, library_uuid) = ensure_library_db_initialized(&library_path)
        .map_err(|e| e.to_string())?;

    // Register in App DB registry (best-effort)
    if let Ok(app_conn) = app_db::open_app_db_connection() {
        let lib_name = schema::get_library_by_path(&conn, &path)
            .ok()
            .flatten()
            .map(|l| l.name.clone());
        let _ = app_schema::upsert_library(&app_conn, &library_uuid, &path, lib_name.as_deref());
        let _ = app_schema::mark_opened(&app_conn, &library_uuid);
    }

    // Auto-load bundled camera profiles (silently skips if not found)
    camera::bundled::auto_load_bundled_profiles(&conn);

    // Load custom camera devices from ~/.dadcam/custom_cameras.json (cross-library portable store)
    camera::devices::load_devices_from_json(&conn);

    // Get library info
    let lib = schema::get_library_by_path(&conn, &path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Library not found in database".to_string())?;

    let clip_count = schema::count_clips(&conn, lib.id).map_err(|e| e.to_string())?;

    // Store library root path (not connection -- spec 3.4)
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    *db = Some(library_path.clone());

    Ok(LibraryResponse {
        id: lib.id,
        root_path: lib.root_path,
        name: lib.name,
        ingest_mode: lib.ingest_mode,
        created_at: lib.created_at,
        clip_count,
        library_uuid,
    })
}

/// Close the current library
#[tauri::command]
pub fn close_library(state: State<DbState>) -> Result<(), String> {
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    *db = None;
    Ok(())
}

/// Create a new library
#[tauri::command]
pub fn create_library(state: State<DbState>, path: String, name: String) -> Result<LibraryResponse, String> {
    let library_path = PathBuf::from(&path);

    // Validate the path exists and is a directory
    if !library_path.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    if !library_path.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    let db_path = get_db_path(&library_path);

    if db_path.exists() {
        return Err(format!("Library already exists at {}", path));
    }

    // Create folder structure
    init_library_folders(&library_path)
        .map_err(|e| format!("Failed to create library folders: {}", e))?;

    // Open library DB, run migrations, create UUID, and backfill stable camera refs (spec 3.5)
    let (conn, library_uuid) = ensure_library_db_initialized(&library_path)
        .map_err(|e| format!("Failed to create database: {}", e))?;

    // Register in App DB registry (best-effort)
    if let Ok(app_conn) = app_db::open_app_db_connection() {
        let _ = app_schema::upsert_library(&app_conn, &library_uuid, &path, Some(&name));
        let _ = app_schema::mark_opened(&app_conn, &library_uuid);
    }

    // Insert default camera profiles
    camera::insert_default_profiles(&conn)
        .map_err(|e| format!("Failed to insert camera profiles: {}", e))?;

    // Auto-load bundled camera profiles from canonical.json
    camera::bundled::auto_load_bundled_profiles(&conn);

    // Load custom camera devices from ~/.dadcam/custom_cameras.json
    camera::devices::load_devices_from_json(&conn);

    // Create library record
    let lib_id = schema::insert_library(&conn, &path, &name, constants::DEFAULT_INGEST_MODE)
        .map_err(|e| format!("Failed to insert library record: {}", e))?;

    let lib = schema::get_library(&conn, lib_id)
        .map_err(|e| format!("Failed to read library record: {}", e))?
        .ok_or_else(|| format!("Library record not found after insert (id={})", lib_id))?;

    // Store library root path (not connection -- spec 3.4)
    let mut db = state.0.lock().map_err(|e| format!("Failed to acquire db lock: {}", e))?;
    *db = Some(library_path.clone());

    Ok(LibraryResponse {
        id: lib.id,
        root_path: lib.root_path,
        name: lib.name,
        ingest_mode: lib.ingest_mode,
        created_at: lib.created_at,
        clip_count: 0,
        library_uuid,
    })
}

/// Get library root path for asset URL construction
#[tauri::command]
pub fn get_library_root(state: State<DbState>) -> Result<String, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let library_root = guard.as_ref().ok_or("No library open")?;
    Ok(library_root.to_string_lossy().to_string())
}

/// Library registry entry enriched with clip count + thumbnail for frontend display.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryLibraryEntry {
    pub library_uuid: String,
    pub path: String,
    pub label: Option<String>,
    pub last_opened_at: Option<String>,
    pub is_pinned: bool,
    pub is_missing: bool,
    pub clip_count: i64,
    pub thumbnail_path: Option<String>,
}

/// List libraries from the App DB registry (survives library deletion/moves).
/// Enriches each entry with clip count and thumbnail by opening the library DB.
#[tauri::command]
pub fn list_registry_libraries() -> Result<Vec<RegistryLibraryEntry>, String> {
    let app_conn = app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    let entries = app_schema::list_recent_libraries(&app_conn).map_err(|e| e.to_string())?;

    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let lib_db_path = std::path::Path::new(&entry.path)
            .join(constants::DADCAM_FOLDER)
            .join(constants::DB_FILENAME);

        let (clip_count, thumbnail_path) = if lib_db_path.exists() {
            let conn = match crate::db::open_db(&lib_db_path) {
                Ok(c) => c,
                Err(_) => {
                    result.push(RegistryLibraryEntry {
                        library_uuid: entry.library_uuid,
                        path: entry.path,
                        label: entry.label,
                        last_opened_at: entry.last_opened_at,
                        is_pinned: entry.is_pinned,
                        is_missing: true,
                        clip_count: 0,
                        thumbnail_path: None,
                    });
                    continue;
                }
            };

            // Get clip count (best-effort)
            let lib = schema::get_library_by_path(&conn, &entry.path).ok().flatten();
            let count = lib.as_ref()
                .and_then(|l| schema::count_clips(&conn, l.id).ok())
                .unwrap_or(0);

            // Get thumbnail (best-effort)
            let thumb: Option<String> = conn.query_row(
                "SELECT a.path FROM clips c
                 JOIN clip_assets ca ON ca.clip_id = c.id
                 JOIN assets a ON a.id = ca.asset_id
                 WHERE ca.role = 'thumb'
                 ORDER BY c.recorded_at DESC
                 LIMIT 1",
                [],
                |row| row.get(0),
            ).ok();

            let full_thumb = thumb.and_then(|t| {
                let full = std::path::Path::new(&entry.path).join(&t);
                if full.exists() {
                    Some(full.to_string_lossy().to_string())
                } else {
                    None
                }
            });

            (count, full_thumb)
        } else {
            (0, None)
        };

        let is_missing = !lib_db_path.exists() || entry.is_missing;

        result.push(RegistryLibraryEntry {
            library_uuid: entry.library_uuid,
            path: entry.path,
            label: entry.label,
            last_opened_at: entry.last_opened_at,
            is_pinned: entry.is_pinned,
            is_missing,
            clip_count,
            thumbnail_path,
        });
    }

    Ok(result)
}
