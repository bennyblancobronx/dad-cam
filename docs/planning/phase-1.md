Dad Cam - Phase 1 Implementation Guide

Version: 1.0
Target Audience: Developers new to Rust/Tauri

---

Overview

Phase 1 builds the core backend engine with CLI only. No UI. The goal is a crash-safe, resumable ingestion system that stores everything in SQLite.

When complete, you can:
- Initialize a library from the command line
- Ingest footage from any folder or SD card
- Unplug mid-ingest and resume cleanly
- Query clips and jobs from the CLI

Prerequisites:
- Rust 1.75+ installed (rustup.rs)
- Basic Rust knowledge (ownership, Result types, modules)
- Node.js 18+ (for Tauri frontend scaffolding)
- A few test video files to work with

---

Part 1: Project Setup

1.1 Create the Tauri Project

Open terminal in your projects folder:

```bash
# Install create-tauri-app if you haven't
cargo install create-tauri-app --locked

# Create the project
cargo create-tauri-app dad-cam

# When prompted:
# - Choose "React" for frontend
# - Choose "TypeScript" for language
# - Choose "npm" for package manager
```

This creates the standard Tauri structure:

```
dad-cam/
  src/                  # React frontend (we'll ignore for Phase 1)
  src-tauri/
    src/
      lib.rs           # Tauri entry point
      main.rs          # Desktop entry
    Cargo.toml         # Rust dependencies
    tauri.conf.json    # Tauri config
  package.json
```

1.2 Add Rust Dependencies

Edit `src-tauri/Cargo.toml` and add these dependencies:

```toml
[dependencies]
# Tauri (already present)
tauri = { version = "2", features = [] }

# Database
rusqlite = { version = "0.31", features = ["bundled"] }

# Hashing
blake3 = "1.5"

# FFmpeg wrapper
ffmpeg-sidecar = "2.0"

# CLI parsing
clap = { version = "4.5", features = ["derive"] }

# JSON handling
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Time handling
chrono = { version = "0.4", features = ["serde"] }

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# File walking
walkdir = "2.5"

# Path utilities
directories = "5.0"
```

Run `cargo check` in the `src-tauri` folder to verify everything compiles.

1.3 Create the Module Structure

Inside `src-tauri/src/`, create this file structure:

```
src-tauri/src/
  lib.rs              # Tauri entry point (exists)
  main.rs             # Desktop entry (exists)
  cli.rs              # CLI command definitions
  db/
    mod.rs            # Database module
    schema.rs         # Table definitions
    migrations.rs     # Migration runner
  hash/
    mod.rs            # Hashing utilities
  ingest/
    mod.rs            # Ingest pipeline
    discover.rs       # File discovery
    copy.rs           # File copying
  metadata/
    mod.rs            # Metadata extraction
    ffprobe.rs        # FFprobe wrapper
    exiftool.rs       # Exiftool wrapper
  jobs/
    mod.rs            # Job system
    runner.rs         # Job execution
  camera/
    mod.rs            # Camera profile matching
  constants.rs        # All hardcoded values
  error.rs            # Custom error types
```

Create these empty files now. We'll fill them in step by step.

1.4 Define Constants

Create `src-tauri/src/constants.rs`:

```rust
// Dad Cam Constants
// These values come from Phase 0 contracts. Do not change without updating contracts.md.

pub const PIPELINE_VERSION: u32 = 1;
pub const DEFAULT_INGEST_MODE: &str = "copy";

// Hashing
pub const HASH_ALGORITHM: &str = "blake3";
pub const HASH_CHUNK_SIZE

// Concurrency defaults (centralized; referenced by Phase 2/4/6/8)
pub const DEFAULT_INGEST_WORKERS: usize = 1;
pub const DEFAULT_PREVIEW_WORKERS: usize = 1;
pub const DEFAULT_SCORE_WORKERS: usize = 1;
pub const DEFAULT_EXPORT_WORKERS: usize = 1;
pub const DEFAULT_ML_WORKERS: usize = 1;

// FFmpeg is CPU-heavy; keep a hard cap even if the UI allows raising it.
pub const MAX_CONCURRENT_FFMPEG: usize = 2;
: usize = 1_048_576; // 1MB
pub const HASH_FAST_SCHEME: &str = "first_last_size_v1";

// Paths
pub const PATH_DB_SEPARATOR: char = '/';
pub const DADCAM_FOLDER: &str = ".dadcam";
pub const DB_FILENAME: &str = "dadcam.db";

// Time
pub const EVENT_TIME_GAP_HOURS: i64 = 4;
pub const TIMESTAMP_PRECEDENCE: [&str; 3] = ["metadata", "folder", "filesystem"];

// Proxy settings (used in Phase 2, defined here for schema)
pub const PROXY_CODEC: &str = "h264";
pub const PROXY_RESOLUTION: u32 = 720;
pub const PROXY_CRF: u32 = 23;

// Thumbnail settings
pub const THUMB_FORMAT: &str = "jpg";
pub const THUMB_QUALITY: u32 = 85;

// Sprite settings
pub const SPRITE_FPS: u32 = 1;
pub const SPRITE_TILE_WIDTH: u32 = 160;

// Camera profiles
pub const CAMERA_PROFILE_FORMAT: &str = "json";

// Storage semantics
pub const RECORDED_AT_STORAGE: &str = "utc";
pub const DERIVED_PARAMS_HASH_ALGO: &str = "blake3";

// Format handling
pub const SUPPORTED_FORMATS: &str = "ffmpeg-native"; // anything ffmpeg accepts
pub const OUTLIER_TYPES: [&str; 2] = ["audio", "image"]; // accepted but flagged

// Job retry settings
pub const JOB_MAX_RETRIES: i32 = 3;
pub const JOB_BASE_BACKOFF_SECONDS: i64 = 60; // 1 minute base, doubles each retry
```

---

Part 2: Database Schema and Migrations

2.1 Understanding the Schema

The database is the spine of Dad Cam. Every clip, asset, job, and setting lives in SQLite.

Key tables:
- `libraries` - Root folders that contain footage
- `assets` - Files (originals, proxies, thumbnails, etc.)
- `clips` - Logical video items (one clip = one primary video file)
- `clip_assets` - Links clips to their assets
- `jobs` - Durable work queue
- `camera_profiles` - Camera detection rules

2.2 Create the Schema Module

Create `src-tauri/src/db/mod.rs`:

```rust
pub mod schema;
pub mod migrations;

use rusqlite::Connection;
use std::path::Path;
use anyhow::Result;

/// Open or create a database at the given path
pub fn open_db(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;

    // Enable foreign keys (must be done per connection)
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    // Run migrations
    migrations::run_migrations(&conn)?;

    Ok(conn)
}

/// Get the database path for a library root
pub fn get_db_path(library_root: &Path) -> std::path::PathBuf {
    library_root
        .join(crate::constants::DADCAM_FOLDER)
        .join(crate::constants::DB_FILENAME)
}
```

2.3 Create the Migrations Module

Create `src-tauri/src/db/migrations.rs`:

```rust
use rusqlite::Connection;
use anyhow::Result;

/// All migrations in order. Each migration is a SQL string.
/// Migrations are forward-only. Never edit or delete a migration after it ships.
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
        type TEXT NOT NULL CHECK (type IN ('original', 'proxy', 'thumb', 'sprite', 'export')),
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
        recorded_at TEXT,
        recorded_at_offset_minutes INTEGER,
        recorded_at_is_estimated INTEGER NOT NULL DEFAULT 0,
        timestamp_source TEXT CHECK (timestamp_source IN ('metadata', 'folder', 'filesystem')),
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
        type TEXT NOT NULL CHECK (type IN ('ingest', 'proxy', 'thumb', 'sprite', 'export', 'hash_full', 'score', 'ml')),
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

        -- Lease / crash-safety
        claimed_by TEXT,           -- worker identifier (hostname/pid/random id)
        run_token TEXT,            -- random token per claim to prevent double-ack
        lease_expires_at TEXT,     -- UTC ISO8601, when the claim expires
        heartbeat_at TEXT,         -- UTC ISO8601, last heartbeat time

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
    CREATE INDEX idx_clips_library ON clips(library_id);
    CREATE INDEX idx_clips_recorded_at ON clips(recorded_at);
    CREATE INDEX idx_jobs_status ON jobs(status);
    CREATE INDEX idx_jobs_type_status ON jobs(type, status);
    CREATE INDEX idx_ingest_files_job ON ingest_files(job_id);
    CREATE INDEX idx_ingest_files_status ON ingest_files(status);
    CREATE INDEX idx_fingerprints_clip ON fingerprints(clip_id);
    CREATE INDEX idx_fingerprints_value ON fingerprints(value);
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

/// Set schema version in database
fn set_schema_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute_batch(&format!("PRAGMA user_version = {}", version))?;
    Ok(())
}

/// Run all pending migrations (crash-safe)
///
/// Rules:
/// - Each migration runs inside its own SQLite transaction.
/// - user_version is only bumped **after** a migration commits.
/// - If the DB user_version is greater than the app supports, refuse to open.
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let current_version = get_schema_version(conn)?;
    let target_version = MIGRATIONS.len() as u32;

    // Refuse to open a DB created by a newer Dad Cam build.
    if current_version > target_version {
        anyhow::bail!(
            "Database schema_version {} is newer than this build supports (max {}).              Please upgrade Dad Cam.",
            current_version,
            target_version
        );
    }

    if current_version == target_version {
        return Ok(());
    }

    // Apply pending migrations one-by-one, each in a transaction.
    for (i, migration) in MIGRATIONS.iter().enumerate() {
        let migration_version = (i + 1) as u32;
        if migration_version <= current_version {
            continue;
        }

        let tx = conn.transaction()?; // BEGIN IMMEDIATE by default under rusqlite
        tx.execute_batch(migration)?;
        // Only bump user_version if the migration succeeded.
        tx.execute_batch(&format!("PRAGMA user_version = {}", migration_version))?;
        tx.commit()?;

        println!("Applied migration {}", migration_version);
    }

    Ok(())
}
```

2.4 Create the Schema Module (Query Helpers)

Create `src-tauri/src/db/schema.rs`:

