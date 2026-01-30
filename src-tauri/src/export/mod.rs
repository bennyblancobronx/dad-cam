// Dad Cam - VHS Export Module
// Orchestrates clip selection, FFmpeg rendering with crossfades, and export history.

pub mod timeline;
pub mod ffmpeg_builder;
pub mod watermark;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{DadCamError, Result};
use crate::tools::ffmpeg_path;
use crate::jobs::progress::{JobProgress, emit_progress};
use crate::licensing;

/// Parameters for a VHS export operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VhsExportParams {
    pub selection_mode: String,
    pub selection_params: serde_json::Value,
    pub ordering: String,
    pub title_text: Option<String>,
    pub output_path: String,
    pub library_path: String,
    /// Crossfade blend duration in milliseconds (from devMenu.jlBlendMs). Default: 500.
    pub blend_duration_ms: Option<u32>,
    /// Title overlay start time in seconds (from devMenu.titleStartSeconds). Default: 5.
    pub title_start_seconds: Option<f64>,
}

/// A single export history entry (read from DB)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportHistoryEntry {
    pub id: i64,
    pub output_path: String,
    pub created_at: String,
    pub selection_mode: String,
    pub ordering: String,
    pub title_text: Option<String>,
    pub resolution: Option<String>,
    pub is_watermarked: bool,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub file_size_bytes: Option<i64>,
    pub clip_count: Option<i64>,
    pub error_message: Option<String>,
    pub completed_at: Option<String>,
}

