// App DB module — ~/.dadcam/app.db
// User-global database that survives library deletion/moves.
// Stores: bundled profiles, user profiles, camera devices, library registry, app settings.

use std::path::PathBuf;
use rusqlite::Connection;
use anyhow::Result;

use crate::constants::{APP_DB_DIR, APP_DB_FILENAME};

/// All App DB migrations in order. Each migration is a SQL string.
/// Uses PRAGMA user_version for version tracking (same pattern as library migrations).
const APP_MIGRATIONS: &[&str] = &[
    // Migration A1: Camera tables (bundled profiles, user profiles, devices)
    r#"
    CREATE TABLE IF NOT EXISTS bundled_profiles (
        slug TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 1,
        match_rules TEXT NOT NULL DEFAULT '{}',
        transform_rules TEXT NOT NULL DEFAULT '{}',
        bundled_version INTEGER NOT NULL DEFAULT 1
    );

    CREATE TABLE IF NOT EXISTS user_profiles (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        uuid TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        version INTEGER NOT NULL DEFAULT 1,
        match_rules TEXT NOT NULL DEFAULT '{}',
        transform_rules TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_user_profiles_name ON user_profiles(name);

    CREATE TABLE IF NOT EXISTS camera_devices (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        uuid TEXT NOT NULL UNIQUE,
        profile_type TEXT NOT NULL DEFAULT 'none'
            CHECK (profile_type IN ('bundled','user','none')),
        profile_ref TEXT NOT NULL DEFAULT '',
        serial_number TEXT,
        fleet_label TEXT,
        usb_fingerprints TEXT NOT NULL DEFAULT '[]',
        rental_notes TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS idx_camera_devices_uuid ON camera_devices(uuid);
    CREATE INDEX IF NOT EXISTS idx_camera_devices_profile ON camera_devices(profile_type, profile_ref);
    "#,

    // Migration A2: Libraries registry + app settings KV
    r#"
    CREATE TABLE IF NOT EXISTS libraries (
        library_uuid TEXT PRIMARY KEY NOT NULL,
        path TEXT NOT NULL,
        label TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        last_opened_at TEXT,
        last_seen_at TEXT,
        is_pinned INTEGER NOT NULL DEFAULT 0,
        is_missing INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX IF NOT EXISTS idx_libraries_last_opened ON libraries(last_opened_at);

    CREATE TABLE IF NOT EXISTS app_settings (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
    "#,

    // Migration A3: Profile staging (spec 3.6 -- authoring writes to staging before publish)
    r#"
    CREATE TABLE IF NOT EXISTS profile_staging (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        source_type TEXT NOT NULL CHECK (source_type IN ('user','new')),
        source_ref TEXT NOT NULL DEFAULT '',
        name TEXT NOT NULL,
        match_rules TEXT NOT NULL DEFAULT '{}',
        transform_rules TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    "#,
];

/// Get the path to the App DB: ~/.dadcam/app.db
pub fn get_app_db_path() -> Result<PathBuf> {
    let home = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(home.home_dir().join(APP_DB_DIR).join(APP_DB_FILENAME))
}

/// Initialize the App DB: create directory, open DB, set pragmas, run migrations.
/// Call once at startup.
pub fn ensure_app_db_initialized() -> Result<()> {
    let db_path = get_app_db_path()?;

    // Create ~/.dadcam/ directory if missing
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            anyhow::anyhow!(
                "Cannot create App DB directory {}: {}. Check directory permissions.",
                parent.display(),
                e
            )
        })?;
    }

    let conn = Connection::open(&db_path)?;

    // Set pragmas
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")?;

    // Check if this is a fresh database (no migrations applied yet)
    let was_fresh = get_app_schema_version(&conn)? == 0;

    // Run migrations
    run_app_migrations(&conn)?;

    // Seed default settings on fresh install (no legacy store to migrate from)
    if was_fresh {
        seed_default_settings(&conn)?;
    }

    Ok(())
}

/// Open a short-lived App DB connection with pragmas set. Does NOT run migrations.
/// Use this for individual commands after ensure_app_db_initialized() has been called.
pub fn open_app_db_connection() -> Result<Connection> {
    let db_path = get_app_db_path()?;

    if !db_path.exists() {
        anyhow::bail!(
            "App DB not found at {}. Call ensure_app_db_initialized() first.",
            db_path.display()
        );
    }

    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch("PRAGMA busy_timeout=5000;")?;

    Ok(conn)
}

/// Get current App DB schema version
fn get_app_schema_version(conn: &Connection) -> Result<u32> {
    let version: u32 = conn.query_row(
        "PRAGMA user_version",
        [],
        |row| row.get(0),
    )?;
    Ok(version)
}

/// Run all pending App DB migrations
fn run_app_migrations(conn: &Connection) -> Result<()> {
    let current_version = get_app_schema_version(conn)?;
    let target_version = APP_MIGRATIONS.len() as u32;

    if current_version > target_version {
        anyhow::bail!(
            "App DB schema version {} is newer than this build supports (max {}). Please upgrade Dad Cam.",
            current_version,
            target_version
        );
    }

    if current_version == target_version {
        return Ok(());
    }

    for (i, migration) in APP_MIGRATIONS.iter().enumerate() {
        let migration_version = (i + 1) as u32;
        if migration_version <= current_version {
            continue;
        }

        conn.execute_batch(migration)?;
        conn.execute_batch(&format!("PRAGMA user_version = {}", migration_version))?;

        log::info!("Applied App DB migration {}", migration_version);
    }

    Ok(())
}

/// Seed default settings for a fresh install.
/// Uses INSERT OR IGNORE so it won't overwrite values set by Tauri Store migration.
fn seed_default_settings(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "INSERT OR IGNORE INTO app_settings (key, value) VALUES ('ui_mode', 'simple');
         INSERT OR IGNORE INTO app_settings (key, value) VALUES ('first_run_completed', 'false');
         INSERT OR IGNORE INTO app_settings (key, value) VALUES ('theme', 'system');
         INSERT OR IGNORE INTO app_settings (key, value) VALUES ('diagnostics_enabled', 'false');"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_db_fresh_init() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("app.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch("PRAGMA busy_timeout=5000;").unwrap();

        run_app_migrations(&conn).unwrap();

        // Verify tables exist
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('bundled_profiles','user_profiles','camera_devices','libraries','app_settings','profile_staging')",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 6, "All 6 App DB tables should exist");

        // Verify version
        let version = get_app_schema_version(&conn).unwrap();
        assert_eq!(version, 3, "App DB should be at version 3 after A1+A2+A3");

        // Verify default settings were seeded
        seed_default_settings(&conn).unwrap();
        let ui_mode: String = conn.query_row(
            "SELECT value FROM app_settings WHERE key = 'ui_mode'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(ui_mode, "simple");

        let theme: String = conn.query_row(
            "SELECT value FROM app_settings WHERE key = 'theme'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(theme, "system");

        let frc: String = conn.query_row(
            "SELECT value FROM app_settings WHERE key = 'first_run_completed'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(frc, "false");
    }

    #[test]
    fn test_app_db_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("app.db");
        let conn = Connection::open(&db_path).unwrap();

        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Run twice — should be idempotent
        run_app_migrations(&conn).unwrap();
        run_app_migrations(&conn).unwrap();

        let version = get_app_schema_version(&conn).unwrap();
        assert_eq!(version, 3);
    }
}