```rust
use rusqlite::{Connection, params};
use anyhow::Result;
use serde::{Deserialize, Serialize};

// ----- Library -----

#[derive(Debug, Serialize, Deserialize)]
pub struct Library {
    pub id: i64,
    pub root_path: String,
    pub name: String,
    pub ingest_mode: String,
    pub created_at: String,
    pub settings: String,
}

pub fn create_library(conn: &Connection, root_path: &str, name: &str, ingest_mode: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO libraries (root_path, name, ingest_mode) VALUES (?1, ?2, ?3)",
        params![root_path, name, ingest_mode],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_library_by_path(conn: &Connection, root_path: &str) -> Result<Option<Library>> {
    let mut stmt = conn.prepare(
        "SELECT id, root_path, name, ingest_mode, created_at, settings FROM libraries WHERE root_path = ?1"
    )?;

    let result = stmt.query_row(params![root_path], |row| {
        Ok(Library {
            id: row.get(0)?,
            root_path: row.get(1)?,
            name: row.get(2)?,
            ingest_mode: row.get(3)?,
            created_at: row.get(4)?,
            settings: row.get(5)?,
        })
    });

    match result {
        Ok(lib) => Ok(Some(lib)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ----- Asset -----

#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
    pub id: i64,
    pub library_id: i64,
    pub asset_type: String,
    pub path: String,
    pub source_uri: Option<String>,
    pub size_bytes: i64,
    pub hash_fast: Option<String>,
    pub hash_fast_scheme: Option<String>,
    pub hash_full: Option<String>,
    pub verified_at: Option<String>,
    pub pipeline_version: Option<i32>,
    pub derived_params: Option<String>,
    pub created_at: String,
}

#[derive(Debug)]
pub struct NewAsset {
    pub library_id: i64,
    pub asset_type: String,
    pub path: String,
    pub source_uri: Option<String>,
    pub size_bytes: i64,
    pub hash_fast: Option<String>,
    pub hash_fast_scheme: Option<String>,
}

pub fn create_asset(conn: &Connection, asset: &NewAsset) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO assets
           (library_id, type, path, source_uri, size_bytes, hash_fast, hash_fast_scheme)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
        params![
            asset.library_id,
            asset.asset_type,
            asset.path,
            asset.source_uri,
            asset.size_bytes,
            asset.hash_fast,
            asset.hash_fast_scheme,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_asset_by_hash(conn: &Connection, library_id: i64, hash_fast: &str) -> Result<Option<Asset>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, library_id, type, path, source_uri, size_bytes,
                  hash_fast, hash_fast_scheme, hash_full, verified_at,
                  pipeline_version, derived_params, created_at
           FROM assets
           WHERE library_id = ?1 AND hash_fast = ?2"#
    )?;

    let result = stmt.query_row(params![library_id, hash_fast], |row| {
        Ok(Asset {
            id: row.get(0)?,
            library_id: row.get(1)?,
            asset_type: row.get(2)?,
            path: row.get(3)?,
            source_uri: row.get(4)?,
            size_bytes: row.get(5)?,
            hash_fast: row.get(6)?,
            hash_fast_scheme: row.get(7)?,
            hash_full: row.get(8)?,
            verified_at: row.get(9)?,
            pipeline_version: row.get(10)?,
            derived_params: row.get(11)?,
            created_at: row.get(12)?,
        })
    });

    match result {
        Ok(asset) => Ok(Some(asset)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn update_asset_verified(conn: &Connection, asset_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE assets SET verified_at = datetime('now') WHERE id = ?1",
        params![asset_id],
    )?;
    Ok(())
}

// ----- Clip -----

#[derive(Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: i64,
    pub library_id: i64,
    pub original_asset_id: i64,
    pub camera_profile_id: Option<i64>,
    pub media_type: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub recorded_at: Option<String>,
    pub recorded_at_offset_minutes: Option<i32>,
    pub recorded_at_is_estimated: bool,
    pub timestamp_source: Option<String>,
    pub created_at: String,
}

#[derive(Debug)]
pub struct NewClip {
    pub library_id: i64,
    pub original_asset_id: i64,
    pub media_type: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub recorded_at: Option<String>,
    pub recorded_at_offset_minutes: Option<i32>,
    pub recorded_at_is_estimated: bool,
    pub timestamp_source: Option<String>,
}

pub fn create_clip(conn: &Connection, clip: &NewClip) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO clips
           (library_id, original_asset_id, media_type, title, duration_ms,
            width, height, fps, codec, recorded_at, recorded_at_offset_minutes,
            recorded_at_is_estimated, timestamp_source)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
        params![
            clip.library_id,
            clip.original_asset_id,
            clip.media_type,
            clip.title,
            clip.duration_ms,
            clip.width,
            clip.height,
            clip.fps,
            clip.codec,
            clip.recorded_at,
            clip.recorded_at_offset_minutes,
            clip.recorded_at_is_estimated,
            clip.timestamp_source,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn link_clip_asset(conn: &Connection, clip_id: i64, asset_id: i64, role: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO clip_assets (clip_id, asset_id, role) VALUES (?1, ?2, ?3)",
        params![clip_id, asset_id, role],
    )?;
    Ok(())
}

pub fn list_clips(conn: &Connection, library_id: i64, limit: i64) -> Result<Vec<Clip>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, library_id, original_asset_id, camera_profile_id, media_type,
                  title, duration_ms, width, height, fps, codec, recorded_at,
                  recorded_at_offset_minutes, recorded_at_is_estimated, timestamp_source, created_at
           FROM clips
           WHERE library_id = ?1
           ORDER BY recorded_at DESC, created_at DESC
           LIMIT ?2"#
    )?;

    let clips = stmt.query_map(params![library_id, limit], |row| {
        Ok(Clip {
            id: row.get(0)?,
            library_id: row.get(1)?,
            original_asset_id: row.get(2)?,
            camera_profile_id: row.get(3)?,
            media_type: row.get(4)?,
            title: row.get(5)?,
            duration_ms: row.get(6)?,
            width: row.get(7)?,
            height: row.get(8)?,
            fps: row.get(9)?,
            codec: row.get(10)?,
            recorded_at: row.get(11)?,
            recorded_at_offset_minutes: row.get(12)?,
            recorded_at_is_estimated: row.get(13)?,
            timestamp_source: row.get(14)?,
            created_at: row.get(15)?,
        })
    })?;

    clips.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ----- Job -----

#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub job_type: String,
    pub status: String,
    pub library_id: Option<i64>,
    pub clip_id: Option<i64>,
    pub asset_id: Option<i64>,
    pub priority: i32,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub progress: Option<i32>,
    pub payload: String,

    // Lease / crash-safety
    pub claimed_by: Option<String>,
    pub run_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub heartbeat_at: Option<String>,

    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug)]
pub struct NewJob {
    pub job_type: String,
    pub library_id: Option<i64>,
    pub clip_id: Option<i64>,
    pub asset_id: Option<i64>,
    pub priority: i32,
    pub payload: String,
}

pub fn create_job(conn: &Connection, job: &NewJob) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO jobs (type, library_id, clip_id, asset_id, priority, payload)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
        params![
            job.job_type,
            job.library_id,
            job.clip_id,
            job.asset_id,
            job.priority,
            job.payload,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn claim_job(conn: &Connection, job_type: &str, lease_seconds: i64, worker_id: &str) -> Result<Option<Job>> {
    // Find a pending job and claim it atomically
    let mut stmt = conn.prepare(
        r#"UPDATE jobs
           SET status = 'running',
               started_at = COALESCE(started_at, datetime('now')),
               claimed_by = ?3,
               run_token = lower(hex(randomblob(16))),
               heartbeat_at = datetime('now'),
               lease_expires_at = datetime('now', '+' || ?2 || ' seconds'),
               attempts = attempts + 1
           WHERE id = (
               SELECT id FROM jobs
               WHERE type = ?1
               AND (
                   status = 'pending'
                   OR (status = 'running' AND lease_expires_at < datetime('now'))
               )
               ORDER BY priority DESC, created_at ASC
               LIMIT 1
           )
           RETURNING id, type, status, library_id, clip_id, asset_id, priority,
                     attempts, last_error, progress, payload,
                     claimed_by, run_token, lease_expires_at, heartbeat_at,
                     created_at, started_at, completed_at"#
    )?;

    let result = stmt.query_row(params![job_type, lease_seconds, worker_id], |row| {
        Ok(Job {
            id: row.get(0)?,
            job_type: row.get(1)?,
            status: row.get(2)?,
            library_id: row.get(3)?,
            clip_id: row.get(4)?,
            asset_id: row.get(5)?,
            priority: row.get(6)?,
            attempts: row.get(7)?,
            last_error: row.get(8)?,
            progress: row.get(9)?,
            payload: row.get(10)?,
            claimed_by: row.get(11)?,
            run_token: row.get(12)?,
            lease_expires_at: row.get(13)?,
            heartbeat_at: row.get(14)?,
            created_at: row.get(15)?,
            started_at: row.get(16)?,
            completed_at: row.get(17)?,
        })
    });

    match result {
        Ok(job) => Ok(Some(job)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }

/// Heartbeat a running job to extend its lease (call periodically during long FFmpeg runs)
pub fn heartbeat_job(conn: &Connection, job_id: i64, run_token: &str, extend_seconds: i64) -> Result<()> {
    conn.execute(
        r#"UPDATE jobs
           SET heartbeat_at = datetime('now'),
               lease_expires_at = datetime('now', '+' || ?3 || ' seconds')
           WHERE id = ?1 AND run_token = ?2 AND status = 'running'"#,
        params![job_id, run_token, extend_seconds],
    )?;
    Ok(())
}

}

pub fn complete_job(conn: &Connection, job_id: i64, run_token: &str) -> Result<()> {
    conn.execute(
        r#"UPDATE jobs
           SET status = 'completed', completed_at = datetime('now'), progress = 100
           WHERE id = ?1 AND run_token = ?2"#,
        params![job_id, run_token],
    )?;
    Ok(())
}

pub fn fail_job(conn: &Connection, job_id: i64, run_token: &str, error: &str) -> Result<()> {
    conn.execute(
        r#"UPDATE jobs
           SET status = 'failed', last_error = ?3, completed_at = datetime('now')
           WHERE id = ?1 AND run_token = ?2"#,
        params![job_id, run_token, error],
    )?;
    Ok(())
}

pub fn cancel_job(conn: &Connection, job_id: i64) -> Result<bool> {
    let rows = conn.execute(
        r#"UPDATE jobs
           SET status = 'cancelled', completed_at = datetime('now')
           WHERE id = ?1 AND status IN ('pending', 'running')"#,
        params![job_id],
    )?;
    Ok(rows > 0)
}

/// Reset a failed job to pending with exponential backoff delay
pub fn retry_job_with_backoff(conn: &Connection, job_id: i64, attempt: i32, base_seconds: i64) -> Result<()> {
    // Exponential backoff: base * 2^attempt (1min, 2min, 4min, 8min...)
    let delay_seconds = base_seconds * (1 << attempt.min(6)); // cap at ~64x base
    conn.execute(
        r#"UPDATE jobs
           SET status = 'pending',
               started_at = NULL,
               lease_expires_at = datetime('now', '+' || ?2 || ' seconds')
           WHERE id = ?1"#,
        params![job_id, delay_seconds],
    )?;
    Ok(())
}

pub fn update_job_progress(conn: &Connection, job_id: i64, progress: i32) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET progress = ?2 WHERE id = ?1",
        params![job_id, progress],
    )?;
    Ok(())
}

pub fn list_jobs(conn: &Connection, status_filter: Option<&str>, limit: i64) -> Result<Vec<Job>> {
    let sql = match status_filter {
        Some(_) => r#"SELECT id, type, status, library_id, clip_id, asset_id, priority,
                             attempts, last_error, progress, payload, lease_expires_at,
                             created_at, started_at, completed_at
                      FROM jobs WHERE status = ?1 ORDER BY created_at DESC LIMIT ?2"#,
        None => r#"SELECT id, type, status, library_id, clip_id, asset_id, priority,
                          attempts, last_error, progress, payload, lease_expires_at,
                          created_at, started_at, completed_at
                   FROM jobs ORDER BY created_at DESC LIMIT ?2"#,
    };

    let mut stmt = conn.prepare(sql)?;

    let jobs = match status_filter {
        Some(status) => stmt.query_map(params![status, limit], row_to_job)?,
        None => stmt.query_map(params![limit], row_to_job)?,
    };

    jobs.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: row.get(0)?,
        job_type: row.get(1)?,
        status: row.get(2)?,
        library_id: row.get(3)?,
        clip_id: row.get(4)?,
        asset_id: row.get(5)?,
        priority: row.get(6)?,
        attempts: row.get(7)?,
        last_error: row.get(8)?,
        progress: row.get(9)?,
        payload: row.get(10)?,
        lease_expires_at: row.get(11)?,
        created_at: row.get(12)?,
        started_at: row.get(13)?,
        completed_at: row.get(14)?,
    })
}

pub fn log_job(conn: &Connection, job_id: i64, level: &str, message: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO job_logs (job_id, level, message) VALUES (?1, ?2, ?3)",
        params![job_id, level, message],
    )?;
    Ok(())
}

// ----- Volume -----

pub fn get_or_create_volume(conn: &Connection, serial: Option<&str>, label: Option<&str>) -> Result<i64> {
    // Try to find existing volume
    if let Some(s) = serial {
        let existing: Result<i64, _> = conn.query_row(
            "SELECT id FROM volumes WHERE serial = ?1",
            params![s],
            |row| row.get(0),
        );
        if let Ok(id) = existing {
            // Update last_seen_at
            conn.execute(
                "UPDATE volumes SET last_seen_at = datetime('now') WHERE id = ?1",
                params![id],
            )?;
            return Ok(id);
        }
    }

    // Create new volume
    conn.execute(
        "INSERT INTO volumes (serial, label) VALUES (?1, ?2)",
        params![serial, label],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn link_asset_volume(conn: &Connection, asset_id: i64, volume_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO asset_volumes (asset_id, volume_id) VALUES (?1, ?2)",
        params![asset_id, volume_id],
    )?;
    Ok(())
}

// ----- Fingerprint -----

pub fn create_fingerprint(conn: &Connection, clip_id: i64, fp_type: &str, value: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO fingerprints (clip_id, type, value) VALUES (?1, ?2, ?3)",
        params![clip_id, fp_type, value],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn find_clips_by_fingerprint(conn: &Connection, fp_type: &str, value: &str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT clip_id FROM fingerprints WHERE type = ?1 AND value = ?2"
    )?;
    let ids = stmt.query_map(params![fp_type, value], |row| row.get(0))?;
    ids.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ----- Ingest File Tracking -----

#[derive(Debug)]
pub struct IngestFile {
    pub id: i64,
    pub job_id: i64,
    pub source_path: String,
    pub status: String,
    pub asset_id: Option<i64>,
    pub clip_id: Option<i64>,
    pub error_message: Option<String>,
}

pub fn create_ingest_file(conn: &Connection, job_id: i64, source_path: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO ingest_files (job_id, source_path) VALUES (?1, ?2)",
        params![job_id, source_path],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_ingest_file_status(
    conn: &Connection,
    id: i64,
    status: &str,
    asset_id: Option<i64>,
    clip_id: Option<i64>,
    error: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"UPDATE ingest_files
           SET status = ?2, asset_id = ?3, clip_id = ?4, error_message = ?5,
               updated_at = datetime('now')
           WHERE id = ?1"#,
        params![id, status, asset_id, clip_id, error],
    )?;
    Ok(())
}

pub fn get_pending_ingest_files(conn: &Connection, job_id: i64) -> Result<Vec<IngestFile>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, job_id, source_path, status, asset_id, clip_id, error_message
           FROM ingest_files
           WHERE job_id = ?1 AND status NOT IN ('complete', 'skipped')
           ORDER BY id"#
    )?;
    let files = stmt.query_map(params![job_id], |row| {
        Ok(IngestFile {
            id: row.get(0)?,
            job_id: row.get(1)?,
            source_path: row.get(2)?,
            status: row.get(3)?,
            asset_id: row.get(4)?,
            clip_id: row.get(5)?,
            error_message: row.get(6)?,
        })
    })?;
    files.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ----- Camera Profile -----

#[derive(Debug, Clone)]
pub struct CameraProfile {
    pub id: i64,
    pub name: String,
    pub version: i32,
    pub match_rules: String,
    pub transform_rules: String,
}

pub fn get_all_camera_profiles(conn: &Connection) -> Result<Vec<CameraProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, version, match_rules, transform_rules FROM camera_profiles"
    )?;
    let profiles = stmt.query_map([], |row| {
        Ok(CameraProfile {
            id: row.get(0)?,
            name: row.get(1)?,
            version: row.get(2)?,
            match_rules: row.get(3)?,
            transform_rules: row.get(4)?,
        })
    })?;
    profiles.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn create_camera_profile(conn: &Connection, name: &str, match_rules: &str, transform_rules: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO camera_profiles (name, match_rules, transform_rules) VALUES (?1, ?2, ?3)",
        params![name, match_rules, transform_rules],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_clip_camera_profile(conn: &Connection, clip_id: i64, profile_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET camera_profile_id = ?2 WHERE id = ?1",
        params![clip_id, profile_id],
    )?;
    Ok(())
}
```

