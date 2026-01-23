// Database module

pub mod migrations;
pub mod schema;

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

/// Initialize library folder structure
pub fn init_library_folders(library_root: &Path) -> Result<()> {
    use crate::constants::*;

    let dadcam = library_root.join(DADCAM_FOLDER);
    std::fs::create_dir_all(&dadcam)?;
    std::fs::create_dir_all(dadcam.join(PROXIES_FOLDER))?;
    std::fs::create_dir_all(dadcam.join(THUMBS_FOLDER))?;
    std::fs::create_dir_all(dadcam.join(SPRITES_FOLDER))?;
    std::fs::create_dir_all(dadcam.join(EXPORTS_FOLDER))?;
    std::fs::create_dir_all(library_root.join(ORIGINALS_FOLDER))?;

    Ok(())
}
