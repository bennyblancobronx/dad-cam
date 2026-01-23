Dad Cam - Phase 7 Implementation Guide

Version: 1.0
Target Audience: Developers new to Rust/Tauri

---

Overview

Phase 7 enables production workflows for professionals. While earlier phases focused on consumer use cases (drag-drop ingest, auto-edit), Phase 7 adds the tools needed for rental houses, production companies, and power users managing large footage libraries across network storage.

When complete, you can:
- Ingest files by reference (NAS mode) without copying
- Queue multiple ingest sources and process them as a batch
- Queue multiple exports and render them sequentially
- Relink offline clips when drives reconnect or files move
- See which volume each clip originated from
- Customize export codecs beyond the basic presets

Prerequisites:
- Phases 1-6 completed and working
- Understanding of Phase 1's volume/fingerprint system
- Understanding of Phase 5/6's export system
- Familiarity with async Rust patterns

Done when: Your production workflow is faster than your current manual workflow.

---

Part 1: Database Extensions

1.1 Understanding the New Tables

Phase 7 adds three new concepts to the database:

- **Batch operations**: Group multiple ingest or export jobs together
- **Codec presets**: Custom encoding settings beyond share/archive
- **Relink sessions**: Track relink attempts and matches

1.2 Add Migration 5

Add this to `src-tauri/src/db/migrations.rs` in the MIGRATIONS array:

```rust
// Migration 5: Pro Mode (Phase 7)
r#"
-- Batch operations table
-- Groups multiple ingest or export jobs for unified progress tracking
CREATE TABLE batch_operations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL REFERENCES libraries(id),
    type TEXT NOT NULL CHECK (type IN ('ingest', 'export')),
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'running', 'paused', 'completed', 'failed', 'cancelled')),
    total_items INTEGER NOT NULL DEFAULT 0,
    completed_items INTEGER NOT NULL DEFAULT 0,
    failed_items INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT
);

-- Batch ingest sources
-- Each source path in a batch ingest operation
CREATE TABLE batch_ingest_sources (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id INTEGER NOT NULL REFERENCES batch_operations(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'scanning', 'ingesting', 'completed', 'failed', 'skipped')),
    files_found INTEGER DEFAULT 0,
    files_ingested INTEGER DEFAULT 0,
    job_id INTEGER REFERENCES jobs(id),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

-- Batch export items
-- Each export recipe in a batch export operation
CREATE TABLE batch_export_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id INTEGER NOT NULL REFERENCES batch_operations(id) ON DELETE CASCADE,
    recipe_id INTEGER NOT NULL REFERENCES export_recipes(id),
    run_id INTEGER REFERENCES export_runs(id),
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'queued', 'rendering', 'completed', 'failed', 'cancelled')),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

-- Custom codec presets
-- User-defined encoding settings beyond share/archive
CREATE TABLE codec_presets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    video_codec TEXT NOT NULL,
    video_params TEXT NOT NULL DEFAULT '{}',
    audio_codec TEXT NOT NULL DEFAULT 'aac',
    audio_params TEXT NOT NULL DEFAULT '{}',
    container TEXT NOT NULL DEFAULT 'mp4',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Insert system presets (mirrors existing share/archive but extensible)
INSERT INTO codec_presets (name, description, is_system, video_codec, video_params, audio_codec, audio_params, container)
VALUES
    ('share', 'H.264 for sharing (smaller files)', 1, 'libx264',
     '{"crf": 23, "preset": "medium", "profile": "high", "level": "4.1"}',
     'aac', '{"bitrate": "192k"}', 'mp4'),
    ('archive', 'ProRes 422 HQ for archival (highest quality)', 1, 'prores_ks',
     '{"profile": 3}',
     'pcm_s16le', '{}', 'mov'),
    ('web', 'H.264 optimized for web streaming', 1, 'libx264',
     '{"crf": 26, "preset": "slow", "profile": "main", "level": "3.1", "movflags": "+faststart"}',
     'aac', '{"bitrate": "128k"}', 'mp4'),
    ('master', 'ProRes 4444 for color grading', 1, 'prores_ks',
     '{"profile": 4}',
     'pcm_s24le', '{}', 'mov');

-- Relink sessions
-- Track relink operations for history and debugging
CREATE TABLE relink_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL REFERENCES libraries(id),
    scan_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'scanning'
        CHECK (status IN ('scanning', 'matching', 'completed', 'cancelled')),
    files_scanned INTEGER DEFAULT 0,
    matches_found INTEGER DEFAULT 0,
    matches_applied INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

-- Relink candidates
-- Potential matches found during a relink scan
CREATE TABLE relink_candidates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER NOT NULL REFERENCES relink_sessions(id) ON DELETE CASCADE,
    clip_id INTEGER NOT NULL REFERENCES clips(id),
    asset_id INTEGER NOT NULL REFERENCES assets(id),
    found_path TEXT NOT NULL,
    match_type TEXT NOT NULL CHECK (match_type IN ('full_hash', 'fast_hash', 'size_duration', 'filename')),
    confidence REAL NOT NULL DEFAULT 1.0,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'applied', 'rejected', 'skipped')),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Add reference mode tracking to libraries
ALTER TABLE libraries ADD COLUMN is_reference_mode INTEGER NOT NULL DEFAULT 0;

-- Add volume display name for UI
ALTER TABLE volumes ADD COLUMN display_name TEXT;
ALTER TABLE volumes ADD COLUMN mount_point TEXT;
ALTER TABLE volumes ADD COLUMN is_network INTEGER NOT NULL DEFAULT 0;

-- Indexes
CREATE INDEX idx_batch_operations_library ON batch_operations(library_id);
CREATE INDEX idx_batch_operations_status ON batch_operations(status);
CREATE INDEX idx_batch_ingest_sources_batch ON batch_ingest_sources(batch_id);
CREATE INDEX idx_batch_export_items_batch ON batch_export_items(batch_id);
CREATE INDEX idx_relink_sessions_library ON relink_sessions(library_id);
CREATE INDEX idx_relink_candidates_session ON relink_candidates(session_id);
CREATE INDEX idx_relink_candidates_clip ON relink_candidates(clip_id);
"#,
```

1.3 Schema Query Helpers

Add these to `src-tauri/src/db/schema.rs`:

```rust
// ----- Batch Operations -----

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchOperation {
    pub id: i64,
    pub library_id: i64,
    pub batch_type: String,
    pub name: String,
    pub status: String,
    pub total_items: i64,
    pub completed_items: i64,
    pub failed_items: i64,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

pub fn create_batch_operation(
    conn: &Connection,
    library_id: i64,
    batch_type: &str,
    name: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO batch_operations (library_id, type, name) VALUES (?1, ?2, ?3)",
        params![library_id, batch_type, name],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_batch_operation(conn: &Connection, id: i64) -> Result<Option<BatchOperation>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, library_id, type, name, status, total_items,
                  completed_items, failed_items, created_at, started_at, completed_at
           FROM batch_operations WHERE id = ?1"#
    )?;

    let result = stmt.query_row(params![id], |row| {
        Ok(BatchOperation {
            id: row.get(0)?,
            library_id: row.get(1)?,
            batch_type: row.get(2)?,
            name: row.get(3)?,
            status: row.get(4)?,
            total_items: row.get(5)?,
            completed_items: row.get(6)?,
            failed_items: row.get(7)?,
            created_at: row.get(8)?,
            started_at: row.get(9)?,
            completed_at: row.get(10)?,
        })
    });

    match result {
        Ok(batch) => Ok(Some(batch)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn update_batch_operation_status(
    conn: &Connection,
    id: i64,
    status: &str,
) -> Result<()> {
    let now = if status == "running" {
        "started_at = datetime('now'),"
    } else if status == "completed" || status == "failed" || status == "cancelled" {
        "completed_at = datetime('now'),"
    } else {
        ""
    };

    conn.execute(
        &format!(
            "UPDATE batch_operations SET status = ?1, {} updated_at = datetime('now') WHERE id = ?2",
            now
        ).replace(", updated_at", ""),
        params![status, id],
    )?;
    Ok(())
}

pub fn update_batch_operation_progress(
    conn: &Connection,
    id: i64,
    completed_items: i64,
    failed_items: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE batch_operations SET completed_items = ?1, failed_items = ?2 WHERE id = ?3",
        params![completed_items, failed_items, id],
    )?;
    Ok(())
}

// ----- Batch Ingest Sources -----

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchIngestSource {
    pub id: i64,
    pub batch_id: i64,
    pub source_path: String,
    pub status: String,
    pub files_found: i64,
    pub files_ingested: i64,
    pub job_id: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub fn add_batch_ingest_source(
    conn: &Connection,
    batch_id: i64,
    source_path: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO batch_ingest_sources (batch_id, source_path) VALUES (?1, ?2)",
        params![batch_id, source_path],
    )?;

    // Update total count
    conn.execute(
        "UPDATE batch_operations SET total_items = total_items + 1 WHERE id = ?1",
        params![batch_id],
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn get_pending_batch_ingest_sources(
    conn: &Connection,
    batch_id: i64,
) -> Result<Vec<BatchIngestSource>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, batch_id, source_path, status, files_found, files_ingested,
                  job_id, error_message, created_at, completed_at
           FROM batch_ingest_sources
           WHERE batch_id = ?1 AND status = 'pending'
           ORDER BY id"#
    )?;

    let sources = stmt.query_map(params![batch_id], |row| {
        Ok(BatchIngestSource {
            id: row.get(0)?,
            batch_id: row.get(1)?,
            source_path: row.get(2)?,
            status: row.get(3)?,
            files_found: row.get(4)?,
            files_ingested: row.get(5)?,
            job_id: row.get(6)?,
            error_message: row.get(7)?,
            created_at: row.get(8)?,
            completed_at: row.get(9)?,
        })
    })?;

    sources.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn update_batch_ingest_source(
    conn: &Connection,
    id: i64,
    status: &str,
    files_found: Option<i64>,
    files_ingested: Option<i64>,
    job_id: Option<i64>,
    error: Option<&str>,
) -> Result<()> {
    let completed = if status == "completed" || status == "failed" || status == "skipped" {
        ", completed_at = datetime('now')"
    } else {
        ""
    };

    conn.execute(
        &format!(
            r#"UPDATE batch_ingest_sources SET
               status = ?1,
               files_found = COALESCE(?2, files_found),
               files_ingested = COALESCE(?3, files_ingested),
               job_id = COALESCE(?4, job_id),
               error_message = ?5
               {}
               WHERE id = ?6"#,
            completed
        ),
        params![status, files_found, files_ingested, job_id, error, id],
    )?;
    Ok(())
}

// ----- Codec Presets -----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecPreset {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub video_codec: String,
    pub video_params: String,
    pub audio_codec: String,
    pub audio_params: String,
    pub container: String,
    pub created_at: String,
}

pub fn get_codec_preset(conn: &Connection, name: &str) -> Result<Option<CodecPreset>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, name, description, is_system, video_codec, video_params,
                  audio_codec, audio_params, container, created_at
           FROM codec_presets WHERE name = ?1"#
    )?;

    let result = stmt.query_row(params![name], |row| {
        Ok(CodecPreset {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            is_system: row.get(3)?,
            video_codec: row.get(4)?,
            video_params: row.get(5)?,
            audio_codec: row.get(6)?,
            audio_params: row.get(7)?,
            container: row.get(8)?,
            created_at: row.get(9)?,
        })
    });

    match result {
        Ok(preset) => Ok(Some(preset)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn list_codec_presets(conn: &Connection) -> Result<Vec<CodecPreset>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, name, description, is_system, video_codec, video_params,
                  audio_codec, audio_params, container, created_at
           FROM codec_presets ORDER BY is_system DESC, name ASC"#
    )?;

    let presets = stmt.query_map([], |row| {
        Ok(CodecPreset {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            is_system: row.get(3)?,
            video_codec: row.get(4)?,
            video_params: row.get(5)?,
            audio_codec: row.get(6)?,
            audio_params: row.get(7)?,
            container: row.get(8)?,
            created_at: row.get(9)?,
        })
    })?;

    presets.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn create_codec_preset(
    conn: &Connection,
    name: &str,
    description: Option<&str>,
    video_codec: &str,
    video_params: &str,
    audio_codec: &str,
    audio_params: &str,
    container: &str,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO codec_presets
           (name, description, video_codec, video_params, audio_codec, audio_params, container)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"#,
        params![name, description, video_codec, video_params, audio_codec, audio_params, container],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete_codec_preset(conn: &Connection, name: &str) -> Result<bool> {
    let rows = conn.execute(
        "DELETE FROM codec_presets WHERE name = ?1 AND is_system = 0",
        params![name],
    )?;
    Ok(rows > 0)
}

// ----- Relink Sessions -----

#[derive(Debug, Serialize, Deserialize)]
pub struct RelinkSession {
    pub id: i64,
    pub library_id: i64,
    pub scan_path: String,
    pub status: String,
    pub files_scanned: i64,
    pub matches_found: i64,
    pub matches_applied: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub fn create_relink_session(
    conn: &Connection,
    library_id: i64,
    scan_path: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO relink_sessions (library_id, scan_path) VALUES (?1, ?2)",
        params![library_id, scan_path],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_relink_session(
    conn: &Connection,
    id: i64,
    status: &str,
    files_scanned: Option<i64>,
    matches_found: Option<i64>,
    matches_applied: Option<i64>,
) -> Result<()> {
    let completed = if status == "completed" || status == "cancelled" {
        ", completed_at = datetime('now')"
    } else {
        ""
    };

    conn.execute(
        &format!(
            r#"UPDATE relink_sessions SET
               status = ?1,
               files_scanned = COALESCE(?2, files_scanned),
               matches_found = COALESCE(?3, matches_found),
               matches_applied = COALESCE(?4, matches_applied)
               {}
               WHERE id = ?5"#,
            completed
        ),
        params![status, files_scanned, matches_found, matches_applied, id],
    )?;
    Ok(())
}

// ----- Relink Candidates -----

#[derive(Debug, Serialize, Deserialize)]
pub struct RelinkCandidate {
    pub id: i64,
    pub session_id: i64,
    pub clip_id: i64,
    pub asset_id: i64,
    pub found_path: String,
    pub match_type: String,
    pub confidence: f64,
    pub status: String,
    pub created_at: String,
}

pub fn create_relink_candidate(
    conn: &Connection,
    session_id: i64,
    clip_id: i64,
    asset_id: i64,
    found_path: &str,
    match_type: &str,
    confidence: f64,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO relink_candidates
           (session_id, clip_id, asset_id, found_path, match_type, confidence)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
        params![session_id, clip_id, asset_id, found_path, match_type, confidence],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_relink_candidates(
    conn: &Connection,
    session_id: i64,
) -> Result<Vec<RelinkCandidate>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, session_id, clip_id, asset_id, found_path, match_type,
                  confidence, status, created_at
           FROM relink_candidates
           WHERE session_id = ?1
           ORDER BY confidence DESC, id"#
    )?;

    let candidates = stmt.query_map(params![session_id], |row| {
        Ok(RelinkCandidate {
            id: row.get(0)?,
            session_id: row.get(1)?,
            clip_id: row.get(2)?,
            asset_id: row.get(3)?,
            found_path: row.get(4)?,
            match_type: row.get(5)?,
            confidence: row.get(6)?,
            status: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?;

    candidates.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn apply_relink_candidate(
    conn: &Connection,
    candidate_id: i64,
) -> Result<()> {
    // Get the candidate details
    let (asset_id, found_path): (i64, String) = conn.query_row(
        "SELECT asset_id, found_path FROM relink_candidates WHERE id = ?1",
        params![candidate_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    // Update the asset's source_uri to the new path
    conn.execute(
        "UPDATE assets SET source_uri = ?1, verified_at = datetime('now') WHERE id = ?2",
        params![found_path, asset_id],
    )?;

    // Mark candidate as applied
    conn.execute(
        "UPDATE relink_candidates SET status = 'applied' WHERE id = ?1",
        params![candidate_id],
    )?;

    Ok(())
}

// ----- Volume Extensions -----

pub fn update_volume_info(
    conn: &Connection,
    volume_id: i64,
    display_name: Option<&str>,
    mount_point: Option<&str>,
    is_network: bool,
) -> Result<()> {
    conn.execute(
        r#"UPDATE volumes SET
           display_name = COALESCE(?1, display_name),
           mount_point = COALESCE(?2, mount_point),
           is_network = ?3,
           last_seen_at = datetime('now')
           WHERE id = ?4"#,
        params![display_name, mount_point, is_network, volume_id],
    )?;
    Ok(())
}

pub fn get_clip_volume_info(
    conn: &Connection,
    clip_id: i64,
) -> Result<Option<(i64, Option<String>, Option<String>, bool)>> {
    let result = conn.query_row(
        r#"SELECT v.id, v.display_name, v.mount_point, v.is_network
           FROM volumes v
           JOIN asset_volumes av ON v.id = av.volume_id
           JOIN clips c ON c.original_asset_id = av.asset_id
           WHERE c.id = ?1
           LIMIT 1"#,
        params![clip_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    );

    match result {
        Ok(info) => Ok(Some(info)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ----- Offline Clips -----

/// Get clips whose original asset file is missing
pub fn get_offline_clips(conn: &Connection, library_id: i64) -> Result<Vec<Clip>> {
    let mut stmt = conn.prepare(
        r#"SELECT c.id, c.library_id, c.original_asset_id, c.camera_profile_id, c.media_type,
                  c.title, c.duration_ms, c.width, c.height, c.fps, c.codec, c.recorded_at,
                  c.recorded_at_offset_minutes, c.recorded_at_is_estimated, c.timestamp_source,
                  c.created_at
           FROM clips c
           JOIN assets a ON c.original_asset_id = a.id
           WHERE c.library_id = ?1
           AND a.verified_at IS NULL
           ORDER BY c.recorded_at DESC"#
    )?;

    let clips = stmt.query_map(params![library_id], |row| {
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
```

---

Part 2: Reference Mode Module

2.1 Understanding Reference Mode

Reference mode (also called "NAS mode") is for professionals who:
- Keep footage on network storage (NAS, SAN)
- Cannot or do not want to copy files into the library
- Need to manage footage across multiple volumes

In reference mode:
- Files stay where they are (no copy to `originals/`)
In reference mode, Dad Cam often cannot write to the same location as the originals (read-only shares, restricted NAS permissions).
To keep the “no cloud, portable library” contract while staying writable:

- `library_root` still points at the originals’ root (or an arbitrary “library anchor” folder).
- `cache_root` is where `.dadcam/` lives (DB + derived assets).
  - Default: `{library_root}/.dadcam`
  - Override: a user-chosen local folder (e.g., `~/DadCamCaches/MyLibrary/.dadcam`)

Implementation detail:
- Store `cache_root` in `libraries.settings` JSON (no schema change required).
- All derived outputs resolve relative to `cache_root` (not the originals’ share).


- The asset's `path` field is empty
- The asset's `source_uri` field contains the original location
- Volume tracking becomes critical for relinking

2.2 Create Reference Mode Module

Create `src-tauri/src/reference/mod.rs`:

```rust
use std::path::Path;
use anyhow::{Result, anyhow};
use crate::db::schema;
use crate::hash;
use crate::ingest::discover;
use rusqlite::Connection;

/// Check if a path is on network storage
pub fn is_network_path(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        // Check if path starts with /Volumes and is a network mount
        if let Some(path_str) = path.to_str() {
            if path_str.starts_with("/Volumes/") {
                // Use diskutil to check if it's a network volume
                let output = std::process::Command::new("diskutil")
                    .args(["info", path_str.split('/').take(3).collect::<Vec<_>>().join("/").as_str()])
                    .output();

                if let Ok(output) = output {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    return stdout.contains("Network") || stdout.contains("afpfs") ||
                           stdout.contains("smbfs") || stdout.contains("nfs");
                }
            }
        }
        false
    }

    #[cfg(target_os = "windows")]
    {
        // UNC paths are network paths
        if let Some(path_str) = path.to_str() {
            return path_str.starts_with("\\\\") || path_str.starts_with("//");
        }
        false
    }

    #[cfg(target_os = "linux")]
    {
        // Check /proc/mounts for the mount type
        if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
            if let Some(path_str) = path.to_str() {
                for line in mounts.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let mount_point = parts[1];
                        let fs_type = parts[2];
                        if path_str.starts_with(mount_point) {
                            return fs_type == "nfs" || fs_type == "nfs4" ||
                                   fs_type == "cifs" || fs_type == "smb";
                        }
                    }
                }
            }
        }
        false
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        false
    }
}

/// Validate that reference mode is appropriate for the source path
pub fn validate_reference_source(source_path: &Path) -> Result<ReferenceValidation> {
    if !source_path.exists() {
        return Err(anyhow!("Source path does not exist: {}", source_path.display()));
    }

    let is_network = is_network_path(source_path);
    let (serial, label) = discover::get_volume_info(source_path);

    // Warn if using reference mode on removable media
    let is_removable = !is_network && serial.is_none();

    Ok(ReferenceValidation {
        is_network,
        is_removable,
        volume_serial: serial,
        volume_label: label,
        warning: if is_removable {
            Some("Source appears to be removable media. Reference mode is recommended for network storage.".to_string())
        } else {
            None
        },
    })
}

#[derive(Debug)]
pub struct ReferenceValidation {
    pub is_network: bool,
    pub is_removable: bool,
    pub volume_serial: Option<String>,
    pub volume_label: Option<String>,
    pub warning: Option<String>,
}

/// Create a referenced asset (does not copy the file)
pub fn create_referenced_asset(
    conn: &Connection,
    library_id: i64,
    source_path: &Path,
) -> Result<i64> {
    // Get file info
    let metadata = std::fs::metadata(source_path)?;
    let file_size = metadata.len() as i64;

    // Compute fast hash
    let (hash_fast, hash_fast_scheme) = hash::compute_fast_hash(source_path)?;

    // Check for duplicates (reference mode is stricter than copy mode)
    //
    // Use: hash_fast + size_bytes + duration_ms when available.
    // This prevents extremely rare "same fast-hash + same size" collisions.
    let duration_ms: Option<i64> = crate::video::probe_duration_ms(source_path).ok();

    if let Some(existing) = schema::get_asset_by_reference_fingerprint(
        conn,
        library_id,
        &hash_fast,
        file_size,
        duration_ms,
    )? {
        return Err(anyhow!("File already exists in library as asset {}", existing.id));
    }

    // Get volume info
    let (vol_serial, vol_label) = discover::get_volume_info(source_path);
    let is_network = is_network_path(source_path);

    // Get or create volume record
    let volume_id = schema::get_or_create_volume(conn, vol_serial.as_deref(), vol_label.as_deref())?;

    // Update volume with additional info
    let mount_point = source_path.ancestors()
        .find(|p| p.parent().map(|pp| pp.to_str() == Some("/Volumes") || pp.to_str() == Some("/")).unwrap_or(false))
        .map(|p| p.to_string_lossy().to_string());

    schema::update_volume_info(
        conn,
        volume_id,
        vol_label.as_deref(),
        mount_point.as_deref(),
        is_network,
    )?;

    // Create asset with empty path (reference mode)
    let source_uri = source_path.to_string_lossy().to_string();
    let asset_id = schema::create_asset(conn, &schema::NewAsset {
        library_id,
        asset_type: "original".to_string(),
        path: String::new(),  // Empty for reference mode
        source_uri: Some(source_uri),
        size_bytes: file_size,
        hash_fast: Some(hash_fast),
        hash_fast_scheme: Some(hash_fast_scheme),
    })?;

    // Link asset to volume
    schema::link_asset_volume(conn, asset_id, volume_id)?;

    Ok(asset_id)
}

/// Check if a referenced file is currently accessible
pub fn check_reference_accessible(source_uri: &str) -> bool {
    Path::new(source_uri).exists()
}

/// Get the effective path for an asset (works for both copy and reference mode)
pub fn get_asset_effective_path(
    library_root: &Path,
    asset_path: &str,
    source_uri: Option<&str>,
) -> Option<std::path::PathBuf> {
    // If path is set, it's a copied file
    if !asset_path.is_empty() {
        let full_path = library_root.join(asset_path);
        if full_path.exists() {
            return Some(full_path);
        }
    }

    // If source_uri is set, it's a referenced file
    if let Some(uri) = source_uri {
        let path = Path::new(uri);
        if path.exists() {
            return Some(path.to_path_buf());
        }
    }

    None
}
```

2.3 Register All Phase 7 Modules

In `src-tauri/src/lib.rs`, add all Phase 7 modules:

```rust
// Phase 7: Pro Mode modules
pub mod batch;      // Batch ingest and export operations
pub mod codec;      // Custom codec presets
pub mod reference;  // Reference mode (NAS workflow)
pub mod relink;     // Offline clip relinking
```

Directory structure after Phase 7:
```
src-tauri/src/
  batch/
    mod.rs          # Batch progress, shared types
    ingest.rs       # Batch ingest operations
    export.rs       # Batch export operations
  codec/
    mod.rs          # Codec presets, FFmpeg arg builder
  reference/
    mod.rs          # Reference mode, network detection
  relink/
    mod.rs          # Relink scanning, matching, applying
```

---

Part 3: Batch Operations Module

3.1 Understanding Batch Operations

Batch operations allow users to:
- Queue multiple ingest sources (folders, SD cards)
- Queue multiple export recipes
- Monitor aggregate progress
- Pause/resume/cancel the entire batch

3.2 Create Batch Module

Create `src-tauri/src/batch/mod.rs`:

```rust
pub mod ingest;
pub mod export;

use crate::db::schema::{BatchOperation, BatchIngestSource};
use rusqlite::Connection;
use anyhow::Result;

/// Get overall progress for a batch operation
pub fn get_batch_progress(conn: &Connection, batch_id: i64) -> Result<BatchProgress> {
    let batch = schema::get_batch_operation(conn, batch_id)?
        .ok_or_else(|| anyhow::anyhow!("Batch not found"))?;

    let progress_percent = if batch.total_items > 0 {
        ((batch.completed_items as f64 / batch.total_items as f64) * 100.0) as i32
    } else {
        0
    };

    Ok(BatchProgress {
        batch_id,
        status: batch.status,
        total_items: batch.total_items,
        completed_items: batch.completed_items,
        failed_items: batch.failed_items,
        progress_percent,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct BatchProgress {
    pub batch_id: i64,
    pub status: String,
    pub total_items: i64,
    pub completed_items: i64,
    pub failed_items: i64,
    pub progress_percent: i32,
}

use crate::db::schema;
```

Create `src-tauri/src/batch/ingest.rs`:

```rust
use std::path::Path;
use anyhow::Result;
use rusqlite::Connection;
use crate::db::schema;
use crate::ingest::discover;
use crate::jobs::runner;

/// Create a batch ingest operation from multiple source paths
pub fn create_batch_ingest(
    conn: &Connection,
    library_id: i64,
    name: &str,
    source_paths: &[&Path],
) -> Result<i64> {
    // Create the batch operation
    let batch_id = schema::create_batch_operation(conn, library_id, "ingest", name)?;

    // Add each source path
    for path in source_paths {
        let path_str = path.to_string_lossy().to_string();
        schema::add_batch_ingest_source(conn, batch_id, &path_str)?;
    }

    Ok(batch_id)
}

/// Start processing a batch ingest operation
pub fn start_batch_ingest(
    conn: &Connection,
    batch_id: i64,
    library_root: &Path,
    ingest_mode: &str,
) -> Result<()> {
    // Update batch status
    schema::update_batch_operation_status(conn, batch_id, "running")?;

    // Get pending sources
    let sources = schema::get_pending_batch_ingest_sources(conn, batch_id)?;

    let mut completed = 0;
    let mut failed = 0;

    for source in sources {
        let source_path = Path::new(&source.source_path);

        // Update source status to scanning
        schema::update_batch_ingest_source(
            conn, source.id, "scanning", None, None, None, None
        )?;

        // Discover files
        match discover::discover_media_files(source_path) {
            Ok(files) => {
                let files_found = files.len() as i64;
                schema::update_batch_ingest_source(
                    conn, source.id, "ingesting", Some(files_found), None, None, None
                )?;

                // Create ingest job
                let payload = serde_json::json!({
                    "source_path": source.source_path,
                    "ingest_mode": ingest_mode,
                });

                let job_id = schema::create_job(conn, &schema::NewJob {
                    job_type: "ingest".to_string(),
                    library_id: Some(schema::get_batch_operation(conn, batch_id)?.unwrap().library_id),
                    clip_id: None,
                    asset_id: None,
                    priority: 0,
                    payload: payload.to_string(),
                })?;

                schema::update_batch_ingest_source(
                    conn, source.id, "ingesting", None, None, Some(job_id), None
                )?;

                // Process the job (in a real app, this would be async)
                match runner::process_ingest_job(conn, job_id, library_root) {
                    Ok(_) => {
                        schema::update_batch_ingest_source(
                            conn, source.id, "completed", None, Some(files_found), None, None
                        )?;
                        completed += 1;
                    }
                    Err(e) => {
                        schema::update_batch_ingest_source(
                            conn, source.id, "failed", None, None, None, Some(&e.to_string())
                        )?;
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                schema::update_batch_ingest_source(
                    conn, source.id, "failed", None, None, None, Some(&e.to_string())
                )?;
                failed += 1;
            }
        }

        // Update batch progress
        schema::update_batch_operation_progress(conn, batch_id, completed, failed)?;
    }

    // Mark batch as completed
    let final_status = if failed > 0 && completed == 0 {
        "failed"
    } else {
        "completed"
    };
    schema::update_batch_operation_status(conn, batch_id, final_status)?;

    Ok(())
}
```

Create `src-tauri/src/batch/export.rs`:

```rust
use anyhow::Result;
use rusqlite::Connection;
use crate::db::schema;

/// Create a batch export operation from multiple recipe IDs
pub fn create_batch_export(
    conn: &Connection,
    library_id: i64,
    name: &str,
    recipe_ids: &[i64],
) -> Result<i64> {
    // Create the batch operation
    let batch_id = schema::create_batch_operation(conn, library_id, "export", name)?;

    // Add each recipe
    for recipe_id in recipe_ids {
        conn.execute(
            "INSERT INTO batch_export_items (batch_id, recipe_id) VALUES (?1, ?2)",
            rusqlite::params![batch_id, recipe_id],
        )?;

        // Update total count
        conn.execute(
            "UPDATE batch_operations SET total_items = total_items + 1 WHERE id = ?1",
            rusqlite::params![batch_id],
        )?;
    }

    Ok(batch_id)
}

/// Get pending export items for a batch
pub fn get_pending_batch_exports(
    conn: &Connection,
    batch_id: i64,
) -> Result<Vec<BatchExportItem>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, batch_id, recipe_id, run_id, status, error_message, created_at, completed_at
           FROM batch_export_items
           WHERE batch_id = ?1 AND status = 'pending'
           ORDER BY id"#
    )?;

    let items = stmt.query_map(rusqlite::params![batch_id], |row| {
        Ok(BatchExportItem {
            id: row.get(0)?,
            batch_id: row.get(1)?,
            recipe_id: row.get(2)?,
            run_id: row.get(3)?,
            status: row.get(4)?,
            error_message: row.get(5)?,
            created_at: row.get(6)?,
            completed_at: row.get(7)?,
        })
    })?;

    items.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BatchExportItem {
    pub id: i64,
    pub batch_id: i64,
    pub recipe_id: i64,
    pub run_id: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Start processing a batch export operation
pub fn start_batch_export(
    conn: &Connection,
    batch_id: i64,
    library_root: &std::path::Path,
) -> Result<()> {
    use crate::export::{selector, assembler, renderer};

    // Update batch status
    schema::update_batch_operation_status(conn, batch_id, "running")?;

    // Get pending items
    let items = get_pending_batch_exports(conn, batch_id)?;

    let mut completed: i64 = 0;
    let mut failed: i64 = 0;

    for item in items {
        // Update item status
        conn.execute(
            "UPDATE batch_export_items SET status = 'rendering' WHERE id = ?1",
            rusqlite::params![item.id],
        )?;

        // Get recipe
        let recipe = match schema::get_export_recipe(conn, item.recipe_id)? {
            Some(r) => r,
            None => {
                conn.execute(
                    "UPDATE batch_export_items SET status = 'failed', error_message = 'Recipe not found' WHERE id = ?1",
                    rusqlite::params![item.id],
                )?;
                failed += 1;
                continue;
            }
        };

        // Create export run
        let recipe_snapshot = serde_json::to_string(&recipe)?;
        let run_id = schema::create_export_run(
            conn,
            item.recipe_id,
            recipe.library_id,
            &format!("Batch export - {}", recipe.name),
            &recipe_snapshot,
        )?;

        // Link run to batch item
        conn.execute(
            "UPDATE batch_export_items SET run_id = ?1 WHERE id = ?2",
            rusqlite::params![run_id, item.id],
        )?;

        // Select clips
        let clips = selector::select_clips(conn, &recipe)?;

        if clips.is_empty() {
            conn.execute(
                "UPDATE batch_export_items SET status = 'failed', error_message = 'No clips match selection criteria' WHERE id = ?1",
                rusqlite::params![item.id],
            )?;
            failed += 1;
            continue;
        }

        // Build export plan
        let plan = match assembler::build_export_plan(&clips, &recipe, library_root) {
            Ok(p) => p,
            Err(e) => {
                conn.execute(
                    "UPDATE batch_export_items SET status = 'failed', error_message = ?1, completed_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![e.to_string(), item.id],
                )?;
                failed += 1;
                continue;
            }
        };

        // Render
        match renderer::render_export(conn, &plan, &recipe, run_id, library_root, None) {
            Ok(_) => {
                conn.execute(
                    "UPDATE batch_export_items SET status = 'completed', completed_at = datetime('now') WHERE id = ?1",
                    rusqlite::params![item.id],
                )?;
                completed += 1;
            }
            Err(e) => {
                conn.execute(
                    "UPDATE batch_export_items SET status = 'failed', error_message = ?1, completed_at = datetime('now') WHERE id = ?2",
                    rusqlite::params![e.to_string(), item.id],
                )?;
                failed += 1;
            }
        }

        // Update batch progress
        schema::update_batch_operation_progress(conn, batch_id, completed, failed)?;
    }

    // Mark batch as completed
    let final_status = if failed > 0 && completed == 0 {
        "failed"
    } else {
        "completed"
    };
    schema::update_batch_operation_status(conn, batch_id, final_status)?;

    Ok(())
}
```