---

Part 3: Hashing Module

3.1 Understanding Fast Hash vs Full Hash

Dad Cam uses two hash strategies (from contracts.md):

- **Fast hash**: First 1MB + last 1MB + file size, concatenated and hashed with BLAKE3
  - Used during ingest for quick dedup
  - Runs synchronously

- **Full hash**: Entire file hashed with BLAKE3
  - Used for verification and relink
  - Runs as background job

3.2 Create the Hash Module

Create `src-tauri/src/hash/mod.rs`:

```rust
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use anyhow::Result;
use crate::constants;

fn validate_constants() {
    assert!(constants::PIPELINE_VERSION >= 1);
    assert!(constants::HASH_CHUNK_SIZE == 1_048_576);
    assert!(constants::MAX_CONCURRENT_FFMPEG >= 1);
}
::{HASH_CHUNK_SIZE, HASH_FAST_SCHEME};

/// Compute the fast hash of a file.
/// Formula: BLAKE3(first_1MB + last_1MB + file_size_bytes)
/// This is used for quick dedup during ingest.
pub fn compute_fast_hash(path: &Path) -> Result<(String, String)> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    let mut hasher = blake3::Hasher::new();

    // Read first chunk (up to 1MB)
    let first_chunk_size = std::cmp::min(file_size as usize, HASH_CHUNK_SIZE);
    let mut first_chunk = vec![0u8; first_chunk_size];
    file.read_exact(&mut first_chunk)?;
    hasher.update(&first_chunk);

    // Read last chunk (up to 1MB) if file is larger than one chunk
    if file_size > HASH_CHUNK_SIZE as u64 {
        let last_chunk_start = file_size - HASH_CHUNK_SIZE as u64;
        file.seek(SeekFrom::Start(last_chunk_start))?;

        let mut last_chunk = vec![0u8; HASH_CHUNK_SIZE];
        file.read_exact(&mut last_chunk)?;
        hasher.update(&last_chunk);
    }

    // Include file size in hash
    hasher.update(&file_size.to_le_bytes());

    let hash = hasher.finalize();
    Ok((hash.to_hex().to_string(), HASH_FAST_SCHEME.to_string()))
}

/// Compute the full hash of a file.
/// This reads the entire file and hashes it with BLAKE3.
/// Use for verification and relink operations.
pub fn compute_full_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();

    // Read in chunks for memory efficiency
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Verify a file matches its expected hash.
/// Returns true if the hashes match.
pub fn verify_file(path: &Path, expected_hash: &str) -> Result<bool> {
    let actual_hash = compute_full_hash(path)?;
    Ok(actual_hash == expected_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_fast_hash_small_file() {
        // Create a small test file
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"test content").unwrap();

        let (hash, scheme) = compute_fast_hash(temp.path()).unwrap();

        assert!(!hash.is_empty());
        assert_eq!(scheme, "first_last_size_v1");
    }

    #[test]
    fn test_fast_hash_deterministic() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"test content").unwrap();

        let (hash1, _) = compute_fast_hash(temp.path()).unwrap();
        let (hash2, _) = compute_fast_hash(temp.path()).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_full_hash() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"test content").unwrap();

        let hash = compute_full_hash(temp.path()).unwrap();

        assert!(!hash.is_empty());
        // BLAKE3 produces 64-character hex strings
        assert_eq!(hash.len(), 64);
    }
}
```

---

Part 4: Metadata Extraction

### 4.0 External Tool Bundling (No PATH Assumptions)

Phase 0 and contracts.md require that **ffmpeg/ffprobe/exiftool are bundled with the app**, not pulled from a system install. That keeps Dad Cam portable and truly offline.

In Phase 1 we’re CLI-only, but we still implement the same rule by:

- Resolving tools from **bundled sidecar locations** (next to the executable; macOS `Contents/Resources` fallback)
- Allowing explicit overrides via environment variables (handy for CI and dev machines)
- Falling back to PATH **only for development convenience**

#### 4.0.1 Add a Tools Resolver Module

Create `src-tauri/src/tools.rs`:

```rust
use std::{
    env,
    path::{Path, PathBuf},
};

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

/// Resolve a bundled sidecar tool path.
///
/// Order:
/// 1) env var override (if set and exists)
/// 2) sidecar next to the executable
/// 3) macOS app bundle Resources fallback
/// 4) PATH fallback (dev-only)
fn resolve_tool(env_key: &str, default_name: &str) -> PathBuf {
    if let Ok(v) = env::var(env_key) {
        let p = PathBuf::from(v);
        if p.exists() {
            return p;
        }
    }

    let mut filename = default_name.to_string();
    if cfg!(windows) && !filename.to_lowercase().ends_with(".exe") {
        filename.push_str(".exe");
    }

    if let Some(dir) = exe_dir() {
        let cand = dir.join(&filename);
        if cand.exists() {
            return cand;
        }

        // macOS packaged app layout:
        //   Dad Cam.app/Contents/MacOS/<exe>
        //   Dad Cam.app/Contents/Resources/<sidecars>
        // When running unbundled, this path won't exist.
        if let Some(contents_dir) = dir.parent() {
            let resources = contents_dir.join("Resources").join(&filename);
            if resources.exists() {
                return resources;
            }
        }
    }

    PathBuf::from(default_name)
}

pub fn ffprobe_path() -> PathBuf {
    resolve_tool("DADCAM_FFPROBE_PATH", "ffprobe")
}

pub fn ffmpeg_path() -> PathBuf {
    resolve_tool("DADCAM_FFMPEG_PATH", "ffmpeg")
}

pub fn exiftool_path() -> PathBuf {
    resolve_tool("DADCAM_EXIFTOOL_PATH", "exiftool")
}
```

#### 4.0.2 Register the Module

In `src-tauri/src/main.rs`, add:

```rust
mod tools;
```

(If you also have `lib.rs`, add `pub mod tools;` there too.)

#### 4.0.3 Bundling Notes (Implementation Guide)

- **Development:** it’s fine to have ffprobe/exiftool on PATH.
- **Release builds:** ship `ffmpeg`, `ffprobe`, and `exiftool` as sidecars.
  - Put them next to the final executable, or in the macOS app `Contents/Resources`.
  - If you prefer “first-run download” for ffmpeg/ffprobe, you can integrate `ffmpeg-sidecar`’s downloader later; the resolver above still works because it checks the executable directory first.

Environment overrides (optional but recommended):
- `DADCAM_FFPROBE_PATH=/absolute/path/to/ffprobe`
- `DADCAM_FFMPEG_PATH=/absolute/path/to/ffmpeg`
- `DADCAM_EXIFTOOL_PATH=/absolute/path/to/exiftool`

---

4.1 FFprobe Wrapper

Create `src-tauri/src/metadata/mod.rs`:

```rust
pub mod ffprobe;
pub mod exiftool;

pub use ffprobe::MediaInfo;
```

