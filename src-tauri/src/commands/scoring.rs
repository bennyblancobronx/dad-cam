// Dad Cam - Phase 4 Scoring Commands
// Commands for clip scoring, overrides, and best clips

use std::path::PathBuf;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::db::{open_db, get_db_path};
use crate::db::schema;
use crate::scoring;
use crate::constants;
use super::DbState;

/// Score information for a clip
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipScoreResponse {
    pub clip_id: i64,
    pub overall_score: f64,
    pub scene_score: f64,
    pub audio_score: f64,
    pub sharpness_score: f64,
    pub motion_score: f64,
    pub reasons: Vec<String>,
    pub effective_score: f64,
    pub has_override: bool,
    pub override_type: Option<String>,
    pub override_value: Option<f64>,
}

/// Score override request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreOverrideRequest {
    pub clip_id: i64,
    pub override_type: String,  // "promote", "demote", "pin", "clear"
    pub value: Option<f64>,
    pub note: Option<String>,
}

/// Scoring status response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoringStatusResponse {
    pub total_clips: i64,
    pub scored_clips: i64,
    pub missing_scores: i64,
    pub outdated_scores: i64,
    pub user_overrides: i64,
}

/// Best clips query parameters
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BestClipsQuery {
    pub threshold: Option<f64>,
    pub limit: Option<i64>,
}

/// Best clip entry
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BestClipEntry {
    pub clip_id: i64,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub effective_score: f64,
    pub thumb_path: Option<String>,
}

/// Get score for a specific clip
#[tauri::command]
pub fn get_clip_score(state: State<DbState>, clip_id: i64) -> Result<Option<ClipScoreResponse>, String> {
    let conn = state.connect()?;

    let score = scoring::analyzer::get_clip_score(&conn, clip_id)
        .map_err(|e| e.to_string())?;

    let Some(score) = score else {
        return Ok(None);
    };

    // Check for override
    let override_info: Option<(String, f64)> = conn.query_row(
        "SELECT override_type, override_value FROM clip_score_overrides WHERE clip_id = ?1",
        [clip_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).ok();

    let (has_override, override_type, override_value, effective_score) = if let Some((otype, oval)) = override_info {
        let effective = match otype.as_str() {
            "pin" => oval,
            "promote" => (score.overall_score + oval).min(1.0),
            "demote" => (score.overall_score - oval).max(0.0),
            _ => score.overall_score,
        };
        (true, Some(otype), Some(oval), effective)
    } else {
        (false, None, None, score.overall_score)
    };

    Ok(Some(ClipScoreResponse {
        clip_id: score.clip_id,
        overall_score: score.overall_score,
        scene_score: score.scene_score,
        audio_score: score.audio_score,
        sharpness_score: score.sharpness_score,
        motion_score: score.motion_score,
        reasons: score.reasons,
        effective_score,
        has_override,
        override_type,
        override_value,
    }))
}

/// Score a specific clip
#[tauri::command]
pub fn score_clip(state: State<DbState>, library_path: String, clip_id: i64, force: bool) -> Result<ClipScoreResponse, String> {
    let library_root = PathBuf::from(&library_path);
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path).map_err(|e| e.to_string())?;

    // Check if already scored (unless force)
    if !force {
        if let Ok(false) = scoring::analyzer::needs_scoring(&conn, clip_id) {
            // Return existing score
            if let Some(existing) = scoring::analyzer::get_clip_score(&conn, clip_id)
                .map_err(|e| e.to_string())?
            {
                return Ok(ClipScoreResponse {
                    clip_id: existing.clip_id,
                    overall_score: existing.overall_score,
                    scene_score: existing.scene_score,
                    audio_score: existing.audio_score,
                    sharpness_score: existing.sharpness_score,
                    motion_score: existing.motion_score,
                    reasons: existing.reasons,
                    effective_score: existing.overall_score,
                    has_override: false,
                    override_type: None,
                    override_value: None,
                });
            }
        }
    }

    // Run analysis
    let result = scoring::analyzer::analyze_clip(&conn, clip_id, &library_root, false)
        .map_err(|e| e.to_string())?;

    // Save the score
    scoring::analyzer::save_clip_score(&conn, &result)
        .map_err(|e| e.to_string())?;

    Ok(ClipScoreResponse {
        clip_id: result.clip_id,
        overall_score: result.overall_score,
        scene_score: result.scene_score,
        audio_score: result.audio_score,
        sharpness_score: result.sharpness_score,
        motion_score: result.motion_score,
        reasons: result.reasons,
        effective_score: result.overall_score,
        has_override: false,
        override_type: None,
        override_value: None,
    })
}

