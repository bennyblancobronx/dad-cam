// Dad Cam - Tauri Library Entry Point

pub mod constants;
pub mod error;
pub mod tools;
pub mod db;
pub mod hash;
pub mod metadata;
pub mod ingest;
pub mod jobs;
pub mod camera;
pub mod preview;

use std::path::PathBuf;
use std::sync::Mutex;
use rusqlite::Connection;
use tauri::State;
use serde::{Deserialize, Serialize};

use db::{open_db, get_db_path, init_library_folders};
use db::schema::{self, Clip, Job, Library};

// State management for database connection
pub struct DbState(pub Mutex<Option<Connection>>);

// Response types for Tauri commands
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipResponse {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    pub media_type: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub recorded_at: Option<String>,
    pub source_folder: Option<String>,
    pub created_at: String,
    pub is_favorite: bool,
    pub is_bad: bool,
}

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestResponse {
    pub job_id: i64,
    pub total_files: usize,
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub clips_created: Vec<i64>,
}

// Convert Clip to ClipResponse
fn clip_to_response(conn: &Connection, clip: Clip) -> ClipResponse {
    let is_favorite = schema::has_clip_tag(conn, clip.id, "favorite").unwrap_or(false);
    let is_bad = schema::has_clip_tag(conn, clip.id, "bad").unwrap_or(false);

    ClipResponse {
        id: clip.id,
        library_id: clip.library_id,
        title: clip.title,
        media_type: clip.media_type,
        duration_ms: clip.duration_ms,
        width: clip.width,
        height: clip.height,
        fps: clip.fps,
        codec: clip.codec,
        recorded_at: clip.recorded_at,
        source_folder: clip.source_folder,
        created_at: clip.created_at,
        is_favorite,
        is_bad,
    }
}

// Tauri Commands

#[tauri::command]
fn open_library(state: State<DbState>, path: String) -> Result<LibraryResponse, String> {
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

#[tauri::command]
fn close_library(state: State<DbState>) -> Result<(), String> {
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    *db = None;
    Ok(())
}

#[tauri::command]
fn create_library(path: String, name: String) -> Result<LibraryResponse, String> {
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

#[tauri::command]
fn get_clips(state: State<DbState>, limit: i64, offset: i64) -> Result<Vec<ClipResponse>, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    // Get library ID (assuming single library for now)
    let lib: Library = conn.query_row(
        "SELECT id, root_path, name, ingest_mode, created_at, settings FROM libraries LIMIT 1",
        [],
        |row| Ok(Library {
            id: row.get(0)?,
            root_path: row.get(1)?,
            name: row.get(2)?,
            ingest_mode: row.get(3)?,
            created_at: row.get(4)?,
            settings: row.get(5)?,
        }),
    ).map_err(|e| e.to_string())?;

    let clips = schema::list_clips(conn, lib.id, limit, offset).map_err(|e| e.to_string())?;

    let responses: Vec<ClipResponse> = clips
        .into_iter()
        .map(|c| clip_to_response(conn, c))
        .collect();

    Ok(responses)
}

#[tauri::command]
fn get_clip(state: State<DbState>, id: i64) -> Result<ClipResponse, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let clip = schema::get_clip(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Clip {} not found", id))?;

    Ok(clip_to_response(conn, clip))
}

#[tauri::command]
fn toggle_tag(state: State<DbState>, clip_id: i64, tag: String) -> Result<bool, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let tag_id = schema::get_tag_id(conn, &tag)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Tag '{}' not found", tag))?;

    let has_tag = schema::has_clip_tag(conn, clip_id, &tag).map_err(|e| e.to_string())?;

    if has_tag {
        schema::remove_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        schema::add_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[tauri::command]
fn set_tag(state: State<DbState>, clip_id: i64, tag: String, value: bool) -> Result<bool, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let tag_id = schema::get_tag_id(conn, &tag)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Tag '{}' not found", tag))?;

    if value {
        schema::add_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
    } else {
        schema::remove_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
    }

    Ok(value)
}

#[tauri::command]
fn start_ingest(state: State<DbState>, source_path: String, library_path: String) -> Result<IngestResponse, String> {
    let library_root = PathBuf::from(&library_path);
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path).map_err(|e| e.to_string())?;

    let lib = schema::get_library_by_path(&conn, &library_path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Library not found".to_string())?;

    // Create and run ingest job
    let job_id = ingest::create_ingest_job(&conn, lib.id, &source_path, &lib.ingest_mode)
        .map_err(|e| e.to_string())?;

    let result = ingest::run_ingest_job(&conn, job_id, &library_root)
        .map_err(|e| e.to_string())?;

    Ok(IngestResponse {
        job_id,
        total_files: result.total_files,
        processed: result.processed,
        skipped: result.skipped,
        failed: result.failed,
        clips_created: result.clips_created,
    })
}

#[tauri::command]
fn get_jobs(state: State<DbState>, status: Option<String>, limit: i64) -> Result<Vec<Job>, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let jobs = schema::list_jobs(conn, None, status.as_deref(), limit)
        .map_err(|e| e.to_string())?;

    Ok(jobs)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(DbState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            open_library,
            close_library,
            create_library,
            get_clips,
            get_clip,
            toggle_tag,
            set_tag,
            start_ingest,
            get_jobs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