Create `src-tauri/src/metadata/ffprobe.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

/// Metadata extracted from a media file using ffprobe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub audio_codec: Option<String>,
    pub has_audio: bool,
    pub is_video: bool,
    pub is_audio_only: bool,
    pub is_image: bool,
    pub creation_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Vec<FfprobeStream>,
    format: FfprobeFormat,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: String,
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    tags: Option<FfprobeTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeTags {
    creation_time: Option<String>,
}

/// Get the path to ffprobe binary.
///
/// Dad Cam policy (contracts.md): ffprobe must be bundled with the app.
/// This helper resolves in this order:
/// 1) Explicit override via env var (DADCAM_FFPROBE_PATH)
/// 2) A bundled sidecar next to the executable (and macOS Resources fallback)
/// 3) PATH fallback (dev-only convenience)
fn get_ffprobe_path() -> std::path::PathBuf {
    crate::tools::ffprobe_path()
}

/// Extract metadata from a media file using ffprobe
pub fn probe(path: &Path) -> Result<MediaInfo> {
    let output = Command::new(get_ffprobe_path())
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ffprobe failed: {}", stderr));
    }

    let json_str = String::from_utf8(output.stdout)?;
    let probe_data: FfprobeOutput = serde_json::from_str(&json_str)?;

    // Find video and audio streams
    let video_stream = probe_data.streams.iter()
        .find(|s| s.codec_type == "video");
    let audio_stream = probe_data.streams.iter()
        .find(|s| s.codec_type == "audio");

    // Parse duration
    let duration_ms = probe_data.format.duration
        .as_ref()
        .and_then(|d| d.parse::<f64>().ok())
        .map(|d| (d * 1000.0) as i64);

    // Parse frame rate from video stream
    let fps = video_stream.and_then(|v| {
        v.avg_frame_rate.as_ref()
            .or(v.r_frame_rate.as_ref())
            .and_then(|fr| parse_frame_rate(fr))
    });

    // Determine media type
    let is_video = video_stream.is_some();
    let has_audio = audio_stream.is_some();
    let is_audio_only = !is_video && has_audio;
    let is_image = is_video && duration_ms.map(|d| d < 100).unwrap_or(false);

    Ok(MediaInfo {
        duration_ms,
        width: video_stream.and_then(|v| v.width),
        height: video_stream.and_then(|v| v.height),
        fps,
        codec: video_stream.and_then(|v| v.codec_name.clone()),
        audio_codec: audio_stream.and_then(|a| a.codec_name.clone()),
        has_audio,
        is_video,
        is_audio_only,
        is_image,
        creation_time: probe_data.format.tags
            .and_then(|t| t.creation_time),
    })
}

/// Parse frame rate string like "30000/1001" or "30/1"
fn parse_frame_rate(fr: &str) -> Option<f64> {
    let parts: Vec<&str> = fr.split('/').collect();
    if parts.len() == 2 {
        let num: f64 = parts[0].parse().ok()?;
        let den: f64 = parts[1].parse().ok()?;
        if den > 0.0 {
            return Some(num / den);
        }
    }
    fr.parse().ok()
}

/// Determine media type string for database
pub fn get_media_type(info: &MediaInfo) -> &'static str {
    if info.is_image {
        "image"
    } else if info.is_audio_only {
        "audio"
    } else {
        "video"
    }
}
```

4.2 ExifTool Wrapper (Simplified)

Create `src-tauri/src/metadata/exiftool.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

/// Metadata extracted from a media file using exiftool
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExifInfo {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub creation_date: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ExifToolOutput {
    #[serde(rename = "Make")]
    make: Option<String>,
    #[serde(rename = "Model")]
    model: Option<String>,
    #[serde(rename = "CreateDate")]
    create_date: Option<String>,
    #[serde(rename = "DateTimeOriginal")]
    datetime_original: Option<String>,
    #[serde(rename = "GPSLatitude")]
    gps_latitude: Option<String>,
    #[serde(rename = "GPSLongitude")]
    gps_longitude: Option<String>,
}

/// Get the path to exiftool binary.
///
/// Dad Cam policy (contracts.md): exiftool must be bundled with the app.
/// Resolution order:
/// 1) DADCAM_EXIFTOOL_PATH env override
/// 2) Bundled sidecar near the executable (and macOS Resources fallback)
/// 3) PATH fallback (dev-only convenience)
fn get_exiftool_path() -> std::path::PathBuf {
    crate::tools::exiftool_path()
}

/// Extract EXIF metadata from a file
pub fn extract(path: &Path) -> Result<ExifInfo> {
    let output = Command::new(get_exiftool_path())
        .args([
            "-json",
            "-Make",
            "-Model",
            "-CreateDate",
            "-DateTimeOriginal",
            "-GPSLatitude",
            "-GPSLongitude",
        ])
        .arg(path)
        .output()?;

    if !output.status.success() {
        // exiftool might fail on some files - return empty info
        return Ok(ExifInfo::default());
    }

    let json_str = String::from_utf8(output.stdout)?;
    let results: Vec<ExifToolOutput> = serde_json::from_str(&json_str)
        .unwrap_or_else(|_| vec![]);

    let exif = results.into_iter().next().unwrap_or(ExifToolOutput {
        make: None,
        model: None,
        create_date: None,
        datetime_original: None,
        gps_latitude: None,
        gps_longitude: None,
    });

    Ok(ExifInfo {
        camera_make: exif.make,
        camera_model: exif.model,
        creation_date: exif.datetime_original.or(exif.create_date),
        gps_latitude: parse_gps_coord(&exif.gps_latitude),
        gps_longitude: parse_gps_coord(&exif.gps_longitude),
    })
}

/// Parse GPS coordinate from exiftool format
fn parse_gps_coord(coord: &Option<String>) -> Option<f64> {
    // exiftool returns coords like "34 deg 5' 12.34\" N"
    // This is a simplified parser - enhance as needed
    coord.as_ref().and_then(|s| {
        // Try to parse as simple float first
        s.parse::<f64>().ok()
    })
}
```

---

Part 5: File Discovery and Ingest

5.1 File Discovery

Create `src-tauri/src/ingest/mod.rs`:

```rust
pub mod discover;
pub mod copy;

pub use discover::discover_media_files;
pub use copy::copy_file_to_library;
```

Create `src-tauri/src/ingest/discover.rs`:

```rust
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use anyhow::Result;

/// Common video file extensions
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "avi", "mkv", "mts", "m2ts", "m4v", "wmv",
    "flv", "webm", "mpg", "mpeg", "3gp", "vob", "ts", "mxf"
];

/// Common audio file extensions (outliers, but accepted)
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "aac", "flac", "ogg", "m4a", "wma"
];

/// Common image file extensions (outliers, but accepted)
const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "heic", "heif"
];

/// A discovered media file with its metadata
#[derive(Debug, Clone)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub media_category: MediaCategory,
    pub parent_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MediaCategory {
    Video,
    Audio,
    Image,
}

/// Discover all media files in a directory
pub fn discover_media_files(source_path: &Path) -> Result<Vec<DiscoveredFile>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(source_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Check if it's a media file
        if let Some(category) = get_media_category(path) {
            let metadata = path.metadata()?;
            let parent_folder = path.parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string());

            files.push(DiscoveredFile {
                path: path.to_path_buf(),
                size_bytes: metadata.len(),
                media_category: category,
                parent_folder,
            });
        }
    }

    // Sort by path for deterministic ordering
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(files)
}

/// Determine the media category of a file by extension
fn get_media_category(path: &Path) -> Option<MediaCategory> {
    let ext = path.extension()?
        .to_str()?
        .to_lowercase();

    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaCategory::Video)
    } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaCategory::Audio)
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Some(MediaCategory::Image)
    } else {
        None
    }
}

/// Check if a path looks like an AVCHD structure
pub fn is_avchd_structure(path: &Path) -> bool {
    // AVCHD typically has BDMV or PRIVATE/AVCHD folders
    path.join("BDMV").exists() || path.join("PRIVATE").join("AVCHD").exists()
}

/// Get sidecar files for a media file
/// Returns paths to XML, XMP, THM, and other companion files
pub fn find_sidecars(media_path: &Path) -> Vec<PathBuf> {
    let mut sidecars = Vec::new();
    let parent = match media_path.parent() {
        Some(p) => p,
        None => return sidecars,
    };

    let stem = match media_path.file_stem() {
        Some(s) => s.to_string_lossy().to_string(),
        None => return sidecars,
    };

    // Common sidecar extensions
    let sidecar_exts = ["xml", "xmp", "thm", "lrf", "srt", "vtt"];

    for ext in &sidecar_exts {
        let sidecar_path = parent.join(format!("{}.{}", stem, ext));
        if sidecar_path.exists() {
            sidecars.push(sidecar_path);
        }
        // Also check uppercase
        let sidecar_path_upper = parent.join(format!("{}.{}", stem, ext.to_uppercase()));
        if sidecar_path_upper.exists() && !sidecars.contains(&sidecar_path_upper) {
            sidecars.push(sidecar_path_upper);
        }
    }

    sidecars
}
```

5.2 File Copying

Create `src-tauri/src/ingest/copy.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use crate::hash;
use crate::constants::DADCAM_FOLDER;

/// Result of copying a file to the library
#[derive(Debug)]
pub struct CopyResult {
    pub dest_path: PathBuf,
    pub relative_path: String,
    pub hash_fast: String,
    pub hash_fast_scheme: String,
    pub verified: bool,
}

/// Copy a file to the library's originals folder.
/// Returns the destination path and hash information.
pub fn copy_file_to_library(
    source_path: &Path,
    library_root: &Path,
    preserve_folder_structure: bool,
    folder_hint: Option<&str>,
) -> Result<CopyResult> {
    // Determine destination folder
    let originals_dir = library_root.join("originals");

    let dest_folder = if preserve_folder_structure {
        if let Some(folder) = folder_hint {
            originals_dir.join(folder)
        } else {
            originals_dir.clone()
        }
    } else {
        originals_dir.clone()
    };

    // Create destination folder if needed
    fs::create_dir_all(&dest_folder)?;

    // Get filename
    let filename = source_path.file_name()
        .ok_or_else(|| anyhow!("No filename in source path"))?;

    // Handle filename conflicts
    let dest_path = resolve_filename_conflict(&dest_folder.join(filename));

    // Copy the file
    fs::copy(source_path, &dest_path)?;

    // Compute hash of the copied file
    let (hash_fast, hash_fast_scheme) = hash::compute_fast_hash(&dest_path)?;

    // Verify the copy by comparing hashes
    let (source_hash, _) = hash::compute_fast_hash(source_path)?;
    let verified = hash_fast == source_hash;

    if !verified {
        // Delete the corrupted copy
        let _ = fs::remove_file(&dest_path);
        return Err(anyhow!("Copy verification failed: hashes do not match"));
    }

    // Compute relative path for database storage (POSIX separators)
    let relative_path = dest_path
        .strip_prefix(library_root)
        .map_err(|_| anyhow!("Failed to compute relative path"))?
        .to_string_lossy()
        .replace('\\', "/");

    Ok(CopyResult {
        dest_path,
        relative_path,
        hash_fast,
        hash_fast_scheme,
        verified,
    })
}

/// Resolve filename conflicts by appending a number
fn resolve_filename_conflict(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or(Path::new(""));
    let stem = path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path.extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    let mut counter = 1;
    loop {
        let new_name = format!("{}_{}{}", stem, counter, ext);
        let new_path = parent.join(&new_name);
        if !new_path.exists() {
            return new_path;
        }
        counter += 1;
        if counter > 10000 {
            // Safety limit
            panic!("Too many filename conflicts");
        }
    }
}

/// Create a reference to an existing file (reference mode).
/// Does not copy the file, just records its location.
pub fn reference_file(
    source_path: &Path,
    library_root: &Path,
) -> Result<CopyResult> {
    // Compute hash of the source file
    let (hash_fast, hash_fast_scheme) = hash::compute_fast_hash(source_path)?;

    // For reference mode, the "path" in DB is empty (no copy)
    // We store the source_uri instead
    let relative_path = String::new();

    Ok(CopyResult {
        dest_path: source_path.to_path_buf(),
        relative_path,
        hash_fast,
        hash_fast_scheme,
        verified: true, // No copy to verify
    })
}
```

