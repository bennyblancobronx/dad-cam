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

/// Extended metadata parsed from the full exiftool dump (sidecar-only, not DB).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifExtendedMetadata {
    pub sensor_type: Option<String>,
    pub focal_length: Option<f64>,
    pub focal_length_35mm: Option<f64>,
    pub scale_factor: Option<f64>,
    pub native_width: Option<i32>,
    pub native_height: Option<i32>,
    pub bits_per_sample: Option<i32>,
    pub color_space: Option<String>,
    pub white_balance: Option<String>,
    pub lens_model: Option<String>,
    pub lens_id: Option<String>,
    pub megapixels: Option<f64>,
    pub rotation: Option<f64>,
    pub compressor_id: Option<String>,
}

/// Result of a full exiftool extraction: raw dump + parsed fields.
pub struct ExifFullResult {
    pub raw_dump: serde_json::Value,
    pub parsed: ExifMetadata,
    pub extended: ExifExtendedMetadata,
    pub success: bool,
    pub exit_code: i32,
    pub error: Option<String>,
}

/// Run exiftool in full dump mode (-j -G -n) and return raw JSON + parsed fields.
pub fn extract_full(path: &Path) -> Result<ExifFullResult> {
    let output = Command::new(crate::tools::exiftool_path())
        .args(["-j", "-G", "-n"])
        .arg(path)
        .output()
        .map_err(|e| DadCamError::ExifTool(format!("Failed to run exiftool: {}", e)))?;

    let exit_code = output.status.code().unwrap_or(-1);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Ok(ExifFullResult {
            raw_dump: serde_json::Value::Null,
            parsed: ExifMetadata::default(),
            extended: ExifExtendedMetadata::default(),
            success: false,
            exit_code,
            error: Some(stderr),
        });
    }

    let raw_array: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| DadCamError::ExifTool(format!("Failed to parse exiftool JSON: {}", e)))?;

    // exiftool returns an array; take the first element
    let raw_dump = raw_array.as_array()
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

    let parsed = parse_core_fields(&raw_dump);
    let extended = parse_extended_fields(&raw_dump);

    Ok(ExifFullResult {
        raw_dump,
        parsed,
        extended,
        success: true,
        exit_code,
        error: None,
    })
}

/// Backward-compatible: run exiftool and return just the parsed core fields.
pub fn extract(path: &Path) -> Result<ExifMetadata> {
    let result = extract_full(path)?;
    if !result.success {
        return Err(DadCamError::ExifTool(
            result.error.unwrap_or_else(|| "exiftool failed".to_string()),
        ));
    }
    Ok(result.parsed)
}

/// Parse core fields from the full -G -n dump.
/// With -G, tags are prefixed: "EXIF:Make", "QuickTime:CreateDate", etc.
/// With -n, numeric values are raw (GPS as decimal, focal as float).
fn parse_core_fields(dump: &serde_json::Value) -> ExifMetadata {
    let mut meta = ExifMetadata::default();

    // Date: prefer EXIF group over QuickTime
    let date = get_grouped_string(dump, "DateTimeOriginal")
        .or_else(|| get_grouped_string(dump, "CreateDate"))
        .or_else(|| get_grouped_string(dump, "MediaCreateDate"));
    meta.recorded_at = date.and_then(|d| parse_exif_date(&d));

    // Camera
    meta.camera_make = get_grouped_string(dump, "Make");
    meta.camera_model = get_grouped_string(dump, "Model");

    // Serial: priority chain
    meta.serial_number = get_grouped_string(dump, "SerialNumber")
        .or_else(|| get_grouped_string(dump, "InternalSerialNumber"));

    // GPS: with -n flag, values are already decimal
    meta.gps_latitude = get_grouped_number(dump, "GPSLatitude");
    meta.gps_longitude = get_grouped_number(dump, "GPSLongitude");

    meta
}

/// Parse extended fields from the full dump (sidecar-only).
fn parse_extended_fields(dump: &serde_json::Value) -> ExifExtendedMetadata {
    let mut ext = ExifExtendedMetadata::default();

    ext.sensor_type = get_grouped_string(dump, "ImageSensorType");
    ext.focal_length = get_grouped_number(dump, "FocalLength");
    ext.focal_length_35mm = get_grouped_number(dump, "FocalLengthIn35mmFormat");
    ext.scale_factor = get_grouped_number(dump, "ScaleFactor35efl");
    ext.native_width = get_grouped_number(dump, "ExifImageWidth").map(|v| v as i32);
    ext.native_height = get_grouped_number(dump, "ExifImageHeight").map(|v| v as i32);
    ext.bits_per_sample = get_grouped_number(dump, "BitsPerSample").map(|v| v as i32);
    ext.color_space = get_grouped_string(dump, "ColorSpace");
    ext.white_balance = get_grouped_string(dump, "WhiteBalance");
    ext.lens_model = get_grouped_string(dump, "LensModel");
    ext.lens_id = get_grouped_string(dump, "LensID");
    ext.megapixels = get_grouped_number(dump, "Megapixels");
    ext.rotation = get_grouped_number(dump, "Rotation");
    ext.compressor_id = get_grouped_string(dump, "CompressorID");

    ext
}

/// Get a string value from a grouped exiftool dump.
/// With -G flag, keys are "Group:TagName". We search for any group containing the tag.
fn get_grouped_string(dump: &serde_json::Value, tag: &str) -> Option<String> {
    let obj = dump.as_object()?;
    // Prefer EXIF group, then any group
    let exif_key = format!("EXIF:{}", tag);
    if let Some(val) = obj.get(&exif_key).and_then(|v| value_to_string(v)) {
        return Some(val);
    }
    // Search all groups
    for (key, val) in obj {
        if key.ends_with(&format!(":{}", tag)) || key == tag {
            if let Some(s) = value_to_string(val) {
                return Some(s);
            }
        }
    }
    None
}

/// Get a numeric value from a grouped exiftool dump.
fn get_grouped_number(dump: &serde_json::Value, tag: &str) -> Option<f64> {
    let obj = dump.as_object()?;
    let exif_key = format!("EXIF:{}", tag);
    if let Some(val) = obj.get(&exif_key).and_then(|v| v.as_f64()) {
        return Some(val);
    }
    for (key, val) in obj {
        if key.ends_with(&format!(":{}", tag)) || key == tag {
            if let Some(n) = val.as_f64() {
                return Some(n);
            }
        }
    }
    None
}

/// Convert a JSON value to string (handles both string and numeric values).
fn value_to_string(val: &serde_json::Value) -> Option<String> {
    match val {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Parse exiftool date format to ISO8601
fn parse_exif_date(date_str: &str) -> Option<String> {
    let normalized = date_str
        .replace(':', "-")
        .replacen("-", ":", 2);

    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
        &normalized.replace(' ', "T"), "%Y-%m-%dT%H:%M:%S",
    ) {
        return Some(format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S")));
    }

    let parts: Vec<&str> = date_str.split_whitespace().collect();
    if parts.len() >= 2 {
        let date_part = parts[0].replace(':', "-");
        let time_part = parts[1];
        return Some(format!("{}T{}Z", date_part, time_part));
    }

    None
}

/// Check if exiftool is available
pub fn is_available() -> bool {
    crate::tools::is_tool_available("exiftool")
}
