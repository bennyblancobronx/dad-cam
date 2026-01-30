// Dad Cam - Dev Menu Commands
// Backend commands for the developer tools panel
// Access: Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win/Linux)
// Alternative: Settings > About > click version 7 times

use std::path::PathBuf;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::commands::DbState;
use crate::constants;
use crate::tools;
use crate::licensing;

/// Tool status entry (name, available, path)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub available: bool,
    pub path: String,
    pub version: Option<String>,
}

/// Test FFmpeg/ffprobe/exiftool availability and return status + version
#[tauri::command]
pub fn test_ffmpeg() -> Result<Vec<ToolStatus>, String> {
    let statuses = tools::check_tools();

    let mut results: Vec<ToolStatus> = Vec::new();

    for (name, available, path) in statuses {
        let version = if available {
            get_tool_version(&name, &path)
        } else {
            None
        };

        results.push(ToolStatus {
            name,
            available,
            path,
            version,
        });
    }

    Ok(results)
}

/// Get version string from a tool
fn get_tool_version(name: &str, path: &str) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("-version")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Extract first line which typically has the version
    let first_line = stdout.lines().next()?;

    // For ffmpeg/ffprobe: "ffmpeg version N.N.N ..."
    // For exiftool: "ExifTool Version Number: N.N"
    match name {
        "exiftool" => {
            // exiftool -version outputs just the number
            let ver_output = std::process::Command::new(path)
                .arg("-ver")
                .output()
                .ok()?;
            let ver = String::from_utf8_lossy(&ver_output.stdout).trim().to_string();
            if ver.is_empty() { None } else { Some(ver) }
        }
        _ => Some(first_line.to_string()),
    }
}

/// Clear proxy, thumbnail, and sprite caches for the open library
#[tauri::command]
pub fn clear_caches(state: State<DbState>) -> Result<String, String> {
    let guard = state.0.lock().map_err(|e| e.to_string())?;
    let library_root = guard.as_ref().ok_or("No library open")?;
    let root = library_root.clone();
    drop(guard);
    let dadcam_dir = root.join(constants::DADCAM_FOLDER);

    let mut cleared = Vec::new();
    let mut total_bytes: u64 = 0;

    // Clear proxies
    let proxies_dir = dadcam_dir.join(constants::PROXIES_FOLDER);
    if proxies_dir.exists() {
        let (count, bytes) = clear_directory(&proxies_dir)?;
        cleared.push(format!("proxies: {} files", count));
        total_bytes += bytes;
    }

    // Clear thumbnails
    let thumbs_dir = dadcam_dir.join(constants::THUMBS_FOLDER);
    if thumbs_dir.exists() {
        let (count, bytes) = clear_directory(&thumbs_dir)?;
        cleared.push(format!("thumbnails: {} files", count));
        total_bytes += bytes;
    }

    // Clear sprites
    let sprites_dir = dadcam_dir.join(constants::SPRITES_FOLDER);
    if sprites_dir.exists() {
        let (count, bytes) = clear_directory(&sprites_dir)?;
        cleared.push(format!("sprites: {} files", count));
        total_bytes += bytes;
    }

    if cleared.is_empty() {
        Ok("No caches to clear".to_string())
    } else {
        let mb = total_bytes as f64 / (1024.0 * 1024.0);
        Ok(format!("Cleared {} ({:.1} MB)", cleared.join(", "), mb))
    }
}

/// Clear all files in a directory (not the directory itself)
fn clear_directory(dir: &PathBuf) -> Result<(usize, u64), String> {
    let mut count = 0;
    let mut bytes = 0u64;

    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        if metadata.is_file() {
            bytes += metadata.len();
            std::fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            count += 1;
        }
    }

    Ok((count, bytes))
}

/// Export the library database by copying the .db file to an output path
#[tauri::command]
pub fn export_database(state: State<DbState>, output_path: String) -> Result<(), String> {
    let conn = state.connect()?;

    // Get the library root path
    let library_path: String = conn
        .query_row(
            "SELECT root_path FROM libraries LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get library path: {}", e))?;

    let db_source = PathBuf::from(&library_path)
        .join(constants::DADCAM_FOLDER)
        .join(constants::DB_FILENAME);

    if !db_source.exists() {
        return Err("Database file not found".to_string());
    }

    // Checkpoint WAL to ensure the file is up-to-date
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .map_err(|e| format!("Failed to checkpoint: {}", e))?;

    let dest = PathBuf::from(&output_path);
    std::fs::copy(&db_source, &dest)
        .map_err(|e| format!("Failed to copy database: {}", e))?;

    Ok(())
}