/// Get scoring status for the library
#[tauri::command]
pub fn get_scoring_status(state: State<DbState>) -> Result<ScoringStatusResponse, String> {
    let conn = state.connect()?;

    // Get library ID from first library (single library mode)
    let library_id: i64 = conn.query_row(
        "SELECT id FROM libraries LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    let total_clips = schema::count_clips(&conn, library_id)
        .map_err(|e| e.to_string())?;

    let scored_clips: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT cs.clip_id) FROM clip_scores cs
         JOIN clips c ON c.id = cs.clip_id WHERE c.library_id = ?1",
        [library_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    let outdated_scores: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_scores cs
         JOIN clips c ON c.id = cs.clip_id
         WHERE c.library_id = ?1 AND (cs.pipeline_version != ?2 OR cs.scoring_version != ?3)",
        rusqlite::params![library_id, constants::PIPELINE_VERSION, constants::SCORING_VERSION],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    let user_overrides: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_score_overrides o
         JOIN clips c ON c.id = o.clip_id WHERE c.library_id = ?1",
        [library_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    Ok(ScoringStatusResponse {
        total_clips,
        scored_clips,
        missing_scores: total_clips - scored_clips,
        outdated_scores,
        user_overrides,
    })
}

/// Get best clips above threshold
#[tauri::command]
pub fn get_best_clips(state: State<DbState>, query: BestClipsQuery) -> Result<Vec<BestClipEntry>, String> {
    let conn = state.connect()?;

    let threshold = query.threshold.unwrap_or(constants::BEST_CLIPS_THRESHOLD);
    let limit = query.limit.unwrap_or(20);

    // Get library ID
    let library_id: i64 = conn.query_row(
        "SELECT id FROM libraries LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    let best = scoring::analyzer::get_best_clips(&conn, library_id, threshold, limit)
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for (clip_id, effective_score) in best {
        if let Some(clip) = schema::get_clip(&conn, clip_id).map_err(|e| e.to_string())? {
            // Get thumb path
            let thumb_path: Option<String> = conn.query_row(
                "SELECT a.path FROM assets a
                 JOIN clip_assets ca ON ca.asset_id = a.id
                 WHERE ca.clip_id = ?1 AND ca.role = 'thumb'",
                [clip_id],
                |row| row.get(0),
            ).ok();

            result.push(BestClipEntry {
                clip_id,
                title: clip.title,
                duration_ms: clip.duration_ms,
                effective_score,
                thumb_path,
            });
        }
    }

    Ok(result)
}

/// Set a score override for a clip
#[tauri::command]
pub fn set_score_override(state: State<DbState>, request: ScoreOverrideRequest) -> Result<ClipScoreResponse, String> {
    let clip_id = request.clip_id;

    {
        let conn = state.connect()?;

        // Verify clip exists
        let _clip = schema::get_clip(&conn, clip_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Clip {} not found", clip_id))?;

        match request.override_type.as_str() {
            "promote" => {
                let adj = request.value.unwrap_or(constants::SCORE_PROMOTE_DEFAULT);
                conn.execute(
                    "INSERT INTO clip_score_overrides (clip_id, override_type, override_value, note)
                     VALUES (?1, 'promote', ?2, ?3)
                     ON CONFLICT(clip_id) DO UPDATE SET
                        override_type = 'promote',
                        override_value = excluded.override_value,
                        note = excluded.note,
                        updated_at = datetime('now')",
                    rusqlite::params![clip_id, adj, request.note],
                ).map_err(|e| e.to_string())?;
            }
            "demote" => {
                let adj = request.value.unwrap_or(constants::SCORE_DEMOTE_DEFAULT);
                conn.execute(
                    "INSERT INTO clip_score_overrides (clip_id, override_type, override_value, note)
                     VALUES (?1, 'demote', ?2, ?3)
                     ON CONFLICT(clip_id) DO UPDATE SET
                        override_type = 'demote',
                        override_value = excluded.override_value,
                        note = excluded.note,
                        updated_at = datetime('now')",
                    rusqlite::params![clip_id, adj, request.note],
                ).map_err(|e| e.to_string())?;
            }
            "pin" => {
                let pin_value = request.value.ok_or("Pin requires a value between 0.0 and 1.0")?;
                conn.execute(
                    "INSERT INTO clip_score_overrides (clip_id, override_type, override_value, note)
                     VALUES (?1, 'pin', ?2, ?3)
                     ON CONFLICT(clip_id) DO UPDATE SET
                        override_type = 'pin',
                        override_value = excluded.override_value,
                        note = excluded.note,
                        updated_at = datetime('now')",
                    rusqlite::params![clip_id, pin_value, request.note],
                ).map_err(|e| e.to_string())?;
            }
            "clear" => {
                conn.execute(
                    "DELETE FROM clip_score_overrides WHERE clip_id = ?1",
                    [clip_id],
                ).map_err(|e| e.to_string())?;
            }
            _ => {
                return Err(format!("Unknown override type: {}", request.override_type));
            }
        }
    } // db lock released here

    // Return updated score
    get_clip_score(state, clip_id)?
        .ok_or_else(|| "Clip has no score yet".to_string())
}

/// Clear a score override for a clip
#[tauri::command]
pub fn clear_score_override(state: State<DbState>, clip_id: i64) -> Result<bool, String> {
    let conn = state.connect()?;

    let deleted = conn.execute(
        "DELETE FROM clip_score_overrides WHERE clip_id = ?1",
        [clip_id],
    ).map_err(|e| e.to_string())?;

    Ok(deleted > 0)
}

/// Queue scoring jobs for all clips needing scores
#[tauri::command]
pub fn queue_scoring_jobs(state: State<DbState>) -> Result<i64, String> {
    let conn = state.connect()?;

    // Get library ID
    let library_id: i64 = conn.query_row(
        "SELECT id FROM libraries LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    let clips_needing_scores = scoring::analyzer::get_clips_needing_scores(&conn, library_id, 1000)
        .map_err(|e| e.to_string())?;

    let mut queued = 0;
    for clip_id in clips_needing_scores {
        schema::insert_job(&conn, &schema::NewJob {
            job_type: "score".to_string(),
            library_id: Some(library_id),
            clip_id: Some(clip_id),
            asset_id: None,
            priority: 2,
            payload: "{}".to_string(),
        }).map_err(|e| e.to_string())?;
        queued += 1;
    }

    Ok(queued)
}
