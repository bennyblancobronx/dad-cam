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
pub mod scoring;
pub mod commands;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;
use serde::{Deserialize, Serialize};

use db::{open_db, get_db_path};
use db::schema::{self, Job};

// Re-export DbState from commands module for state management
pub use commands::DbState;

// Ingest-specific response type (not part of Phase 3 commands)
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

// Ingest Commands (separate from Phase 3 clip/library/tag commands)

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
        .plugin(tauri_plugin_dialog::init())
        .manage(DbState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            // Phase 3 commands from commands module
            commands::open_library,
            commands::close_library,
            commands::create_library,
            commands::get_library_root,
            commands::get_clips,
            commands::get_clip,
            commands::get_clips_filtered,
            commands::get_clip_view,
            commands::toggle_tag,
            commands::set_tag,
            // Phase 4 scoring commands
            commands::get_clip_score,
            commands::score_clip,
            commands::get_scoring_status,
            commands::get_best_clips,
            commands::set_score_override,
            commands::clear_score_override,
            commands::queue_scoring_jobs,
            // Ingest commands
            start_ingest,
            get_jobs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
