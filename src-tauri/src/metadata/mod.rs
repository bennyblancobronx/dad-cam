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

    // Media type
    pub media_type: String,
}

/// Extract metadata from a media file
pub fn extract_metadata(path: &Path) -> Result<MediaMetadata> {
    // Try ffprobe first
    let ffprobe_meta = ffprobe::probe(path)?;

    // Try exiftool for additional camera metadata
    let exif_meta = exiftool::extract(path).ok();

    // Merge results
    let mut meta = ffprobe_meta;

    if let Some(exif) = exif_meta {
        // Prefer exiftool dates (more reliable for camera files)
        if meta.recorded_at.is_none() && exif.recorded_at.is_some() {
            meta.recorded_at = exif.recorded_at;
            meta.recorded_at_source = Some("exiftool".to_string());
        }
        // Add camera info
        if meta.camera_make.is_none() {
            meta.camera_make = exif.camera_make;
        }
        if meta.camera_model.is_none() {
            meta.camera_model = exif.camera_model;
        }
    }

    Ok(meta)
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
    // Try YYYY-MM-DD format
    if let Ok(date) = chrono::NaiveDate::parse_from_str(folder_name, "%Y-%m-%d") {
        return Some(format!("{}T00:00:00Z", date));
    }

    // Try YYYYMMDD format
    if let Ok(date) = chrono::NaiveDate::parse_from_str(folder_name, "%Y%m%d") {
        return Some(format!("{}T00:00:00Z", date));
    }

    // Try to extract date from folder name like "2019-07-04 Birthday"
    let date_regex = regex::Regex::new(r"(\d{4})-(\d{2})-(\d{2})").ok()?;
    if let Some(caps) = date_regex.captures(folder_name) {
        let year = caps.get(1)?.as_str();
        let month = caps.get(2)?.as_str();
        let day = caps.get(3)?.as_str();
        return Some(format!("{}-{}-{}T00:00:00Z", year, month, day));
    }

    None
}
