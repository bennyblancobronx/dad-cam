// Metadata extraction module

pub mod ffprobe;
pub mod exiftool;

use std::path::Path;
use serde::{Deserialize, Serialize};
use crate::error::Result;

/// Combined metadata from all sources
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaMetadata {
    // Video properties
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub bitrate: Option<i64>,
    pub container: Option<String>,

    // Audio properties
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_sample_rate: Option<i32>,

    // Date/time
    pub recorded_at: Option<String>,
    pub recorded_at_source: Option<String>,

    // Camera info
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub serial_number: Option<String>,

    // Media type
    pub media_type: String,
}

/// Full extraction result including raw dumps for sidecar storage.
pub struct FullExtractionResult {
    pub metadata: MediaMetadata,
    pub raw_exif_dump: serde_json::Value,
    pub raw_ffprobe_dump: serde_json::Value,
    pub exif_extended: exiftool::ExifExtendedMetadata,
    pub ffprobe_extended: ffprobe::FFprobeExtendedFields,
    pub exiftool_success: bool,
    pub exiftool_exit_code: i32,
    pub exiftool_error: Option<String>,
    pub ffprobe_success: bool,
    pub ffprobe_exit_code: i32,
    pub ffprobe_error: Option<String>,
}

/// Extract metadata from a media file with full raw dumps (gold-standard).
pub fn extract_metadata_full(path: &Path) -> Result<FullExtractionResult> {
    // Run ffprobe (full dump mode)
    let ffprobe_result = ffprobe::probe_full(path)?;

    // Run exiftool (full dump mode) -- non-fatal if it fails
    let exif_result = exiftool::extract_full(path);

    let mut meta = ffprobe_result.metadata;

    // Merge exiftool data
    if let Ok(ref exif) = exif_result {
        if exif.success {
            // Prefer exiftool dates (more reliable for camera files)
            if meta.recorded_at.is_none() && exif.parsed.recorded_at.is_some() {
                meta.recorded_at = exif.parsed.recorded_at.clone();
                meta.recorded_at_source = Some("exiftool".to_string());
            }
            if meta.camera_make.is_none() {
                meta.camera_make = exif.parsed.camera_make.clone();
            }
            if meta.camera_model.is_none() {
                meta.camera_model = exif.parsed.camera_model.clone();
            }
            if meta.serial_number.is_none() {
                meta.serial_number = exif.parsed.serial_number.clone();
            }
        }
    }

    let (exif_dump, exif_ext, exif_ok, exif_code, exif_err) = match exif_result {
        Ok(r) => (r.raw_dump, r.extended, r.success, r.exit_code, r.error),
        Err(e) => (
            serde_json::Value::Null,
            exiftool::ExifExtendedMetadata::default(),
            false, -1, Some(e.to_string()),
        ),
    };

    Ok(FullExtractionResult {
        metadata: meta,
        raw_exif_dump: exif_dump,
        raw_ffprobe_dump: ffprobe_result.raw_dump,
        exif_extended: exif_ext,
        ffprobe_extended: ffprobe_result.extended,
        exiftool_success: exif_ok,
        exiftool_exit_code: exif_code,
        exiftool_error: exif_err,
        ffprobe_success: ffprobe_result.success,
        ffprobe_exit_code: ffprobe_result.exit_code,
        ffprobe_error: ffprobe_result.error,
    })
}

/// Backward-compatible: extract metadata without raw dumps.
pub fn extract_metadata(path: &Path) -> Result<MediaMetadata> {
    let result = extract_metadata_full(path)?;
    Ok(result.metadata)
}

/// Determine media type from file extension
pub fn detect_media_type(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if crate::constants::VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        "video".to_string()
    } else if crate::constants::AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        "audio".to_string()
    } else if crate::constants::IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        "image".to_string()
    } else {
        "video".to_string() // Default to video for unknown
    }
}

/// Try to parse timestamp from folder name (e.g., "2019-07-04" or "20190704")
pub fn parse_folder_date(folder_name: &str) -> Option<String> {
    if let Ok(date) = chrono::NaiveDate::parse_from_str(folder_name, "%Y-%m-%d") {
        return Some(format!("{}T00:00:00Z", date));
    }

    if let Ok(date) = chrono::NaiveDate::parse_from_str(folder_name, "%Y%m%d") {
        return Some(format!("{}T00:00:00Z", date));
    }

    let date_regex = regex::Regex::new(r"(\d{4})-(\d{2})-(\d{2})").ok()?;
    if let Some(caps) = date_regex.captures(folder_name) {
        let year = caps.get(1)?.as_str();
        let month = caps.get(2)?.as_str();
        let day = caps.get(3)?.as_str();
        return Some(format!("{}-{}-{}T00:00:00Z", year, month, day));
    }

    None
}
