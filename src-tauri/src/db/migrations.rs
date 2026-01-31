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

    // Migration 2: Scoring tables (Phase 4)
    r#"
    -- Clip scores table (machine-generated)
    CREATE TABLE clip_scores (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        clip_id INTEGER NOT NULL UNIQUE REFERENCES clips(id) ON DELETE CASCADE,

        -- Overall score (weighted combination)
        overall_score REAL NOT NULL CHECK (overall_score >= 0 AND overall_score <= 1),

        -- Component scores (each 0-1)
        scene_score REAL NOT NULL DEFAULT 0,
        audio_score REAL NOT NULL DEFAULT 0,
        sharpness_score REAL NOT NULL DEFAULT 0,
        motion_score REAL NOT NULL DEFAULT 0,

        -- Reasons array (JSON)
        reasons TEXT NOT NULL DEFAULT '[]',

        -- Versioning for invalidation
        pipeline_version INTEGER NOT NULL,
        scoring_version INTEGER NOT NULL DEFAULT 1,

        -- Timestamps
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- User score overrides (human preference)
    CREATE TABLE clip_score_overrides (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        clip_id INTEGER NOT NULL UNIQUE REFERENCES clips(id) ON DELETE CASCADE,

        -- Override type: 'promote' adds to score, 'demote' subtracts, 'pin' sets exact
        override_type TEXT NOT NULL CHECK (override_type IN ('promote', 'demote', 'pin')),

        -- For 'pin' type, this is the exact score. For promote/demote, this is the adjustment.
        override_value REAL NOT NULL DEFAULT 0.2,

        -- Optional note from user
        note TEXT,

        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Indexes
    CREATE INDEX idx_clip_scores_overall ON clip_scores(overall_score DESC);
    CREATE INDEX idx_clip_scores_clip ON clip_scores(clip_id);
    CREATE INDEX idx_clip_scores_version ON clip_scores(pipeline_version);
    CREATE INDEX idx_clip_score_overrides_clip ON clip_score_overrides(clip_id);
    "#,

    // Migration 3: Events system for clip organization (Phase 6)
    r#"
    -- Events table
    CREATE TABLE events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT,
        type TEXT NOT NULL CHECK (type IN ('date_range', 'clip_selection')),
        -- For date_range type
        date_start TEXT,
        date_end TEXT,
        -- Metadata
        color TEXT DEFAULT '#3b82f6',
        icon TEXT DEFAULT 'calendar',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- Event clips (for clip_selection type, or manual additions to date_range)
    CREATE TABLE event_clips (
        event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
        clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
        added_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (event_id, clip_id)
    );

    -- Indexes
    CREATE INDEX idx_events_library ON events(library_id);
    CREATE INDEX idx_events_type ON events(type);
    CREATE INDEX idx_events_date_range ON events(date_start, date_end);
    CREATE INDEX idx_event_clips_event ON event_clips(event_id);
    CREATE INDEX idx_event_clips_clip ON event_clips(clip_id);
    "#,

    // Migration 4: Export history (VHS Export)
    r#"
    CREATE TABLE export_history (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        library_id INTEGER NOT NULL REFERENCES libraries(id),
        output_path TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        selection_mode TEXT NOT NULL,
        selection_params TEXT DEFAULT '{}',
        ordering TEXT NOT NULL DEFAULT 'chronological',
        title_text TEXT,
        resolution TEXT,
        is_watermarked INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL DEFAULT 'pending',
        duration_ms INTEGER,
        file_size_bytes INTEGER,
        clip_count INTEGER,
        error_message TEXT,
        completed_at TEXT
    );
    CREATE INDEX idx_export_history_library ON export_history(library_id);
    CREATE INDEX idx_export_history_status ON export_history(status);
    "#,

    // Migration 5: Camera devices + clip device linkage (Phase 5)
    r#"
    CREATE TABLE camera_devices (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        uuid TEXT NOT NULL UNIQUE,
        profile_id INTEGER REFERENCES camera_profiles(id),
        serial_number TEXT,
        fleet_label TEXT,
        usb_fingerprints TEXT DEFAULT '[]',
        rental_notes TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    ALTER TABLE clips ADD COLUMN camera_device_id INTEGER REFERENCES camera_devices(id);

    CREATE INDEX idx_camera_devices_uuid ON camera_devices(uuid);
    CREATE INDEX idx_clips_camera_device ON clips(camera_device_id);
    "#,

    // Migration 6: Library identity metadata (L0 from libraryfix.md)
    // KV table for portable library metadata. Source of truth for library_uuid.
    r#"
    CREATE TABLE IF NOT EXISTS library_meta (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
    "#,

    // Migration 7: Stable camera references on clips (L6 from libraryfix.md)
    // Adds profile_type/profile_ref/device_uuid instead of integer FKs.
    // Old camera_profile_id and camera_device_id columns are kept (never drop).
    r#"
    ALTER TABLE clips ADD COLUMN camera_profile_type TEXT;
    ALTER TABLE clips ADD COLUMN camera_profile_ref TEXT;
    ALTER TABLE clips ADD COLUMN camera_device_uuid TEXT;

    CREATE INDEX IF NOT EXISTS idx_clips_camera_profile_ref
        ON clips(camera_profile_type, camera_profile_ref);
    CREATE INDEX IF NOT EXISTS idx_clips_camera_device_uuid
        ON clips(camera_device_uuid);
    "#,

    // Migration 8: VHS deterministic recipe definitions (L7 from libraryfix.md)
    // Stores rebuildable recipe definitions. Coexists with export_history (Migration 4)
    // which stores execution logs. vhs_edits.edit_uuid can be referenced by export_history.
    r#"
    CREATE TABLE IF NOT EXISTS vhs_edits (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        edit_uuid TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        pipeline_version INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),

        recipe_hash TEXT NOT NULL,
        input_clip_ids TEXT NOT NULL,
        title_text TEXT NOT NULL DEFAULT '',
        title_offset_seconds INTEGER NOT NULL DEFAULT 5,
        audio_blend_params TEXT NOT NULL,
        transform_overrides TEXT NOT NULL,

        output_relpath TEXT,
        output_hash TEXT
    );
    CREATE INDEX IF NOT EXISTS idx_vhs_edits_created_at ON vhs_edits(created_at);
    "#,

    // Migration 9: Gold-standard import verification (importplan.md)
    // Ingest sessions, manifest entries, and verified_method on assets.
    r#"
    -- Ingest sessions (one per import from a device)
    CREATE TABLE ingest_sessions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        job_id INTEGER NOT NULL REFERENCES jobs(id),
        source_root TEXT NOT NULL,
        device_serial TEXT,
        device_label TEXT,
        device_mount_point TEXT,
        device_capacity_bytes INTEGER,
        status TEXT NOT NULL DEFAULT 'discovering'
            CHECK (status IN ('discovering','ingesting','verifying','rescanning','complete','failed')),
        manifest_hash TEXT,
        rescan_hash TEXT,
        safe_to_wipe_at TEXT,
        started_at TEXT NOT NULL DEFAULT (datetime('now')),
        finished_at TEXT
    );

    -- Manifest entries (one per discovered eligible file on device)
    CREATE TABLE ingest_manifest_entries (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        session_id INTEGER NOT NULL REFERENCES ingest_sessions(id),
        relative_path TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        mtime TEXT,
        hash_fast TEXT,
        hash_source_full TEXT,
        asset_id INTEGER REFERENCES assets(id),
        result TEXT NOT NULL DEFAULT 'pending'
            CHECK (result IN ('pending','copying','copied_verified','dedup_verified',
                              'skipped_ineligible','failed','changed')),
        error_code TEXT,
        error_detail TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX idx_manifest_session ON ingest_manifest_entries(session_id);
    CREATE INDEX idx_manifest_result ON ingest_manifest_entries(result);

    -- Add verified_method to assets
    ALTER TABLE assets ADD COLUMN verified_method TEXT;
    "#,

    // Migration 10: Sidecar gold-standard import (sidecar-importplan.md)
    // Sidecars become first-class manifest entries with their own rows.
    r#"
    ALTER TABLE ingest_manifest_entries ADD COLUMN entry_type TEXT NOT NULL DEFAULT 'media'
        CHECK (entry_type IN ('media', 'sidecar'));
    ALTER TABLE ingest_manifest_entries ADD COLUMN parent_entry_id INTEGER
        REFERENCES ingest_manifest_entries(id);
    CREATE INDEX idx_manifest_entry_type ON ingest_manifest_entries(entry_type);
    CREATE INDEX idx_manifest_parent ON ingest_manifest_entries(parent_entry_id);
    "#,

    // Migration 11: Metadata extraction state machine (metadata-plan.md Layer 6)
    // Tracks per-clip extraction + matching pipeline state for crash recovery.
    // Existing clips backfilled as 'verified' (they have metadata from the old pipeline).
    // Also expands jobs table to support rematch/reextract job types by recreating
    // with the updated CHECK constraint.
    r#"
    ALTER TABLE clips ADD COLUMN metadata_status TEXT
        CHECK (metadata_status IN ('pending','extracting','extracted','matching','matched','verified','extraction_failed'))
        DEFAULT 'verified';

    ALTER TABLE ingest_sessions ADD COLUMN metadata_complete_at TEXT;

    CREATE INDEX idx_clips_metadata_status ON clips(metadata_status);

    -- Recreate jobs table with expanded type CHECK to include rematch + reextract.
    -- This preserves all existing data.
    -- Must defer FK checks during the table swap (DROP old -> RENAME new).
    PRAGMA defer_foreign_keys = ON;
    DROP TABLE IF EXISTS jobs_new;
    BEGIN;
    CREATE TABLE jobs_new (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        type TEXT NOT NULL CHECK (type IN ('ingest', 'proxy', 'thumb', 'sprite', 'export', 'hash_full', 'score', 'ml', 'batch_ingest', 'batch_export', 'relink_scan', 'rematch', 'reextract')),
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
    INSERT INTO jobs_new SELECT * FROM jobs;
    DROP TABLE jobs;
    ALTER TABLE jobs_new RENAME TO jobs;
    COMMIT;

    CREATE INDEX idx_jobs_status ON jobs(status);
    CREATE INDEX idx_jobs_type_status ON jobs(type, status);
    CREATE INDEX idx_jobs_library ON jobs(library_id);
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