---

Part 6: Job System

6.1 Job Runner

Create `src-tauri/src/jobs/mod.rs`:

```rust
pub mod runner;

pub use runner::JobRunner;
```

Create `src-tauri/src/jobs/runner.rs`:

```rust
use std::path::Path;
use std::thread;
use std::time::Duration;
use rusqlite::Connection;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::schema::{self, Job, NewJob};
use crate::db;
use crate::ingest::{discover_media_files, copy_file_to_library};
use crate::metadata::ffprobe;
use crate::hash;

/// Job runner configuration
pub struct JobRunner {
    pub library_root: std::path::PathBuf,
    pub lease_seconds: i64,
    pub max_retries: i32,
}

/// Payload for ingest jobs
#[derive(Debug, Serialize, Deserialize)]
pub struct IngestPayload {
    pub source_path: String,
    pub ingest_mode: String,
}

impl JobRunner {
    pub fn new(library_root: &Path) -> Self {
        Self {
            library_root: library_root.to_path_buf(),
            lease_seconds: 300, // 5 minute lease
            max_retries: 3,
        }
    }

    /// Run the job loop until no more jobs are available
    pub fn run_until_empty(&self, job_type: &str) -> Result<u32> {
        let db_path = db::get_db_path(&self.library_root);
        let conn = db::open_db(&db_path)?;
        let mut completed = 0;

        loop {
            match schema::claim_job(&conn, job_type, self.lease_seconds)? {
                Some(job) => {
                    println!("Processing job {} (type: {})", job.id, job.job_type);

                    match self.process_job(&conn, &job) {
                        Ok(()) => {
                            schema::complete_job(&conn, job.id)?;
                            completed += 1;
                            println!("Job {} completed", job.id);
                        }
                        Err(e) => {
                            let error_msg = format!("{:?}", e);
                            schema::fail_job(&conn, job.id, &error_msg)?;
                            schema::log_job(&conn, job.id, "error", &error_msg)?;
                            println!("Job {} failed: {}", job.id, error_msg);

                            // Check if we should retry
                            if job.attempts < self.max_retries {
                                // Reset to pending for retry
                                conn.execute(
                                    "UPDATE jobs SET status = 'pending' WHERE id = ?1",
                                    [job.id],
                                )?;
                            }
                        }
                    }
                }
                None => {
                    // No more jobs
                    break;
                }
            }
        }

        Ok(completed)
    }

    /// Process a single job based on its type
    fn process_job(&self, conn: &Connection, job: &Job) -> Result<()> {
        match job.job_type.as_str() {
            "ingest" => self.process_ingest_job(conn, job),
            "hash_full" => self.process_hash_full_job(conn, job),
            _ => {
                println!("Unknown job type: {}", job.job_type);
                Ok(())
            }
        }
    }

    /// Process an ingest job
    fn process_ingest_job(&self, conn: &Connection, job: &Job) -> Result<()> {
        let payload: IngestPayload = serde_json::from_str(&job.payload)?;
        let source_path = Path::new(&payload.source_path);

        // Get library info
        let library = schema::get_library_by_path(
            conn,
            &self.library_root.to_string_lossy()
        )?.ok_or_else(|| anyhow::anyhow!("Library not found"))?;

        // Discover files
        let files = discover_media_files(source_path)?;
        let total_files = files.len();

        schema::log_job(conn, job.id, "info",
            &format!("Discovered {} media files", total_files))?;

        for (index, discovered) in files.iter().enumerate() {
            // Update progress
            let progress = ((index as f64 / total_files as f64) * 100.0) as i32;
            schema::update_job_progress(conn, job.id, progress)?;

            // Check for existing file with same hash
            let (hash_fast, hash_fast_scheme) = hash::compute_fast_hash(&discovered.path)?;

            if let Some(_existing) = schema::get_asset_by_hash(conn, library.id, &hash_fast)? {
                schema::log_job(conn, job.id, "info",
                    &format!("Skipping duplicate: {}", discovered.path.display()))?;
                continue;
            }

            // Copy or reference the file
            let (asset_path, source_uri) = if payload.ingest_mode == "copy" {
                let copy_result = copy_file_to_library(
                    &discovered.path,
                    &self.library_root,
                    true,
                    discovered.parent_folder.as_deref(),
                )?;
                (copy_result.relative_path, Some(discovered.path.to_string_lossy().to_string()))
            } else {
                // Reference mode
                (String::new(), Some(discovered.path.to_string_lossy().to_string()))
            };

            // Create asset record
            let asset_id = schema::create_asset(conn, &schema::NewAsset {
                library_id: library.id,
                asset_type: "original".to_string(),
                path: asset_path,
                source_uri,
                size_bytes: discovered.size_bytes as i64,
                hash_fast: Some(hash_fast),
                hash_fast_scheme: Some(hash_fast_scheme),
            })?;

            // Extract metadata
            let media_info = ffprobe::probe(&discovered.path)?;
            let media_type = ffprobe::get_media_type(&media_info);

            // Determine timestamp
            let (recorded_at, timestamp_source, is_estimated) =
                determine_timestamp(&discovered.path, &media_info);

            // Create clip record
            let title = discovered.path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string());

            let clip_id = schema::create_clip(conn, &schema::NewClip {
                library_id: library.id,
                original_asset_id: asset_id,
                media_type: media_type.to_string(),
                title,
                duration_ms: media_info.duration_ms,
                width: media_info.width,
                height: media_info.height,
                fps: media_info.fps,
                codec: media_info.codec,
                recorded_at,
                recorded_at_offset_minutes: None,
                recorded_at_is_estimated: is_estimated,
                timestamp_source,
            })?;

            // Link asset to clip
            schema::link_clip_asset(conn, clip_id, asset_id, "primary")?;

            // Queue background hash job
            schema::create_job(conn, &schema::NewJob {
                job_type: "hash_full".to_string(),
                library_id: Some(library.id),
                clip_id: Some(clip_id),
                asset_id: Some(asset_id),
                priority: -1, // Low priority
                payload: "{}".to_string(),
            })?;

            schema::log_job(conn, job.id, "info",
                &format!("Ingested: {}", discovered.path.display()))?;
        }

        Ok(())
    }

    /// Process a full hash job
    fn process_hash_full_job(&self, conn: &Connection, job: &Job) -> Result<()> {
        let asset_id = job.asset_id
            .ok_or_else(|| anyhow::anyhow!("No asset_id in hash_full job"))?;

        // Get asset path
        let asset_path: String = conn.query_row(
            "SELECT path FROM assets WHERE id = ?1",
            [asset_id],
            |row| row.get(0),
        )?;

        let full_path = self.library_root.join(&asset_path);

        if !full_path.exists() {
            return Err(anyhow::anyhow!("Asset file not found: {}", full_path.display()));
        }

        // Compute full hash
        let hash_full = hash::compute_full_hash(&full_path)?;

        // Update asset
        conn.execute(
            "UPDATE assets SET hash_full = ?1, verified_at = datetime('now') WHERE id = ?2",
            rusqlite::params![hash_full, asset_id],
        )?;

        schema::log_job(conn, job.id, "info",
            &format!("Computed full hash for asset {}", asset_id))?;

        Ok(())
    }
}

/// Determine the best timestamp for a clip
fn determine_timestamp(
    path: &Path,
    media_info: &ffprobe::MediaInfo,
) -> (Option<String>, Option<String>, bool) {
    // Try embedded metadata first
    if let Some(ref creation_time) = media_info.creation_time {
        return (Some(creation_time.clone()), Some("metadata".to_string()), false);
    }

    // Try folder name parsing (e.g., "2019-07-04")
    if let Some(parent) = path.parent() {
        if let Some(folder_name) = parent.file_name() {
            let folder_str = folder_name.to_string_lossy();
            if let Some(date) = parse_date_from_folder(&folder_str) {
                return (Some(date), Some("folder".to_string()), true);
            }
        }
    }

    // Fall back to filesystem modified time
    if let Ok(metadata) = path.metadata() {
        if let Ok(modified) = metadata.modified() {
            let datetime: chrono::DateTime<chrono::Utc> = modified.into();
            return (
                Some(datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                Some("filesystem".to_string()),
                true,
            );
        }
    }

    (None, None, true)
}

/// Try to parse a date from a folder name
fn parse_date_from_folder(folder_name: &str) -> Option<String> {
    // Common patterns: "2019-07-04", "20190704", "July 2019", etc.

    // Try YYYY-MM-DD
    if folder_name.len() >= 10 {
        let potential_date = &folder_name[..10];
        if chrono::NaiveDate::parse_from_str(potential_date, "%Y-%m-%d").is_ok() {
            return Some(format!("{}T00:00:00Z", potential_date));
        }
    }

    // Try YYYYMMDD
    if folder_name.len() >= 8 {
        let potential_date = &folder_name[..8];
        if let Ok(date) = chrono::NaiveDate::parse_from_str(potential_date, "%Y%m%d") {
            return Some(format!("{}T00:00:00Z", date.format("%Y-%m-%d")));
        }
    }

    None
}
```

---

Part 7: CLI Interface

7.1 CLI Commands

Create `src-tauri/src/cli.rs`:

