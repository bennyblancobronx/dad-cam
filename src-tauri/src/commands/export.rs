// Dad Cam - VHS Export Commands

use std::path::PathBuf;
use tauri::{AppHandle, State};
use crate::commands::DbState;
use crate::db::schema;
use crate::export::{self, VhsExportParams, ExportHistoryEntry};
use crate::jobs;
use crate::jobs::progress::{JobProgress, emit_progress};

/// Start a VHS export. Opens its own DB connection (like start_ingest).
#[tauri::command]
pub fn start_vhs_export(
    app: AppHandle,
    state: State<DbState>,
    params: VhsExportParams,
) -> Result<String, String> {
    // Get library info via short-lived connection
    let library_root = PathBuf::from(&params.library_path);
    let conn = state.connect()?;
    let library_id = {
        let lib = schema::get_library_by_path(&conn, &params.library_path)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Library not found".to_string())?;
        lib.id
    };

    // Create a job ID for progress tracking
    let job_id = format!("export-{}", uuid::Uuid::new_v4());
    let job_id_str = job_id.clone();

    // Register cancel flag
    let cancel_flag = jobs::register_cancel_flag(&job_id_str);

    // Emit initial progress
    emit_progress(&app, &JobProgress::new(&job_id_str, "init", 0, 1)
        .with_message("Starting VHS export..."));

    // Run the export
    let result = export::run_vhs_export(
        &conn,
        library_id,
        &library_root,
        &params,
        &app,
        &cancel_flag,
        &job_id_str,
    );

    // Clean up cancel flag
    jobs::remove_cancel_flag(&job_id_str);

    match result {
        Ok(()) => Ok(job_id),
        Err(e) => {
            emit_progress(&app, &JobProgress::new(&job_id_str, "error", 0, 1)
                .error(e.to_string()));
            Err(e.to_string())
        }
    }
}

/// Get export history for the current library
#[tauri::command]
pub fn get_export_history(
    state: State<DbState>,
    limit: Option<i64>,
) -> Result<Vec<ExportHistoryEntry>, String> {
    let conn = state.connect()?;

    // Get library_id (the open library is always id=1 in its own DB)
    let library_id = 1_i64;
    let limit = limit.unwrap_or(20);

    export::list_export_history(&conn, library_id, limit)
        .map_err(|e| e.to_string())
}

/// Cancel an in-progress export
#[tauri::command]
pub fn cancel_export(job_id: String) -> Result<bool, String> {
    Ok(jobs::request_cancel(&job_id))
}
