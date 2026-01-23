// Dad Cam - Phase 3 Library Commands
// Commands for library open/close/create operations

use std::path::PathBuf;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::db::{open_db, get_db_path, init_library_folders};
use crate::db::schema;
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
}

/// Open an existing library
#[tauri::command]
pub fn open_library(state: State<DbState>, path: String) -> Result<LibraryResponse, String> {
    let library_path = PathBuf::from(&path);
    let db_path = get_db_path(&library_path);

    if !db_path.exists() {
        return Err(format!("No library found at {}", path));
    }

    let conn = open_db(&db_path).map_err(|e| e.to_string())?;

    // Get library info
    let lib = schema::get_library_by_path(&conn, &path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Library not found in database".to_string())?;

    let clip_count = schema::count_clips(&conn, lib.id).map_err(|e| e.to_string())?;

    // Store connection
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    *db = Some(conn);

    Ok(LibraryResponse {
        id: lib.id,
        root_path: lib.root_path,
        name: lib.name,
        ingest_mode: lib.ingest_mode,
        created_at: lib.created_at,
        clip_count,
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
pub fn create_library(path: String, name: String) -> Result<LibraryResponse, String> {
    let library_path = PathBuf::from(&path);
    let db_path = get_db_path(&library_path);

    if db_path.exists() {
        return Err(format!("Library already exists at {}", path));
    }

    // Create folder structure
    init_library_folders(&library_path).map_err(|e| e.to_string())?;

    // Open database
    let conn = open_db(&db_path).map_err(|e| e.to_string())?;

    // Insert default camera profiles
    camera::insert_default_profiles(&conn).map_err(|e| e.to_string())?;

    // Create library record
    let lib_id = schema::insert_library(&conn, &path, &name, constants::DEFAULT_INGEST_MODE)
        .map_err(|e| e.to_string())?;

    let lib = schema::get_library(&conn, lib_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to create library".to_string())?;

    Ok(LibraryResponse {
        id: lib.id,
        root_path: lib.root_path,
        name: lib.name,
        ingest_mode: lib.ingest_mode,
        created_at: lib.created_at,
        clip_count: 0,
    })
}

/// Get library root path for asset URL construction
#[tauri::command]
pub fn get_library_root(state: State<DbState>) -> Result<String, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let path: String = conn.query_row(
        "SELECT root_path FROM libraries LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| format!("Failed to get path: {}", e))?;

    Ok(path)
}
