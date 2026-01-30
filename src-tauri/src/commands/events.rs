// Dad Cam - Events Commands (Phase 6)
// Tauri commands for event management

use tauri::State;
use serde::{Deserialize, Serialize};
use rusqlite::Connection;
use super::DbState;
use crate::db::schema::{self, Event, NewEvent, EventUpdate, Clip, Library};

// ----- Helper Functions -----

/// Get the current library from an open database connection.
/// Each library has its own SQLite file, so there's always exactly one library.
fn get_current_library(conn: &Connection) -> Result<Library, String> {
    conn.query_row(
        "SELECT id, root_path, name, ingest_mode, created_at, settings FROM libraries LIMIT 1",
        [],
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
    ).map_err(|e| format!("No library found: {}", e))
}

// ----- Response Types -----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventView {
    pub id: i64,
    pub library_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub event_type: String,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub color: String,
    pub icon: String,
    pub clip_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventClipsResponse {
    pub clips: Vec<EventClipView>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventClipView {
    pub id: i64,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub recorded_at: Option<String>,
    pub thumbnail_path: Option<String>,
    pub proxy_path: Option<String>,
    pub original_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DateGroup {
    pub date: String,
    pub clip_count: i64,
}

// ----- Helper Functions -----

fn event_to_view(conn: &rusqlite::Connection, event: Event) -> Result<EventView, String> {
    let clip_count = schema::get_event_clip_count(conn, event.id)
        .map_err(|e| e.to_string())?;

    Ok(EventView {
        id: event.id,
        library_id: event.library_id,
        name: event.name,
        description: event.description,
        event_type: event.event_type,
        date_start: event.date_start,
        date_end: event.date_end,
        color: event.color,
        icon: event.icon,
        clip_count,
        created_at: event.created_at,
        updated_at: event.updated_at,
    })
}

fn clip_to_view(conn: &rusqlite::Connection, clip: Clip, library_root: &str) -> EventClipView {
    let paths = schema::get_clip_asset_paths(conn, clip.id).unwrap_or_default();

    EventClipView {
        id: clip.id,
        title: clip.title,
        duration_ms: clip.duration_ms,
        width: clip.width,
        height: clip.height,
        recorded_at: clip.recorded_at,
        thumbnail_path: paths.get("thumb").map(|p| format!("{}/{}", library_root, p)),
        proxy_path: paths.get("proxy").map(|p| format!("{}/{}", library_root, p)),
        original_path: paths.get("primary").map(|p| format!("{}/{}", library_root, p)),
    }
}

// ----- Commands -----

/// Create a new event
#[tauri::command]
pub fn create_event(
    state: State<DbState>,
    name: String,
    event_type: String,
    description: Option<String>,
    date_start: Option<String>,
    date_end: Option<String>,
    color: Option<String>,
    icon: Option<String>,
) -> Result<EventView, String> {
    let conn = state.connect()?;

    // Get current library
    let lib = get_current_library(&conn)?;

    // Validate event type
    if event_type != "date_range" && event_type != "clip_selection" {
        return Err("Invalid event type. Must be 'date_range' or 'clip_selection'".to_string());
    }

    // For date_range, require start and end dates
    if event_type == "date_range" {
        if date_start.is_none() || date_end.is_none() {
            return Err("date_range events require date_start and date_end".to_string());
        }
    }

    let new_event = NewEvent {
        library_id: lib.id,
        name,
        description,
        event_type,
        date_start,
        date_end,
        color,
        icon,
    };

    let event_id = schema::insert_event(&conn, &new_event)
        .map_err(|e| e.to_string())?;

    let event = schema::get_event(&conn, event_id)
        .map_err(|e| e.to_string())?
        .ok_or("Failed to retrieve created event")?;

    event_to_view(&conn, event)
}

/// Get all events for the current library
#[tauri::command]
pub fn get_events(state: State<DbState>) -> Result<Vec<EventView>, String> {
    let conn = state.connect()?;

    let lib = get_current_library(&conn)?;

    let events = schema::list_events(&conn, lib.id)
        .map_err(|e| e.to_string())?;

    events
        .into_iter()
        .map(|e| event_to_view(&conn, e))
        .collect()
}

/// Get a single event by ID
#[tauri::command]
pub fn get_event(state: State<DbState>, event_id: i64) -> Result<EventView, String> {
    let conn = state.connect()?;

    let event = schema::get_event(&conn, event_id)
        .map_err(|e| e.to_string())?
        .ok_or("Event not found")?;

    event_to_view(&conn, event)
}

/// Update an event
#[tauri::command]
pub fn update_event(
    state: State<DbState>,
    event_id: i64,
    name: Option<String>,
    description: Option<String>,
    date_start: Option<String>,
    date_end: Option<String>,
    color: Option<String>,
    icon: Option<String>,
) -> Result<EventView, String> {
    let conn = state.connect()?;

    let updates = EventUpdate {
        name,
        description,
        date_start,
        date_end,
        color,
        icon,
    };

    schema::update_event(&conn, event_id, &updates)
        .map_err(|e| e.to_string())?;

    let event = schema::get_event(&conn, event_id)
        .map_err(|e| e.to_string())?
        .ok_or("Event not found after update")?;

    event_to_view(&conn, event)
}

/// Delete an event
#[tauri::command]
pub fn delete_event(state: State<DbState>, event_id: i64) -> Result<(), String> {
    let conn = state.connect()?;

    // Verify event exists before deleting
    schema::get_event(&conn, event_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Event {} not found", event_id))?;

    schema::delete_event(&conn, event_id)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Add clips to an event
#[tauri::command]
pub fn add_clips_to_event(
    state: State<DbState>,
    event_id: i64,
    clip_ids: Vec<i64>,
) -> Result<(), String> {
    let conn = state.connect()?;

    // Validate event exists
    let event = schema::get_event(&conn, event_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Event {} not found", event_id))?;

    // Validate clips exist and belong to same library
    for clip_id in &clip_ids {
        let clip = schema::get_clip(&conn, *clip_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Clip {} not found", clip_id))?;

        if clip.library_id != event.library_id {
            return Err(format!("Clip {} belongs to a different library", clip_id));
        }
    }

    schema::add_clips_to_event(&conn, event_id, &clip_ids)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Remove clips from an event
#[tauri::command]
pub fn remove_clips_from_event(
    state: State<DbState>,
    event_id: i64,
    clip_ids: Vec<i64>,
) -> Result<(), String> {
    let conn = state.connect()?;

    // Validate event exists
    schema::get_event(&conn, event_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Event {} not found", event_id))?;

    schema::remove_clips_from_event(&conn, event_id, &clip_ids)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get clips for an event with pagination
#[tauri::command]
pub fn get_event_clips(
    state: State<DbState>,
    event_id: i64,
    offset: i64,
    limit: i64,
) -> Result<EventClipsResponse, String> {
    let conn = state.connect()?;

    let lib = get_current_library(&conn)?;

    let total = schema::get_event_clip_count(&conn, event_id)
        .map_err(|e| e.to_string())?;

    let clips = schema::get_event_clips(&conn, event_id, limit, offset)
        .map_err(|e| e.to_string())?;

    let clip_views: Vec<EventClipView> = clips
        .into_iter()
        .map(|c| clip_to_view(&conn, c, &lib.root_path))
        .collect();

    Ok(EventClipsResponse {
        clips: clip_views,
        total,
        offset,
        limit,
    })
}

/// Get clips grouped by date (for date navigation)
#[tauri::command]
pub fn get_clips_grouped_by_date(state: State<DbState>) -> Result<Vec<DateGroup>, String> {
    let conn = state.connect()?;

    let lib = get_current_library(&conn)?;

    let groups = schema::get_clips_grouped_by_date(&conn, lib.id)
        .map_err(|e| e.to_string())?;

    Ok(groups
        .into_iter()
        .map(|(date, count)| DateGroup {
            date,
            clip_count: count,
        })
        .collect())
}

/// Check if a year is a leap year
fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get the number of days in a month
fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 0,
    }
}

/// Validate date string is in YYYY-MM-DD format with proper calendar rules
fn validate_date_format(date: &str) -> Result<(), String> {
    // Check length and format
    if date.len() != 10 {
        return Err("Invalid date format. Expected YYYY-MM-DD".to_string());
    }

    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return Err("Invalid date format. Expected YYYY-MM-DD".to_string());
    }

    // Validate year (4 digits)
    let year: u16 = parts[0].parse()
        .map_err(|_| "Invalid year in date".to_string())?;
    if year < 1900 || year > 2100 {
        return Err("Year out of valid range (1900-2100)".to_string());
    }

    // Validate month (1-12)
    let month: u8 = parts[1].parse()
        .map_err(|_| "Invalid month in date".to_string())?;
    if month < 1 || month > 12 {
        return Err("Month must be between 1 and 12".to_string());
    }

    // Validate day with proper days-per-month and leap year handling
    let day: u8 = parts[2].parse()
        .map_err(|_| "Invalid day in date".to_string())?;
    let max_day = days_in_month(year, month);
    if day < 1 || day > max_day {
        return Err(format!("Day must be between 1 and {} for {}/{}", max_day, month, year));
    }

    Ok(())
}

/// Get clips for a specific date
#[tauri::command]
pub fn get_clips_by_date(
    state: State<DbState>,
    date: String,
    offset: i64,
    limit: i64,
) -> Result<EventClipsResponse, String> {
    // Validate date format before querying
    validate_date_format(&date)?;

    let conn = state.connect()?;

    let lib = get_current_library(&conn)?;

    let total = schema::count_clips_by_date(&conn, lib.id, &date)
        .map_err(|e| e.to_string())?;

    let clips = schema::get_clips_by_date(&conn, lib.id, &date, limit, offset)
        .map_err(|e| e.to_string())?;

    let clip_views: Vec<EventClipView> = clips
        .into_iter()
        .map(|c| clip_to_view(&conn, c, &lib.root_path))
        .collect();

    Ok(EventClipsResponse {
        clips: clip_views,
        total,
        offset,
        limit,
    })
}
