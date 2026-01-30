// Database schema types and query helpers

use rusqlite::{Connection, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use crate::error::{DadCamError, Result};

// ----- Library -----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub id: i64,
    pub root_path: String,
    pub name: String,
    pub ingest_mode: String,
    pub created_at: String,
    pub settings: String,
}

pub fn insert_library(conn: &Connection, root_path: &str, name: &str, ingest_mode: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO libraries (root_path, name, ingest_mode) VALUES (?1, ?2, ?3)",
        params![root_path, name, ingest_mode],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_library(conn: &Connection, id: i64) -> Result<Option<Library>> {
    let result = conn.query_row(
        "SELECT id, root_path, name, ingest_mode, created_at, settings FROM libraries WHERE id = ?1",
        params![id],
        |row| {
            Ok(Library {
                id: row.get(0)?,
                root_path: row.get(1)?,
                name: row.get(2)?,
                ingest_mode: row.get(3)?,
                created_at: row.get(4)?,
                settings: row.get(5)?,
            })
        },
    ).optional()?;
    Ok(result)
}

pub fn get_library_by_path(conn: &Connection, root_path: &str) -> Result<Option<Library>> {
    let result = conn.query_row(
        "SELECT id, root_path, name, ingest_mode, created_at, settings FROM libraries WHERE root_path = ?1",
        params![root_path],
        |row| {
            Ok(Library {
                id: row.get(0)?,
                root_path: row.get(1)?,
                name: row.get(2)?,
                ingest_mode: row.get(3)?,
                created_at: row.get(4)?,
                settings: row.get(5)?,
            })
        },
    ).optional()?;
    Ok(result)
}

// ----- Asset -----

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone)]
pub struct NewAsset {
    pub library_id: i64,
    pub asset_type: String,
    pub path: String,
    pub source_uri: Option<String>,
    pub size_bytes: i64,
    pub hash_fast: Option<String>,
    pub hash_fast_scheme: Option<String>,
}

pub fn insert_asset(conn: &Connection, asset: &NewAsset) -> Result<i64> {
    conn.execute(
        "INSERT INTO assets (library_id, type, path, source_uri, size_bytes, hash_fast, hash_fast_scheme)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
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

pub fn get_asset(conn: &Connection, id: i64) -> Result<Option<Asset>> {
    let result = conn.query_row(
        "SELECT id, library_id, type, path, source_uri, size_bytes, hash_fast, hash_fast_scheme,
                hash_full, verified_at, pipeline_version, derived_params, created_at
         FROM assets WHERE id = ?1",
        params![id],
        |row| {
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
        },
    ).optional()?;
    Ok(result)
}

pub fn find_asset_by_hash(conn: &Connection, library_id: i64, hash_fast: &str) -> Result<Option<Asset>> {
    let result = conn.query_row(
        "SELECT id, library_id, type, path, source_uri, size_bytes, hash_fast, hash_fast_scheme,
                hash_full, verified_at, pipeline_version, derived_params, created_at
         FROM assets WHERE library_id = ?1 AND hash_fast = ?2",
        params![library_id, hash_fast],
        |row| {
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
        },
    ).optional()?;
    Ok(result)
}

pub fn update_asset_hash_full(conn: &Connection, id: i64, hash_full: &str) -> Result<()> {
    conn.execute(
        "UPDATE assets SET hash_full = ?1 WHERE id = ?2",
        params![hash_full, id],
    )?;
    Ok(())
}

pub fn update_asset_verified(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE assets SET verified_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Set verified_at and verified_method on an asset.
pub fn update_asset_verified_with_method(conn: &Connection, id: i64, method: &str) -> Result<()> {
    conn.execute(
        "UPDATE assets SET verified_at = datetime('now'), verified_method = ?1 WHERE id = ?2",
        params![method, id],
    )?;
    Ok(())
}

/// Clear verification on an asset (e.g. when secondary verification finds mismatch).
pub fn clear_asset_verified(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE assets SET verified_at = NULL, verified_method = NULL WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// ----- Clip -----

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_sample_rate: Option<i32>,
    pub recorded_at: Option<String>,
    pub recorded_at_offset_minutes: Option<i32>,
    pub recorded_at_is_estimated: bool,
    pub timestamp_source: Option<String>,
    pub source_folder: Option<String>,
    pub created_at: String,
    // Stable camera references (Migration 7 / L6)
    pub camera_profile_type: Option<String>,
    pub camera_profile_ref: Option<String>,
    pub camera_device_uuid: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewClip {
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
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_sample_rate: Option<i32>,
    pub recorded_at: Option<String>,
    pub recorded_at_offset_minutes: Option<i32>,
    pub recorded_at_is_estimated: bool,
    pub timestamp_source: Option<String>,
    pub source_folder: Option<String>,
    // Stable camera references (Migration 7 / L6)
    pub camera_profile_type: Option<String>,
    pub camera_profile_ref: Option<String>,
    pub camera_device_uuid: Option<String>,
}

pub fn insert_clip(conn: &Connection, clip: &NewClip) -> Result<i64> {
    conn.execute(
        "INSERT INTO clips (library_id, original_asset_id, camera_profile_id, media_type, title,
                           duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                           audio_sample_rate, recorded_at, recorded_at_offset_minutes,
                           recorded_at_is_estimated, timestamp_source, source_folder,
                           camera_profile_type, camera_profile_ref, camera_device_uuid)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
        params![
            clip.library_id,
            clip.original_asset_id,
            clip.camera_profile_id,
            clip.media_type,
            clip.title,
            clip.duration_ms,
            clip.width,
            clip.height,
            clip.fps,
            clip.codec,
            clip.audio_codec,
            clip.audio_channels,
            clip.audio_sample_rate,
            clip.recorded_at,
            clip.recorded_at_offset_minutes,
            clip.recorded_at_is_estimated,
            clip.timestamp_source,
            clip.source_folder,
            clip.camera_profile_type,
            clip.camera_profile_ref,
            clip.camera_device_uuid,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_clip(conn: &Connection, id: i64) -> Result<Option<Clip>> {
    let result = conn.query_row(
        "SELECT id, library_id, original_asset_id, camera_profile_id, media_type, title,
                duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                audio_sample_rate, recorded_at, recorded_at_offset_minutes, recorded_at_is_estimated,
                timestamp_source, source_folder, created_at,
                camera_profile_type, camera_profile_ref, camera_device_uuid
         FROM clips WHERE id = ?1",
        params![id],
        |row| {
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
                audio_codec: row.get(11)?,
                audio_channels: row.get(12)?,
                audio_sample_rate: row.get(13)?,
                recorded_at: row.get(14)?,
                recorded_at_offset_minutes: row.get(15)?,
                recorded_at_is_estimated: row.get(16)?,
                timestamp_source: row.get(17)?,
                source_folder: row.get(18)?,
                created_at: row.get(19)?,
                camera_profile_type: row.get(20)?,
                camera_profile_ref: row.get(21)?,
                camera_device_uuid: row.get(22)?,
            })
        },
    ).optional()?;
    Ok(result)
}

/// Get clip by its original asset ID
pub fn get_clip_by_asset(conn: &Connection, asset_id: i64) -> Result<Option<Clip>> {
    let result = conn.query_row(
        "SELECT id, library_id, original_asset_id, camera_profile_id, media_type, title,
                duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                audio_sample_rate, recorded_at, recorded_at_offset_minutes, recorded_at_is_estimated,
                timestamp_source, source_folder, created_at,
                camera_profile_type, camera_profile_ref, camera_device_uuid
         FROM clips WHERE original_asset_id = ?1",
        params![asset_id],
        |row| {
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
                audio_codec: row.get(11)?,
                audio_channels: row.get(12)?,
                audio_sample_rate: row.get(13)?,
                recorded_at: row.get(14)?,
                recorded_at_offset_minutes: row.get(15)?,
                recorded_at_is_estimated: row.get(16)?,
                timestamp_source: row.get(17)?,
                source_folder: row.get(18)?,
                created_at: row.get(19)?,
                camera_profile_type: row.get(20)?,
                camera_profile_ref: row.get(21)?,
                camera_device_uuid: row.get(22)?,
            })
        },
    ).optional()?;
    Ok(result)
}

pub fn list_clips(conn: &Connection, library_id: i64, limit: i64, offset: i64) -> Result<Vec<Clip>> {
    let mut stmt = conn.prepare(
        "SELECT id, library_id, original_asset_id, camera_profile_id, media_type, title,
                duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                audio_sample_rate, recorded_at, recorded_at_offset_minutes, recorded_at_is_estimated,
                timestamp_source, source_folder, created_at,
                camera_profile_type, camera_profile_ref, camera_device_uuid
         FROM clips WHERE library_id = ?1
         ORDER BY recorded_at DESC, created_at DESC
         LIMIT ?2 OFFSET ?3"
    )?;

    let clips = stmt.query_map(params![library_id, limit, offset], |row| {
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
            audio_codec: row.get(11)?,
            audio_channels: row.get(12)?,
            audio_sample_rate: row.get(13)?,
            recorded_at: row.get(14)?,
            recorded_at_offset_minutes: row.get(15)?,
            recorded_at_is_estimated: row.get(16)?,
            timestamp_source: row.get(17)?,
            source_folder: row.get(18)?,
            created_at: row.get(19)?,
            camera_profile_type: row.get(20)?,
            camera_profile_ref: row.get(21)?,
            camera_device_uuid: row.get(22)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(clips)
}

pub fn count_clips(conn: &Connection, library_id: i64) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clips WHERE library_id = ?1",
        params![library_id],
        |row| row.get(0),
    )?;
    Ok(count)
}

// ----- Clip Assets -----

pub fn link_clip_asset(conn: &Connection, clip_id: i64, asset_id: i64, role: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO clip_assets (clip_id, asset_id, role) VALUES (?1, ?2, ?3)",
        params![clip_id, asset_id, role],
    )?;
    Ok(())
}

/// Get asset path for a clip by role (original, proxy, thumb, sprite)
pub fn get_clip_asset_path(conn: &Connection, clip_id: i64, role: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT a.path FROM clip_assets ca
         JOIN assets a ON ca.asset_id = a.id
         WHERE ca.clip_id = ?1 AND ca.role = ?2
         LIMIT 1",
        params![clip_id, role],
        |row| row.get(0),
    ).optional()?;
    Ok(result)
}

