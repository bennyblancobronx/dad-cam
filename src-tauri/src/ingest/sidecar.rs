// Sidecar JSON writer
// Writes per-clip metadata snapshots to .dadcam/sidecars/<clip_id>.json
// Schema matches v0.3.0 spec: nested structure with raw dumps, metadata_snapshot,
// extraction_status, extended_metadata, camera_match, match_audit,
// ingest_timestamps, derived_asset_paths, rental_audit.

use std::path::Path;
use serde::Serialize;
use crate::error::Result;
use crate::constants::{DADCAM_FOLDER, SIDECARS_FOLDER, PROXIES_FOLDER, THUMBS_FOLDER, SPRITES_FOLDER};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarData {
    pub original_file_path: String,
    pub file_hash_blake3: Option<String>,
    // Raw dumps (Layer 0: immutable baseline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_exif_dump: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ffprobe: Option<serde_json::Value>,
    // Extraction status (G7)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_status: Option<ExtractionStatus>,
    pub metadata_snapshot: MetadataSnapshot,
    // Extended metadata from exiftool + ffprobe (Layer 1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended_metadata: Option<ExtendedMetadata>,
    pub camera_match: CameraMatchSnapshot,
    // Match audit trail (Layer 5)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_audit: Option<MatchAudit>,
    pub ingest_timestamps: IngestTimestamps,
    pub derived_asset_paths: DerivedAssetPaths,
    pub rental_audit: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionStatus {
    pub status: String,
    pub exiftool: ToolExtractionStatus,
    pub ffprobe: ToolExtractionStatus,
    pub extracted_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolExtractionStatus {
    pub success: bool,
    pub exit_code: i32,
    pub error: Option<String>,
    pub pipeline_version: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataSnapshot {
    pub media_type: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_sample_rate: Option<i32>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub recorded_at: Option<String>,
    pub timestamp_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendedMetadata {
    // From exiftool
    pub sensor_type: Option<String>,
    pub focal_length: Option<f64>,
    pub focal_length_35mm: Option<f64>,
    pub scale_factor: Option<f64>,
    pub native_width: Option<i32>,
    pub native_height: Option<i32>,
    pub bits_per_sample: Option<i32>,
    pub exif_color_space: Option<String>,
    pub white_balance: Option<String>,
    pub lens_model: Option<String>,
    pub lens_id: Option<String>,
    pub megapixels: Option<f64>,
    pub rotation: Option<f64>,
    pub compressor_id: Option<String>,
    // From ffprobe
    pub field_order: Option<String>,
    pub bits_per_raw_sample: Option<String>,
    pub color_space: Option<String>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub sample_aspect_ratio: Option<String>,
    pub codec_profile: Option<String>,
    pub codec_level: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraMatchSnapshot {
    pub device_id: Option<i64>,
    pub profile_id: Option<i64>,
    pub confidence: f64,
    pub reason: String,
    pub profile_type: Option<String>,
    pub profile_ref: Option<String>,
    pub device_uuid: Option<String>,
}

/// Match audit trail (Layer 5: full audit of every matching decision)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchAudit {
    pub matched_at: String,
    pub matcher_version: u32,
    pub match_source: String,
    pub input_signature: MatchInputSignature,
    pub candidates: Vec<MatchCandidate>,
    pub winner: MatchWinner,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchInputSignature {
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub codec: Option<String>,
    pub container: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub fps: Option<f64>,
    pub field_order: Option<String>,
    pub compressor_id: Option<String>,
    pub folder_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCandidate {
    pub slug: String,
    pub score: f64,
    pub rejected: bool,
    pub reject_reason: Option<String>,
    pub matched_rules: Vec<String>,
    pub failed_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchWinner {
    pub slug: String,
    pub confidence: f64,
    pub assignment_reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestTimestamps {
    pub discovered_at: String,
    pub copied_at: String,
    pub indexed_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivedAssetPaths {
    pub proxy: Option<String>,
    pub thumb: Option<String>,
    pub sprite: Option<String>,
}

/// Build expected derived asset paths for a clip (jobs may not have run yet).
pub fn expected_derived_paths(library_root: &Path, clip_id: i64) -> DerivedAssetPaths {
    let dadcam = library_root.join(DADCAM_FOLDER);
    DerivedAssetPaths {
        proxy: Some(dadcam.join(PROXIES_FOLDER).join(format!("{}.mp4", clip_id)).to_string_lossy().to_string()),
        thumb: Some(dadcam.join(THUMBS_FOLDER).join(format!("{}.jpg", clip_id)).to_string_lossy().to_string()),
        sprite: Some(dadcam.join(SPRITES_FOLDER).join(format!("{}", clip_id)).to_string_lossy().to_string()),
    }
}

/// Write a sidecar JSON file for a clip into .dadcam/sidecars/
/// Uses atomic temp-fsync-rename to prevent half-written files on crash.
pub fn write_sidecar(library_root: &Path, clip_id: i64, data: &SidecarData) -> Result<()> {
    let sidecars_dir = library_root
        .join(DADCAM_FOLDER)
        .join(SIDECARS_FOLDER);
    std::fs::create_dir_all(&sidecars_dir)?;

    let filename = format!("{}.json", clip_id);
    let final_path = sidecars_dir.join(&filename);
    let tmp_filename = format!(".tmp_{}.json", clip_id);
    let tmp_path = sidecars_dir.join(&tmp_filename);

    // 1. Serialize to JSON
    let json = serde_json::to_string_pretty(data)?;

    // 2. Validate: round-trip parse to confirm valid JSON before writing
    let _: serde_json::Value = serde_json::from_str(&json)?;

    // 3. Write to temp file
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(json.as_bytes())?;
        // 4. fsync the file
        file.sync_all()?;
    }

    // 5. Atomic rename: tmp -> final
    std::fs::rename(&tmp_path, &final_path)?;

    // 6. fsync parent directory (best-effort, not all platforms require this)
    if let Ok(dir) = std::fs::File::open(&sidecars_dir) {
        let _ = dir.sync_all();
    }

    Ok(())
}