```rust
use std::path::PathBuf;
use clap::{Parser, Subcommand};
use anyhow::Result;

use crate::db::{self, schema};
use crate::jobs::JobRunner;
use crate::constants::{DADCAM_FOLDER, DEFAULT_INGEST_MODE};

#[derive(Parser)]
#[command(name = "dadcam")]
#[command(about = "Dad Cam - Video library for dad cam footage")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new library at the given path
    Init {
        /// Path to the library root folder
        library_root: PathBuf,

        /// Library name (defaults to folder name)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Ingest footage from a path
    Ingest {
        /// Path to ingest from (folder or SD card)
        source_path: PathBuf,

        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Ingest mode: copy or reference
        #[arg(short, long, default_value = "copy")]
        mode: String,
    },

    /// List clips in the library
    List {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Maximum number of clips to show
        #[arg(short = 'n', long, default_value = "20")]
        limit: i64,
    },

    /// Show details for a specific clip
    Show {
        /// Clip ID
        clip_id: i64,

        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
    },

    /// List and manage jobs
    Jobs {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Filter by status (pending, running, completed, failed)
        #[arg(short, long)]
        status: Option<String>,

        /// Run pending jobs
        #[arg(short, long)]
        run: bool,

        /// Job type to run (default: all)
        #[arg(short = 't', long)]
        job_type: Option<String>,
    },

    /// Scan a path for files that match missing originals (relink)
    RelinkScan {
        /// Path to scan
        scan_path: PathBuf,

        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
    },
}

/// Run the CLI
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { library_root, name } => {
            cmd_init(&library_root, name.as_deref())
        }
        Commands::Ingest { source_path, library, mode } => {
            let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
            cmd_ingest(&source_path, &lib_root, &mode)
        }
        Commands::List { library, limit } => {
            let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
            cmd_list(&lib_root, limit)
        }
        Commands::Show { clip_id, library } => {
            let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
            cmd_show(clip_id, &lib_root)
        }
        Commands::Jobs { library, status, run, job_type } => {
            let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
            cmd_jobs(&lib_root, status.as_deref(), run, job_type.as_deref())
        }
        Commands::RelinkScan { scan_path, library } => {
            let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
            cmd_relink_scan(&scan_path, &lib_root)
        }
    }
}

fn cmd_init(library_root: &PathBuf, name: Option<&str>) -> Result<()> {
    // Create .dadcam folder
    let dadcam_dir = library_root.join(DADCAM_FOLDER);
    std::fs::create_dir_all(&dadcam_dir)?;

    // Create subdirectories
    std::fs::create_dir_all(dadcam_dir.join("proxies"))?;
    std::fs::create_dir_all(dadcam_dir.join("thumbs"))?;
    std::fs::create_dir_all(dadcam_dir.join("sprites"))?;
    std::fs::create_dir_all(dadcam_dir.join("exports"))?;

    // Create originals folder
    std::fs::create_dir_all(library_root.join("originals"))?;

    // Initialize database
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    // Determine library name
    let lib_name = name.map(String::from).unwrap_or_else(|| {
        library_root.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "My Library".to_string())
    });

    // Create library record
    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    schema::create_library(&conn, &root_path, &lib_name, DEFAULT_INGEST_MODE)?;

    println!("Initialized library: {}", lib_name);
    println!("Location: {}", library_root.display());

    Ok(())
}

fn cmd_ingest(source_path: &PathBuf, library_root: &PathBuf, mode: &str) -> Result<()> {
    // Verify library exists
    let db_path = db::get_db_path(library_root);
    if !db_path.exists() {
        return Err(anyhow::anyhow!(
            "No library found at {}. Run 'dadcam init' first.",
            library_root.display()
        ));
    }

    let conn = db::open_db(&db_path)?;

    // Get library
    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found in database"))?;

    // Create ingest job
    let payload = serde_json::json!({
        "source_path": source_path.canonicalize()?.to_string_lossy().to_string(),
        "ingest_mode": mode,
    });

    let job_id = schema::create_job(&conn, &schema::NewJob {
        job_type: "ingest".to_string(),
        library_id: Some(library.id),
        clip_id: None,
        asset_id: None,
        priority: 10, // High priority
        payload: payload.to_string(),
    })?;

    println!("Created ingest job {} for: {}", job_id, source_path.display());

    // Run the job immediately
    let runner = JobRunner::new(library_root);
    let completed = runner.run_until_empty("ingest")?;

    println!("Completed {} ingest jobs", completed);

    // Also run any queued hash jobs
    let hash_completed = runner.run_until_empty("hash_full")?;
    if hash_completed > 0 {
        println!("Completed {} hash jobs", hash_completed);
    }

    Ok(())
}

fn cmd_list(library_root: &PathBuf, limit: i64) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    let clips = schema::list_clips(&conn, library.id, limit)?;

    if clips.is_empty() {
        println!("No clips in library.");
        return Ok(());
    }

    println!("{:<6} {:<30} {:<10} {:<12} {:<20}",
        "ID", "Title", "Type", "Duration", "Recorded");
    println!("{}", "-".repeat(80));

    for clip in clips {
        let duration = clip.duration_ms
            .map(|ms| format_duration(ms))
            .unwrap_or_else(|| "-".to_string());

        let recorded = clip.recorded_at
            .as_ref()
            .map(|r| r.chars().take(19).collect::<String>())
            .unwrap_or_else(|| "-".to_string());

        let title = if clip.title.len() > 28 {
            format!("{}...", &clip.title[..25])
        } else {
            clip.title.clone()
        };

        println!("{:<6} {:<30} {:<10} {:<12} {:<20}",
            clip.id, title, clip.media_type, duration, recorded);
    }

    Ok(())
}

fn cmd_show(clip_id: i64, library_root: &PathBuf) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let clip: schema::Clip = conn.query_row(
        r#"SELECT id, library_id, original_asset_id, camera_profile_id, media_type,
                  title, duration_ms, width, height, fps, codec, recorded_at,
                  recorded_at_offset_minutes, recorded_at_is_estimated, timestamp_source, created_at
           FROM clips WHERE id = ?1"#,
        [clip_id],
        |row| Ok(schema::Clip {
            id: row.get(0)?,
            library_id: row.get(1)?,
            original_asset_id: row.get(2)?,
            camera_profile_id: row.get(3)?,
            media_type: row.get(4)?,
            title: row.get(5)?,
            duration_ms: row.get(6)?,
            width: row.get(7)?,
            height: row.get(8)?,
            fps: row.get(9)?,
            codec: row.get(10)?,
            recorded_at: row.get(11)?,
            recorded_at_offset_minutes: row.get(12)?,
            recorded_at_is_estimated: row.get(13)?,
            timestamp_source: row.get(14)?,
            created_at: row.get(15)?,
        }),
    )?;

    println!("Clip #{}", clip.id);
    println!("  Title:     {}", clip.title);
    println!("  Type:      {}", clip.media_type);

    if let Some(dur) = clip.duration_ms {
        println!("  Duration:  {}", format_duration(dur));
    }

    if let (Some(w), Some(h)) = (clip.width, clip.height) {
        println!("  Size:      {}x{}", w, h);
    }

    if let Some(fps) = clip.fps {
        println!("  FPS:       {:.2}", fps);
    }

    if let Some(ref codec) = clip.codec {
        println!("  Codec:     {}", codec);
    }

    if let Some(ref recorded) = clip.recorded_at {
        let estimated = if clip.recorded_at_is_estimated { " (estimated)" } else { "" };
        println!("  Recorded:  {}{}", recorded, estimated);
    }

    if let Some(ref source) = clip.timestamp_source {
        println!("  Time src:  {}", source);
    }

    println!("  Created:   {}", clip.created_at);

    Ok(())
}

fn cmd_jobs(
    library_root: &PathBuf,
    status: Option<&str>,
    run: bool,
    job_type: Option<&str>,
) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    if run {
        let runner = JobRunner::new(library_root);

        let types = match job_type {
            Some(t) => vec![t],
            None => vec!["ingest", "hash_full", "proxy", "thumb", "sprite"],
        };

        for jt in types {
            let completed = runner.run_until_empty(jt)?;
            if completed > 0 {
                println!("Completed {} {} jobs", completed, jt);
            }
        }
        return Ok(());
    }

    let jobs = schema::list_jobs(&conn, status, 50)?;

    if jobs.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    println!("{:<6} {:<12} {:<10} {:<8} {:<20}",
        "ID", "Type", "Status", "Progress", "Created");
    println!("{}", "-".repeat(60));

    for job in jobs {
        let progress = job.progress
            .map(|p| format!("{}%", p))
            .unwrap_or_else(|| "-".to_string());

        let created = job.created_at.chars().take(19).collect::<String>();

        println!("{:<6} {:<12} {:<10} {:<8} {:<20}",
            job.id, job.job_type, job.status, progress, created);
    }

    Ok(())
}

fn cmd_relink_scan(scan_path: &PathBuf, library_root: &PathBuf) -> Result<()> {
    println!("Relink scan not yet implemented.");
    println!("Would scan: {}", scan_path.display());
    println!("For library: {}", library_root.display());
    Ok(())
}

/// Format duration in milliseconds to human readable
fn format_duration(ms: i64) -> String {
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, mins % 60, secs % 60)
    } else {
        format!("{}:{:02}", mins, secs % 60)
    }
}
```

7.2 Update Main Entry Point

Update `src-tauri/src/main.rs`:

```rust
// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod constants;
mod db;
mod hash;
mod ingest;
mod jobs;
mod metadata;
// mod camera; // Add when implementing camera profiles
// mod error;  // Add when implementing custom errors

fn main() {
    // Check if running as CLI or GUI
    let args: Vec<String> = std::env::args().collect();

    // If we have CLI arguments (beyond just the program name), run CLI mode
    if args.len() > 1 && !args[1].starts_with("--tauri") {
        if let Err(e) = cli::run() {
            eprintln!("Error: {:?}", e);
            std::process::exit(1);
        }
    } else {
        // Run Tauri app (Phase 3)
        // For now, just print a message
        println!("GUI mode not implemented yet. Use CLI commands.");
        println!("Run 'dadcam --help' for usage.");
    }
}
```

---

Part 8: Testing Your Implementation

8.1 Build and Test

```bash
# Navigate to src-tauri
cd src-tauri

# Build the CLI
cargo build

# Run tests
cargo test

# Test the CLI
./target/debug/dadcam --help
```

8.2 Test Workflow

```bash
# 1. Create a test library
mkdir ~/test-library
./target/debug/dadcam init ~/test-library --name "Test Library"

# 2. Verify structure was created
ls -la ~/test-library/
ls -la ~/test-library/.dadcam/

# 3. Ingest some test footage
./target/debug/dadcam ingest /path/to/your/videos --library ~/test-library

# 4. List ingested clips
./target/debug/dadcam list --library ~/test-library

# 5. Show a specific clip
./target/debug/dadcam show 1 --library ~/test-library

# 6. Check job status
./target/debug/dadcam jobs --library ~/test-library
```

8.3 Test Crash Recovery

```bash
# 1. Start a large ingest
./target/debug/dadcam ingest /path/to/lots/of/videos --library ~/test-library

# 2. While running, press Ctrl+C to simulate crash

# 3. Resume by running again
./target/debug/dadcam jobs --library ~/test-library --run

# Should pick up where it left off
```

---

Part 8a: Camera Profile Matcher

8a.1 Understanding Camera Profiles

Camera profiles detect which camera recorded a clip based on metadata patterns (make, model, codec, resolution). This enables per-camera transforms in later phases.

Create `src-tauri/src/camera/mod.rs`:

