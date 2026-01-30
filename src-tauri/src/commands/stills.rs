// Dad Cam - Stills Export Commands
// Export high-quality still frames from video clips

use std::path::PathBuf;
use std::process::Command;
use tauri::State;
use serde::{Deserialize, Serialize};

use crate::commands::DbState;
use crate::db::schema;
use crate::tools::ffmpeg_path;

/// Request to export a still frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StillExportRequest {
    pub clip_id: i64,
    pub timestamp_ms: i64,
    pub output_path: String,
    pub format: String, // "jpg" or "png"
}

/// Result of still frame export
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StillExportResult {
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

/// Export a still frame from a video clip at the specified timestamp
/// Uses the original video file (not proxy) for maximum quality
#[tauri::command]
pub fn export_still(
    state: State<DbState>,
    request: StillExportRequest,
) -> Result<StillExportResult, String> {
    // Validate format
    if request.format != "jpg" && request.format != "png" {
        return Err("Unsupported format. Use jpg or png.".to_string());
    }

    // Get database connection
    let conn = state.connect()?;

    // Get clip info
    let clip = schema::get_clip(&conn, request.clip_id)
        .map_err(|e| e.to_string())?
        .ok_or("Clip not found")?;

    // Get original asset path
    let original_asset = schema::get_asset(&conn, clip.original_asset_id)
        .map_err(|e| e.to_string())?
        .ok_or("Original asset not found")?;

    // Get library root to build full path
    let library = schema::get_library(&conn, clip.library_id)
        .map_err(|e| e.to_string())?
        .ok_or("Library not found")?;

    let original_path = PathBuf::from(&library.root_path).join(&original_asset.path);

    // Verify original file exists
    if !original_path.exists() {
        return Err(format!(
            "Original file offline: {}",
            original_asset.path
        ));
    }

    // Convert timestamp to seconds
    let timestamp_secs = request.timestamp_ms as f64 / 1000.0;

    // Build output path
    let output_path = PathBuf::from(&request.output_path);

    // Get FFmpeg path
    let ffmpeg = ffmpeg_path();

    // Build FFmpeg command
    // -ss before -i for fast seeking
    // -vframes 1 to extract single frame
    let mut cmd = Command::new(&ffmpeg);
    cmd.args([
        "-ss",
        &format!("{:.3}", timestamp_secs),
        "-i",
        original_path.to_str().ok_or("Invalid path encoding")?,
        "-vframes",
        "1",
    ]);

    // Format-specific quality args
    match request.format.as_str() {
        "jpg" => {
            cmd.args(["-q:v", "2"]); // High quality JPEG (2 is very high)
        }
        "png" => {
            cmd.args(["-compression_level", "6"]); // Balanced compression
        }
        _ => unreachable!(),
    }

    // Output path with overwrite
    cmd.arg("-y")
        .arg(output_path.to_str().ok_or("Invalid output path encoding")?);

    // Execute FFmpeg
    let output = cmd.output().map_err(|e| format!("FFmpeg failed to start: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg failed to export frame: {}", stderr));
    }

    // Verify output file exists
    if !output_path.exists() {
        return Err("Export completed but output file not found".to_string());
    }

    // Get output file info
    let metadata = std::fs::metadata(&output_path)
        .map_err(|e| format!("Failed to read output: {}", e))?;

    // Get dimensions via ffprobe (optional, fallback to 0)
    let (width, height) = get_image_dimensions(&output_path).unwrap_or((0, 0));

    Ok(StillExportResult {
        output_path: request.output_path,
        width,
        height,
        size_bytes: metadata.len(),
    })
}

/// Get image dimensions using ffprobe
fn get_image_dimensions(path: &PathBuf) -> Option<(u32, u32)> {
    let ffprobe = crate::tools::ffprobe_path();
    let output = Command::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(',').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().ok()?;
        let h = parts[1].parse().ok()?;
        return Some((w, h));
    }
    None
}