/// Get all asset paths for a clip as a map of role -> path
pub fn get_clip_asset_paths(conn: &Connection, clip_id: i64) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT ca.role, a.path FROM clip_assets ca
         JOIN assets a ON ca.asset_id = a.id
         WHERE ca.clip_id = ?1"
    )?;

    let mut paths = std::collections::HashMap::new();
    let rows = stmt.query_map(params![clip_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (role, path) = row?;
        paths.insert(role, path);
    }

    Ok(paths)
}

// ----- Jobs -----

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub claimed_by: Option<String>,
    pub run_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub job_type: String,
    pub library_id: Option<i64>,
    pub clip_id: Option<i64>,
    pub asset_id: Option<i64>,
    pub priority: i32,
    pub payload: String,
}

pub fn insert_job(conn: &Connection, job: &NewJob) -> Result<i64> {
    conn.execute(
        "INSERT INTO jobs (type, library_id, clip_id, asset_id, priority, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
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

pub fn get_job(conn: &Connection, id: i64) -> Result<Option<Job>> {
    let result = conn.query_row(
        "SELECT id, type, status, library_id, clip_id, asset_id, priority, attempts, last_error,
                progress, payload, claimed_by, run_token, lease_expires_at, heartbeat_at,
                created_at, started_at, completed_at
         FROM jobs WHERE id = ?1",
        params![id],
        |row| {
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
        },
    ).optional()?;
    Ok(result)
}

pub fn list_jobs(conn: &Connection, library_id: Option<i64>, status: Option<&str>, limit: i64) -> Result<Vec<Job>> {
    let sql = match (library_id, status) {
        (Some(_), Some(_)) =>
            "SELECT id, type, status, library_id, clip_id, asset_id, priority, attempts, last_error,
                    progress, payload, claimed_by, run_token, lease_expires_at, heartbeat_at,
                    created_at, started_at, completed_at
             FROM jobs WHERE library_id = ?1 AND status = ?2
             ORDER BY priority DESC, created_at ASC LIMIT ?3",
        (Some(_), None) =>
            "SELECT id, type, status, library_id, clip_id, asset_id, priority, attempts, last_error,
                    progress, payload, claimed_by, run_token, lease_expires_at, heartbeat_at,
                    created_at, started_at, completed_at
             FROM jobs WHERE library_id = ?1
             ORDER BY priority DESC, created_at ASC LIMIT ?2",
        (None, Some(_)) =>
            "SELECT id, type, status, library_id, clip_id, asset_id, priority, attempts, last_error,
                    progress, payload, claimed_by, run_token, lease_expires_at, heartbeat_at,
                    created_at, started_at, completed_at
             FROM jobs WHERE status = ?1
             ORDER BY priority DESC, created_at ASC LIMIT ?2",
        (None, None) =>
            "SELECT id, type, status, library_id, clip_id, asset_id, priority, attempts, last_error,
                    progress, payload, claimed_by, run_token, lease_expires_at, heartbeat_at,
                    created_at, started_at, completed_at
             FROM jobs ORDER BY priority DESC, created_at ASC LIMIT ?1",
    };

    let mut stmt = conn.prepare(sql)?;

    let jobs = match (library_id, status) {
        (Some(lib_id), Some(st)) => stmt.query_map(params![lib_id, st, limit], map_job)?,
        (Some(lib_id), None) => stmt.query_map(params![lib_id, limit], map_job)?,
        (None, Some(st)) => stmt.query_map(params![st, limit], map_job)?,
        (None, None) => stmt.query_map(params![limit], map_job)?,
    }.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(jobs)
}

fn map_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
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
}

pub fn update_job_status(conn: &Connection, id: i64, status: &str) -> Result<()> {
    let now = if status == "completed" || status == "failed" {
        "completed_at = datetime('now'),"
    } else if status == "running" {
        "started_at = datetime('now'),"
    } else {
        ""
    };

    conn.execute(
        &format!("UPDATE jobs SET status = ?1, {} WHERE id = ?2", now.trim_end_matches(',')),
        params![status, id],
    )?;
    Ok(())
}

pub fn update_job_progress(conn: &Connection, id: i64, progress: i32) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET progress = ?1, heartbeat_at = datetime('now') WHERE id = ?2",
        params![progress, id],
    )?;
    Ok(())
}

