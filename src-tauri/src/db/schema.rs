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
}

pub fn insert_clip(conn: &Connection, clip: &NewClip) -> Result<i64> {
    conn.execute(
        "INSERT INTO clips (library_id, original_asset_id, camera_profile_id, media_type, title,
                           duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                           audio_sample_rate, recorded_at, recorded_at_offset_minutes,
                           recorded_at_is_estimated, timestamp_source, source_folder)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_clip(conn: &Connection, id: i64) -> Result<Option<Clip>> {
    let result = conn.query_row(
        "SELECT id, library_id, original_asset_id, camera_profile_id, media_type, title,
                duration_ms, width, height, fps, codec, audio_codec, audio_channels,
                audio_sample_rate, recorded_at, recorded_at_offset_minutes, recorded_at_is_estimated,
                timestamp_source, source_folder, created_at
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
                timestamp_source, source_folder, created_at
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
                timestamp_source, source_folder, created_at
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
