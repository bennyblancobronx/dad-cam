// Dad Cam - Phase 3 Clip Commands
// Commands for clip listing, filtering, and retrieval

use tauri::State;
use serde::{Deserialize, Serialize};

use crate::db::schema::{self, Clip, Library};
use super::DbState;

/// Basic clip response (for backward compatibility)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipResponse {
    pub id: i64,
    pub library_id: i64,
    pub title: String,
    pub media_type: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub recorded_at: Option<String>,
    pub source_folder: Option<String>,
    pub created_at: String,
    pub is_favorite: bool,
    pub is_bad: bool,
}

/// Enhanced clip view with asset paths for Phase 3 UI
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipView {
    pub id: i64,
    pub title: String,
    pub media_type: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub recorded_at: Option<String>,
    pub thumb_path: Option<String>,
    pub proxy_path: Option<String>,
    pub sprite_path: Option<String>,
    pub is_favorite: bool,
    pub is_bad: bool,
}

/// Query parameters for clip listing with filters
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipQuery {
    pub offset: i64,
    pub limit: i64,
    pub filter: Option<String>,       // "all", "favorites", "bad", "unreviewed"
    pub search: Option<String>,       // filename search
    pub date_from: Option<String>,    // ISO date string
    pub date_to: Option<String>,      // ISO date string
    pub sort_by: Option<String>,      // "recorded_at", "title", "created_at"
    pub sort_order: Option<String>,   // "asc", "desc"
}

/// Paginated response for clips
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipListResponse {
    pub clips: Vec<ClipView>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

// Convert Clip to ClipResponse
fn clip_to_response(conn: &rusqlite::Connection, clip: Clip) -> ClipResponse {
    let is_favorite = schema::has_clip_tag(conn, clip.id, "favorite").unwrap_or(false);
    let is_bad = schema::has_clip_tag(conn, clip.id, "bad").unwrap_or(false);

    ClipResponse {
        id: clip.id,
        library_id: clip.library_id,
        title: clip.title,
        media_type: clip.media_type,
        duration_ms: clip.duration_ms,
        width: clip.width,
        height: clip.height,
        fps: clip.fps,
        codec: clip.codec,
        recorded_at: clip.recorded_at,
        source_folder: clip.source_folder,
        created_at: clip.created_at,
        is_favorite,
        is_bad,
    }
}

/// Get clips with basic pagination (backward compatible)
#[tauri::command]
pub fn get_clips(state: State<DbState>, limit: i64, offset: i64) -> Result<Vec<ClipResponse>, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    // Get library ID (assuming single library for now)
    let lib: Library = conn.query_row(
        "SELECT id, root_path, name, ingest_mode, created_at, settings FROM libraries LIMIT 1",
        [],
        |row| Ok(Library {
            id: row.get(0)?,
            root_path: row.get(1)?,
            name: row.get(2)?,
            ingest_mode: row.get(3)?,
            created_at: row.get(4)?,
            settings: row.get(5)?,
        }),
    ).map_err(|e| e.to_string())?;

    let clips = schema::list_clips(conn, lib.id, limit, offset).map_err(|e| e.to_string())?;

    let responses: Vec<ClipResponse> = clips
        .into_iter()
        .map(|c| clip_to_response(conn, c))
        .collect();

    Ok(responses)
}

/// Get a single clip by ID (backward compatible)
#[tauri::command]
pub fn get_clip(state: State<DbState>, id: i64) -> Result<ClipResponse, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let clip = schema::get_clip(conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Clip {} not found", id))?;

    Ok(clip_to_response(conn, clip))
}

