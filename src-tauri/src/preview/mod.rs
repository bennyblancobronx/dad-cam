// Preview pipeline module - Phase 2
//
// Handles generation of derived assets:
// - Proxies: H.264 720p videos for smooth playback
// - Thumbnails: JPG poster frames for grid display
// - Sprites: Tiled JPG strips for hover scrubbing

pub mod proxy;
pub mod thumb;
pub mod sprite;

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use rusqlite::{Connection, params, OptionalExtension};

use crate::constants::{
    PIPELINE_VERSION, PROXY_CODEC, PROXY_RESOLUTION, PROXY_CRF,
    THUMB_FORMAT, THUMB_QUALITY, SPRITE_FPS, SPRITE_TILE_WIDTH,
    DADCAM_FOLDER,
};
use crate::db::schema::Asset;
use crate::error::Result;

/// Parameters used to generate a derived asset.
/// These params are hashed to detect when regeneration is needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedParams {
    pub pipeline_version: u32,
    pub preset: String,
    /// Camera profile ID - changes trigger regeneration (per development-plan.md)
    pub camera_profile_id: Option<i64>,
    /// Source file hash - changes trigger regeneration (per development-plan.md)
    pub source_hash: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl DerivedParams {
    /// Create params for proxy generation.
    /// camera_profile_id and source_hash are included to trigger regeneration when they change.
    pub fn for_proxy(
        deinterlace: bool,
        lut_id: Option<i64>,
        camera_profile_id: Option<i64>,
        source_hash: Option<String>,
    ) -> Self {
        Self {
            pipeline_version: PIPELINE_VERSION,
            preset: "proxy_720p".to_string(),
            camera_profile_id,
            source_hash,
            extra: serde_json::json!({
                "codec": PROXY_CODEC,
                "resolution": PROXY_RESOLUTION,
                "crf": PROXY_CRF,
                "deinterlace": deinterlace,
                "lut_id": lut_id,
            }),
        }
    }

    /// Create params for thumbnail generation.
    pub fn for_thumb(camera_profile_id: Option<i64>, source_hash: Option<String>) -> Self {
        Self {
            pipeline_version: PIPELINE_VERSION,
            preset: "thumb_480".to_string(),
            camera_profile_id,
            source_hash,
            extra: serde_json::json!({
                "format": THUMB_FORMAT,
                "quality": THUMB_QUALITY,
                "max_width": 480,
            }),
        }
    }

    /// Create params for sprite generation.
    pub fn for_sprite(
        duration_ms: i64,
        camera_profile_id: Option<i64>,
        source_hash: Option<String>,
    ) -> Self {
        // Calculate frame count (1 fps, max 120 frames)
        let duration_secs = duration_ms / 1000;
        let frame_count = duration_secs.min(120).max(1);

        Self {
            pipeline_version: PIPELINE_VERSION,
            preset: "sprite_160".to_string(),
            camera_profile_id,
            source_hash,
            extra: serde_json::json!({
                "fps": SPRITE_FPS,
                "tile_width": SPRITE_TILE_WIDTH,
                "frame_count": frame_count,
            }),
        }
    }

    /// Compute a hash of these params for comparison.
    pub fn hash(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        let hash = blake3::hash(json.as_bytes());
        hash.to_hex()[..16].to_string() // Short hash for filename
    }

    /// Convert to JSON string for storage.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Get the output path for a derived asset.
pub fn get_derived_path(
    library_root: &Path,
    clip_id: i64,
    role: &str,
    params: &DerivedParams,
    extension: &str,
) -> PathBuf {
    let subdir = match role {
        "proxy" => "proxies",
        "thumb" => "thumbs",
        "sprite" => "sprites",
        _ => "derived",
    };

    let params_hash = params.hash();
    let filename = format!("{}_{}.{}", clip_id, params_hash, extension);

    library_root
        .join(DADCAM_FOLDER)
        .join(subdir)
        .join(filename)
}

/// Convert a library-absolute path to a relative path for DB storage.
pub fn to_relative_path(library_root: &Path, absolute_path: &Path) -> String {
    absolute_path
        .strip_prefix(library_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| absolute_path.to_string_lossy().to_string())
}

/// Check if a derived asset is stale and needs regeneration.
/// Takes the clip to check camera_profile_id, and optionally the source asset hash.
pub fn is_asset_stale(
    asset: &Asset,
    current_params: &DerivedParams,
    source_hash: Option<&str>,
) -> bool {
    // Check pipeline version
    if let Some(stored_version) = asset.pipeline_version {
        if (stored_version as u32) < current_params.pipeline_version {
            return true; // Older version
        }
    } else {
        return true; // No version stored
    }

    // Check params hash (includes camera_profile_id, lut_id, etc.)
    if let Some(ref stored_params) = asset.derived_params {
        match serde_json::from_str::<DerivedParams>(stored_params) {
            Ok(stored) => {
                if stored.hash() != current_params.hash() {
                    return true; // Params changed
                }
                // Check source hash if provided (for source file changes)
                if let Some(src_hash) = source_hash {
                    if let Some(ref stored_src) = stored.source_hash {
                        if stored_src != src_hash {
                            return true; // Source file changed
                        }
                    }
                }
            }
            Err(_) => return true, // Can't parse stored params
        }
    } else {
        return true; // No params stored
    }

    false // Not stale
}

/// Find existing derived asset for a clip.
pub fn find_derived_asset(
    conn: &Connection,
    clip_id: i64,
    role: &str,
) -> Result<Option<Asset>> {
    let result = conn.query_row(
        r#"SELECT a.id, a.library_id, a.type, a.path, a.source_uri, a.size_bytes,
                  a.hash_fast, a.hash_fast_scheme, a.hash_full, a.verified_at,
                  a.pipeline_version, a.derived_params, a.created_at
           FROM assets a
           JOIN clip_assets ca ON ca.asset_id = a.id
           WHERE ca.clip_id = ?1 AND ca.role = ?2"#,
        params![clip_id, role],
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

/// Create a derived asset record.
pub fn create_derived_asset(
    conn: &Connection,
    library_id: i64,
    asset_type: &str,
    path: &str,
    size_bytes: i64,
    pipeline_version: u32,
    derived_params: &str,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO assets
           (library_id, type, path, size_bytes, pipeline_version, derived_params)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
        params![
            library_id,
            asset_type,
            path,
            size_bytes,
            pipeline_version as i32,
            derived_params,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update an existing derived asset.
pub fn update_derived_asset(
    conn: &Connection,
    asset_id: i64,
    path: &str,
    size_bytes: i64,
    pipeline_version: u32,
    derived_params: &str,
) -> Result<()> {
    conn.execute(
        r#"UPDATE assets
           SET path = ?2, size_bytes = ?3, pipeline_version = ?4, derived_params = ?5
           WHERE id = ?1"#,
        params![
            asset_id,
            path,
            size_bytes,
            pipeline_version as i32,
            derived_params,
        ],
    )?;
    Ok(())
}

/// Get clips that need preview generation.
pub fn get_clips_needing_previews(
    conn: &Connection,
    library_id: i64,
    role: &str,
    limit: i64,
) -> Result<Vec<crate::db::schema::Clip>> {
    use crate::db::schema::Clip;

    // Find clips that don't have the specified derived asset
    let mut stmt = conn.prepare(
        r#"SELECT c.id, c.library_id, c.original_asset_id, c.camera_profile_id,
                  c.media_type, c.title, c.duration_ms, c.width, c.height, c.fps,
                  c.codec, c.audio_codec, c.audio_channels, c.audio_sample_rate,
                  c.recorded_at, c.recorded_at_offset_minutes,
                  c.recorded_at_is_estimated, c.timestamp_source, c.source_folder, c.created_at
           FROM clips c
           WHERE c.library_id = ?1
           AND NOT EXISTS (
               SELECT 1 FROM clip_assets ca
               WHERE ca.clip_id = c.id AND ca.role = ?2
           )
           ORDER BY c.created_at DESC
           LIMIT ?3"#
    )?;

    let clips = stmt.query_map(params![library_id, role, limit], |row| {
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

/// Get the original asset for a clip.
pub fn get_clip_original_asset(conn: &Connection, clip_id: i64) -> Result<Option<Asset>> {
    let result = conn.query_row(
        r#"SELECT a.id, a.library_id, a.type, a.path, a.source_uri, a.size_bytes,
                  a.hash_fast, a.hash_fast_scheme, a.hash_full, a.verified_at,
                  a.pipeline_version, a.derived_params, a.created_at
           FROM assets a
           JOIN clips c ON c.original_asset_id = a.id
           WHERE c.id = ?1"#,
        params![clip_id],
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

/// Queue preview generation jobs for a clip.
pub fn queue_preview_jobs(conn: &Connection, clip_id: i64, library_id: i64) -> Result<()> {
    use crate::db::schema::{insert_job, NewJob};

    // Queue thumbnail job (higher priority - needed for UI)
    insert_job(conn, &NewJob {
        job_type: "thumb".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 8, // High priority
        payload: "{}".to_string(),
    })?;

    // Queue proxy job
    insert_job(conn, &NewJob {
        job_type: "proxy".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 5, // Medium priority
        payload: "{}".to_string(),
    })?;

    // Queue sprite job (lower priority)
    insert_job(conn, &NewJob {
        job_type: "sprite".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 3, // Lower priority
        payload: "{}".to_string(),
    })?;

    Ok(())
}
