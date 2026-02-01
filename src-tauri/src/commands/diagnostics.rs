// Dad Cam - Diagnostics Commands
// Get/set crash reporting preference, log directory access, log export,
// support bundle export, runtime log level, system health.

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use crate::db;
use crate::constants;
use crate::tools;
use super::DbState;

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

// ---------------------------------------------------------------------------
// Support Bundle Export
// ---------------------------------------------------------------------------

/// Export a complete support bundle: log files + summary.txt with system info.
/// Collects everything needed for troubleshooting into one folder.
#[tauri::command]
pub fn export_support_bundle(
    app: AppHandle,
    state: State<DbState>,
    target_dir: String,
) -> Result<String, String> {
    let bundle_dir = std::path::Path::new(&target_dir).join("dadcam-support-bundle");
    std::fs::create_dir_all(&bundle_dir)
        .map_err(|e| format!("Could not create bundle dir: {}", e))?;

    let mut summary_lines: Vec<String> = Vec::new();
    summary_lines.push("Dad Cam Support Bundle".to_string());
    summary_lines.push(format!("Generated: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
    summary_lines.push(format!("App Version: {}", env!("CARGO_PKG_VERSION")));
    summary_lines.push(format!("OS: {} {}", std::env::consts::OS, std::env::consts::ARCH));
    summary_lines.push(String::new());

    // Tool versions
    summary_lines.push("--- External Tools ---".to_string());
    let tool_statuses = tools::check_tools();
    for (name, available, path) in &tool_statuses {
        let status = if *available { "OK" } else { "MISSING" };
        summary_lines.push(format!("{}: {} ({})", name, status, path));
    }
    summary_lines.push(String::new());

    // App DB settings
    summary_lines.push("--- App Settings ---".to_string());
    if let Ok(app_conn) = db::app_db::open_app_db_connection() {
        if let Ok(Some(mode)) = db::app_schema::get_setting(&app_conn, "ui_mode") {
            summary_lines.push(format!("UI Mode: {}", mode));
        }
        if let Ok(Some(diag)) = db::app_schema::get_setting(&app_conn, "diagnostics_enabled") {
            summary_lines.push(format!("Diagnostics: {}", diag));
        }
        if let Ok(Some(level)) = db::app_schema::get_setting(&app_conn, "log_level") {
            summary_lines.push(format!("Log Level: {}", level));
        }
    }
    summary_lines.push(String::new());

    // Library stats (if open)
    summary_lines.push("--- Library ---".to_string());
    if let Ok(conn) = state.connect() {
        let clips: i64 = conn.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0)).unwrap_or(0);
        let assets: i64 = conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0)).unwrap_or(0);
        let events: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0)).unwrap_or(0);
        let jobs_total: i64 = conn.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0)).unwrap_or(0);
        let jobs_failed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE status = 'failed'", [], |r| r.get(0)
        ).unwrap_or(0);
        let jobs_pending: i64 = conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE status = 'pending'", [], |r| r.get(0)
        ).unwrap_or(0);

        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap_or(0);
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap_or(4096);
        let db_size_mb = (page_count * page_size) as f64 / (1024.0 * 1024.0);

        summary_lines.push(format!("Clips: {}", clips));
        summary_lines.push(format!("Assets: {}", assets));
        summary_lines.push(format!("Events: {}", events));
        summary_lines.push(format!("Jobs total: {}, pending: {}, failed: {}", jobs_total, jobs_pending, jobs_failed));
        summary_lines.push(format!("DB size: {:.2} MB", db_size_mb));

        // Disk usage for library folders
        if let Ok(root) = state.library_root() {
            if let Some(ref root_path) = root {
                let dadcam_dir = root_path.join(constants::DADCAM_FOLDER);
                let originals_dir = root_path.join(constants::ORIGINALS_FOLDER);
                summary_lines.push(format!("Originals size: {}", dir_size_display(&originals_dir)));
                summary_lines.push(format!("Proxies size: {}", dir_size_display(&dadcam_dir.join(constants::PROXIES_FOLDER))));
                summary_lines.push(format!("Thumbs size: {}", dir_size_display(&dadcam_dir.join(constants::THUMBS_FOLDER))));
                summary_lines.push(format!("Sprites size: {}", dir_size_display(&dadcam_dir.join(constants::SPRITES_FOLDER))));
            }
        }

        // Last 5 failed jobs
        let mut stmt = conn.prepare(
            "SELECT id, job_type, last_error, created_at FROM jobs WHERE status = 'failed' ORDER BY id DESC LIMIT 5"
        ).ok();
        if let Some(ref mut s) = stmt {
            let rows: Vec<(i64, String, Option<String>, String)> = s.query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            }).ok().map(|r| r.filter_map(|x| x.ok()).collect()).unwrap_or_default();

            if !rows.is_empty() {
                summary_lines.push(String::new());
                summary_lines.push("--- Recent Failed Jobs ---".to_string());
                for (id, jtype, err, created) in &rows {
                    let err_msg = err.as_deref().unwrap_or("(no error message)");
                    summary_lines.push(format!("Job {} ({}): {} [{}]", id, jtype, err_msg, created));
                }
            }
        }
    } else {
        summary_lines.push("No library open".to_string());
    }

    // Write summary.txt
    let summary_path = bundle_dir.join("summary.txt");
    std::fs::write(&summary_path, summary_lines.join("\n"))
        .map_err(|e| format!("Failed to write summary: {}", e))?;

    // Copy log files into bundle
    let logs_dir = bundle_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).ok();

    if let Ok(log_dir) = app.path().app_log_dir() {
        if log_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&log_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("log") {
                        if let Some(name) = path.file_name() {
                            let _ = std::fs::copy(&path, logs_dir.join(name));
                        }
                    }
                }
            }
        }
    }

    Ok(bundle_dir.to_string_lossy().to_string())
}