/// Enhanced clip query with filtering, sorting, and asset paths for Phase 3 UI
#[tauri::command]
pub fn get_clips_filtered(state: State<DbState>, query: ClipQuery) -> Result<ClipListResponse, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    // Get library ID
    let library_id: i64 = conn.query_row(
        "SELECT id FROM libraries LIMIT 1",
        [],
        |row| row.get(0),
    ).map_err(|e| format!("No library: {}", e))?;

    // Build WHERE clause
    let mut conditions = vec!["c.library_id = ?1".to_string()];
    let mut param_idx = 2;

    // Filter handling
    let filter_sql = match query.filter.as_deref() {
        Some("favorites") => Some(
            "EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = c.id AND t.name = 'favorite')".to_string()
        ),
        Some("bad") => Some(
            "EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = c.id AND t.name = 'bad')".to_string()
        ),
        Some("unreviewed") => Some(
            "NOT EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = c.id AND t.name IN ('favorite', 'bad'))".to_string()
        ),
        _ => None,
    };
    if let Some(sql) = filter_sql {
        conditions.push(sql);
    }

    // Search filter
    let search_pattern = query.search.as_ref().map(|s| format!("%{}%", s));
    if search_pattern.is_some() {
        conditions.push(format!("c.title LIKE ?{}", param_idx));
        param_idx += 1;
    }

    // Date range filters
    if query.date_from.is_some() {
        conditions.push(format!("c.recorded_at >= ?{}", param_idx));
        param_idx += 1;
    }
    if query.date_to.is_some() {
        conditions.push(format!("c.recorded_at <= ?{}", param_idx));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    // Sort handling
    let sort_column = match query.sort_by.as_deref() {
        Some("title") => "c.title",
        Some("created_at") => "c.created_at",
        _ => "c.recorded_at",
    };
    let sort_order = match query.sort_order.as_deref() {
        Some("asc") => "ASC",
        _ => "DESC",
    };

    // Count total
    let count_sql = format!("SELECT COUNT(*) FROM clips c WHERE {}", where_clause);

    let total: i64 = {
        let mut stmt = conn.prepare(&count_sql).map_err(|e| format!("Count prepare failed: {}", e))?;
        let mut idx = 0;
        stmt.raw_bind_parameter(idx + 1, library_id).map_err(|e| e.to_string())?;
        idx += 1;
        if let Some(ref pattern) = search_pattern {
            stmt.raw_bind_parameter(idx + 1, pattern.as_str()).map_err(|e| e.to_string())?;
            idx += 1;
        }
        if let Some(ref date) = query.date_from {
            stmt.raw_bind_parameter(idx + 1, date.as_str()).map_err(|e| e.to_string())?;
            idx += 1;
        }
        if let Some(ref date) = query.date_to {
            stmt.raw_bind_parameter(idx + 1, date.as_str()).map_err(|e| e.to_string())?;
        }
        let mut rows = stmt.raw_query();
        let row = rows.next().map_err(|e| e.to_string())?.ok_or("No count result")?;
        row.get::<usize, i64>(0).map_err(|e| e.to_string())?
    };

    // Main query with asset paths via subqueries
    let sql = format!(
        r#"SELECT
            c.id, c.title, c.media_type, c.duration_ms, c.width, c.height, c.recorded_at,
            (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id WHERE ca.clip_id = c.id AND ca.role = 'thumb' LIMIT 1) as thumb_path,
            (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1) as proxy_path,
            (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id WHERE ca.clip_id = c.id AND ca.role = 'sprite' LIMIT 1) as sprite_path,
            EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = c.id AND t.name = 'favorite') as is_favorite,
            EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = c.id AND t.name = 'bad') as is_bad
        FROM clips c
        WHERE {}
        ORDER BY {} {} NULLS LAST
        LIMIT ?{} OFFSET ?{}"#,
        where_clause, sort_column, sort_order, param_idx, param_idx + 1
    );

    let clips: Vec<ClipView> = {
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Query prepare failed: {}", e))?;
        let mut idx = 0;
        stmt.raw_bind_parameter(idx + 1, library_id).map_err(|e| e.to_string())?;
        idx += 1;
        if let Some(ref pattern) = search_pattern {
            stmt.raw_bind_parameter(idx + 1, pattern.as_str()).map_err(|e| e.to_string())?;
            idx += 1;
        }
        if let Some(ref date) = query.date_from {
            stmt.raw_bind_parameter(idx + 1, date.as_str()).map_err(|e| e.to_string())?;
            idx += 1;
        }
        if let Some(ref date) = query.date_to {
            stmt.raw_bind_parameter(idx + 1, date.as_str()).map_err(|e| e.to_string())?;
            idx += 1;
        }
        stmt.raw_bind_parameter(idx + 1, query.limit).map_err(|e| e.to_string())?;
        stmt.raw_bind_parameter(idx + 2, query.offset).map_err(|e| e.to_string())?;

        let mut rows = stmt.raw_query();
        let mut results = Vec::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            results.push(ClipView {
                id: row.get(0).map_err(|e| e.to_string())?,
                title: row.get(1).map_err(|e| e.to_string())?,
                media_type: row.get(2).map_err(|e| e.to_string())?,
                duration_ms: row.get(3).map_err(|e| e.to_string())?,
                width: row.get(4).map_err(|e| e.to_string())?,
                height: row.get(5).map_err(|e| e.to_string())?,
                recorded_at: row.get(6).map_err(|e| e.to_string())?,
                thumb_path: row.get(7).map_err(|e| e.to_string())?,
                proxy_path: row.get(8).map_err(|e| e.to_string())?,
                sprite_path: row.get(9).map_err(|e| e.to_string())?,
                is_favorite: row.get::<usize, i32>(10).map_err(|e| e.to_string())? == 1,
                is_bad: row.get::<usize, i32>(11).map_err(|e| e.to_string())? == 1,
            });
        }
        results
    };

    Ok(ClipListResponse {
        clips,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

/// Get a single clip with asset paths for Phase 3 UI
#[tauri::command]
pub fn get_clip_view(state: State<DbState>, id: i64) -> Result<ClipView, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let sql = r#"SELECT
        c.id, c.title, c.media_type, c.duration_ms, c.width, c.height, c.recorded_at,
        (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id WHERE ca.clip_id = c.id AND ca.role = 'thumb' LIMIT 1) as thumb_path,
        (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1) as proxy_path,
        (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id WHERE ca.clip_id = c.id AND ca.role = 'sprite' LIMIT 1) as sprite_path,
        EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = c.id AND t.name = 'favorite') as is_favorite,
        EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id WHERE ct.clip_id = c.id AND t.name = 'bad') as is_bad
    FROM clips c
    WHERE c.id = ?1"#;

    conn.query_row(sql, [id], |row| {
        Ok(ClipView {
            id: row.get(0)?,
            title: row.get(1)?,
            media_type: row.get(2)?,
            duration_ms: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            recorded_at: row.get(6)?,
            thumb_path: row.get(7)?,
            proxy_path: row.get(8)?,
            sprite_path: row.get(9)?,
            is_favorite: row.get::<_, i32>(10)? == 1,
            is_bad: row.get::<_, i32>(11)? == 1,
        })
    }).map_err(|e| format!("Clip not found: {}", e))
}