---

Part 4: Relinking Module

4.1 Understanding Relinking

Relinking reconnects clips to their original files when:
- A drive is reconnected after being offline
- Files are moved to a new location
- Files are restored from backup

Phase 1 created the foundations (fingerprints, volumes). Phase 7 builds the full workflow:
1. Scan a path for media files
2. Match files against offline clips using fingerprints
3. Present candidates to the user
4. Apply selected matches

4.2 Create Relink Module

Create `src-tauri/src/relink/mod.rs`:

```rust
use std::path::Path;
use anyhow::Result;
use rusqlite::Connection;
use crate::db::schema;
use crate::hash;
use crate::ingest::discover;
use crate::metadata::ffprobe;

/// Confidence levels for different match types
const CONFIDENCE_FULL_HASH: f64 = 1.0;
const CONFIDENCE_FAST_HASH: f64 = 0.95;
const CONFIDENCE_SIZE_DURATION: f64 = 0.8;
const CONFIDENCE_FILENAME: f64 = 0.5;

/// Start a relink scan session
pub fn start_relink_scan(
    conn: &Connection,
    library_id: i64,
    scan_path: &Path,
) -> Result<i64> {
    let session_id = schema::create_relink_session(
        conn,
        library_id,
        &scan_path.to_string_lossy(),
    )?;

    Ok(session_id)
}

/// Scan a path and find matches for offline clips
pub fn scan_for_matches(
    conn: &Connection,
    session_id: i64,
    library_id: i64,
    scan_path: &Path,
    progress_callback: Option<&dyn Fn(i64, i64)>,
) -> Result<ScanResult> {
    // Update session status
    schema::update_relink_session(conn, session_id, "scanning", None, None, None)?;

    // Discover media files in scan path
    let files = discover::discover_media_files(scan_path)?;
    let total_files = files.len() as i64;

    // Get offline clips (clips with unverified assets)
    let offline_clips = schema::get_offline_clips(conn, library_id)?;

    if offline_clips.is_empty() {
        schema::update_relink_session(
            conn, session_id, "completed", Some(total_files), Some(0), Some(0)
        )?;
        return Ok(ScanResult {
            session_id,
            files_scanned: total_files,
            matches_found: 0,
            offline_clips: 0,
        });
    }

    // Build lookup maps for fingerprints
    let mut fast_hash_map: std::collections::HashMap<String, Vec<(i64, i64)>> = std::collections::HashMap::new();
    let mut size_duration_map: std::collections::HashMap<String, Vec<(i64, i64)>> = std::collections::HashMap::new();
    let mut filename_map: std::collections::HashMap<String, Vec<(i64, i64)>> = std::collections::HashMap::new();

    for clip in &offline_clips {
        // Get asset for this clip
        let asset_result: Result<(Option<String>, Option<String>), _> = conn.query_row(
            "SELECT hash_fast, source_uri FROM assets WHERE id = ?1",
            rusqlite::params![clip.original_asset_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        if let Ok((hash_fast, source_uri)) = asset_result {
            // Add to fast hash map
            if let Some(ref hash) = hash_fast {
                fast_hash_map.entry(hash.clone())
                    .or_insert_with(Vec::new)
                    .push((clip.id, clip.original_asset_id));
            }

            // Add to size_duration map
            if let Some(duration_ms) = clip.duration_ms {
                let asset_size: Result<i64, _> = conn.query_row(
                    "SELECT size_bytes FROM assets WHERE id = ?1",
                    rusqlite::params![clip.original_asset_id],
                    |row| row.get(0),
                );
                if let Ok(size) = asset_size {
                    let fp = format!("{}_{}", size, duration_ms);
                    size_duration_map.entry(fp)
                        .or_insert_with(Vec::new)
                        .push((clip.id, clip.original_asset_id));
                }
            }

            // Add to filename map
            if let Some(ref uri) = source_uri {
                if let Some(filename) = Path::new(uri).file_name() {
                    let name = filename.to_string_lossy().to_lowercase();
                    filename_map.entry(name)
                        .or_insert_with(Vec::new)
                        .push((clip.id, clip.original_asset_id));
                }
            }
        }
    }

    // Update session status to matching
    schema::update_relink_session(conn, session_id, "matching", Some(total_files), None, None)?;

    let mut matches_found = 0;

    // Scan each file
    for (index, discovered) in files.iter().enumerate() {
        if let Some(callback) = progress_callback {
            callback(index as i64 + 1, total_files);
        }

        // Try fast hash match first (most reliable)
        if let Ok((hash_fast, _)) = hash::compute_fast_hash(&discovered.path) {
            if let Some(clips) = fast_hash_map.get(&hash_fast) {
                for (clip_id, asset_id) in clips {
                    schema::create_relink_candidate(
                        conn,
                        session_id,
                        *clip_id,
                        *asset_id,
                        &discovered.path.to_string_lossy(),
                        "fast_hash",
                        CONFIDENCE_FAST_HASH,
                    )?;
                    matches_found += 1;
                }
                continue; // Skip other match types if we found a hash match
            }
        }

        // Try size + duration match
        if let Ok(media_info) = ffprobe::probe(&discovered.path) {
            if let Some(duration_ms) = media_info.duration_ms {
                let file_size = std::fs::metadata(&discovered.path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);
                let fp = format!("{}_{}", file_size, duration_ms);

                if let Some(clips) = size_duration_map.get(&fp) {
                    for (clip_id, asset_id) in clips {
                        schema::create_relink_candidate(
                            conn,
                            session_id,
                            *clip_id,
                            *asset_id,
                            &discovered.path.to_string_lossy(),
                            "size_duration",
                            CONFIDENCE_SIZE_DURATION,
                        )?;
                        matches_found += 1;
                    }
                    continue;
                }
            }
        }

        // Try filename match (lowest confidence)
        if let Some(filename) = discovered.path.file_name() {
            let name = filename.to_string_lossy().to_lowercase();
            if let Some(clips) = filename_map.get(&name) {
                for (clip_id, asset_id) in clips {
                    schema::create_relink_candidate(
                        conn,
                        session_id,
                        *clip_id,
                        *asset_id,
                        &discovered.path.to_string_lossy(),
                        "filename",
                        CONFIDENCE_FILENAME,
                    )?;
                    matches_found += 1;
                }
            }
        }
    }

    // Update session as completed
    schema::update_relink_session(
        conn, session_id, "completed",
        Some(total_files), Some(matches_found), Some(0)
    )?;

    Ok(ScanResult {
        session_id,
        files_scanned: total_files,
        matches_found,
        offline_clips: offline_clips.len() as i64,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct ScanResult {
    pub session_id: i64,
    pub files_scanned: i64,
    pub matches_found: i64,
    pub offline_clips: i64,
}

/// Apply a single relink candidate
pub fn apply_relink(conn: &Connection, candidate_id: i64) -> Result<()> {
    schema::apply_relink_candidate(conn, candidate_id)?;

    // Update the session's applied count
    let session_id: i64 = conn.query_row(
        "SELECT session_id FROM relink_candidates WHERE id = ?1",
        rusqlite::params![candidate_id],
        |row| row.get(0),
    )?;

    conn.execute(
        "UPDATE relink_sessions SET matches_applied = matches_applied + 1 WHERE id = ?1",
        rusqlite::params![session_id],
    )?;

    Ok(())
}

/// Apply all high-confidence candidates automatically
pub fn apply_all_high_confidence(
    conn: &Connection,
    session_id: i64,
    min_confidence: f64,
) -> Result<i64> {
    let candidates = schema::get_relink_candidates(conn, session_id)?;
    let mut applied = 0;

    for candidate in candidates {
        if candidate.status == "pending" && candidate.confidence >= min_confidence {
            apply_relink(conn, candidate.id)?;
            applied += 1;
        }
    }

    Ok(applied)
}

/// Reject a relink candidate
pub fn reject_relink(conn: &Connection, candidate_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE relink_candidates SET status = 'rejected' WHERE id = ?1",
        rusqlite::params![candidate_id],
    )?;
    Ok(())
}
```

---

Part 5: Codec Presets Module

5.1 Understanding Codec Presets

Phase 5 had two hardcoded presets (share/archive). Phase 7 adds:
- Custom user-defined presets
- Full control over video/audio codec parameters
- Resolution and frame rate overrides

5.2 Create Codec Module

Create `src-tauri/src/codec/mod.rs`:

```rust
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use crate::db::schema::CodecPreset;

/// Video codec parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crf: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movflags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pix_fmt: Option<String>,
}

/// Audio codec parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<u32>,
}

impl Default for VideoParams {
    fn default() -> Self {
        Self {
            crf: Some(23),
            bitrate: None,
            preset: Some("medium".to_string()),
            profile: Some("high".to_string()),
            level: Some("4.1".to_string()),
            movflags: None,
            pix_fmt: None,
        }
    }
}

impl Default for AudioParams {
    fn default() -> Self {
        Self {
            bitrate: Some("192k".to_string()),
            sample_rate: None,
            channels: None,
        }
    }
}

/// Build FFmpeg arguments from a codec preset
pub fn build_ffmpeg_args(preset: &CodecPreset) -> Result<Vec<String>> {
    let mut args = Vec::new();

    // Parse video params
    let video_params: VideoParams = serde_json::from_str(&preset.video_params)
        .unwrap_or_default();

    // Video codec
    args.push("-c:v".to_string());
    args.push(preset.video_codec.clone());

    // Video params
    if let Some(crf) = video_params.crf {
        args.push("-crf".to_string());
        args.push(crf.to_string());
    }
    if let Some(ref bitrate) = video_params.bitrate {
        args.push("-b:v".to_string());
        args.push(bitrate.clone());
    }
    if let Some(ref preset_name) = video_params.preset {
        args.push("-preset".to_string());
        args.push(preset_name.clone());
    }
    if let Some(ref profile) = video_params.profile {
        // ProRes uses -profile:v with numeric values
        if preset.video_codec.starts_with("prores") {
            args.push("-profile:v".to_string());
            args.push(profile.clone());
        } else {
            args.push("-profile:v".to_string());
            args.push(profile.clone());
        }
    }
    if let Some(ref level) = video_params.level {
        args.push("-level".to_string());
        args.push(level.clone());
    }
    if let Some(ref pix_fmt) = video_params.pix_fmt {
        args.push("-pix_fmt".to_string());
        args.push(pix_fmt.clone());
    }
    if let Some(ref movflags) = video_params.movflags {
        args.push("-movflags".to_string());
        args.push(movflags.clone());
    }

    // Parse audio params
    let audio_params: AudioParams = serde_json::from_str(&preset.audio_params)
        .unwrap_or_default();

    // Audio codec
    args.push("-c:a".to_string());
    args.push(preset.audio_codec.clone());

    // Audio params
    if let Some(ref bitrate) = audio_params.bitrate {
        args.push("-b:a".to_string());
        args.push(bitrate.clone());
    }
    if let Some(sample_rate) = audio_params.sample_rate {
        args.push("-ar".to_string());
        args.push(sample_rate.to_string());
    }
    if let Some(channels) = audio_params.channels {
        args.push("-ac".to_string());
        args.push(channels.to_string());
    }

    Ok(args)
}

/// Get container extension for a preset
pub fn get_container_extension(preset: &CodecPreset) -> &str {
    match preset.container.as_str() {
        "mp4" => "mp4",
        "mov" => "mov",
        "mkv" => "mkv",
        "webm" => "webm",
        _ => "mp4",
    }
}

/// Validate that a codec preset is usable
pub fn validate_preset(preset: &CodecPreset) -> Result<()> {
    // Check video codec is known
    let valid_video_codecs = [
        "libx264", "libx265", "prores_ks", "dnxhd", "mjpeg", "vp9", "av1"
    ];
    if !valid_video_codecs.contains(&preset.video_codec.as_str()) {
        return Err(anyhow!("Unknown video codec: {}", preset.video_codec));
    }

    // Check audio codec is known
    let valid_audio_codecs = [
        "aac", "mp3", "pcm_s16le", "pcm_s24le", "flac", "opus", "vorbis"
    ];
    if !valid_audio_codecs.contains(&preset.audio_codec.as_str()) {
        return Err(anyhow!("Unknown audio codec: {}", preset.audio_codec));
    }

    // Check container is known
    let valid_containers = ["mp4", "mov", "mkv", "webm", "avi"];
    if !valid_containers.contains(&preset.container.as_str()) {
        return Err(anyhow!("Unknown container: {}", preset.container));
    }

    Ok(())
}
```

---

Part 6: CLI Commands

6.1 Add Phase 7 CLI Commands

Add these to your CLI command enum in `src-tauri/src/cli.rs`:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Create a library in reference mode (NAS workflow)
    InitReference {
        /// Path to library root
        #[arg(short, long)]
        path: PathBuf,

        /// Library name
        #[arg(short, long)]
        name: String,
    },

    /// Start a batch ingest from multiple sources
    BatchIngest {
        /// Source paths to ingest (can specify multiple)
        #[arg(required = true)]
        sources: Vec<PathBuf>,

        /// Library path
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Batch name
        #[arg(short, long, default_value = "Batch Ingest")]
        name: String,
    },

    /// Start a batch export from multiple recipes
    BatchExport {
        /// Recipe IDs to export (can specify multiple)
        #[arg(required = true)]
        recipes: Vec<i64>,

        /// Library path
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Batch name
        #[arg(short, long, default_value = "Batch Export")]
        name: String,
    },

    /// Scan a path and relink offline clips
    Relink {
        /// Path to scan for files
        scan_path: PathBuf,

        /// Library path
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Automatically apply high-confidence matches
        #[arg(short, long)]
        auto_apply: bool,

        /// Minimum confidence for auto-apply (0.0-1.0)
        #[arg(short, long, default_value = "0.95")]
        min_confidence: f64,
    },

    /// List codec presets
    ListPresets,

    /// Create a custom codec preset
    CreatePreset {
        /// Preset name
        name: String,

        /// Video codec (libx264, libx265, prores_ks, etc.)
        #[arg(short, long, default_value = "libx264")]
        video_codec: String,

        /// CRF value (0-51, lower = better quality)
        #[arg(short, long)]
        crf: Option<u32>,

        /// Video bitrate (e.g., "10M")
        #[arg(short, long)]
        bitrate: Option<String>,

        /// Audio codec (aac, pcm_s16le, etc.)
        #[arg(short, long, default_value = "aac")]
        audio_codec: String,

        /// Container format (mp4, mov, mkv)
        #[arg(long, default_value = "mp4")]
        container: String,

        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Show volume information for clips
    VolumeInfo {
        /// Library path
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Specific clip ID (optional)
        #[arg(short, long)]
        clip_id: Option<i64>,
    },

    /// List offline clips
    ListOffline {
        /// Library path
        #[arg(short, long)]
        library: Option<PathBuf>,
    },
}
```

6.2 Implement CLI Command Handlers

```rust
fn cmd_init_reference(path: &PathBuf, name: &str) -> Result<()> {
    use crate::reference;

    // Validate path
    let validation = reference::validate_reference_source(path)?;

    if let Some(warning) = validation.warning {
        println!("Warning: {}", warning);
    }

    // Create .dadcam folder
    let dadcam_folder = path.join(crate::constants::DADCAM_FOLDER);
    std::fs::create_dir_all(&dadcam_folder)?;

    // Initialize database
    let db_path = db::get_db_path(path);
    let conn = db::open_db(&db_path)?;

    // Create library with reference mode flag
    let root_path = path.canonicalize()?.to_string_lossy().to_string();
    conn.execute(
        "INSERT INTO libraries (root_path, name, ingest_mode, is_reference_mode) VALUES (?1, ?2, 'reference', 1)",
        rusqlite::params![root_path, name],
    )?;

    println!("Created reference-mode library: {}", name);
    println!("  Path: {}", root_path);
    println!("  Network storage: {}", if validation.is_network { "Yes" } else { "No" });
    if let Some(label) = validation.volume_label {
        println!("  Volume: {}", label);
    }

    Ok(())
}

fn cmd_batch_ingest(sources: &[PathBuf], library_root: &PathBuf, name: &str) -> Result<()> {
    use crate::batch::ingest;

    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    // Convert PathBuf refs to Path refs
    let source_refs: Vec<&Path> = sources.iter().map(|p| p.as_path()).collect();

    // Create batch
    let batch_id = ingest::create_batch_ingest(&conn, library.id, name, &source_refs)?;
    println!("Created batch ingest {} with {} sources", batch_id, sources.len());

    // Start processing
    println!("Starting batch ingest...");
    ingest::start_batch_ingest(&conn, batch_id, library_root, &library.ingest_mode)?;

    // Show results
    let batch = schema::get_batch_operation(&conn, batch_id)?.unwrap();
    println!("\nBatch ingest complete:");
    println!("  Total sources: {}", batch.total_items);
    println!("  Completed: {}", batch.completed_items);
    println!("  Failed: {}", batch.failed_items);

    Ok(())
}

fn cmd_relink(scan_path: &PathBuf, library_root: &PathBuf, auto_apply: bool, min_confidence: f64) -> Result<()> {
    use crate::relink;

    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    println!("Starting relink scan...");
    println!("  Scan path: {}", scan_path.display());

    // Start scan
    let session_id = relink::start_relink_scan(&conn, library.id, scan_path)?;

    // Scan with progress
    let result = relink::scan_for_matches(
        &conn,
        session_id,
        library.id,
        scan_path,
        Some(&|current, total| {
            print!("\rScanning: {}/{} files", current, total);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }),
    )?;
    println!(); // New line after progress

    println!("\nScan complete:");
    println!("  Files scanned: {}", result.files_scanned);
    println!("  Offline clips: {}", result.offline_clips);
    println!("  Matches found: {}", result.matches_found);

    if result.matches_found == 0 {
        return Ok(());
    }

    // Get candidates
    let candidates = schema::get_relink_candidates(&conn, session_id)?;

    println!("\nCandidates:");
    for candidate in &candidates {
        let confidence_pct = (candidate.confidence * 100.0) as i32;
        println!("  [{}%] Clip {} -> {} ({})",
            confidence_pct,
            candidate.clip_id,
            candidate.found_path,
            candidate.match_type,
        );
    }

    if auto_apply {
        let applied = relink::apply_all_high_confidence(&conn, session_id, min_confidence)?;
        println!("\nAuto-applied {} matches (confidence >= {}%)",
            applied,
            (min_confidence * 100.0) as i32
        );
    } else {
        println!("\nUse 'dadcam relink-apply <candidate_id>' to apply matches");
        println!("Or run with --auto-apply to apply high-confidence matches automatically");
    }

    Ok(())
}

fn cmd_list_presets() -> Result<()> {
    // For presets, we need a temporary connection
    // In a real app, this would use a shared config database
    let temp_db = std::env::temp_dir().join("dadcam_presets.db");
    let conn = db::open_db(&temp_db)?;

    let presets = schema::list_codec_presets(&conn)?;

    println!("Codec Presets:");
    println!("{:<15} {:<12} {:<12} {:<10} {}", "Name", "Video", "Audio", "Container", "Description");
    println!("{}", "-".repeat(70));

    for preset in presets {
        let system_marker = if preset.is_system { " [system]" } else { "" };
        println!("{:<15} {:<12} {:<12} {:<10} {}{}",
            preset.name,
            preset.video_codec,
            preset.audio_codec,
            preset.container,
            preset.description.unwrap_or_default(),
            system_marker,
        );
    }

    Ok(())
}

