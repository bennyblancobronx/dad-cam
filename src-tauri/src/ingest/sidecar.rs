// Sidecar JSON writer
// Writes per-clip metadata snapshots to .dadcam/sidecars/<clip_id>.json
// Schema matches v0.2.0 spec: nested structure with metadata_snapshot,
// camera_match, ingest_timestamps, derived_asset_paths, rental_audit.

use std::path::Path;
use serde::Serialize;
use crate::error::Result;
use crate::constants::{DADCAM_FOLDER, SIDECARS_FOLDER, PROXIES_FOLDER, THUMBS_FOLDER, SPRITES_FOLDER};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarData {
    pub original_file_path: String,
    pub file_hash_blake3: Option<String>,
    pub metadata_snapshot: MetadataSnapshot,
    pub camera_match: CameraMatchSnapshot,
    pub ingest_timestamps: IngestTimestamps,
    pub derived_asset_paths: DerivedAssetPaths,
    pub rental_audit: Option<serde_json::Value>,
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
pub struct CameraMatchSnapshot {
    pub device_id: Option<i64>,
    pub profile_id: Option<i64>,
    pub confidence: f64,
    pub reason: String,
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
pub fn write_sidecar(library_root: &Path, clip_id: i64, data: &SidecarData) -> Result<()> {
    let sidecars_dir = library_root
        .join(DADCAM_FOLDER)
        .join(SIDECARS_FOLDER);
    std::fs::create_dir_all(&sidecars_dir)?;

    let filename = format!("{}.json", clip_id);
    let path = sidecars_dir.join(filename);

    let json = serde_json::to_string_pretty(data)?;
    std::fs::write(&path, json)?;

    Ok(())
}
