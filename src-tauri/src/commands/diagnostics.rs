// Dad Cam - Diagnostics Commands
// Get/set crash reporting preference, log directory access, log export.

use tauri::{AppHandle, Manager};
use crate::db;

/// Get whether anonymous crash reporting is enabled
#[tauri::command]
pub fn get_diagnostics_enabled(_app: AppHandle) -> Result<bool, String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    let enabled = db::app_schema::get_setting(&conn, "diagnostics_enabled")
        .map_err(|e| e.to_string())?
        .map(|v| v == "true")
        .unwrap_or(false);
    Ok(enabled)
}

/// Set whether anonymous crash reporting is enabled
#[tauri::command]
pub fn set_diagnostics_enabled(_app: AppHandle, enabled: bool) -> Result<(), String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    db::app_schema::set_setting(&conn, "diagnostics_enabled", if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())
}

/// Get the path to the log directory for this platform
#[tauri::command]
pub fn get_log_directory(app: AppHandle) -> Result<String, String> {
    let log_dir = app.path()
        .app_log_dir()
        .map_err(|e| format!("Could not determine log directory: {}", e))?;
    Ok(log_dir.to_string_lossy().to_string())
}

/// Export recent log files to a user-chosen directory.
/// Copies all .log files from the app log directory to the target.
#[tauri::command]
pub fn export_logs(app: AppHandle, target_dir: String) -> Result<usize, String> {
    let log_dir = app.path()
        .app_log_dir()
        .map_err(|e| format!("Could not determine log directory: {}", e))?;

    if !log_dir.exists() {
        return Ok(0);
    }

    let target = std::path::Path::new(&target_dir);
    if !target.exists() {
        std::fs::create_dir_all(target).map_err(|e| format!("Could not create target dir: {}", e))?;
    }

    let mut copied = 0;
    let entries = std::fs::read_dir(&log_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("log") {
            if let Some(name) = path.file_name() {
                let dest = target.join(name);
                std::fs::copy(&path, &dest).map_err(|e| {
                    format!("Failed to copy {}: {}", path.display(), e)
                })?;
                copied += 1;
            }
        }
    }

    Ok(copied)
}