/// Calculate directory size for display
fn dir_size_display(path: &std::path::Path) -> String {
    if !path.exists() {
        return "(not found)".to_string();
    }
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }
    format_bytes(total)
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ---------------------------------------------------------------------------
// System Health
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemHealth {
    pub pending_jobs: Vec<(String, i64)>,
    pub failed_jobs_24h: i64,
    pub last_error: Option<String>,
    pub originals_size: String,
    pub derived_size: String,
    pub db_size: String,
}

/// Get system health: pending jobs, failures, disk usage.
#[tauri::command]
pub fn get_system_health(state: State<DbState>) -> Result<SystemHealth, String> {
    let conn = state.connect()?;

    // Pending jobs by type
    let mut stmt = conn.prepare(
        "SELECT job_type, COUNT(*) FROM jobs WHERE status = 'pending' GROUP BY job_type ORDER BY COUNT(*) DESC"
    ).map_err(|e| e.to_string())?;
    let pending_jobs: Vec<(String, i64)> = stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?))
    }).map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();

    // Failed jobs in last 24h
    let failed_jobs_24h: i64 = conn.query_row(
        "SELECT COUNT(*) FROM jobs WHERE status = 'failed' AND created_at > datetime('now', '-24 hours')",
        [], |r| r.get(0)
    ).unwrap_or(0);

    // Last error
    let last_error: Option<String> = conn.query_row(
        "SELECT last_error FROM jobs WHERE status = 'failed' AND last_error IS NOT NULL ORDER BY id DESC LIMIT 1",
        [], |r| r.get(0)
    ).ok();

    // Disk usage
    let (originals_size, derived_size, db_size) = if let Ok(Some(root)) = state.library_root() {
        let dadcam_dir = root.join(constants::DADCAM_FOLDER);
        let orig = dir_size_display(&root.join(constants::ORIGINALS_FOLDER));
        let proxies = dir_size_bytes(&dadcam_dir.join(constants::PROXIES_FOLDER));
        let thumbs = dir_size_bytes(&dadcam_dir.join(constants::THUMBS_FOLDER));
        let sprites = dir_size_bytes(&dadcam_dir.join(constants::SPRITES_FOLDER));
        let derived = format_bytes(proxies + thumbs + sprites);

        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap_or(0);
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap_or(4096);
        let db = format_bytes((page_count * page_size) as u64);

        (orig, derived, db)
    } else {
        ("N/A".to_string(), "N/A".to_string(), "N/A".to_string())
    };

    Ok(SystemHealth {
        pending_jobs,
        failed_jobs_24h,
        last_error,
        originals_size,
        derived_size,
        db_size,
    })
}

fn dir_size_bytes(path: &std::path::Path) -> u64 {
    if !path.exists() { return 0; }
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }
    total
}

// ---------------------------------------------------------------------------
// Runtime Log Level
// ---------------------------------------------------------------------------

/// Get the current log level setting
#[tauri::command]
pub fn get_log_level() -> Result<String, String> {
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    let level = db::app_schema::get_setting(&conn, "log_level")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| if cfg!(debug_assertions) { "debug".to_string() } else { "info".to_string() });
    Ok(level)
}

/// Set the log level at runtime. Accepts: "debug", "info", "warn", "error".
/// Persists to App DB and applies immediately via log::set_max_level.
#[tauri::command]
pub fn set_log_level(level: String) -> Result<(), String> {
    let filter = match level.to_lowercase().as_str() {
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => return Err(format!("Invalid log level: {}. Use debug, info, warn, or error.", level)),
    };

    // Apply immediately
    log::set_max_level(filter);
    log::info!("Log level changed to: {}", level);

    // Persist
    let conn = db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    db::app_schema::set_setting(&conn, "log_level", &level.to_lowercase())
        .map_err(|e| e.to_string())
}