```rust
use std::path::Path;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::schema::CameraProfile;
use crate::metadata::ffprobe::MediaInfo;
use crate::metadata::exiftool::ExifInfo;

/// Result of matching a clip against camera profiles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraMatch {
    pub profile_id: i64,
    pub profile_name: String,
    pub confidence: f64,  // 0.0 to 1.0
    pub match_reasons: Vec<String>,
}

/// Rules for matching a camera profile
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MatchRules {
    /// Camera make (case-insensitive contains)
    pub make: Option<String>,
    /// Camera model (case-insensitive contains)
    pub model: Option<String>,
    /// Video codec (exact match)
    pub codec: Option<String>,
    /// Width (exact match)
    pub width: Option<i32>,
    /// Height (exact match)
    pub height: Option<i32>,
    /// FPS range (min, max)
    pub fps_range: Option<(f64, f64)>,
    /// File extension (lowercase)
    pub extension: Option<String>,
    /// Folder name pattern (case-insensitive contains)
    pub folder_pattern: Option<String>,
}

/// Match a clip against all available camera profiles.
/// Returns the best match if confidence >= 0.5, or None.
pub fn match_camera_profile(
    profiles: &[CameraProfile],
    media_info: &MediaInfo,
    exif_info: &ExifInfo,
    file_path: &Path,
) -> Option<CameraMatch> {
    let mut best_match: Option<CameraMatch> = None;

    for profile in profiles {
        // Parse match rules from JSON
        let rules: MatchRules = serde_json::from_str(&profile.match_rules)
            .unwrap_or_default();

        let (confidence, reasons) = compute_match_score(&rules, media_info, exif_info, file_path);

        if confidence >= 0.5 {
            if best_match.as_ref().map(|m| confidence > m.confidence).unwrap_or(true) {
                best_match = Some(CameraMatch {
                    profile_id: profile.id,
                    profile_name: profile.name.clone(),
                    confidence,
                    match_reasons: reasons,
                });
            }
        }
    }

    best_match
}

/// Compute match score for a single profile
fn compute_match_score(
    rules: &MatchRules,
    media_info: &MediaInfo,
    exif_info: &ExifInfo,
    file_path: &Path,
) -> (f64, Vec<String>) {
    let mut score = 0.0;
    let mut max_score = 0.0;
    let mut reasons = Vec::new();

    // Make match (weight: 0.3)
    if let Some(ref expected_make) = rules.make {
        max_score += 0.3;
        if let Some(ref actual_make) = exif_info.camera_make {
            if actual_make.to_lowercase().contains(&expected_make.to_lowercase()) {
                score += 0.3;
                reasons.push(format!("make: {}", actual_make));
            }
        }
    }

    // Model match (weight: 0.3)
    if let Some(ref expected_model) = rules.model {
        max_score += 0.3;
        if let Some(ref actual_model) = exif_info.camera_model {
            if actual_model.to_lowercase().contains(&expected_model.to_lowercase()) {
                score += 0.3;
                reasons.push(format!("model: {}", actual_model));
            }
        }
    }

    // Codec match (weight: 0.2)
    if let Some(ref expected_codec) = rules.codec {
        max_score += 0.2;
        if let Some(ref actual_codec) = media_info.codec {
            if actual_codec.to_lowercase() == expected_codec.to_lowercase() {
                score += 0.2;
                reasons.push(format!("codec: {}", actual_codec));
            }
        }
    }

    // Resolution match (weight: 0.15)
    if rules.width.is_some() || rules.height.is_some() {
        max_score += 0.15;
        let width_match = rules.width.map(|w| media_info.width == Some(w)).unwrap_or(true);
        let height_match = rules.height.map(|h| media_info.height == Some(h)).unwrap_or(true);
        if width_match && height_match {
            score += 0.15;
            if let (Some(w), Some(h)) = (media_info.width, media_info.height) {
                reasons.push(format!("resolution: {}x{}", w, h));
            }
        }
    }

    // FPS range match (weight: 0.1)
    if let Some((min_fps, max_fps)) = rules.fps_range {
        max_score += 0.1;
        if let Some(fps) = media_info.fps {
            if fps >= min_fps && fps <= max_fps {
                score += 0.1;
                reasons.push(format!("fps: {:.2}", fps));
            }
        }
    }

    // Extension match (weight: 0.1)
    if let Some(ref expected_ext) = rules.extension {
        max_score += 0.1;
        if let Some(actual_ext) = file_path.extension() {
            if actual_ext.to_string_lossy().to_lowercase() == expected_ext.to_lowercase() {
                score += 0.1;
                reasons.push(format!("extension: {}", expected_ext));
            }
        }
    }

    // Folder pattern match (weight: 0.1)
    if let Some(ref pattern) = rules.folder_pattern {
        max_score += 0.1;
        if let Some(parent) = file_path.parent() {
            let folder_str = parent.to_string_lossy().to_lowercase();
            if folder_str.contains(&pattern.to_lowercase()) {
                score += 0.1;
                reasons.push(format!("folder: {}", pattern));
            }
        }
    }

    // Normalize score to 0-1 range
    let confidence = if max_score > 0.0 {
        score / max_score
    } else {
        0.0
    };

    (confidence, reasons)
}

/// Load default camera profiles into database
pub fn seed_default_profiles(conn: &rusqlite::Connection) -> Result<()> {
    use crate::db::schema::create_camera_profile;

    // Check if profiles already exist
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM camera_profiles",
        [],
        |row| row.get(0),
    )?;

    if count > 0 {
        return Ok(()); // Already seeded
    }

    // Canon AVCHD camcorders
    let canon_rules = serde_json::json!({
        "make": "Canon",
        "codec": "h264",
        "extension": "mts"
    });
    create_camera_profile(conn, "Canon AVCHD", &canon_rules.to_string(), "{}")?;

    // Sony Handycam
    let sony_rules = serde_json::json!({
        "make": "Sony",
        "folder_pattern": "AVCHD"
    });
    create_camera_profile(conn, "Sony Handycam", &sony_rules.to_string(), "{}")?;

    // Panasonic camcorders
    let pana_rules = serde_json::json!({
        "make": "Panasonic",
        "extension": "mts"
    });
    create_camera_profile(conn, "Panasonic AVCHD", &pana_rules.to_string(), "{}")?;

    // Generic DV camera
    let dv_rules = serde_json::json!({
        "codec": "dvvideo",
        "extension": "avi"
    });
    create_camera_profile(conn, "DV Camera", &dv_rules.to_string(), "{}")?;

    // GoPro
    let gopro_rules = serde_json::json!({
        "make": "GoPro",
        "extension": "mp4"
    });
    create_camera_profile(conn, "GoPro", &gopro_rules.to_string(), "{}")?;

    Ok(())
}
```

8a.2 Update main.rs to include camera module

Add to the module declarations in `main.rs`:

```rust
mod camera;
```

---

Part 8b: Volume Identity Tracking

Volume identity helps with relinking when files move between drives.

Add this function to `src-tauri/src/ingest/discover.rs`:

```rust
use std::process::Command;

/// Get volume information for a path (serial number and label)
/// Returns (serial, label) - both may be None on some platforms
pub fn get_volume_info(path: &Path) -> (Option<String>, Option<String>) {
    #[cfg(target_os = "macos")]
    {
        get_volume_info_macos(path)
    }
    #[cfg(target_os = "windows")]
    {
        get_volume_info_windows(path)
    }
    #[cfg(target_os = "linux")]
    {
        get_volume_info_linux(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        (None, None)
    }
}

#[cfg(target_os = "macos")]
fn get_volume_info_macos(path: &Path) -> (Option<String>, Option<String>) {
    // Get mount point for path
    let output = Command::new("df")
        .arg(path)
        .output()
        .ok();

    let mount_point = output.and_then(|o| {
        let stdout = String::from_utf8_lossy(&o.stdout);
        stdout.lines().nth(1)
            .and_then(|line| line.split_whitespace().last())
            .map(String::from)
    });

    // Get volume name from mount point
    let label = mount_point.as_ref().and_then(|mp| {
        std::path::Path::new(mp)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    });

    // Try to get volume UUID using diskutil
    let serial = mount_point.and_then(|mp| {
        Command::new("diskutil")
            .args(["info", &mp])
            .output()
            .ok()
            .and_then(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.lines()
                    .find(|line| line.contains("Volume UUID:"))
                    .and_then(|line| line.split(':').nth(1))
                    .map(|s| s.trim().to_string())
            })
    });

    (serial, label)
}

#[cfg(target_os = "windows")]
fn get_volume_info_windows(path: &Path) -> (Option<String>, Option<String>) {
    // Get drive letter
    let drive = path.components().next()
        .and_then(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.len() >= 2 {
                Some(format!("{}\\", &s[..2]))
            } else {
                None
            }
        });

    if let Some(ref drive_path) = drive {
        // Use wmic to get volume serial and label
        let output = Command::new("wmic")
            .args(["volume", "where", &format!("DriveLetter='{}'", &drive_path[..2]), "get", "SerialNumber,Label", "/format:csv"])
            .output()
            .ok();

        if let Some(o) = output {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = stdout.lines()
                .filter(|l| !l.trim().is_empty())
                .last()
                .map(|l| l.split(',').collect())
                .unwrap_or_default();

            if parts.len() >= 3 {
                let label = if parts[1].is_empty() { None } else { Some(parts[1].to_string()) };
                let serial = if parts[2].is_empty() { None } else { Some(parts[2].to_string()) };
                return (serial, label);
            }
        }
    }

    (None, None)
}

#[cfg(target_os = "linux")]
fn get_volume_info_linux(path: &Path) -> (Option<String>, Option<String>) {
    // Find device for path using df
    let output = Command::new("df")
        .arg(path)
        .output()
        .ok();

    let device = output.and_then(|o| {
        let stdout = String::from_utf8_lossy(&o.stdout);
        stdout.lines().nth(1)
            .and_then(|line| line.split_whitespace().next())
            .map(String::from)
    });

    if let Some(ref dev) = device {
        // Get UUID using blkid
        let uuid = Command::new("blkid")
            .args(["-s", "UUID", "-o", "value", dev])
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        // Get label using blkid
        let label = Command::new("blkid")
            .args(["-s", "LABEL", "-o", "value", dev])
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        return (uuid, label);
    }

    (None, None)
}
```

---

Part 8c: Integrated Ingest Pipeline

Now we integrate all pieces: per-file tracking, sidecars, volumes, fingerprints, and camera profiles.

Replace the `process_ingest_job` function in `src-tauri/src/jobs/runner.rs`:

```rust
/// Process an ingest job with full feature support
fn process_ingest_job(&self, conn: &Connection, job: &Job) -> Result<()> {
    let payload: IngestPayload = serde_json::from_str(&job.payload)?;
    let source_path = Path::new(&payload.source_path);

    // Get library info
    let library = schema::get_library_by_path(
        conn,
        &self.library_root.to_string_lossy()
    )?.ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    // Load camera profiles for matching
    let camera_profiles = schema::get_all_camera_profiles(conn)?;

    // Get volume info for source path
    let (vol_serial, vol_label) = crate::ingest::discover::get_volume_info(source_path);
    let volume_id = if vol_serial.is_some() || vol_label.is_some() {
        Some(schema::get_or_create_volume(conn, vol_serial.as_deref(), vol_label.as_deref())?)
    } else {
        None
    };

    // Check for resume: get existing ingest files for this job
    let existing_files = schema::get_pending_ingest_files(conn, job.id)?;
    let is_resume = !existing_files.is_empty();

    // Discover files (or use existing list for resume)
    let files = if is_resume {
        schema::log_job(conn, job.id, "info", "Resuming interrupted ingest")?;
        existing_files.iter()
            .map(|f| crate::ingest::discover::DiscoveredFile {
                path: PathBuf::from(&f.source_path),
                size_bytes: 0, // Will re-read from disk
                media_category: crate::ingest::discover::MediaCategory::Video,
                parent_folder: None,
            })
            .collect()
    } else {
        let discovered = crate::ingest::discover::discover_media_files(source_path)?;
        // Register all discovered files in tracking table
        for file in &discovered {
            schema::create_ingest_file(conn, job.id, &file.path.to_string_lossy())?;
        }
        schema::log_job(conn, job.id, "info",
            &format!("Discovered {} media files", discovered.len()))?;
        discovered
    };

    let total_files = files.len();

    for (index, discovered) in files.iter().enumerate() {
        // Update progress
        let progress = ((index as f64 / total_files as f64) * 100.0) as i32;
        schema::update_job_progress(conn, job.id, progress)?;

        // Find or create ingest file record
        let ingest_file_id: i64 = conn.query_row(
            "SELECT id FROM ingest_files WHERE job_id = ?1 AND source_path = ?2",
            rusqlite::params![job.id, discovered.path.to_string_lossy().to_string()],
            |row| row.get(0),
        ).unwrap_or(0);

        if ingest_file_id == 0 {
            continue; // Skip if not in tracking table
        }

        // Check if already complete
        let current_status: String = conn.query_row(
            "SELECT status FROM ingest_files WHERE id = ?1",
            [ingest_file_id],
            |row| row.get(0),
        ).unwrap_or_default();

        if current_status == "complete" || current_status == "skipped" {
            continue; // Already processed
        }

        // Mark as copying
        schema::update_ingest_file_status(conn, ingest_file_id, "copying", None, None, None)?;

        // Compute fast hash for dedup check
        let (hash_fast, hash_fast_scheme) = match hash::compute_fast_hash(&discovered.path) {
            Ok(h) => h,
            Err(e) => {
                schema::update_ingest_file_status(
                    conn, ingest_file_id, "failed", None, None, Some(&e.to_string()))?;
                schema::log_job(conn, job.id, "warn",
                    &format!("Hash failed for {}: {}", discovered.path.display(), e))?;
                continue;
            }
        };

        // Check for duplicate
        if let Some(_existing) = schema::get_asset_by_hash(conn, library.id, &hash_fast)? {
            schema::update_ingest_file_status(conn, ingest_file_id, "skipped", None, None, None)?;
            schema::log_job(conn, job.id, "info",
                &format!("Skipping duplicate: {}", discovered.path.display()))?;
            continue;
        }

        // Mark as hashing (done above) then copying
        schema::update_ingest_file_status(conn, ingest_file_id, "hashing", None, None, None)?;

        // Copy or reference the file
        let (asset_path, source_uri) = if payload.ingest_mode == "copy" {
            match copy_file_to_library(
                &discovered.path,
                &self.library_root,
                true,
                discovered.parent_folder.as_deref(),
            ) {
                Ok(result) => (result.relative_path, Some(discovered.path.to_string_lossy().to_string())),
                Err(e) => {
                    schema::update_ingest_file_status(
                        conn, ingest_file_id, "failed", None, None, Some(&e.to_string()))?;
                    schema::log_job(conn, job.id, "error",
                        &format!("Copy failed for {}: {}", discovered.path.display(), e))?;
                    continue;
                }
            }
        } else {
            (String::new(), Some(discovered.path.to_string_lossy().to_string()))
        };

        // Get actual file size
        let file_size = discovered.path.metadata()
            .map(|m| m.len() as i64)
            .unwrap_or(0);

        // Mark as metadata extraction
        schema::update_ingest_file_status(conn, ingest_file_id, "metadata", None, None, None)?;

        // Create asset record
        let asset_id = schema::create_asset(conn, &schema::NewAsset {
            library_id: library.id,
            asset_type: "original".to_string(),
            path: asset_path,
            source_uri,
            size_bytes: file_size,
            hash_fast: Some(hash_fast.clone()),
            hash_fast_scheme: Some(hash_fast_scheme),
        })?;

        // Link asset to volume
        if let Some(vid) = volume_id {
            schema::link_asset_volume(conn, asset_id, vid)?;
        }

        // Extract metadata
        let media_info = ffprobe::probe(&discovered.path)?;
        let media_type = ffprobe::get_media_type(&media_info);

        // Extract EXIF info for camera matching
        let exif_info = crate::metadata::exiftool::extract(&discovered.path)
            .unwrap_or_default();

        // Determine timestamp
        let (recorded_at, timestamp_source, is_estimated) =
            determine_timestamp(&discovered.path, &media_info);

        // Create clip record
        let title = discovered.path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let clip_id = schema::create_clip(conn, &schema::NewClip {
            library_id: library.id,
            original_asset_id: asset_id,
            media_type: media_type.to_string(),
            title,
            duration_ms: media_info.duration_ms,
            width: media_info.width,
            height: media_info.height,
            fps: media_info.fps,
            codec: media_info.codec.clone(),
            recorded_at,
            recorded_at_offset_minutes: None,
            recorded_at_is_estimated: is_estimated,
            timestamp_source,
        })?;

        // Link asset to clip
        schema::link_clip_asset(conn, clip_id, asset_id, "primary")?;

        // Match camera profile
        if !camera_profiles.is_empty() {
            if let Some(camera_match) = crate::camera::match_camera_profile(
                &camera_profiles,
                &media_info,
                &exif_info,
                &discovered.path,
            ) {
                schema::update_clip_camera_profile(conn, clip_id, camera_match.profile_id)?;
                schema::log_job(conn, job.id, "info",
                    &format!("Matched camera: {} ({:.0}%)",
                        camera_match.profile_name, camera_match.confidence * 100.0))?;
            }
        }

        // Create fingerprint for relink (size + duration)
        if let Some(duration_ms) = media_info.duration_ms {
            let fingerprint_value = format!("{}_{}", file_size, duration_ms);
            schema::create_fingerprint(conn, clip_id, "size_duration", &fingerprint_value)?;
        }

        // Find and ingest sidecars
        let sidecars = crate::ingest::discover::find_sidecars(&discovered.path);
        for sidecar_path in sidecars {
            let sidecar_size = sidecar_path.metadata()
                .map(|m| m.len() as i64)
                .unwrap_or(0);

            // Copy sidecar
            let (sidecar_dest, sidecar_source) = if payload.ingest_mode == "copy" {
                match copy_file_to_library(
                    &sidecar_path,
                    &self.library_root,
                    true,
                    discovered.parent_folder.as_deref(),
                ) {
                    Ok(result) => (result.relative_path, Some(sidecar_path.to_string_lossy().to_string())),
                    Err(e) => {
                        schema::log_job(conn, job.id, "warn",
                            &format!("Sidecar copy failed: {}", e))?;
                        continue;
                    }
                }
            } else {
                (String::new(), Some(sidecar_path.to_string_lossy().to_string()))
            };

            // Create sidecar asset
            let sidecar_asset_id = schema::create_asset(conn, &schema::NewAsset {
                library_id: library.id,
                asset_type: "original".to_string(),
                path: sidecar_dest,
                source_uri: sidecar_source,
                size_bytes: sidecar_size,
                hash_fast: None,
                hash_fast_scheme: None,
            })?;

            // Link sidecar to clip
            schema::link_clip_asset(conn, clip_id, sidecar_asset_id, "sidecar")?;

            schema::log_job(conn, job.id, "info",
                &format!("Ingested sidecar: {}", sidecar_path.display()))?;
        }

        // Queue background hash job
        schema::create_job(conn, &schema::NewJob {
            job_type: "hash_full".to_string(),
            library_id: Some(library.id),
            clip_id: Some(clip_id),
            asset_id: Some(asset_id),
            priority: -1,
            payload: "{}".to_string(),
        })?;

        // Mark file as complete
        schema::update_ingest_file_status(
            conn, ingest_file_id, "complete", Some(asset_id), Some(clip_id), None)?;

        schema::log_job(conn, job.id, "info",
            &format!("Ingested: {}", discovered.path.display()))?;
    }

    Ok(())
}
```

Don't forget to add the necessary imports at the top of `runner.rs`:

```rust
use std::path::PathBuf;
```

---

Part 8d: Relink Scan Implementation

Replace the stub `cmd_relink_scan` function in `cli.rs`:

```rust
fn cmd_relink_scan(scan_path: &PathBuf, library_root: &PathBuf) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let _library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    println!("Scanning {} for relink candidates...", scan_path.display());

    // Discover media files in scan path
    let files = crate::ingest::discover::discover_media_files(scan_path)?;
    println!("Found {} media files", files.len());

    let mut matches_found = 0;

    for file in &files {
        // Get file size
        let size = match file.path.metadata() {
            Ok(m) => m.len() as i64,
            Err(_) => continue,
        };

        // Probe for duration
        let media_info = match crate::metadata::ffprobe::probe(&file.path) {
            Ok(info) => info,
            Err(_) => continue,
        };

        // Create fingerprint to search
        if let Some(duration_ms) = media_info.duration_ms {
            let fingerprint = format!("{}_{}", size, duration_ms);

            // Search for matching clips
            let clip_ids = schema::find_clips_by_fingerprint(&conn, "size_duration", &fingerprint)?;

            if !clip_ids.is_empty() {
                matches_found += clip_ids.len();
                println!("\nPotential match:");
                println!("  File: {}", file.path.display());
                println!("  Size: {} bytes, Duration: {}ms", size, duration_ms);
                println!("  Matches clip IDs: {:?}", clip_ids);
            }
        }
    }

    if matches_found == 0 {
        println!("\nNo relink candidates found.");
    } else {
        println!("\nFound {} potential matches.", matches_found);
        println!("Use 'dadcam relink <clip_id> <file_path>' to relink (Phase 2).");
    }

    Ok(())
}
```

---

Part 8e: Job Cancel Command

Add cancel subcommand to the Jobs command in `cli.rs`:

Update the Jobs command enum:

```rust
/// List and manage jobs
Jobs {
    /// Library root (defaults to current directory)
    #[arg(short, long)]
    library: Option<PathBuf>,

    /// Filter by status (pending, running, completed, failed, cancelled)
    #[arg(short, long)]
    status: Option<String>,

    /// Run pending jobs
    #[arg(short, long)]
    run: bool,

    /// Job type to run (default: all)
    #[arg(short = 't', long)]
    job_type: Option<String>,

    /// Cancel a specific job by ID
    #[arg(short, long)]
    cancel: Option<i64>,
},
```

Update the `cmd_jobs` function:

```rust
fn cmd_jobs(
    library_root: &PathBuf,
    status: Option<&str>,
    run: bool,
    job_type: Option<&str>,
    cancel: Option<i64>,
) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    // Handle cancel
    if let Some(job_id) = cancel {
        if schema::cancel_job(&conn, job_id)? {
            println!("Job {} cancelled.", job_id);
        } else {
            println!("Job {} could not be cancelled (already completed or not found).", job_id);
        }
        return Ok(());
    }

    if run {
        let runner = JobRunner::new(library_root);

        let types = match job_type {
            Some(t) => vec![t],
            None => vec!["ingest", "hash_full", "proxy", "thumb", "sprite"],
        };

        for jt in types {
            let completed = runner.run_until_empty(jt)?;
            if completed > 0 {
                println!("Completed {} {} jobs", completed, jt);
            }
        }
        return Ok(());
    }

    let jobs = schema::list_jobs(&conn, status, 50)?;

    if jobs.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    println!("{:<6} {:<12} {:<10} {:<8} {:<20}",
        "ID", "Type", "Status", "Progress", "Created");
    println!("{}", "-".repeat(60));

    for job in jobs {
        let progress = job.progress
            .map(|p| format!("{}%", p))
            .unwrap_or_else(|| "-".to_string());

        let created = job.created_at.chars().take(19).collect::<String>();

        println!("{:<6} {:<12} {:<10} {:<8} {:<20}",
            job.id, job.job_type, job.status, progress, created);
    }

    Ok(())
}
```

Update the match in `run()`:

```rust
Commands::Jobs { library, status, run, job_type, cancel } => {
    let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
    cmd_jobs(&lib_root, status.as_deref(), run, job_type.as_deref(), cancel)
}
```

---

Part 9: Checklist

Before moving to Phase 2, verify:

[ ] Library initialization creates correct folder structure
[ ] Database schema applies cleanly
[ ] Ingest discovers video, audio, and image files
[ ] Ingest handles duplicates (same hash = skip)
[ ] Copy verification works (hash match after copy)
[ ] Metadata extraction runs on all clips
[ ] Jobs are durable (survive crashes)
[ ] Jobs are resumable (can re-run after interrupt)
[ ] Per-file ingest state tracks each file individually
[ ] Sidecars (THM, XML, XMP) are discovered and copied
[ ] Sidecars are linked to clips with role="sidecar"
[ ] Volume serial/label is captured during ingest
[ ] Fingerprints (size_duration) are created for each clip
[ ] Camera profiles are matched and assigned
[ ] Job cancellation works (dadcam jobs --cancel ID)
[ ] Relink scan finds candidates by fingerprint
[ ] CLI commands all work
[ ] list shows clips with correct metadata
[ ] show displays full clip details

---

Resources

- [Tauri 2.0 Docs](https://v2.tauri.app/)
- [rusqlite Docs](https://docs.rs/rusqlite)
- [BLAKE3 Rust Crate](https://docs.rs/blake3)
- [ffmpeg-sidecar Crate](https://docs.rs/ffmpeg-sidecar)
- [clap CLI Parser](https://docs.rs/clap)
- [rusqlite_migration](https://docs.rs/rusqlite_migration)

---

Next Steps

After Phase 1 is complete:
- Phase 2: Preview Pipeline (proxies, thumbnails, sprites)
- Phase 3: Desktop App Shell (Tauri + React UI)

See development-plan.md for the full roadmap.

---

End of Phase 1 Implementation Guide
