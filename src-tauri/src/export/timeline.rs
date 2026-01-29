// Dad Cam - VHS Export Timeline
// Clip selection queries by mode (date_range, event, favorites, score, all)
// and ordering (chronological, score_desc, score_asc, shuffle).

use rusqlite::{params, Connection};
use rand::seq::SliceRandom;
use rand::SeedableRng;

use crate::error::Result;
use super::{ExportClip, VhsExportParams};

/// Select clips for export based on selection mode and ordering.
pub fn select_clips(
    conn: &Connection,
    library_id: i64,
    params: &VhsExportParams,
) -> Result<Vec<ExportClip>> {
    let mode = params.selection_mode.as_str();
    let sel = &params.selection_params;

    let mut clips = match mode {
        "date_range" => select_by_date_range(conn, library_id, sel)?,
        "event" => select_by_event(conn, sel)?,
        "favorites" => select_favorites(conn, library_id)?,
        "score" => {
            let threshold = sel.get("threshold")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.6);
            select_by_score(conn, library_id, threshold)?
        }
        "all" | _ => select_all(conn, library_id)?,
    };

    // Apply ordering
    match params.ordering.as_str() {
        "score_desc" => order_by_score(conn, &mut clips, false),
        "score_asc" => order_by_score(conn, &mut clips, true),
        "shuffle" => {
            let seed = sel.get("seed")
                .and_then(|v| v.as_u64())
                .unwrap_or(42);
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            clips.shuffle(&mut rng);
        }
        "chronological" | _ => {
            // Already ordered by recorded_at ASC from queries
        }
    }

    Ok(clips)
}

/// Select clips by date range
fn select_by_date_range(
    conn: &Connection,
    library_id: i64,
    sel: &serde_json::Value,
) -> Result<Vec<ExportClip>> {
    let date_from = sel.get("dateFrom").and_then(|v| v.as_str()).unwrap_or("");
    let date_to = sel.get("dateTo").and_then(|v| v.as_str()).unwrap_or("");

    let mut stmt = conn.prepare(
        "SELECT c.id, c.duration_ms, c.audio_codec,
                COALESCE(
                    (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
                     WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1),
                    (SELECT a.path FROM assets a WHERE a.id = c.original_asset_id)
                ) as clip_path
         FROM clips c
         WHERE c.library_id = ?1
           AND c.media_type = 'video'
           AND date(c.recorded_at) BETWEEN date(?2) AND date(?3)
         ORDER BY c.recorded_at ASC"
    )?;

    let clips = stmt.query_map(params![library_id, date_from, date_to], map_clip)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(clips)
}

/// Select clips belonging to an event
fn select_by_event(
    conn: &Connection,
    sel: &serde_json::Value,
) -> Result<Vec<ExportClip>> {
    let event_id = sel.get("eventId")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let mut stmt = conn.prepare(
        "SELECT c.id, c.duration_ms, c.audio_codec,
                COALESCE(
                    (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
                     WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1),
                    (SELECT a.path FROM assets a WHERE a.id = c.original_asset_id)
                ) as clip_path
         FROM clips c
         JOIN event_clips ec ON ec.clip_id = c.id
         WHERE ec.event_id = ?1
           AND c.media_type = 'video'
         ORDER BY c.recorded_at ASC"
    )?;

    let clips = stmt.query_map(params![event_id], map_clip)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(clips)
}

/// Select favorite clips
fn select_favorites(conn: &Connection, library_id: i64) -> Result<Vec<ExportClip>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.duration_ms, c.audio_codec,
                COALESCE(
                    (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
                     WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1),
                    (SELECT a.path FROM assets a WHERE a.id = c.original_asset_id)
                ) as clip_path
         FROM clips c
         JOIN clip_tags ct ON ct.clip_id = c.id
         JOIN tags t ON t.id = ct.tag_id AND t.name = 'favorite'
         WHERE c.library_id = ?1
           AND c.media_type = 'video'
         ORDER BY c.recorded_at ASC"
    )?;

    let clips = stmt.query_map(params![library_id], map_clip)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(clips)
}

/// Select clips above a score threshold
fn select_by_score(conn: &Connection, library_id: i64, threshold: f64) -> Result<Vec<ExportClip>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.duration_ms, c.audio_codec,
                COALESCE(
                    (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
                     WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1),
                    (SELECT a.path FROM assets a WHERE a.id = c.original_asset_id)
                ) as clip_path
         FROM clips c
         JOIN clip_scores cs ON cs.clip_id = c.id
         WHERE c.library_id = ?1
           AND c.media_type = 'video'
           AND cs.overall_score >= ?2
         ORDER BY c.recorded_at ASC"
    )?;

    let clips = stmt.query_map(params![library_id, threshold], map_clip)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(clips)
}

/// Select all video clips
fn select_all(conn: &Connection, library_id: i64) -> Result<Vec<ExportClip>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.duration_ms, c.audio_codec,
                COALESCE(
                    (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
                     WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1),
                    (SELECT a.path FROM assets a WHERE a.id = c.original_asset_id)
                ) as clip_path
         FROM clips c
         WHERE c.library_id = ?1
           AND c.media_type = 'video'
         ORDER BY c.recorded_at ASC"
    )?;

    let clips = stmt.query_map(params![library_id], map_clip)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(clips)
}

/// Map a row to ExportClip
fn map_clip(row: &rusqlite::Row) -> rusqlite::Result<ExportClip> {
    let audio_codec: Option<String> = row.get(2)?;
    Ok(ExportClip {
        clip_id: row.get(0)?,
        duration_ms: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
        has_audio: audio_codec.is_some(),
        path: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
    })
}

/// Sort clips in-place by their score (fetched from DB).
/// ascending=false means highest score first.
fn order_by_score(conn: &Connection, clips: &mut [ExportClip], ascending: bool) {
    // Build a map of clip_id -> score
    let ids: Vec<i64> = clips.iter().map(|c| c.clip_id).collect();
    let mut scores = std::collections::HashMap::new();

    for id in &ids {
        if let Ok(score) = conn.query_row(
            "SELECT overall_score FROM clip_scores WHERE clip_id = ?1",
            params![id],
            |row| row.get::<_, f64>(0),
        ) {
            scores.insert(*id, score);
        }
    }

    clips.sort_by(|a, b| {
        let sa = scores.get(&a.clip_id).copied().unwrap_or(0.0);
        let sb = scores.get(&b.clip_id).copied().unwrap_or(0.0);
        if ascending {
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
        }
    });
}