fn cmd_list_offline(library_root: &PathBuf) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    let offline_clips = schema::get_offline_clips(&conn, library.id)?;

    if offline_clips.is_empty() {
        println!("No offline clips found.");
        return Ok(());
    }

    println!("Offline Clips ({}):", offline_clips.len());
    println!("{:<8} {:<40} {:<15} {}", "ID", "Title", "Duration", "Last Known Path");
    println!("{}", "-".repeat(80));

    for clip in offline_clips {
        // Get asset source_uri
        let source_uri: Option<String> = conn.query_row(
            "SELECT source_uri FROM assets WHERE id = ?1",
            rusqlite::params![clip.original_asset_id],
            |row| row.get(0),
        ).ok().flatten();

        let duration = clip.duration_ms
            .map(|ms| format!("{}:{:02}", ms / 60000, (ms % 60000) / 1000))
            .unwrap_or_else(|| "?:??".to_string());

        let path = source_uri.unwrap_or_else(|| "Unknown".to_string());
        let truncated_path = if path.len() > 30 {
            format!("...{}", &path[path.len()-27..])
        } else {
            path
        };

        println!("{:<8} {:<40} {:<15} {}",
            clip.id,
            if clip.title.len() > 38 { format!("{}...", &clip.title[..35]) } else { clip.title },
            duration,
            truncated_path,
        );
    }

    Ok(())
}
```

---

Part 7: Tauri Commands

Add these commands to expose Phase 7 functionality to the UI.

In `src-tauri/src/lib.rs`:

```rust
// ----- Reference Mode -----

#[tauri::command]
async fn validate_reference_path(path: String) -> Result<ReferenceValidationResponse, String> {
    use crate::reference;

    let path = std::path::Path::new(&path);
    match reference::validate_reference_source(path) {
        Ok(validation) => Ok(ReferenceValidationResponse {
            is_network: validation.is_network,
            is_removable: validation.is_removable,
            volume_label: validation.volume_label,
            warning: validation.warning,
        }),
        Err(e) => Err(e.to_string()),
    }
}

#[derive(serde::Serialize)]
struct ReferenceValidationResponse {
    is_network: bool,
    is_removable: bool,
    volume_label: Option<String>,
    warning: Option<String>,
}

#[tauri::command]
async fn create_reference_library(
    state: tauri::State<'_, AppState>,
    path: String,
    name: String,
) -> Result<i64, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    // Create .dadcam folder
    let lib_path = std::path::Path::new(&path);
    let dadcam_folder = lib_path.join(crate::constants::DADCAM_FOLDER);
    std::fs::create_dir_all(&dadcam_folder).map_err(|e| e.to_string())?;

    // Create library with reference mode
    conn.execute(
        "INSERT INTO libraries (root_path, name, ingest_mode, is_reference_mode) VALUES (?1, ?2, 'reference', 1)",
        rusqlite::params![path, name],
    ).map_err(|e| e.to_string())?;

    Ok(conn.last_insert_rowid())
}

// ----- Batch Operations -----

