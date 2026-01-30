// Database module

pub mod migrations;
pub mod schema;
pub mod app_db;
pub mod app_schema;

use rusqlite::Connection;
use std::path::Path;
use anyhow::Result;

use crate::constants::{DADCAM_FOLDER, DB_FILENAME};

/// Open or create a database at the given path
pub fn open_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;

    // Enable foreign keys (must be done per connection)
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    // Enable WAL mode for better concurrency
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;

    // Set busy timeout to avoid SQLITE_BUSY under contention (spec 3.4)
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;

    // Run migrations
    migrations::run_migrations(&conn)?;

    Ok(conn)
}

/// Get the database path for a library root
pub fn get_db_path(library_root: &Path) -> std::path::PathBuf {
    library_root
        .join(DADCAM_FOLDER)
        .join(DB_FILENAME)
}

/// Get the .dadcam folder path for a library root
pub fn get_dadcam_path(library_root: &Path) -> std::path::PathBuf {
    library_root.join(DADCAM_FOLDER)
}

/// Open a library DB, run all migrations, ensure library_meta/UUID exists,
/// and backfill stable camera refs for clips with legacy integer IDs (spec 6.2).
/// Call once when opening or creating a library. Returns (Connection, library_uuid).
pub fn ensure_library_db_initialized(library_root: &Path) -> Result<(Connection, String)> {
    let db_path = get_db_path(library_root);
    let conn = open_db(&db_path)?;
    let uuid = app_schema::get_or_create_library_uuid(&conn)?;

    // Backfill stable camera refs for clips with legacy integer IDs (spec 6.2, one-time)
    let backfilled = app_schema::backfill_stable_camera_refs(&conn);
    if backfilled > 0 {
        eprintln!("Backfilled stable camera refs for {} clips", backfilled);
    }

    Ok((conn, uuid))
}

/// Open a library DB connection with pragmas but NO migrations.
/// Use for short-lived reads after ensure_library_db_initialized() has been called.
pub fn open_library_db_connection(library_root: &Path) -> Result<Connection> {
    let db_path = get_db_path(library_root);
    if !db_path.exists() {
        anyhow::bail!("Library DB not found at {}", db_path.display());
    }
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
    Ok(conn)
}

/// Initialize library folder structure
pub fn init_library_folders(library_root: &Path) -> Result<()> {
    use crate::constants::*;

    let dadcam = library_root.join(DADCAM_FOLDER);
    std::fs::create_dir_all(&dadcam)?;
    std::fs::create_dir_all(dadcam.join(PROXIES_FOLDER))?;
    std::fs::create_dir_all(dadcam.join(THUMBS_FOLDER))?;
    std::fs::create_dir_all(dadcam.join(SPRITES_FOLDER))?;
    std::fs::create_dir_all(dadcam.join(EXPORTS_FOLDER))?;
    std::fs::create_dir_all(dadcam.join(SIDECARS_FOLDER))?;
    std::fs::create_dir_all(library_root.join(ORIGINALS_FOLDER))?;

    Ok(())
}
