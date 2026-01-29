// Unified camera matching engine
// Priority order:
// 1. Custom device by USB fingerprint (100%)
// 2. Custom device by serial number (95%)
// 3. Custom device by make+model + strong heuristics (80%)
// 4. Bundled profile by make+model (80%)
// 5. Bundled profile by filename pattern (70%)
// 6. Unknown -- generic profile silently

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::metadata::MediaMetadata;
use super::devices::{self, CameraDevice};
use super::{match_camera_profile, MatchResult};

/// Result of unified camera matching (device + profile)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraMatchResult {
    pub device_id: Option<i64>,
    pub profile_id: Option<i64>,
    pub profile_name: Option<String>,
    pub device_label: Option<String>,
    pub confidence: f64,
    pub reason: String,
}

/// Unified camera matching: tries device matching first, then profile matching.
/// Never returns an error for unknown cameras -- falls back silently to generic.
pub fn match_camera(
    conn: &Connection,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
    usb_fingerprints: Option<&[String]>,
) -> CameraMatchResult {
    // Priority 1: Custom device by USB fingerprint (100% confidence)
    if let Some(fps) = usb_fingerprints {
        for fp in fps {
            if let Ok(Some(device)) = devices::find_device_by_usb_fingerprint(conn, fp) {
                return device_match_result(&device, 1.0, "USB fingerprint match");
            }
        }
    }

    // Priority 2: Custom device by serial number (95% confidence)
    if let Some(ref serial) = metadata.serial_number {
        if let Ok(Some(device)) = devices::find_device_by_serial(conn, serial) {
            return device_match_result(&device, 0.95, "Serial number match");
        }
    }

    // Priority 3: Custom device by make+model heuristics (80% confidence)
    if let (Some(ref make), Some(ref model)) = (&metadata.camera_make, &metadata.camera_model) {
        if let (Ok(all_devices), Ok(all_profiles)) = (devices::get_all_devices(conn), super::get_all_profiles(conn)) {
            for device in &all_devices {
                if let Some(profile_id) = device.profile_id {
                    if let Some(profile) = all_profiles.iter().find(|p| p.id == profile_id) {
                        let make_match = profile.match_rules.make.as_ref()
                            .map_or(false, |makes| makes.iter().any(|m| make.to_lowercase().contains(&m.to_lowercase())));
                        let model_match = profile.match_rules.model.as_ref()
                            .map_or(false, |models| models.iter().any(|m| model.to_lowercase().contains(&m.to_lowercase())));

                        if make_match && model_match {
                            return device_match_result(device, 0.80, "Make+model match to registered device");
                        }
                    }
                }
            }
        }
    }

    // Priority 4-5: Bundled/DB profile matching (via existing match_camera_profile)
    if let Ok(Some(profile_match)) = match_camera_profile(conn, metadata, source_folder) {
        return profile_match_result(&profile_match);
    }

    // Priority 6: Unknown -- generic fallback, silent
    CameraMatchResult {
        device_id: None,
        profile_id: None,
        profile_name: None,
        device_label: None,
        confidence: 0.0,
        reason: "No camera match (generic fallback)".to_string(),
    }
}

fn device_match_result(device: &CameraDevice, confidence: f64, reason: &str) -> CameraMatchResult {
    CameraMatchResult {
        device_id: Some(device.id),
        profile_id: device.profile_id,
        profile_name: None, // Could be resolved but not needed at match time
        device_label: device.fleet_label.clone(),
        confidence,
        reason: reason.to_string(),
    }
}

fn profile_match_result(profile_match: &MatchResult) -> CameraMatchResult {
    CameraMatchResult {
        device_id: None,
        profile_id: Some(profile_match.profile_id),
        profile_name: Some(profile_match.profile_name.clone()),
        device_label: None,
        confidence: profile_match.confidence,
        reason: profile_match.reasons.join(", "),
    }
}