/// Execute raw SQL query (dev license only).
/// `target`: "library" (default) or "app" to select which DB to query.
#[tauri::command]
pub fn execute_raw_sql(state: State<DbState>, sql: String, target: Option<String>) -> Result<String, String> {
    // Gate behind dev license
    if !licensing::is_allowed("raw_sql") {
        return Err("Raw SQL requires a Dev license".to_string());
    }

    let target = target.unwrap_or_else(|| "library".to_string());

    // Open the appropriate connection (short-lived per spec 3.4)
    let conn: rusqlite::Connection = if target == "app" {
        crate::db::app_db::open_app_db_connection()
            .map_err(|e| format!("Failed to open App DB: {}", e))?
    } else {
        state.connect()?
    };

    let sql = sql.trim();

    // Detect if it's a SELECT or other read-only query
    let upper = sql.to_uppercase();
    if upper.starts_with("SELECT") || upper.starts_with("PRAGMA") || upper.starts_with("EXPLAIN") {
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let col_count = stmt.column_count();
        let col_names: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let mut rows_output = Vec::new();
        rows_output.push(col_names.join(" | "));
        rows_output.push("-".repeat(rows_output[0].len()));

        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        let mut row_count = 0;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let mut values = Vec::new();
            for i in 0..col_count {
                let val: String = row
                    .get::<_, rusqlite::types::Value>(i)
                    .map(|v| format!("{:?}", v))
                    .unwrap_or_else(|_| "NULL".to_string());
                values.push(val);
            }
            rows_output.push(values.join(" | "));
            row_count += 1;
            if row_count >= 100 {
                rows_output.push(format!("... (limited to 100 rows)"));
                break;
            }
        }

        rows_output.push(format!("\n{} row(s)", row_count));
        Ok(rows_output.join("\n"))
    } else {
        // Execute as statement
        let affected = conn.execute(sql, []).map_err(|e| e.to_string())?;
        Ok(format!("{} row(s) affected", affected))
    }
}

/// Generate rental license keys
#[tauri::command]
pub fn generate_rental_keys(count: u32) -> Result<Vec<String>, String> {
    if count == 0 || count > 100 {
        return Err("Count must be between 1 and 100".to_string());
    }

    Ok(licensing::generate_rental_keys(count))
}

/// Export full exiftool JSON dump for a clip to an output path
#[tauri::command]
pub fn export_exif_dump(state: State<DbState>, clip_id: i64, output_path: String) -> Result<(), String> {
    let conn = state.connect()?;

    // Get the clip's relative path and the library root
    let (rel_path, library_path): (String, String) = conn
        .query_row(
            "SELECT c.rel_path, l.root_path FROM clips c JOIN libraries l ON c.library_id = l.id WHERE c.id = ?1",
            [clip_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Clip not found: {}", e))?;

    let clip_file = PathBuf::from(&library_path)
        .join(constants::ORIGINALS_FOLDER)
        .join(&rel_path);

    if !clip_file.exists() {
        return Err(format!("Source file not found: {}", clip_file.display()));
    }

    let exiftool = tools::exiftool_path();
    let output = std::process::Command::new(&exiftool)
        .arg("-j")
        .arg(&clip_file)
        .output()
        .map_err(|e| format!("Failed to run exiftool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("exiftool failed: {}", stderr));
    }

    std::fs::write(&output_path, &output.stdout)
        .map_err(|e| format!("Failed to write output: {}", e))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Profile staging (spec 3.6 -- authoring writes to staging before publish)
// ---------------------------------------------------------------------------

/// Stage a profile edit for validation before publishing.
#[tauri::command]
pub fn stage_profile_edit(
    source_type: String,
    source_ref: String,
    name: String,
    match_rules: String,
    transform_rules: String,
) -> Result<crate::db::app_schema::StagedProfile, String> {
    if !licensing::is_allowed("raw_sql") {
        return Err("Staging requires a Dev license".to_string());
    }
    let conn = crate::db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    crate::db::app_schema::stage_profile_edit(&conn, &source_type, &source_ref, &name, &match_rules, &transform_rules)
        .map_err(|e| e.to_string())
}

/// List all staged profile edits.
#[tauri::command]
pub fn list_staged_profiles() -> Result<Vec<crate::db::app_schema::StagedProfile>, String> {
    let conn = crate::db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    crate::db::app_schema::list_staged_profiles(&conn).map_err(|e| e.to_string())
}

/// Validate all staged profiles. Returns error descriptions for failures.
#[tauri::command]
pub fn validate_staged_profiles() -> Result<Vec<(i64, String)>, String> {
    let conn = crate::db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    crate::db::app_schema::validate_staged_profiles(&conn).map_err(|e| e.to_string())
}

/// Publish all staged profiles (validates first, then applies).
#[tauri::command]
pub fn publish_staged_profiles() -> Result<u32, String> {
    if !licensing::is_allowed("raw_sql") {
        return Err("Publishing requires a Dev license".to_string());
    }
    let conn = crate::db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    crate::db::app_schema::publish_staged_profiles(&conn).map_err(|e| e.to_string())
}

/// Discard all staged profile edits.
#[tauri::command]
pub fn discard_staged_profiles() -> Result<u32, String> {
    let conn = crate::db::app_db::open_app_db_connection().map_err(|e| e.to_string())?;
    crate::db::app_schema::discard_staged_profiles(&conn).map_err(|e| e.to_string())
}

/// Get database statistics
#[tauri::command]
pub fn get_db_stats(state: State<DbState>) -> Result<String, String> {
    let conn = state.connect()?;

    let mut stats = Vec::new();

    // Clip count
    let clips: i64 = conn
        .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))
        .unwrap_or(0);
    stats.push(format!("Clips: {}", clips));

    // Asset count
    let assets: i64 = conn
        .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
        .unwrap_or(0);
    stats.push(format!("Assets: {}", assets));

    // Event count
    let events: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .unwrap_or(0);
    stats.push(format!("Events: {}", events));

    // Job count
    let jobs: i64 = conn
        .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
        .unwrap_or(0);
    stats.push(format!("Jobs: {}", jobs));

    // DB file size
    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .unwrap_or(0);
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |row| row.get(0))
        .unwrap_or(4096);
    let db_size_mb = (page_count * page_size) as f64 / (1024.0 * 1024.0);
    stats.push(format!("Database size: {:.2} MB", db_size_mb));

    Ok(stats.join("\n"))
}
