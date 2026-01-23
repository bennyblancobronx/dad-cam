// Database migrations
// Migrations are forward-only. Never edit or delete a migration after it ships.

use rusqlite::Connection;
use anyhow::Result;

/// All migrations in order. Each migration is a SQL string.
const MIGRATIONS: &[&str] = &[
    // Migration 1: Initial schema
    r#"
    -- Libraries table
    CREATE TABLE libraries (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        root_path TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        ingest_mode TEXT NOT NULL DEFAULT 'copy',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        settings TEXT DEFAULT '{}'
    );

    -- Assets table (files)
    CREATE TABLE assets (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        library_id INTEGER NOT NULL REFERENCES libraries(id),
        type TEXT NOT NULL CHECK (type IN ('original', 'proxy', 'thumb', 'sprite', 'export', 'sidecar')),
        path TEXT NOT NULL,
        source_uri TEXT,
        size_bytes INTEGER NOT NULL,
        hash_fast TEXT,
        hash_fast_scheme TEXT,
        hash_full TEXT,
        verified_at TEXT,
        pipeline_version INTEGER,
        derived_params TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(library_id, path)
    );

    -- Clips table (logical video items)
    CREATE TABLE clips (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        library_id INTEGER NOT NULL REFERENCES libraries(id),
        original_asset_id INTEGER NOT NULL REFERENCES assets(id),
        camera_profile_id INTEGER REFERENCES camera_profiles(id),
        media_type TEXT NOT NULL CHECK (media_type IN ('video', 'audio', 'image')),
        title TEXT NOT NULL,
        duration_ms INTEGER,
        width INTEGER,
        height INTEGER,
        fps REAL,
        codec TEXT,
        audio_codec TEXT,
        audio_channels INTEGER,
        audio_sample_rate INTEGER,
        recorded_at TEXT,
        recorded_at_offset_minutes INTEGER,
        recorded_at_is_estimated INTEGER NOT NULL DEFAULT 0,
        timestamp_source TEXT CHECK (timestamp_source IN ('metadata', 'folder', 'filesystem')),
        source_folder TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Clip-Asset mapping
    CREATE TABLE clip_assets (
        clip_id INTEGER NOT NULL REFERENCES clips(id),
        asset_id INTEGER NOT NULL REFERENCES assets(id),
        role TEXT NOT NULL CHECK (role IN ('primary', 'proxy', 'thumb', 'sprite', 'sidecar')),
        PRIMARY KEY (clip_id, asset_id)
    );

    -- Camera profiles
    CREATE TABLE camera_profiles (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE,
        version INTEGER NOT NULL DEFAULT 1,
        match_rules TEXT NOT NULL DEFAULT '{}',
        transform_rules TEXT NOT NULL DEFAULT '{}'
    );

    -- Tags
    CREATE TABLE tags (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE,
        is_system INTEGER NOT NULL DEFAULT 0
    );

    -- Insert system tags
    INSERT INTO tags (name, is_system) VALUES ('favorite', 1);
    INSERT INTO tags (name, is_system) VALUES ('bad', 1);
    INSERT INTO tags (name, is_system) VALUES ('archived', 1);

    -- Clip-Tag mapping
    CREATE TABLE clip_tags (
        clip_id INTEGER NOT NULL REFERENCES clips(id),
        tag_id INTEGER NOT NULL REFERENCES tags(id),
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (clip_id, tag_id)
    );

    -- Jobs table (durable work queue)
    CREATE TABLE jobs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        type TEXT NOT NULL CHECK (type IN ('ingest', 'proxy', 'thumb', 'sprite', 'export', 'hash_full', 'score', 'ml', 'batch_ingest', 'batch_export', 'relink_scan')),
        status TEXT NOT NULL DEFAULT 'pending'
            CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
        library_id INTEGER REFERENCES libraries(id),
        clip_id INTEGER REFERENCES clips(id),
        asset_id INTEGER REFERENCES assets(id),
        priority INTEGER NOT NULL DEFAULT 0,
        attempts INTEGER NOT NULL DEFAULT 0,
        last_error TEXT,
        progress INTEGER DEFAULT 0,
        payload TEXT DEFAULT '{}',
        claimed_by TEXT,
        run_token TEXT,
        lease_expires_at TEXT,
        heartbeat_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        started_at TEXT,
        completed_at TEXT
    );

    -- Job logs
    CREATE TABLE job_logs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id INTEGER NOT NULL REFERENCES jobs(id),
        level TEXT NOT NULL CHECK (level IN ('info', 'warn', 'error')),
        message TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Volumes (for relink)
    CREATE TABLE volumes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        serial TEXT,
        label TEXT,
        mount_point TEXT,
        is_network INTEGER NOT NULL DEFAULT 0,
        last_seen_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Fingerprints (for relink)
    CREATE TABLE fingerprints (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        clip_id INTEGER NOT NULL REFERENCES clips(id),
        type TEXT NOT NULL CHECK (type IN ('size_duration', 'sample_hash', 'full_hash')),
        value TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Ingest file tracking (per-file crash recovery)
    CREATE TABLE ingest_files (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id INTEGER NOT NULL REFERENCES jobs(id),
        source_path TEXT NOT NULL,
        dest_path TEXT,
        status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'copying', 'hashing', 'metadata', 'complete', 'failed', 'skipped')),
        asset_id INTEGER REFERENCES assets(id),
        clip_id INTEGER REFERENCES clips(id),
        error_message TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Asset to volume linking (for relink)
    CREATE TABLE asset_volumes (
        asset_id INTEGER NOT NULL REFERENCES assets(id),
        volume_id INTEGER NOT NULL REFERENCES volumes(id),
        PRIMARY KEY (asset_id, volume_id)
    );

    -- Indexes for common queries
    CREATE INDEX idx_assets_library ON assets(library_id);
    CREATE INDEX idx_assets_hash_fast ON assets(hash_fast);
    CREATE INDEX idx_assets_hash_full ON assets(hash_full);
    CREATE INDEX idx_clips_library ON clips(library_id);
    CREATE INDEX idx_clips_recorded_at ON clips(recorded_at);
    CREATE INDEX idx_clips_media_type ON clips(media_type);
    CREATE INDEX idx_jobs_status ON jobs(status);
    CREATE INDEX idx_jobs_type_status ON jobs(type, status);
    CREATE INDEX idx_jobs_library ON jobs(library_id);
    CREATE INDEX idx_ingest_files_job ON ingest_files(job_id);
    CREATE INDEX idx_ingest_files_status ON ingest_files(status);
    CREATE INDEX idx_fingerprints_clip ON fingerprints(clip_id);
    CREATE INDEX idx_fingerprints_value ON fingerprints(value);
    CREATE INDEX idx_clip_tags_clip ON clip_tags(clip_id);
    CREATE INDEX idx_clip_tags_tag ON clip_tags(tag_id);
    "#,
];

/// Get current schema version from database
fn get_schema_version(conn: &Connection) -> Result<u32> {
    let version: u32 = conn.query_row(
        "PRAGMA user_version",
        [],
        |row| row.get(0)
    )?;
    Ok(version)
}

/// Run all pending migrations (crash-safe)
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let current_version = get_schema_version(conn)?;
    let target_version = MIGRATIONS.len() as u32;

    // Refuse to open a DB created by a newer Dad Cam build
    if current_version > target_version {
        anyhow::bail!(
            "Database schema version {} is newer than this build supports (max {}). Please upgrade Dad Cam.",
            current_version,
            target_version
        );
    }

    if current_version == target_version {
        return Ok(());
    }

    // Apply pending migrations one-by-one
    for (i, migration) in MIGRATIONS.iter().enumerate() {
        let migration_version = (i + 1) as u32;
        if migration_version <= current_version {
            continue;
        }

        conn.execute_batch(migration)?;
        conn.execute_batch(&format!("PRAGMA user_version = {}", migration_version))?;

        eprintln!("Applied migration {}", migration_version);
    }

    Ok(())
}