pub fn update_job_error(conn: &Connection, id: i64, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET last_error = ?1, attempts = attempts + 1 WHERE id = ?2",
        params![error, id],
    )?;
    Ok(())
}

pub fn cancel_job(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET status = 'cancelled', completed_at = datetime('now') WHERE id = ?1 AND status IN ('pending', 'running')",
        params![id],
    )?;
    Ok(())
}

// ----- Ingest Files -----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestFile {
    pub id: i64,
    pub job_id: i64,
    pub source_path: String,
    pub dest_path: Option<String>,
    pub status: String,
    pub asset_id: Option<i64>,
    pub clip_id: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub fn insert_ingest_file(conn: &Connection, job_id: i64, source_path: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO ingest_files (job_id, source_path) VALUES (?1, ?2)",
        params![job_id, source_path],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_ingest_file_status(conn: &Connection, id: i64, status: &str, error: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE ingest_files SET status = ?1, error_message = ?2, updated_at = datetime('now') WHERE id = ?3",
        params![status, error, id],
    )?;
    Ok(())
}

pub fn update_ingest_file_complete(conn: &Connection, id: i64, dest_path: &str, asset_id: i64, clip_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE ingest_files SET status = 'complete', dest_path = ?1, asset_id = ?2, clip_id = ?3, updated_at = datetime('now') WHERE id = ?4",
        params![dest_path, asset_id, clip_id, id],
    )?;
    Ok(())
}