#[tauri::command]
async fn create_batch_ingest(
    state: tauri::State<'_, AppState>,
    library_id: i64,
    name: String,
    source_paths: Vec<String>,
) -> Result<i64, String> {
    use crate::batch::ingest;

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let paths: Vec<std::path::PathBuf> = source_paths.iter().map(std::path::PathBuf::from).collect();
    let path_refs: Vec<&std::path::Path> = paths.iter().map(|p| p.as_path()).collect();

    ingest::create_batch_ingest(&conn, library_id, &name, &path_refs)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_batch_progress(
    state: tauri::State<'_, AppState>,
    batch_id: i64,
) -> Result<crate::batch::BatchProgress, String> {
    use crate::batch;

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    batch::get_batch_progress(&conn, batch_id).map_err(|e| e.to_string())
}

// ----- Relink -----

#[tauri::command]
async fn start_relink_scan(
    state: tauri::State<'_, AppState>,
    library_id: i64,
    scan_path: String,
) -> Result<crate::relink::ScanResult, String> {
    use crate::relink;

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let path = std::path::Path::new(&scan_path);

    let session_id = relink::start_relink_scan(&conn, library_id, path)
        .map_err(|e| e.to_string())?;

    relink::scan_for_matches(&conn, session_id, library_id, path, None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_relink_candidates(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> Result<Vec<schema::RelinkCandidate>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    schema::get_relink_candidates(&conn, session_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply_relink_candidate(
    state: tauri::State<'_, AppState>,
    candidate_id: i64,
) -> Result<(), String> {
    use crate::relink;

    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    relink::apply_relink(&conn, candidate_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_offline_clips(
    state: tauri::State<'_, AppState>,
    library_id: i64,
) -> Result<Vec<schema::Clip>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    schema::get_offline_clips(&conn, library_id).map_err(|e| e.to_string())
}

// ----- Codec Presets -----

#[tauri::command]
async fn list_codec_presets(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<schema::CodecPreset>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    schema::list_codec_presets(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_codec_preset(
    state: tauri::State<'_, AppState>,
    name: String,
    description: Option<String>,
    video_codec: String,
    video_params: String,
    audio_codec: String,
    audio_params: String,
    container: String,
) -> Result<i64, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    schema::create_codec_preset(
        &conn,
        &name,
        description.as_deref(),
        &video_codec,
        &video_params,
        &audio_codec,
        &audio_params,
        &container,
    ).map_err(|e| e.to_string())
}

// ----- Volume Info -----

#[tauri::command]
async fn get_clip_volume_info(
    state: tauri::State<'_, AppState>,
    clip_id: i64,
) -> Result<Option<VolumeInfoResponse>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    match schema::get_clip_volume_info(&conn, clip_id).map_err(|e| e.to_string())? {
        Some((id, display_name, mount_point, is_network)) => Ok(Some(VolumeInfoResponse {
            volume_id: id,
            display_name,
            mount_point,
            is_network,
        })),
        None => Ok(None),
    }
}

#[derive(serde::Serialize)]
struct VolumeInfoResponse {
    volume_id: i64,
    display_name: Option<String>,
    mount_point: Option<String>,
    is_network: bool,
}
```

Register commands in the Tauri builder:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    validate_reference_path,
    create_reference_library,
    create_batch_ingest,
    get_batch_progress,
    start_relink_scan,
    get_relink_candidates,
    apply_relink_candidate,
    get_offline_clips,
    list_codec_presets,
    create_codec_preset,
    get_clip_volume_info,
])
```

---

Part 8: UI Components (React/TypeScript)

8.1 Relink Panel Component

Create `src/components/RelinkPanel.tsx`:

```tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface RelinkCandidate {
  id: number;
  clip_id: number;
  found_path: string;
  match_type: string;
  confidence: number;
  status: string;
}

interface ScanResult {
  session_id: number;
  files_scanned: number;
  matches_found: number;
  offline_clips: number;
}

interface Props {
  libraryId: number;
  onComplete: () => void;
}

export function RelinkPanel({ libraryId, onComplete }: Props) {
  const [scanPath, setScanPath] = useState('');
  const [scanning, setScanning] = useState(false);
  const [result, setResult] = useState<ScanResult | null>(null);
  const [candidates, setCandidates] = useState<RelinkCandidate[]>([]);
  const [error, setError] = useState<string | null>(null);

  const startScan = async () => {
    if (!scanPath) return;

    setScanning(true);
    setError(null);

    try {
      const scanResult = await invoke<ScanResult>('start_relink_scan', {
        libraryId,
        scanPath,
      });
      setResult(scanResult);

      // Load candidates
      const candidateList = await invoke<RelinkCandidate[]>('get_relink_candidates', {
        sessionId: scanResult.session_id,
      });
      setCandidates(candidateList);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  const applyCandidate = async (candidateId: number) => {
    try {
      await invoke('apply_relink_candidate', { candidateId });
      // Update local state
      setCandidates(prev =>
        prev.map(c => c.id === candidateId ? { ...c, status: 'applied' } : c)
      );
    } catch (e) {
      setError(String(e));
    }
  };

  const applyAll = async () => {
    for (const candidate of candidates) {
      if (candidate.status === 'pending' && candidate.confidence >= 0.95) {
        await applyCandidate(candidate.id);
      }
    }
    onComplete();
  };

  const getConfidenceColor = (confidence: number) => {
    if (confidence >= 0.95) return 'text-green-600';
    if (confidence >= 0.8) return 'text-yellow-600';
    return 'text-red-600';
  };

  return (
    <div className="p-4">
      <h2 className="text-lg font-semibold mb-4">Relink Offline Clips</h2>

      {/* Scan Input */}
      <div className="flex gap-2 mb-4">
        <input
          type="text"
          value={scanPath}
          onChange={(e) => setScanPath(e.target.value)}
          placeholder="Path to scan for files..."
          className="flex-1 px-3 py-2 border rounded"
          disabled={scanning}
        />
        <button
          onClick={startScan}
          disabled={scanning || !scanPath}
          className="px-4 py-2 bg-blue-600 text-white rounded disabled:opacity-50"
        >
          {scanning ? 'Scanning...' : 'Scan'}
        </button>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-100 text-red-700 rounded">
          {error}
        </div>
      )}

      {/* Results */}
      {result && (
        <div className="mb-4 p-3 bg-gray-100 rounded">
          <p>Files scanned: {result.files_scanned}</p>
          <p>Offline clips: {result.offline_clips}</p>
          <p>Matches found: {result.matches_found}</p>
        </div>
      )}

      {/* Candidates */}
      {candidates.length > 0 && (
        <>
          <div className="flex justify-between items-center mb-2">
            <h3 className="font-medium">Candidates</h3>
            <button
              onClick={applyAll}
              className="px-3 py-1 bg-green-600 text-white text-sm rounded"
            >
              Apply All High Confidence
            </button>
          </div>

          <div className="space-y-2">
            {candidates.map((candidate) => (
              <div
                key={candidate.id}
                className={`p-3 border rounded flex justify-between items-center
                  ${candidate.status === 'applied' ? 'bg-green-50 border-green-200' : 'bg-white'}`}
              >
                <div>
                  <p className="font-medium">Clip {candidate.clip_id}</p>
                  <p className="text-sm text-gray-600 truncate max-w-md">
                    {candidate.found_path}
                  </p>
                  <p className="text-xs text-gray-500">
                    Match: {candidate.match_type} |{' '}
                    <span className={getConfidenceColor(candidate.confidence)}>
                      {Math.round(candidate.confidence * 100)}% confidence
                    </span>
                  </p>
                </div>

                {candidate.status === 'pending' && (
                  <button
                    onClick={() => applyCandidate(candidate.id)}
                    className="px-3 py-1 bg-blue-600 text-white text-sm rounded"
                  >
                    Apply
                  </button>
                )}
                {candidate.status === 'applied' && (
                  <span className="text-green-600 text-sm">Applied</span>
                )}
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
```

8.2 Batch Progress Component

Create `src/components/BatchProgress.tsx`:

```tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface BatchProgress {
  batch_id: number;
  status: string;
  total_items: number;
  completed_items: number;
  failed_items: number;
  progress_percent: number;
}

interface Props {
  batchId: number;
  onComplete: () => void;
}

export function BatchProgress({ batchId, onComplete }: Props) {
  const [progress, setProgress] = useState<BatchProgress | null>(null);

  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const p = await invoke<BatchProgress>('get_batch_progress', { batchId });
        setProgress(p);

        if (p.status === 'completed' || p.status === 'failed' || p.status === 'cancelled') {
          clearInterval(interval);
          onComplete();
        }
      } catch (e) {
        console.error('Failed to get batch progress:', e);
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [batchId, onComplete]);

  if (!progress) {
    return <div>Loading...</div>;
  }

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'completed': return 'text-green-600';
      case 'failed': return 'text-red-600';
      case 'running': return 'text-blue-600';
      default: return 'text-gray-600';
    }
  };

  return (
    <div className="p-4 border rounded">
      <div className="flex justify-between mb-2">
        <span className="font-medium">Batch Operation</span>
        <span className={getStatusColor(progress.status)}>
          {progress.status.charAt(0).toUpperCase() + progress.status.slice(1)}
        </span>
      </div>

      {/* Progress Bar */}
      <div className="w-full bg-gray-200 rounded-full h-4 mb-2">
        <div
          className="bg-blue-600 h-4 rounded-full transition-all"
          style={{ width: `${progress.progress_percent}%` }}
        />
      </div>

      <div className="flex justify-between text-sm text-gray-600">
        <span>{progress.completed_items} / {progress.total_items} completed</span>
        {progress.failed_items > 0 && (
          <span className="text-red-600">{progress.failed_items} failed</span>
        )}
      </div>
    </div>
  );
}
```

8.3 Volume Badge Component

Create `src/components/VolumeBadge.tsx`:

```tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface VolumeInfo {
  volume_id: number;
  display_name: string | null;
  mount_point: string | null;
  is_network: boolean;
}

interface Props {
  clipId: number;
}

export function VolumeBadge({ clipId }: Props) {
  const [volumeInfo, setVolumeInfo] = useState<VolumeInfo | null>(null);

  useEffect(() => {
    invoke<VolumeInfo | null>('get_clip_volume_info', { clipId })
      .then(setVolumeInfo)
      .catch(console.error);
  }, [clipId]);

  if (!volumeInfo) {
    return null;
  }

  const icon = volumeInfo.is_network ? 'network' : 'drive';
  const label = volumeInfo.display_name || volumeInfo.mount_point || 'Unknown Volume';

  return (
    <span
      className={`inline-flex items-center px-2 py-1 text-xs rounded
        ${volumeInfo.is_network ? 'bg-purple-100 text-purple-700' : 'bg-gray-100 text-gray-700'}`}
      title={volumeInfo.mount_point || undefined}
    >
      {icon === 'network' ? (
        <svg className="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
          <path d="M13 6a3 3 0 11-6 0 3 3 0 016 0zM18 8a2 2 0 11-4 0 2 2 0 014 0zM14 15a4 4 0 00-8 0v3h8v-3zM6 8a2 2 0 11-4 0 2 2 0 014 0zM16 18v-3a5.972 5.972 0 00-.75-2.906A3.005 3.005 0 0119 15v3h-3zM4.75 12.094A5.973 5.973 0 004 15v3H1v-3a3 3 0 013.75-2.906z" />
        </svg>
      ) : (
        <svg className="w-3 h-3 mr-1" fill="currentColor" viewBox="0 0 20 20">
          <path d="M2 6a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2V6zm14 1a1 1 0 10-2 0 1 1 0 002 0zm-6 0a1 1 0 10-2 0 1 1 0 002 0zM2 13a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2v-2zm14 1a1 1 0 10-2 0 1 1 0 002 0zm-6 0a1 1 0 10-2 0 1 1 0 002 0z" />
        </svg>
      )}
      {label}
    </span>
  );
}
```

---

Part 9: Testing Workflow

9.1 Manual Test Checklist

Reference Mode:
- [ ] `dadcam init-reference --path /Volumes/NAS/Footage --name "NAS Library"`
- [ ] Verify library created with `is_reference_mode = 1`
- [ ] Ingest a file and verify it stays in place (not copied)
- [ ] Verify `source_uri` is set, `path` is empty

Batch Ingest:
- [ ] `dadcam batch-ingest /path/to/sd1 /path/to/sd2 --library .`
- [ ] Verify batch operation created
- [ ] Verify all sources processed
- [ ] Verify progress tracking works

Batch Export:
- [ ] Create multiple export recipes
- [ ] `dadcam batch-export 1 2 3 --library .`
- [ ] Verify exports render sequentially
- [ ] Verify batch progress updates

Relinking:
- [ ] Create a library and ingest some files
- [ ] Move the original files to a new location
- [ ] Run `dadcam list-offline --library .` to see offline clips
- [ ] Run `dadcam relink /new/location --library .`
- [ ] Verify matches found
- [ ] Run with `--auto-apply` and verify clips reconnected

Codec Presets:
- [ ] `dadcam list-presets` shows system presets
- [ ] Create custom preset: `dadcam create-preset "4k-web" --video-codec libx264 --crf 20`
- [ ] Verify preset appears in list
- [ ] Use preset in export and verify FFmpeg args

Volume Info:
- [ ] `dadcam volume-info --library .` shows volume info
- [ ] Verify network volumes marked correctly
- [ ] Verify UI shows volume badges on clips

9.2 Integration Tests

Add to `src-tauri/tests/phase7_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_reference_mode_validation() {
        // Test that local paths are detected correctly
        let local_path = std::env::temp_dir();
        let validation = crate::reference::validate_reference_source(&local_path).unwrap();
        assert!(!validation.is_network);
    }

    #[test]
    fn test_batch_operation_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = crate::db::open_db(&db_path).unwrap();

        // Create library
        crate::db::schema::create_library(&conn, temp_dir.path().to_str().unwrap(), "Test", "copy").unwrap();

        // Create batch
        let batch_id = crate::db::schema::create_batch_operation(&conn, 1, "ingest", "Test Batch").unwrap();
        assert!(batch_id > 0);

        // Verify batch exists
        let batch = crate::db::schema::get_batch_operation(&conn, batch_id).unwrap();
        assert!(batch.is_some());
        assert_eq!(batch.unwrap().name, "Test Batch");
    }

    #[test]
    fn test_codec_preset_ffmpeg_args() {
        let preset = crate::db::schema::CodecPreset {
            id: 1,
            name: "test".to_string(),
            description: None,
            is_system: false,
            video_codec: "libx264".to_string(),
            video_params: r#"{"crf": 23, "preset": "medium"}"#.to_string(),
            audio_codec: "aac".to_string(),
            audio_params: r#"{"bitrate": "192k"}"#.to_string(),
            container: "mp4".to_string(),
            created_at: "".to_string(),
        };

        let args = crate::codec::build_ffmpeg_args(&preset).unwrap();
        assert!(args.contains(&"-c:v".to_string()));
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"-crf".to_string()));
        assert!(args.contains(&"23".to_string()));
    }

    #[test]
    fn test_relink_fingerprint_matching() {
        // Test that size_duration fingerprints match correctly
        let fp1 = format!("{}_{}", 1024000, 5000); // 1MB, 5 seconds
        let fp2 = format!("{}_{}", 1024000, 5000);
        assert_eq!(fp1, fp2);

        let fp3 = format!("{}_{}", 1024001, 5000); // Different size
        assert_ne!(fp1, fp3);
    }
}
```

---

Part 10: Verification Checklist

Before considering Phase 7 complete, verify:

Database:
- [ ] Migration 5 applies cleanly
- [ ] `batch_operations` table exists with correct schema
- [ ] `codec_presets` table has system presets
- [ ] `relink_sessions` and `relink_candidates` tables exist
- [ ] `libraries.is_reference_mode` column exists
- [ ] `volumes` table has new columns (display_name, mount_point, is_network)

Reference Mode:
- [ ] Can create reference-mode library
- [ ] Files are not copied during ingest
- [ ] source_uri is properly set
- [ ] Network paths are detected correctly
- [ ] Volume info is tracked

Batch Operations:
- [ ] Batch ingest processes multiple sources
- [ ] Batch export renders multiple recipes
- [ ] Progress tracking works
- [ ] Failed items are tracked separately
- [ ] Batch can be cancelled

Relinking:
- [ ] Offline clips are detected
- [ ] Scan finds matches using fingerprints
- [ ] Different match types have correct confidence
- [ ] Applying relink updates asset source_uri
- [ ] Auto-apply respects confidence threshold

Codec Presets:
- [ ] System presets are immutable
- [ ] Custom presets can be created/deleted
- [ ] FFmpeg args are generated correctly
- [ ] Export uses selected preset

UI:
- [ ] Relink panel shows candidates
- [ ] Batch progress updates in real-time
- [ ] Volume badges appear on clips
- [ ] Codec preset picker works in export

---

Deferred to Later Phases

- Full hash verification during relink (expensive, optional)
- Automatic relink when volumes mount
- Batch operation pause/resume
- Custom FFmpeg filter chains in presets
- Multi-library batch operations
- Relink conflict resolution (multiple candidates for same clip)
- Watch folders for automatic ingest


---

# Addendum: Phase 7 to 100% (v1.1 “Pro Mode That Actually Works”)

This addendum upgrades Phase 7 to **production-correct, internally consistent, and truth-in-spec**.
It implements every fix identified in the Phase 7 audit:
- Fix brittle migration/helper bugs (`updated_at`, `type` column)
- Correct mount/network detection for Reference Mode
- Fix duplicate detection (fast-hash-only is unsafe)
- Make batch operations truly async + cancellable (no “in a real app…” gaps)
- Make codec presets real (scale/fps overrides + export integration)
- Make relink trustworthy (verified_at lifecycle + conflict handling)
- Add minimal tests that prove correctness across core paths

This stays within Phase 7 scope: **Pro workflows** (NAS/reference, batch operations, relink, presets).

---

## 0) One Decision You MUST Lock In (Presets Storage)

Phase 7 must choose where `codec_presets` live:

### ✅ Recommended: Global presets (pro expectation)
Store presets in a **global config DB**, not per library:
- macOS: `~/Library/Application Support/dadcam/config.db`
- Windows: `%APPDATA%\dadcam\config.db`
- Linux: `~/.config/dadcam/config.db`

Pros:
- reuse presets across libraries
- CLI can list presets without opening a library
- pro users expect “system presets”

If you choose global presets, remove `codec_presets` from the library migrations and create a config DB migration set.

If you choose per-library presets, do the opposite: **remove the temp DB approach** and always open the library DB.

This addendum assumes **GLOBAL presets** (recommended).

---

## 1) Database & Migrations (v1.1)

### 1.1 Migration A: Fix `batch_operations.type` → `op_type` + add updated_at

**Why**
- `type` is a long-term footgun.
- `update_batch_operation_status` currently references `updated_at` without schema support.

Add a migration (next in your MIGRATIONS array):

```rust
// Migration X: Phase 7.1 batch schema hardening
r#"
-- Rename type column (SQLite supports rename column in modern versions; otherwise use table rebuild)
ALTER TABLE batch_operations RENAME COLUMN type TO op_type;

-- Add updated_at columns
ALTER TABLE batch_operations ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));
ALTER TABLE batch_ingest_sources ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));
ALTER TABLE batch_export_items ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));

-- Uniqueness constraints
CREATE UNIQUE INDEX IF NOT EXISTS uq_batch_ingest_source
  ON batch_ingest_sources(batch_id, source_path);

CREATE UNIQUE INDEX IF NOT EXISTS uq_batch_export_item
  ON batch_export_items(batch_id, recipe_id);

CREATE UNIQUE INDEX IF NOT EXISTS uq_relink_candidate
  ON relink_candidates(session_id, clip_id, found_path);
"#,
```

> If your SQLite version doesn’t allow `RENAME COLUMN`, do the safe table-rebuild pattern (create new table, copy, drop old, rename). Put that into this migration.

### 1.2 Migration B: Fix relink match_type truth-in-spec

Your CHECK allows `full_hash` but v1.0 doesn’t generate it. For v1.1, remove it:

```rust
// Migration Y: Phase 7.1 relink match_type fix
r#"
-- Rebuild relink_candidates with corrected CHECK if needed, or relax the constraint:
-- SQLite cannot ALTER CHECK; easiest is table rebuild:
-- 1) create relink_candidates_new with CHECK excluding full_hash
-- 2) copy rows
-- 3) drop old, rename new
"#,
```

Alternative: keep `full_hash` and implement it behind `--full-hash`, but that’s more scope.

---

## 2) Schema Helpers (Rust) — Remove SQL String Hacks

### 2.1 Fix `update_batch_operation_status` (no brittle string replace)

```rust
pub fn update_batch_operation_status(conn: &Connection, id: i64, status: &str) -> Result<()> {
    // Always update status + updated_at, and conditionally set started/completed timestamps
    let sql = match status {
        "running" => r#"UPDATE batch_operations
                        SET status = ?1,
                            started_at = COALESCE(started_at, datetime('now')),
                            updated_at = datetime('now')
                        WHERE id = ?2"#,
        "completed" | "failed" | "cancelled" => r#"UPDATE batch_operations
                        SET status = ?1,
                            completed_at = COALESCE(completed_at, datetime('now')),
                            updated_at = datetime('now')
                        WHERE id = ?2"#,
        _ => r#"UPDATE batch_operations
                SET status = ?1,
                    updated_at = datetime('now')
                WHERE id = ?2"#,
    };

    conn.execute(sql, params![status, id])?;
    Ok(())
}
```

### 2.2 Implement batch cancel/pause/resume (required for “Pro”)

```rust
pub fn set_batch_status(conn: &Connection, batch_id: i64, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE batch_operations SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![status, batch_id],
    )?;
    Ok(())
}

pub fn cancel_batch(conn: &Connection, batch_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE batch_operations SET status='cancelled', updated_at=datetime('now'), completed_at=COALESCE(completed_at, datetime('now')) WHERE id=?1",
        params![batch_id],
    )?;

    conn.execute(
        "UPDATE batch_ingest_sources SET status='cancelled', updated_at=datetime('now') WHERE batch_id=?1 AND status IN ('pending','queued','scanning','running')",
        params![batch_id],
    )?;

    conn.execute(
        "UPDATE batch_export_items SET status='cancelled', updated_at=datetime('now') WHERE batch_id=?1 AND status IN ('pending','queued','running')",
        params![batch_id],
    )?;

    Ok(())
}
```

Runner contract (must be enforced):
- Before starting each item: if batch is `paused` → stop; if `cancelled` → stop.

---

## 3) Reference Mode (NAS / External Drives) — Correct mount detection

### 3.1 Replace fragile mount resolution

Implement:

```rust
pub fn resolve_mount_root(path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // /Volumes/<VolumeName>/...
        let mut it = path.components();
        match (it.next(), it.next(), it.next()) {
            (Some(a), Some(b), Some(c))
                if a.as_os_str() == "/" && b.as_os_str() == "Volumes" =>
            {
                let mut root = PathBuf::from("/");
                root.push("Volumes");
                root.push(c.as_os_str());
                Some(root)
            }
            _ => None,
        }
    }

    #[cfg(target_os = "linux")]
    {
        // read /proc/mounts, choose the longest mount point that prefixes path
        // (implement this with a small parser; no external commands)
        None
    }

    #[cfg(target_os = "windows")]
    {
        // drive root (C:\) or UNC share root (\\server\share)
        None
    }
}
```

### 3.2 Cache detection and avoid repeated diskutil calls

If you keep `diskutil` (macOS), wrap it:
- timeout (e.g., 1s)
- cache result for 10 minutes per mount root
- failures should produce `"unknown"` not “false”

Add a `VolumeInfo` struct:
```rust
pub struct VolumeInfo {
  pub mount_root: PathBuf,
  pub is_network: Option<bool>,
  pub is_removable: Option<bool>,
  pub fs_type: Option<String>,
  pub last_checked_at: std::time::Instant,
}
```

---

## 4) Reference Ingest — Fix duplicate detection (fast-hash-only is unsafe)

### 4.1 Composite duplicate check (hash_fast + size_bytes [+ duration])

Replace “already exists by hash_fast alone” with:

```sql
SELECT id, source_uri FROM assets
WHERE library_id = ?1 AND hash_fast = ?2 AND size_bytes = ?3
LIMIT 1;
```

Behavior:
- If match exists AND `source_uri` is identical → idempotent (safe no-op)
- If match exists but path differs → allow a new reference (pro users may mount same NAS at different roots)
- Store additional “fingerprints” if you want stronger matching (size_duration), but this is minimum correct.

---

## 5) Batch Operations — Make them truly async + job-runner driven

### 5.1 Remove inline “process job now” calls
Batch ingest/export should:
- create batch records + items
- enqueue jobs
- return batch_id immediately

Job runner processes each item and updates item status.

### 5.2 Status rollup rules (source of truth)

Batch progress is derived from items:
- total = count(items)
- completed = count(status='completed')
- failed = count(status='failed')
- cancelled = count(status='cancelled')
- running = any(status='running')

Batch status:
- `pending` if no jobs started
- `running` if any item running
- `paused` if user paused
- `cancelled` if cancelled
- `failed` if any failed and policy says fail-fast
- `completed` if all items completed or skipped

Implement one helper:
```rust
pub fn recompute_batch_status(conn: &Connection, batch_id: i64) -> Result<()> { /* ... */ }
```

---

## 6) Codec Presets — Make them real, and wire into exports

### 6.1 Add scale/fps overrides (truth-in-spec)
Update `VideoParams`:

```rust
pub struct VideoParams {
  pub codec: String,
  pub preset: Option<String>,
  pub crf: Option<i64>,
  pub bitrate: Option<String>,
  pub pix_fmt: Option<String>,
  pub profile: Option<String>,
  pub tune: Option<String>,
  pub scale: Option<String>, // e.g. "1920:-2"
  pub fps: Option<u32>,      // e.g. 30
}
```

Update `build_ffmpeg_args`:
- if scale set: add `-vf scale=...` (or merge with existing filter chain)
- if fps set: add `-r <fps>`

### 6.2 Global config DB for presets (recommended)
Create `config_db.rs` for open/migrate the config db.
Migration for presets should live there, not in library migrations.

CLI `dadcam presets list` uses config DB.

UI preset dropdown reads from config DB.

### 6.3 Wire preset into export recipes and renderer
Add to `export_recipes` (library DB):
- `codec_preset_id INTEGER NULL` (or `codec_preset_name TEXT`)
Snapshot into `export_runs.recipe_snapshot`.

When rendering:
- load preset from config DB
- apply to FFmpeg final encode settings

---

## 7) Relink — Verified lifecycle + conflict-safe apply

### 7.1 verified_at lifecycle contract

- On reference ingest success: set `assets.verified_at = now()`
- On library open or “Verify References” action:
  - for each referenced asset:
    - if accessible: set verified_at now
    - else: set verified_at NULL
- Offline definition:
  - reference assets with `verified_at IS NULL` are offline

### 7.2 Conflict-safe apply
When applying one candidate for a clip:
- set that candidate to `applied`
- set all other candidates for the same `(session_id, clip_id)` to `skipped`

SQL example:
```sql
UPDATE relink_candidates
SET status = CASE WHEN id = ?1 THEN 'applied' ELSE 'skipped' END
WHERE session_id = ?2 AND clip_id = ?3 AND status = 'pending';
```

---

## 8) Tests (minimum to prove Phase 7 correctness)

Add tests that cover:

1) Batch cancel:
- create batch with items
- start runner
- cancel mid-way
- ensure remaining items → cancelled and runner stops

2) Reference mount parsing (macOS unit test):
- `/Volumes/MyNAS/share/foo.mov` → mount `/Volumes/MyNAS`

3) Preset arg build:
- H.264 preset emits `-c:v libx264` and optional `-vf scale=...` and `-r fps`
- ProRes preset emits `-c:v prores_ks -profile:v 3 -pix_fmt yuv422p10le`

4) Relink conflict:
- two candidates for same clip
- apply one
- ensure other becomes skipped

---

# Phase 7 Done-When Checklist (100% Definition)

**Reference mode**
- [ ] mount root resolution correct per platform
- [ ] volume/network detection cached, timeboxed, safe fallback (“unknown”)
- [ ] duplicate detection not fast-hash-only
- [ ] offline detection is correct and lifecycle defined

**Batch ops**
- [ ] ingest/export batches enqueue jobs (no synchronous “fake”)
- [ ] pause/resume/cancel works and is enforced in runner
- [ ] batch progress derived from item states; batch status recomputed reliably
- [ ] uniqueness constraints prevent duplicate items

**Presets**
- [ ] global preset DB exists + migrations run
- [ ] presets support codec + scale + fps overrides
- [ ] export pipeline can select a preset and snapshot it for reproducibility

**Relink**
- [ ] candidates generated reliably; apply updates verified_at
- [ ] conflict-safe apply marks other candidates skipped
- [ ] UI reflects offline and relink status clearly

**Tests**
- [ ] cancel batch test
- [ ] mount parsing test
- [ ] preset args test
- [ ] relink conflict test

---

End of Addendum
