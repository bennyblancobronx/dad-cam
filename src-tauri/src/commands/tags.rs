// Dad Cam - Phase 3 Tag Commands
// Commands for tag toggling and management

use tauri::State;

use crate::db::schema;
use super::DbState;

/// Toggle a tag on a clip (add if missing, remove if present)
#[tauri::command]
pub fn toggle_tag(state: State<DbState>, clip_id: i64, tag: String) -> Result<bool, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let tag_id = schema::get_tag_id(conn, &tag)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Tag '{}' not found", tag))?;

    let has_tag = schema::has_clip_tag(conn, clip_id, &tag).map_err(|e| e.to_string())?;

    if has_tag {
        schema::remove_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        schema::add_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
        Ok(true)
    }
}

/// Set a tag to a specific value
#[tauri::command]
pub fn set_tag(state: State<DbState>, clip_id: i64, tag: String, value: bool) -> Result<bool, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let conn = db.as_ref().ok_or("No library open")?;

    let tag_id = schema::get_tag_id(conn, &tag)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Tag '{}' not found", tag))?;

    if value {
        schema::add_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
    } else {
        schema::remove_clip_tag(conn, clip_id, tag_id).map_err(|e| e.to_string())?;
    }

    Ok(value)
}