pub fn get_pending_ingest_files(conn: &Connection, job_id: i64) -> Result<Vec<IngestFile>> {
    let mut stmt = conn.prepare(
        "SELECT id, job_id, source_path, dest_path, status, asset_id, clip_id, error_message, created_at, updated_at
         FROM ingest_files WHERE job_id = ?1 AND status IN ('pending', 'copying', 'hashing', 'metadata')
         ORDER BY id ASC"
    )?;

    let files = stmt.query_map(params![job_id], |row| {
        Ok(IngestFile {
            id: row.get(0)?,
            job_id: row.get(1)?,
            source_path: row.get(2)?,
            dest_path: row.get(3)?,
            status: row.get(4)?,
            asset_id: row.get(5)?,
            clip_id: row.get(6)?,
            error_message: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(files)
}

// ----- Fingerprints -----

pub fn insert_fingerprint(conn: &Connection, clip_id: i64, fp_type: &str, value: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO fingerprints (clip_id, type, value) VALUES (?1, ?2, ?3)",
        params![clip_id, fp_type, value],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Find clips by fingerprint value
pub fn find_clips_by_fingerprint(conn: &Connection, fp_type: &str, value: &str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT clip_id FROM fingerprints WHERE type = ?1 AND value = ?2"
    )?;
    let ids = stmt.query_map(params![fp_type, value], |row| row.get(0))?
        .collect::<std::result::Result<Vec<i64>, _>>()?;
    Ok(ids)
}

/// Get all missing assets (assets with no file at path)
pub fn get_missing_assets(conn: &Connection, library_id: i64) -> Result<Vec<Asset>> {
    let mut stmt = conn.prepare(
        "SELECT id, library_id, type, path, source_uri, size_bytes, hash_fast, hash_fast_scheme,
                hash_full, verified_at, pipeline_version, derived_params, created_at
         FROM assets
         WHERE library_id = ?1 AND type = 'original' AND path != ''"
    )?;

    let assets = stmt.query_map(params![library_id], |row| {
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
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(assets)
}

/// Get fingerprints for a clip
pub fn get_clip_fingerprints(conn: &Connection, clip_id: i64) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT type, value FROM fingerprints WHERE clip_id = ?1"
    )?;
    let fingerprints = stmt.query_map(params![clip_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(fingerprints)
}

// ----- Tags -----

pub fn get_tag_id(conn: &Connection, name: &str) -> Result<Option<i64>> {
    let result = conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        params![name],
        |row| row.get(0),
    ).optional()?;
    Ok(result)
}

pub fn add_clip_tag(conn: &Connection, clip_id: i64, tag_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO clip_tags (clip_id, tag_id) VALUES (?1, ?2)",
        params![clip_id, tag_id],
    )?;
    Ok(())
}

pub fn remove_clip_tag(conn: &Connection, clip_id: i64, tag_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM clip_tags WHERE clip_id = ?1 AND tag_id = ?2",
        params![clip_id, tag_id],
    )?;
    Ok(())
}

pub fn has_clip_tag(conn: &Connection, clip_id: i64, tag_name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = ?1 AND t.name = ?2",
        params![clip_id, tag_name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

// ----- Volumes -----

/// Get or create a volume record by serial/label
pub fn get_or_create_volume(conn: &Connection, serial: Option<&str>, label: Option<&str>, mount_point: Option<&str>) -> Result<i64> {
    // Try to find existing volume by serial (if provided)
    if let Some(ser) = serial {
        if let Some(id) = conn.query_row(
            "SELECT id FROM volumes WHERE serial = ?1",
            params![ser],
            |row| row.get::<_, i64>(0),
        ).optional()? {
            // Update last_seen_at
            conn.execute(
                "UPDATE volumes SET last_seen_at = datetime('now'), mount_point = COALESCE(?1, mount_point) WHERE id = ?2",
                params![mount_point, id],
            )?;
            return Ok(id);
        }
    }

    // Try to find by label if no serial
    if serial.is_none() {
        if let Some(lbl) = label {
            if let Some(id) = conn.query_row(
                "SELECT id FROM volumes WHERE serial IS NULL AND label = ?1",
                params![lbl],
                |row| row.get::<_, i64>(0),
            ).optional()? {
                conn.execute(
                    "UPDATE volumes SET last_seen_at = datetime('now'), mount_point = COALESCE(?1, mount_point) WHERE id = ?2",
                    params![mount_point, id],
                )?;
                return Ok(id);
            }
        }
    }

    // Create new volume
    conn.execute(
        "INSERT INTO volumes (serial, label, mount_point) VALUES (?1, ?2, ?3)",
        params![serial, label, mount_point],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Link an asset to a volume
pub fn link_asset_volume(conn: &Connection, asset_id: i64, volume_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO asset_volumes (asset_id, volume_id) VALUES (?1, ?2)",
        params![asset_id, volume_id],
    )?;
    Ok(())
}

// ----- Camera Profiles -----

/// Update a clip's camera profile
pub fn update_clip_camera_profile(conn: &Connection, clip_id: i64, camera_profile_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET camera_profile_id = ?1 WHERE id = ?2",
        params![camera_profile_id, clip_id],
    )?;
    Ok(())
}

/// Update stable camera references on a clip (Migration 7 / L6 columns).
pub fn update_clip_camera_refs(
    conn: &Connection,
    clip_id: i64,
    profile_type: Option<&str>,
    profile_ref: Option<&str>,
    device_uuid: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE clips SET camera_profile_type = ?1, camera_profile_ref = ?2, camera_device_uuid = ?3
         WHERE id = ?4",
        params![profile_type, profile_ref, device_uuid, clip_id],
    )?;
    Ok(())
}

// ----- Events (Phase 6) -----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: i64,
    pub library_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub event_type: String,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub color: String,
    pub icon: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEvent {
    pub library_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub event_type: String,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// Insert a new event
pub fn insert_event(conn: &Connection, event: &NewEvent) -> Result<i64> {
    conn.execute(
        "INSERT INTO events (library_id, name, description, type, date_start, date_end, color, icon)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, COALESCE(?7, '#3b82f6'), COALESCE(?8, 'calendar'))",
        params![
            event.library_id,
            event.name,
            event.description,
            event.event_type,
            event.date_start,
            event.date_end,
            event.color,
            event.icon,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get event by ID
pub fn get_event(conn: &Connection, id: i64) -> Result<Option<Event>> {
    let result = conn.query_row(
        "SELECT id, library_id, name, description, type, date_start, date_end, color, icon, created_at, updated_at
         FROM events WHERE id = ?1",
        params![id],
        |row| {
            Ok(Event {
                id: row.get(0)?,
                library_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                event_type: row.get(4)?,
                date_start: row.get(5)?,
                date_end: row.get(6)?,
                color: row.get(7)?,
                icon: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        },
    ).optional()?;
    Ok(result)
}

/// List all events for a library
pub fn list_events(conn: &Connection, library_id: i64) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT id, library_id, name, description, type, date_start, date_end, color, icon, created_at, updated_at
         FROM events WHERE library_id = ?1
         ORDER BY created_at DESC"
    )?;

    let events = stmt.query_map(params![library_id], |row| {
        Ok(Event {
            id: row.get(0)?,
            library_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            event_type: row.get(4)?,
            date_start: row.get(5)?,
            date_end: row.get(6)?,
            color: row.get(7)?,
            icon: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(events)
}

/// Update an event
pub fn update_event(conn: &Connection, id: i64, updates: &EventUpdate) -> Result<()> {
    let mut set_clauses = vec!["updated_at = datetime('now')".to_string()];
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref name) = updates.name {
        set_clauses.push(format!("name = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(name.clone()));
    }
    if let Some(ref desc) = updates.description {
        set_clauses.push(format!("description = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(desc.clone()));
    }
    if let Some(ref date_start) = updates.date_start {
        set_clauses.push(format!("date_start = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(date_start.clone()));
    }
    if let Some(ref date_end) = updates.date_end {
        set_clauses.push(format!("date_end = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(date_end.clone()));
    }
    if let Some(ref color) = updates.color {
        set_clauses.push(format!("color = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(color.clone()));
    }
    if let Some(ref icon) = updates.icon {
        set_clauses.push(format!("icon = ?{}", params_vec.len() + 1));
        params_vec.push(Box::new(icon.clone()));
    }

    params_vec.push(Box::new(id));
    let id_param = params_vec.len();

    let sql = format!(
        "UPDATE events SET {} WHERE id = ?{}",
        set_clauses.join(", "),
        id_param
    );

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, params_refs.as_slice())?;
    Ok(())
}

/// Delete an event
pub fn delete_event(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM events WHERE id = ?1", params![id])?;
    Ok(())
}

/// Add clips to an event
pub fn add_clips_to_event(conn: &Connection, event_id: i64, clip_ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO event_clips (event_id, clip_id) VALUES (?1, ?2)"
    )?;
    for clip_id in clip_ids {
        stmt.execute(params![event_id, clip_id])?;
    }
    Ok(())
}

/// Remove clips from an event
pub fn remove_clips_from_event(conn: &Connection, event_id: i64, clip_ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare(
        "DELETE FROM event_clips WHERE event_id = ?1 AND clip_id = ?2"
    )?;
    for clip_id in clip_ids {
        stmt.execute(params![event_id, clip_id])?;
    }
    Ok(())
}

/// Get clip IDs for an event (explicit + date range)
pub fn get_event_clip_ids(conn: &Connection, event_id: i64) -> Result<Vec<i64>> {
    // First get the event to check its type
    let event = get_event(conn, event_id)?
        .ok_or_else(|| DadCamError::NotFound(format!("Event {} not found", event_id)))?;

    let mut clip_ids: Vec<i64> = Vec::new();

    // Get explicitly added clips
    let mut stmt = conn.prepare(
        "SELECT clip_id FROM event_clips WHERE event_id = ?1"
    )?;
    let explicit_ids: Vec<i64> = stmt.query_map(params![event_id], |row| row.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    clip_ids.extend(explicit_ids);

    // For date_range events, also get clips within the date range
    if event.event_type == "date_range" {
        if let (Some(start), Some(end)) = (&event.date_start, &event.date_end) {
            let mut range_stmt = conn.prepare(
                "SELECT id FROM clips
                 WHERE library_id = ?1
                 AND date(recorded_at) >= date(?2)
                 AND date(recorded_at) <= date(?3)
                 AND id NOT IN (SELECT clip_id FROM event_clips WHERE event_id = ?4)"
            )?;
            let range_ids: Vec<i64> = range_stmt.query_map(
                params![event.library_id, start, end, event_id],
                |row| row.get(0)
            )?.collect::<std::result::Result<Vec<_>, _>>()?;
            clip_ids.extend(range_ids);
        }
    }

    Ok(clip_ids)
}

/// Get clip count for an event
pub fn get_event_clip_count(conn: &Connection, event_id: i64) -> Result<i64> {
    let clip_ids = get_event_clip_ids(conn, event_id)?;
    Ok(clip_ids.len() as i64)
}

/// Get clips for an event with pagination (optimized with SQL LIMIT/OFFSET)
pub fn get_event_clips(conn: &Connection, event_id: i64, limit: i64, offset: i64) -> Result<Vec<Clip>> {
    // Get the event to check its type
    let event = get_event(conn, event_id)?
        .ok_or_else(|| DadCamError::NotFound(format!("Event {} not found", event_id)))?;

    // For date_range events with valid dates, include clips from date range
    if event.event_type == "date_range" {
        if let (Some(ref start), Some(ref end)) = (&event.date_start, &event.date_end) {
            // Combine explicit clips with date range clips
            let mut stmt = conn.prepare(
                "SELECT id, library_id, original_asset_id, camera_profile_id, media_type, title,
                        duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                        audio_sample_rate, recorded_at, recorded_at_offset_minutes, recorded_at_is_estimated,
                        timestamp_source, source_folder, created_at,
                        camera_profile_type, camera_profile_ref, camera_device_uuid
                 FROM clips
                 WHERE id IN (SELECT clip_id FROM event_clips WHERE event_id = ?1)
                    OR (library_id = ?2 AND date(recorded_at) >= date(?3) AND date(recorded_at) <= date(?4))
                 ORDER BY recorded_at DESC, created_at DESC
                 LIMIT ?5 OFFSET ?6"
            )?;
            let clips = stmt.query_map(
                params![event_id, event.library_id, start, end, limit, offset],
                map_clip
            )?.collect::<std::result::Result<Vec<_>, _>>()?;
            return Ok(clips);
        }
    }

    // For clip_selection events or date_range without dates - only explicit clips
    let mut stmt = conn.prepare(
        "SELECT id, library_id, original_asset_id, camera_profile_id, media_type, title,
                duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                audio_sample_rate, recorded_at, recorded_at_offset_minutes, recorded_at_is_estimated,
                timestamp_source, source_folder, created_at,
                camera_profile_type, camera_profile_ref, camera_device_uuid
         FROM clips
         WHERE id IN (SELECT clip_id FROM event_clips WHERE event_id = ?1)
         ORDER BY recorded_at DESC, created_at DESC
         LIMIT ?2 OFFSET ?3"
    )?;
    let clips = stmt.query_map(
        params![event_id, limit, offset],
        map_clip
    )?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(clips)
}

fn map_clip(row: &rusqlite::Row) -> rusqlite::Result<Clip> {
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
        audio_codec: row.get(11)?,
        audio_channels: row.get(12)?,
        audio_sample_rate: row.get(13)?,
        recorded_at: row.get(14)?,
        recorded_at_offset_minutes: row.get(15)?,
        recorded_at_is_estimated: row.get(16)?,
        timestamp_source: row.get(17)?,
        source_folder: row.get(18)?,
        created_at: row.get(19)?,
        camera_profile_type: row.get(20)?,
        camera_profile_ref: row.get(21)?,
        camera_device_uuid: row.get(22)?,
    })
}

/// Get clips grouped by date
pub fn get_clips_grouped_by_date(conn: &Connection, library_id: i64) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT date(recorded_at) as clip_date, COUNT(*) as clip_count
         FROM clips
         WHERE library_id = ?1 AND recorded_at IS NOT NULL
         GROUP BY clip_date
         ORDER BY clip_date DESC"
    )?;

    let groups = stmt.query_map(params![library_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(groups)
}

/// Get clips for a specific date
pub fn get_clips_by_date(conn: &Connection, library_id: i64, date: &str, limit: i64, offset: i64) -> Result<Vec<Clip>> {
    let mut stmt = conn.prepare(
        "SELECT id, library_id, original_asset_id, camera_profile_id, media_type, title,
                duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                audio_sample_rate, recorded_at, recorded_at_offset_minutes, recorded_at_is_estimated,
                timestamp_source, source_folder, created_at,
                camera_profile_type, camera_profile_ref, camera_device_uuid
         FROM clips
         WHERE library_id = ?1 AND date(recorded_at) = date(?2)
         ORDER BY recorded_at DESC
         LIMIT ?3 OFFSET ?4"
    )?;

    let clips = stmt.query_map(params![library_id, date, limit, offset], map_clip)?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(clips)
}

/// Count clips for a specific date
pub fn count_clips_by_date(conn: &Connection, library_id: i64, date: &str) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clips WHERE library_id = ?1 AND date(recorded_at) = date(?2)",
        params![library_id, date],
        |row| row.get(0),
    )?;
    Ok(count)
}

// ---------------------------------------------------------------------------
// VHS Edit Recipes (Migration 8 / L7 -- deterministic recipe definitions)
// ---------------------------------------------------------------------------

/// A VHS edit recipe stored in Library DB
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VhsEdit {
    pub id: i64,
    pub edit_uuid: String,
    pub name: String,
    pub pipeline_version: i32,
    pub created_at: String,
    pub recipe_hash: String,
    pub input_clip_ids: String,
    pub title_text: String,
    pub title_offset_seconds: i32,
    pub audio_blend_params: String,
    pub transform_overrides: String,
    pub output_relpath: Option<String>,
    pub output_hash: Option<String>,
}

/// Parameters for creating a VHS edit recipe
pub struct NewVhsEdit {
    pub name: String,
    pub pipeline_version: i32,
    pub input_clip_ids: Vec<i64>,
    pub title_text: String,
    pub title_offset_seconds: i32,
    pub audio_blend_params: serde_json::Value,
    pub transform_overrides: serde_json::Value,
}

/// Compute the canonical recipe hash (BLAKE3 of canonical JSON bytes).
/// Canonical JSON: stable key ordering, arrays preserve order, no whitespace.
fn compute_recipe_hash(edit: &NewVhsEdit) -> String {
    use std::collections::BTreeMap;

    let mut canonical = BTreeMap::new();
    canonical.insert("audio_blend_params", serde_json::to_value(&edit.audio_blend_params).unwrap());
    canonical.insert("input_clip_ids", serde_json::to_value(&edit.input_clip_ids).unwrap());
    canonical.insert("pipeline_version", serde_json::to_value(edit.pipeline_version).unwrap());
    canonical.insert("title_offset_seconds", serde_json::to_value(edit.title_offset_seconds).unwrap());
    canonical.insert("title_text", serde_json::to_value(&edit.title_text).unwrap());
    canonical.insert("transform_overrides", serde_json::to_value(&edit.transform_overrides).unwrap());

    let canonical_json = serde_json::to_string(&canonical).unwrap();
    let hash = blake3::hash(canonical_json.as_bytes());
    hash.to_hex().to_string()
}

/// Insert a VHS edit recipe. Computes recipe_hash from canonical inputs.
pub fn insert_vhs_edit(conn: &Connection, edit: &NewVhsEdit) -> Result<VhsEdit> {
    let edit_uuid = uuid::Uuid::new_v4().to_string();
    let recipe_hash = compute_recipe_hash(edit);
    let clip_ids_json = serde_json::to_string(&edit.input_clip_ids)?;
    let audio_json = serde_json::to_string(&edit.audio_blend_params)?;
    let transform_json = serde_json::to_string(&edit.transform_overrides)?;

    conn.execute(
        "INSERT INTO vhs_edits (edit_uuid, name, pipeline_version, recipe_hash, input_clip_ids,
         title_text, title_offset_seconds, audio_blend_params, transform_overrides)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            edit_uuid, edit.name, edit.pipeline_version, recipe_hash,
            clip_ids_json, edit.title_text, edit.title_offset_seconds,
            audio_json, transform_json,
        ],
    )?;

    let id = conn.last_insert_rowid();
    let created_at: String = conn.query_row(
        "SELECT created_at FROM vhs_edits WHERE id = ?1", [id], |row| row.get(0),
    )?;

    Ok(VhsEdit {
        id,
        edit_uuid,
        name: edit.name.clone(),
        pipeline_version: edit.pipeline_version,
        created_at,
        recipe_hash,
        input_clip_ids: clip_ids_json,
        title_text: edit.title_text.clone(),
        title_offset_seconds: edit.title_offset_seconds,
        audio_blend_params: audio_json,
        transform_overrides: transform_json,
        output_relpath: None,
        output_hash: None,
    })
}

/// Get a VHS edit by UUID.
pub fn get_vhs_edit(conn: &Connection, edit_uuid: &str) -> Result<Option<VhsEdit>> {
    use rusqlite::OptionalExtension;
    let result = conn.query_row(
        "SELECT id, edit_uuid, name, pipeline_version, created_at, recipe_hash,
                input_clip_ids, title_text, title_offset_seconds,
                audio_blend_params, transform_overrides, output_relpath, output_hash
         FROM vhs_edits WHERE edit_uuid = ?1",
        [edit_uuid],
        |row| {
            Ok(VhsEdit {
                id: row.get(0)?,
                edit_uuid: row.get(1)?,
                name: row.get(2)?,
                pipeline_version: row.get(3)?,
                created_at: row.get(4)?,
                recipe_hash: row.get(5)?,
                input_clip_ids: row.get(6)?,
                title_text: row.get(7)?,
                title_offset_seconds: row.get(8)?,
                audio_blend_params: row.get(9)?,
                transform_overrides: row.get(10)?,
                output_relpath: row.get(11)?,
                output_hash: row.get(12)?,
            })
        },
    ).optional()?;
    Ok(result)
}

/// Update the output fields of a VHS edit after a build.
pub fn update_vhs_edit_output(conn: &Connection, edit_uuid: &str, output_relpath: &str, output_hash: &str) -> Result<()> {
    conn.execute(
        "UPDATE vhs_edits SET output_relpath = ?1, output_hash = ?2 WHERE edit_uuid = ?3",
        params![output_relpath, output_hash, edit_uuid],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Ingest Sessions + Manifest Entries (Migration 9 -- gold-standard import)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestSession {
    pub id: i64,
    pub job_id: i64,
    pub source_root: String,
    pub device_serial: Option<String>,
    pub device_label: Option<String>,
    pub device_mount_point: Option<String>,
    pub device_capacity_bytes: Option<i64>,
    pub status: String,
    pub manifest_hash: Option<String>,
    pub rescan_hash: Option<String>,
    pub safe_to_wipe_at: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

pub struct NewIngestSession {
    pub job_id: i64,
    pub source_root: String,
    pub device_serial: Option<String>,
    pub device_label: Option<String>,
    pub device_mount_point: Option<String>,
    pub device_capacity_bytes: Option<i64>,
}

pub fn insert_ingest_session(conn: &Connection, session: &NewIngestSession) -> Result<i64> {
    conn.execute(
        "INSERT INTO ingest_sessions (job_id, source_root, device_serial, device_label, device_mount_point, device_capacity_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session.job_id,
            session.source_root,
            session.device_serial,
            session.device_label,
            session.device_mount_point,
            session.device_capacity_bytes,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_ingest_session(conn: &Connection, id: i64) -> Result<Option<IngestSession>> {
    let result = conn.query_row(
        "SELECT id, job_id, source_root, device_serial, device_label, device_mount_point,
                device_capacity_bytes, status, manifest_hash, rescan_hash, safe_to_wipe_at,
                started_at, finished_at
         FROM ingest_sessions WHERE id = ?1",
        params![id],
        |row| {
            Ok(IngestSession {
                id: row.get(0)?,
                job_id: row.get(1)?,
                source_root: row.get(2)?,
                device_serial: row.get(3)?,
                device_label: row.get(4)?,
                device_mount_point: row.get(5)?,
                device_capacity_bytes: row.get(6)?,
                status: row.get(7)?,
                manifest_hash: row.get(8)?,
                rescan_hash: row.get(9)?,
                safe_to_wipe_at: row.get(10)?,
                started_at: row.get(11)?,
                finished_at: row.get(12)?,
            })
        },
    ).optional()?;
    Ok(result)
}

pub fn get_ingest_session_by_job(conn: &Connection, job_id: i64) -> Result<Option<IngestSession>> {
    let result = conn.query_row(
        "SELECT id, job_id, source_root, device_serial, device_label, device_mount_point,
                device_capacity_bytes, status, manifest_hash, rescan_hash, safe_to_wipe_at,
                started_at, finished_at
         FROM ingest_sessions WHERE job_id = ?1",
        params![job_id],
        |row| {
            Ok(IngestSession {
                id: row.get(0)?,
                job_id: row.get(1)?,
                source_root: row.get(2)?,
                device_serial: row.get(3)?,
                device_label: row.get(4)?,
                device_mount_point: row.get(5)?,
                device_capacity_bytes: row.get(6)?,
                status: row.get(7)?,
                manifest_hash: row.get(8)?,
                rescan_hash: row.get(9)?,
                safe_to_wipe_at: row.get(10)?,
                started_at: row.get(11)?,
                finished_at: row.get(12)?,
            })
        },
    ).optional()?;
    Ok(result)
}

pub fn update_ingest_session_status(conn: &Connection, id: i64, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE ingest_sessions SET status = ?1 WHERE id = ?2",
        params![status, id],
    )?;
    Ok(())
}

pub fn update_ingest_session_manifest_hash(conn: &Connection, id: i64, hash: &str) -> Result<()> {
    conn.execute(
        "UPDATE ingest_sessions SET manifest_hash = ?1 WHERE id = ?2",
        params![hash, id],
    )?;
    Ok(())
}

pub fn update_ingest_session_rescan(conn: &Connection, id: i64, rescan_hash: &str, safe: bool) -> Result<()> {
    if safe {
        conn.execute(
            "UPDATE ingest_sessions SET rescan_hash = ?1, safe_to_wipe_at = datetime('now') WHERE id = ?2",
            params![rescan_hash, id],
        )?;
    } else {
        conn.execute(
            "UPDATE ingest_sessions SET rescan_hash = ?1 WHERE id = ?2",
            params![rescan_hash, id],
        )?;
    }
    Ok(())
}

pub fn update_ingest_session_finished(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE ingest_sessions SET finished_at = datetime('now') WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

// --- Manifest Entries ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: i64,
    pub session_id: i64,
    pub relative_path: String,
    pub size_bytes: i64,
    pub mtime: Option<String>,
    pub hash_fast: Option<String>,
    pub hash_source_full: Option<String>,
    pub asset_id: Option<i64>,
    pub result: String,
    pub error_code: Option<String>,
    pub error_detail: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// "media" or "sidecar" (Migration 10)
    pub entry_type: String,
    /// FK to parent media entry for sidecars; NULL for media and orphan sidecars
    pub parent_entry_id: Option<i64>,
}

pub struct NewManifestEntry {
    pub session_id: i64,
    pub relative_path: String,
    pub size_bytes: i64,
    pub mtime: Option<String>,
    /// "media" or "sidecar" (Migration 10)
    pub entry_type: String,
    /// FK to parent media entry for sidecars; None for media and orphan sidecars
    pub parent_entry_id: Option<i64>,
}

pub fn insert_manifest_entry(conn: &Connection, entry: &NewManifestEntry) -> Result<i64> {
    conn.execute(
        "INSERT INTO ingest_manifest_entries (session_id, relative_path, size_bytes, mtime, entry_type, parent_entry_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![entry.session_id, entry.relative_path, entry.size_bytes, entry.mtime, entry.entry_type, entry.parent_entry_id],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_manifest_entries(conn: &Connection, session_id: i64) -> Result<Vec<ManifestEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, relative_path, size_bytes, mtime, hash_fast, hash_source_full,
                asset_id, result, error_code, error_detail, created_at, updated_at,
                entry_type, parent_entry_id
         FROM ingest_manifest_entries WHERE session_id = ?1
         ORDER BY id ASC"
    )?;
    let entries = stmt.query_map(params![session_id], |row| {
        Ok(ManifestEntry {
            id: row.get(0)?,
            session_id: row.get(1)?,
            relative_path: row.get(2)?,
            size_bytes: row.get(3)?,
            mtime: row.get(4)?,
            hash_fast: row.get(5)?,
            hash_source_full: row.get(6)?,
            asset_id: row.get(7)?,
            result: row.get(8)?,
            error_code: row.get(9)?,
            error_detail: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
            entry_type: row.get(13)?,
            parent_entry_id: row.get(14)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(entries)
}

pub fn get_pending_manifest_entries(conn: &Connection, session_id: i64) -> Result<Vec<ManifestEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, relative_path, size_bytes, mtime, hash_fast, hash_source_full,
                asset_id, result, error_code, error_detail, created_at, updated_at,
                entry_type, parent_entry_id
         FROM ingest_manifest_entries WHERE session_id = ?1 AND result = 'pending'
         ORDER BY id ASC"
    )?;
    let entries = stmt.query_map(params![session_id], |row| {
        Ok(ManifestEntry {
            id: row.get(0)?,
            session_id: row.get(1)?,
            relative_path: row.get(2)?,
            size_bytes: row.get(3)?,
            mtime: row.get(4)?,
            hash_fast: row.get(5)?,
            hash_source_full: row.get(6)?,
            asset_id: row.get(7)?,
            result: row.get(8)?,
            error_code: row.get(9)?,
            error_detail: row.get(10)?,
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
            entry_type: row.get(13)?,
            parent_entry_id: row.get(14)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(entries)
}

pub fn update_manifest_entry_result(
    conn: &Connection,
    id: i64,
    result: &str,
    hash_source_full: Option<&str>,
    asset_id: Option<i64>,
    error_code: Option<&str>,
    error_detail: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE ingest_manifest_entries
         SET result = ?1, hash_source_full = ?2, asset_id = ?3,
             error_code = ?4, error_detail = ?5, updated_at = datetime('now')
         WHERE id = ?6",
        params![result, hash_source_full, asset_id, error_code, error_detail, id],
    )?;
    Ok(())
}

pub fn update_manifest_entry_hash_fast(conn: &Connection, id: i64, hash_fast: &str) -> Result<()> {
    conn.execute(
        "UPDATE ingest_manifest_entries SET hash_fast = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![hash_fast, id],
    )?;
    Ok(())
}

// --- Session verification roll-up ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionVerificationStatus {
    pub session_id: i64,
    pub total_entries: i64,
    pub copied_verified: i64,
    pub dedup_verified: i64,
    pub failed: i64,
    pub changed: i64,
    pub pending: i64,
    pub all_verified: bool,
    pub safe_to_wipe: bool,
    pub safe_to_wipe_at: Option<String>,
    /// Total sidecar entries in this session (sidecar-importplan 12.7)
    pub sidecar_total: i64,
    /// Sidecar entries that failed verification
    pub sidecar_failed: i64,
}

pub fn get_session_verification_status(conn: &Connection, session_id: i64) -> Result<SessionVerificationStatus> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1",
        params![session_id], |row| row.get(0),
    )?;
    let copied_verified: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1 AND result = 'copied_verified'",
        params![session_id], |row| row.get(0),
    )?;
    let dedup_verified: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1 AND result = 'dedup_verified'",
        params![session_id], |row| row.get(0),
    )?;
    let failed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1 AND result = 'failed'",
        params![session_id], |row| row.get(0),
    )?;
    let changed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1 AND result = 'changed'",
        params![session_id], |row| row.get(0),
    )?;
    let pending: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1 AND result IN ('pending','copying')",
        params![session_id], |row| row.get(0),
    )?;

    let all_verified = total > 0 && (copied_verified + dedup_verified) == total;

    // Sidecar-specific counts (sidecar-importplan 12.7)
    let sidecar_total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1 AND entry_type = 'sidecar'",
        params![session_id], |row| row.get(0),
    )?;
    let sidecar_failed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ingest_manifest_entries WHERE session_id = ?1 AND entry_type = 'sidecar' AND result = 'failed'",
        params![session_id], |row| row.get(0),
    )?;

    let session = get_ingest_session(conn, session_id)?;
    let safe_to_wipe_at = session.as_ref().and_then(|s| s.safe_to_wipe_at.clone());
    let safe_to_wipe = safe_to_wipe_at.is_some();

    Ok(SessionVerificationStatus {
        session_id,
        total_entries: total,
        copied_verified,
        dedup_verified,
        failed,
        changed,
        pending,
        all_verified,
        safe_to_wipe,
        safe_to_wipe_at,
        sidecar_total,
        sidecar_failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_vhs_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(r#"
            CREATE TABLE vhs_edits (
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
        "#).unwrap();
        conn
    }

    // Spec 9.5: Creating recipe stores canonical recipe_hash
    #[test]
    fn test_vhs_recipe_stores_canonical_hash() {
        let conn = setup_vhs_db();

        let edit = NewVhsEdit {
            name: "Birthday 2024".to_string(),
            pipeline_version: 1,
            input_clip_ids: vec![1, 2, 3],
            title_text: "Birthday Party".to_string(),
            title_offset_seconds: 5,
            audio_blend_params: serde_json::json!({"crossfade": 0.5}),
            transform_overrides: serde_json::json!({}),
        };

        let result = insert_vhs_edit(&conn, &edit).unwrap();
        assert!(!result.recipe_hash.is_empty(), "recipe_hash should be computed");
        assert!(!result.edit_uuid.is_empty(), "edit_uuid should be generated");

        // Retrieve and verify
        let stored = get_vhs_edit(&conn, &result.edit_uuid).unwrap().unwrap();
        assert_eq!(stored.recipe_hash, result.recipe_hash);
        assert_eq!(stored.name, "Birthday 2024");
        assert_eq!(stored.input_clip_ids, "[1,2,3]");
    }

    // Spec 9.5: Same inputs yield same recipe_hash (deterministic)
    #[test]
    fn test_vhs_recipe_hash_deterministic() {
        let conn = setup_vhs_db();

        let make_edit = || NewVhsEdit {
            name: "Test Edit".to_string(),
            pipeline_version: 1,
            input_clip_ids: vec![10, 20, 30],
            title_text: "Test Title".to_string(),
            title_offset_seconds: 5,
            audio_blend_params: serde_json::json!({"volume": 0.8}),
            transform_overrides: serde_json::json!({"crop": {"top": 10}}),
        };

        let edit1 = insert_vhs_edit(&conn, &make_edit()).unwrap();
        let edit2 = insert_vhs_edit(&conn, &make_edit()).unwrap();

        // Same inputs must produce same recipe_hash
        assert_eq!(edit1.recipe_hash, edit2.recipe_hash,
            "Identical inputs must produce identical recipe_hash");

        // But different edit_uuids (unique per insert)
        assert_ne!(edit1.edit_uuid, edit2.edit_uuid);
    }

    // Spec 9.5: Changing inputs mutates recipe_hash (formulas don't mutate stored recipes)
    #[test]
    fn test_vhs_changing_inputs_changes_hash() {
        let conn = setup_vhs_db();

        let edit_a = NewVhsEdit {
            name: "Edit A".to_string(),
            pipeline_version: 1,
            input_clip_ids: vec![1, 2, 3],
            title_text: "Title A".to_string(),
            title_offset_seconds: 5,
            audio_blend_params: serde_json::json!({}),
            transform_overrides: serde_json::json!({}),
        };

        let edit_b = NewVhsEdit {
            name: "Edit B".to_string(),
            pipeline_version: 1,
            input_clip_ids: vec![1, 2, 4], // different clip list
            title_text: "Title A".to_string(),
            title_offset_seconds: 5,
            audio_blend_params: serde_json::json!({}),
            transform_overrides: serde_json::json!({}),
        };

        let result_a = insert_vhs_edit(&conn, &edit_a).unwrap();
        let result_b = insert_vhs_edit(&conn, &edit_b).unwrap();

        // Different inputs must produce different recipe_hash
        assert_ne!(result_a.recipe_hash, result_b.recipe_hash,
            "Different inputs must produce different recipe_hash");

        // Original recipe is unchanged (stored recipes are immutable)
        let stored_a = get_vhs_edit(&conn, &result_a.edit_uuid).unwrap().unwrap();
        assert_eq!(stored_a.recipe_hash, result_a.recipe_hash,
            "Stored recipe should not be mutated by subsequent inserts");
        assert_eq!(stored_a.input_clip_ids, "[1,2,3]");
    }

    // Spec 9.5: Output hash stored after build
    #[test]
    fn test_vhs_output_hash_update() {
        let conn = setup_vhs_db();

        let edit = NewVhsEdit {
            name: "Output Test".to_string(),
            pipeline_version: 1,
            input_clip_ids: vec![1],
            title_text: "".to_string(),
            title_offset_seconds: 5,
            audio_blend_params: serde_json::json!({}),
            transform_overrides: serde_json::json!({}),
        };

        let result = insert_vhs_edit(&conn, &edit).unwrap();
        assert!(result.output_relpath.is_none());
        assert!(result.output_hash.is_none());

        // Simulate build completing
        update_vhs_edit_output(&conn, &result.edit_uuid, ".dadcam/exports/out.mp4", "abc123hash").unwrap();

        let stored = get_vhs_edit(&conn, &result.edit_uuid).unwrap().unwrap();
        assert_eq!(stored.output_relpath, Some(".dadcam/exports/out.mp4".to_string()));
        assert_eq!(stored.output_hash, Some("abc123hash".to_string()));
    }
}
