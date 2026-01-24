// Phase 4: Main scoring analyzer
// Entry point for clip analysis and score computation

use std::path::Path;
use anyhow::Result;
use rusqlite::Connection;

use crate::db::schema;
use crate::constants::{PIPELINE_VERSION, R_NON_VIDEO, R_UNAVAILABLE};
use crate::preview;
use super::{ScoringResult, scene, audio, sharpness, motion};

/// Get the best available video path for scoring (proxy preferred, fallback to original)
fn get_scoring_video_path(conn: &Connection, clip_id: i64, original_asset_id: i64, library_root: &Path) -> Result<(std::path::PathBuf, bool)> {
    // Try to use proxy first (faster to analyze, already processed)
    if let Ok(Some(proxy_asset)) = preview::find_derived_asset(conn, clip_id, "proxy") {
        let proxy_path = library_root.join(&proxy_asset.path);
        if proxy_path.exists() {
            return Ok((proxy_path, true));
        }
    }

    // Fall back to original asset
    let asset = schema::get_asset(conn, original_asset_id)?
        .ok_or_else(|| anyhow::anyhow!("Asset for clip {} not found", clip_id))?;

    let original_path = library_root.join(&asset.path);
    if !original_path.exists() {
        anyhow::bail!("Video file not found: {}", original_path.display());
    }

    Ok((original_path, false))
}

/// Analyze a clip and compute its scores
pub fn analyze_clip(conn: &Connection, clip_id: i64, library_root: &Path, verbose: bool) -> Result<ScoringResult> {
    let mut result = ScoringResult::new(clip_id);

    // Get the clip
    let clip = schema::get_clip(conn, clip_id)?
        .ok_or_else(|| anyhow::anyhow!("Clip {} not found", clip_id))?;

    // Get best available video path (proxy preferred)
    let (video_path, using_proxy) = get_scoring_video_path(conn, clip_id, clip.original_asset_id, library_root)?;

    if verbose {
        eprintln!("Analyzing clip {}: {}", clip_id, clip.title);
        if using_proxy {
            eprintln!("  Using proxy for analysis");
        }
    }

    // Skip non-video content (audio, images get neutral scores)
    if clip.media_type != "video" {
        if verbose {
            eprintln!("  Skipping non-video media type: {}", clip.media_type);
        }
        result.scene_score = 0.5;
        result.audio_score = 0.5;
        result.sharpness_score = 0.5;
        result.motion_score = 0.5;
        result.add_reason(R_NON_VIDEO);
        result.compute_overall();
        return Ok(result);
    }

    // Get duration for analysis parameters
    let duration_ms = clip.duration_ms.unwrap_or(0);

    // 1. Scene analysis
    match scene::analyze_scenes(&video_path, duration_ms, verbose) {
        Ok((score, reason)) => {
            result.scene_score = score;
            if let Some(r) = reason {
                result.add_reason(&r);
            }
        }
        Err(e) => {
            if verbose {
                eprintln!("  Scene analysis failed: {}", e);
            }
            result.scene_score = 0.5;
            result.add_reason(R_UNAVAILABLE);
        }
    }

    // 2. Audio analysis
    match audio::analyze_audio(&video_path, duration_ms, verbose) {
        Ok((score, reason)) => {
            result.audio_score = score;
            if let Some(r) = reason {
                result.add_reason(&r);
            }
        }
        Err(e) => {
            if verbose {
                eprintln!("  Audio analysis failed: {}", e);
            }
            result.audio_score = 0.5;
            result.add_reason(R_UNAVAILABLE);
        }
    }

    // 3. Sharpness analysis
    match sharpness::analyze_sharpness(&video_path, duration_ms, verbose) {
        Ok((score, reason)) => {
            result.sharpness_score = score;
            if let Some(r) = reason {
                result.add_reason(&r);
            }
        }
        Err(e) => {
            if verbose {
                eprintln!("  Sharpness analysis failed: {}", e);
            }
            result.sharpness_score = 0.5;
            result.add_reason(R_UNAVAILABLE);
        }
    }

    // 4. Motion analysis
    match motion::analyze_motion(&video_path, duration_ms, verbose) {
        Ok((score, reason)) => {
            result.motion_score = score;
            if let Some(r) = reason {
                result.add_reason(&r);
            }
        }
        Err(e) => {
            if verbose {
                eprintln!("  Motion analysis failed: {}", e);
            }
            result.motion_score = 0.5;
            result.add_reason(R_UNAVAILABLE);
        }
    }

    // Compute overall score
    result.compute_overall();

    if verbose {
        eprintln!("  Scene: {:.2}, Audio: {:.2}, Sharpness: {:.2}, Motion: {:.2}",
            result.scene_score, result.audio_score, result.sharpness_score, result.motion_score);
        eprintln!("  Overall: {:.2}", result.overall_score);
    }

    Ok(result)
}