/// Insert a new export history record, returning its id
pub fn insert_export_history(
    conn: &Connection,
    library_id: i64,
    params: &VhsExportParams,
    is_watermarked: bool,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO export_history
         (library_id, output_path, selection_mode, selection_params, ordering, title_text, is_watermarked, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending')",
        rusqlite::params![
            library_id,
            params.output_path,
            params.selection_mode,
            params.selection_params.to_string(),
            params.ordering,
            params.title_text,
            is_watermarked as i32,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update export status on completion or failure
pub fn update_export_status(
    conn: &Connection,
    export_id: i64,
    status: &str,
    duration_ms: Option<i64>,
    file_size_bytes: Option<i64>,
    clip_count: Option<i64>,
    resolution: Option<&str>,
    error_message: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE export_history
         SET status = ?1, duration_ms = ?2, file_size_bytes = ?3, clip_count = ?4,
             resolution = ?5, error_message = ?6, completed_at = datetime('now')
         WHERE id = ?7",
        rusqlite::params![
            status,
            duration_ms,
            file_size_bytes,
            clip_count,
            resolution,
            error_message,
            export_id,
        ],
    )?;
    Ok(())
}

/// List recent export history entries
pub fn list_export_history(conn: &Connection, library_id: i64, limit: i64) -> Result<Vec<ExportHistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, output_path, created_at, selection_mode, ordering, title_text,
                resolution, is_watermarked, status, duration_ms, file_size_bytes,
                clip_count, error_message, completed_at
         FROM export_history
         WHERE library_id = ?1
         ORDER BY created_at DESC
         LIMIT ?2",
    )?;

    let entries = stmt
        .query_map(params![library_id, limit], |row| {
            Ok(ExportHistoryEntry {
                id: row.get(0)?,
                output_path: row.get(1)?,
                created_at: row.get(2)?,
                selection_mode: row.get(3)?,
                ordering: row.get(4)?,
                title_text: row.get(5)?,
                resolution: row.get(6)?,
                is_watermarked: row.get::<_, i32>(7)? != 0,
                status: row.get(8)?,
                duration_ms: row.get(9)?,
                file_size_bytes: row.get(10)?,
                clip_count: row.get(11)?,
                error_message: row.get(12)?,
                completed_at: row.get(13)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Info about a clip selected for export
#[derive(Debug, Clone)]
pub struct ExportClip {
    pub clip_id: i64,
    pub path: String,       // Relative path (proxy preferred, original fallback)
    pub duration_ms: i64,
    pub has_audio: bool,
}

/// Run the full VHS export pipeline.
/// Opens its own DB connection to avoid holding the shared Mutex.
pub fn run_vhs_export(
    conn: &Connection,
    library_id: i64,
    library_root: &Path,
    params: &VhsExportParams,
    app: &AppHandle,
    cancel_flag: &AtomicBool,
    job_id: &str,
) -> Result<()> {
    let should_wm = licensing::should_watermark();

    // Record in history
    let export_id = insert_export_history(conn, library_id, params, should_wm)?;

    // Phase 1: select clips
    emit_progress(app, &JobProgress::new(job_id, "select", 0, 1)
        .with_message("Selecting clips..."));

    let clips = timeline::select_clips(conn, library_id, params)?;
    if clips.is_empty() {
        let _ = update_export_status(conn, export_id, "failed", None, None, None, None, Some("No clips matched selection criteria"));
        return Err(DadCamError::Other("No clips matched selection criteria".to_string()));
    }

    let clip_count = clips.len() as i64;

    if crate::jobs::is_cancelled(cancel_flag) {
        let _ = update_export_status(conn, export_id, "cancelled", None, None, Some(clip_count), None, None);
        return Ok(());
    }

    // Phase 2: build FFmpeg command
    emit_progress(app, &JobProgress::new(job_id, "build", 0, 1)
        .with_message(format!("Building export with {} clips...", clip_count)));

    let output_path = PathBuf::from(&params.output_path);
    let tmp_path = output_path.with_extension("tmp.mp4");

    let blend_sec = params.blend_duration_ms.unwrap_or(500) as f64 / 1000.0;
    let title_start_sec = params.title_start_seconds.unwrap_or(5.0);

    let ffmpeg_args = ffmpeg_builder::build_export_command(
        &clips,
        library_root,
        &tmp_path,
        params.title_text.as_deref(),
        should_wm,
        blend_sec,
        title_start_sec,
    )?;

    if crate::jobs::is_cancelled(cancel_flag) {
        let _ = update_export_status(conn, export_id, "cancelled", None, None, Some(clip_count), None, None);
        return Ok(());
    }

    // Phase 3: run FFmpeg
    emit_progress(app, &JobProgress::new(job_id, "render", 0, 100)
        .with_message("Rendering export..."));

    let ffmpeg = ffmpeg_path();
    let mut cmd = std::process::Command::new(&ffmpeg);
    for arg in &ffmpeg_args {
        cmd.arg(arg);
    }
    cmd.stderr(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::null());

    let mut child = cmd.spawn()
        .map_err(|e| DadCamError::FFmpeg(format!("Failed to start FFmpeg: {}", e)))?;

    // Parse stderr for progress (time= lines)
    let total_duration_ms: i64 = clips.iter().map(|c| c.duration_ms).sum();
    let stderr = child.stderr.take();

    if let Some(stderr) = stderr {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if crate::jobs::is_cancelled(cancel_flag) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&tmp_path);
                let _ = update_export_status(conn, export_id, "cancelled", None, None, Some(clip_count), None, None);
                emit_progress(app, &JobProgress::new(job_id, "render", 0, 100).cancelled());
                return Ok(());
            }

            if let Ok(line) = line {
                // Parse FFmpeg time= progress
                if let Some(time_str) = parse_ffmpeg_time(&line) {
                    let progress_ms = (time_str * 1000.0) as u64;
                    let total_ms = total_duration_ms.max(1) as u64;
                    let percent = ((progress_ms as f64 / total_ms as f64) * 100.0).min(99.0) as u64;
                    emit_progress(app, &JobProgress::new(job_id, "render", percent, 100)
                        .with_message(format!("Rendering... {}%", percent)));
                }
            }
        }
    }

    let status = child.wait()
        .map_err(|e| DadCamError::FFmpeg(format!("FFmpeg process error: {}", e)))?;

    if !status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        let msg = format!("FFmpeg exited with code {}", status.code().unwrap_or(-1));
        let _ = update_export_status(conn, export_id, "failed", None, None, Some(clip_count), None, Some(&msg));
        return Err(DadCamError::FFmpeg(msg));
    }

    // Phase 4: atomic rename
    std::fs::rename(&tmp_path, &output_path)
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            DadCamError::Io(e)
        })?;

    // Get output file size
    let file_size = std::fs::metadata(&output_path)
        .map(|m| m.len() as i64)
        .ok();

    let resolution = if should_wm { Some("720p") } else { Some("1080p") };

    update_export_status(
        conn, export_id, "completed",
        Some(total_duration_ms), file_size, Some(clip_count),
        resolution, None,
    )?;

    emit_progress(app, &JobProgress::new(job_id, "complete", 100, 100)
        .with_message("Export complete"));

    Ok(())
}

/// Parse FFmpeg time= from stderr line. Returns seconds.
fn parse_ffmpeg_time(line: &str) -> Option<f64> {
    // Format: "time=00:01:23.45" or "time=HH:MM:SS.ms"
    let idx = line.find("time=")?;
    let after = &line[idx + 5..];
    let end = after.find(|c: char| c == ' ' || c == '\r' || c == '\n').unwrap_or(after.len());
    let time_str = &after[..end];

    // Handle negative or "N/A"
    if time_str.starts_with('-') || time_str.starts_with('N') {
        return None;
    }

    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() == 3 {
        let hours: f64 = parts[0].parse().ok()?;
        let minutes: f64 = parts[1].parse().ok()?;
        let seconds: f64 = parts[2].parse().ok()?;
        Some(hours * 3600.0 + minutes * 60.0 + seconds)
    } else {
        None
    }
}
