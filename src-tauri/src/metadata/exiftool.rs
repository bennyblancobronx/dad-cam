// ExifTool wrapper for camera metadata extraction

use std::path::Path;
use std::process::Command;
use serde::{Deserialize, Serialize};
use crate::error::{DadCamError, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExifMetadata {
    pub recorded_at: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub serial_number: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ExifToolOutput {
    #[serde(rename = "DateTimeOriginal")]
    date_time_original: Option<String>,
    #[serde(rename = "CreateDate")]
    create_date: Option<String>,
    #[serde(rename = "MediaCreateDate")]
    media_create_date: Option<String>,
    #[serde(rename = "Make")]
    make: Option<String>,
    #[serde(rename = "Model")]
    model: Option<String>,
    #[serde(rename = "SerialNumber")]
    serial_number: Option<String>,
    #[serde(rename = "InternalSerialNumber")]
    internal_serial_number: Option<String>,
    #[serde(rename = "GPSLatitude")]
    gps_latitude: Option<String>,
    #[serde(rename = "GPSLongitude")]
    gps_longitude: Option<String>,
}

/// Run exiftool on a file and extract metadata
pub fn extract(path: &Path) -> Result<ExifMetadata> {
    let output = Command::new(crate::tools::exiftool_path())
        .args([
            "-j",
            "-DateTimeOriginal",
            "-CreateDate",
            "-MediaCreateDate",
            "-Make",
            "-Model",
            "-SerialNumber",
            "-InternalSerialNumber",
            "-GPSLatitude",
            "-GPSLongitude",
        ])
        .arg(path)
        .output()
        .map_err(|e| DadCamError::ExifTool(format!("Failed to run exiftool: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DadCamError::ExifTool(format!("exiftool failed: {}", stderr)));
    }

    let exif_output: Vec<ExifToolOutput> = serde_json::from_slice(&output.stdout)
        .map_err(|e| DadCamError::ExifTool(format!("Failed to parse exiftool output: {}", e)))?;

    let exif = exif_output.into_iter().next().unwrap_or_default();

    let mut meta = ExifMetadata::default();

    // Extract date (prefer DateTimeOriginal, then CreateDate, then MediaCreateDate)
    let raw_date = exif.date_time_original
        .or(exif.create_date)
        .or(exif.media_create_date);

    meta.recorded_at = raw_date.and_then(|d| parse_exif_date(&d));
    meta.camera_make = exif.make;
    meta.camera_model = exif.model;
    meta.serial_number = exif.serial_number.or(exif.internal_serial_number);

    // Parse GPS if available
    if let Some(lat_str) = exif.gps_latitude {
        meta.gps_latitude = parse_gps_coord(&lat_str);
    }
    if let Some(lon_str) = exif.gps_longitude {
        meta.gps_longitude = parse_gps_coord(&lon_str);
    }

    Ok(meta)
}

/// Parse exiftool date format to ISO8601
/// Input: "2019:07:04 12:30:45" or similar
/// Output: "2019-07-04T12:30:45Z"
fn parse_exif_date(date_str: &str) -> Option<String> {
    // Handle "YYYY:MM:DD HH:MM:SS" format
    let normalized = date_str
        .replace(':', "-")
        .replacen("-", ":", 2); // Restore time colons

    // Try parsing with chrono
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&normalized.replace(" ", "T"), "%Y-%m-%dT%H:%M:%S") {
        return Some(format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S")));
    }

    // Try alternate format
    let parts: Vec<&str> = date_str.split_whitespace().collect();
    if parts.len() >= 2 {
        let date_part = parts[0].replace(':', "-");
        let time_part = parts[1];
        return Some(format!("{}T{}Z", date_part, time_part));
    }

    None
}

/// Parse GPS coordinate string to decimal degrees
fn parse_gps_coord(coord_str: &str) -> Option<f64> {
    // ExifTool may return "34 deg 3' 30.00" N" format
    // or already decimal like "34.0583"

    // Try direct parse first
    if let Ok(val) = coord_str.parse::<f64>() {
        return Some(val);
    }

    // Try DMS format: capture degrees, minutes, seconds, and optional direction
    let re = regex::Regex::new(r#"(\d+)\s*(?:deg|°)\s*(\d+)'\s*([\d.]+)"?\s*([NSEW])?"#).ok()?;
    let caps = re.captures(coord_str)?;

    let deg: f64 = caps.get(1)?.as_str().parse().ok()?;
    let min: f64 = caps.get(2)?.as_str().parse().ok()?;
    let sec: f64 = caps.get(3)?.as_str().parse().ok()?;
    let dir = caps.get(4).map(|m| m.as_str()).unwrap_or("N");

    let mut decimal = deg + min / 60.0 + sec / 3600.0;
    if dir == "S" || dir == "W" {
        decimal = -decimal;
    }

    Some(decimal)
}

/// Check if exiftool is available
pub fn is_available() -> bool {
    crate::tools::is_tool_available("exiftool")
}

impl Default for ExifToolOutput {
    fn default() -> Self {
        Self {
            date_time_original: None,
            create_date: None,
            media_create_date: None,
            make: None,
            model: None,
            serial_number: None,
            internal_serial_number: None,
            gps_latitude: None,
            gps_longitude: None,
        }
    }
}