/// Check if a clip needs scoring (no score or outdated version)
pub fn needs_scoring(conn: &Connection, clip_id: i64) -> Result<bool> {
    let existing = get_clip_score(conn, clip_id)?;

    match existing {
        None => Ok(true),
        Some(score) => {
            // Rescore if pipeline or scoring version changed
            Ok(score.pipeline_version != PIPELINE_VERSION
                || score.scoring_version != crate::constants::SCORING_VERSION)
        }
    }
}

/// Get existing score for a clip
pub fn get_clip_score(conn: &Connection, clip_id: i64) -> Result<Option<super::ClipScore>> {
    use rusqlite::OptionalExtension;

    let result = conn.query_row(
        "SELECT id, clip_id, overall_score, scene_score, audio_score, sharpness_score, motion_score,
                reasons, pipeline_version, scoring_version, created_at, updated_at
         FROM clip_scores WHERE clip_id = ?1",
        [clip_id],
        |row| {
            let reasons_json: String = row.get(7)?;
            let reasons: Vec<String> = serde_json::from_str(&reasons_json).unwrap_or_default();

            Ok(super::ClipScore {
                id: row.get(0)?,
                clip_id: row.get(1)?,
                overall_score: row.get(2)?,
                scene_score: row.get(3)?,
                audio_score: row.get(4)?,
                sharpness_score: row.get(5)?,
                motion_score: row.get(6)?,
                reasons,
                pipeline_version: row.get(8)?,
                scoring_version: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        },
    ).optional()?;

    Ok(result)
}

/// Save or update a clip score
pub fn save_clip_score(conn: &Connection, result: &ScoringResult) -> Result<i64> {
    let reasons_json = serde_json::to_string(&result.reasons)?;

    conn.execute(
        "INSERT INTO clip_scores (clip_id, overall_score, scene_score, audio_score, sharpness_score,
                                  motion_score, reasons, pipeline_version, scoring_version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(clip_id) DO UPDATE SET
            overall_score = excluded.overall_score,
            scene_score = excluded.scene_score,
            audio_score = excluded.audio_score,
            sharpness_score = excluded.sharpness_score,
            motion_score = excluded.motion_score,
            reasons = excluded.reasons,
            pipeline_version = excluded.pipeline_version,
            scoring_version = excluded.scoring_version,
            updated_at = datetime('now')",
        rusqlite::params![
            result.clip_id,
            result.overall_score,
            result.scene_score,
            result.audio_score,
            result.sharpness_score,
            result.motion_score,
            reasons_json,
            result.pipeline_version,
            result.scoring_version,
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get clips that need scoring
pub fn get_clips_needing_scores(conn: &Connection, library_id: i64, limit: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT c.id FROM clips c
         LEFT JOIN clip_scores cs ON cs.clip_id = c.id
         WHERE c.library_id = ?1
           AND (cs.id IS NULL
                OR cs.pipeline_version != ?2
                OR cs.scoring_version != ?3)
         LIMIT ?4"
    )?;

    let ids = stmt.query_map(
        rusqlite::params![library_id, PIPELINE_VERSION, crate::constants::SCORING_VERSION, limit],
        |row| row.get(0),
    )?.collect::<std::result::Result<Vec<i64>, _>>()?;

    Ok(ids)
}

/// Get best clips above threshold
/// Pinned and promoted clips always appear regardless of threshold (per Phase 4 spec 10.8)
pub fn get_best_clips(conn: &Connection, library_id: i64, threshold: f64, limit: i64) -> Result<Vec<(i64, f64)>> {
    // This query applies overrides to compute effective scores
    // Per Phase 4 spec 10.8:
    // - Pinned clips always appear (override = pin)
    // - Promoted clips get a bump (add override_value)
    // - Demoted clips are treated as 0 effective score (still included if they meet threshold)
    // - Sort order: pinned/promoted first, then by effective score, then by recorded_at
    let mut stmt = conn.prepare(
        r#"SELECT c.id,
           CASE
               WHEN o.override_type = 'pin' THEN o.override_value
               WHEN o.override_type = 'promote' THEN MIN(1.0, cs.overall_score + o.override_value)
               WHEN o.override_type = 'demote' THEN MAX(0.0, cs.overall_score - o.override_value)
               ELSE cs.overall_score
           END as effective_score,
           CASE
               WHEN o.override_type = 'pin' THEN 2
               WHEN o.override_type = 'promote' THEN 1
               ELSE 0
           END as priority
         FROM clips c
         JOIN clip_scores cs ON cs.clip_id = c.id
         LEFT JOIN clip_score_overrides o ON o.clip_id = c.id
         WHERE c.library_id = ?1
           AND (
               o.override_type IN ('pin', 'promote')
               OR (
                   CASE
                       WHEN o.override_type = 'demote' THEN MAX(0.0, cs.overall_score - o.override_value)
                       ELSE cs.overall_score
                   END >= ?2
               )
           )
         ORDER BY priority DESC, effective_score DESC, c.recorded_at DESC
         LIMIT ?3"#
    )?;

    let results = stmt.query_map(
        rusqlite::params![library_id, threshold, limit],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?.collect::<std::result::Result<Vec<(i64, f64)>, _>>()?;

    Ok(results)
}
