// Dad Cam - Licensing Commands
// 4 Tauri commands for license state, activation, deactivation, and feature gating

use crate::licensing;

/// Get current license state
#[tauri::command]
pub fn get_license_state() -> Result<licensing::LicenseState, String> {
    Ok(licensing::check_license())
}

/// Activate a license key
#[tauri::command]
pub fn activate_license(key: String) -> Result<licensing::LicenseState, String> {
    licensing::activate_key(&key)
}

/// Deactivate the current license (remove key from keychain)
/// Returns the new license state (trial) so frontend can sync cache
#[tauri::command]
pub fn deactivate_license() -> Result<licensing::LicenseState, String> {
    licensing::deactivate()?;
    Ok(licensing::check_license())
}

/// Check if a specific feature is allowed under the current license
#[tauri::command]
pub fn is_feature_allowed(feature: String) -> Result<bool, String> {
    Ok(licensing::is_allowed(&feature))
}
