// Bundled camera profile database loader
// Loads profiles from resources/cameras/canonical.json at startup
// System works without this file (falls back to DB defaults)

use rusqlite::Connection;
use crate::error::Result;
use super::{CameraProfile, MatchRules, TransformRules, insert_profile};

/// Load bundled camera profiles from a JSON file and insert any missing ones into the DB.
/// Returns the count of newly inserted profiles.
pub fn load_bundled_profiles(conn: &Connection, json_path: &std::path::Path) -> Result<u32> {
    if !json_path.exists() {
        return Ok(0);
    }

    let content = std::fs::read_to_string(json_path)
        .map_err(|e| crate::error::DadCamError::Io(e))?;

    let entries: Vec<BundledProfileEntry> = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Warning: Failed to parse canonical.json: {}", e);
            return Ok(0);
        }
    };

    let mut inserted = 0u32;

    for entry in entries {
        // Check if profile already exists by name
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM camera_profiles WHERE name = ?1)",
            [&entry.name],
            |row| row.get(0),
        )?;

        if !exists {
            let profile = CameraProfile {
                id: 0,
                name: entry.name,
                version: entry.version.unwrap_or(1),
                match_rules: entry.match_rules.unwrap_or_default(),
                transform_rules: entry.transform_rules.unwrap_or_default(),
            };
            insert_profile(conn, &profile)?;
            inserted += 1;
        }
    }

    Ok(inserted)
}

/// Auto-load bundled camera profiles from known paths.
/// Tries several locations (dev, production). Silently returns 0 if not found.
pub fn auto_load_bundled_profiles(conn: &Connection) -> u32 {
    // Try known locations for canonical.json
    let candidates = [
        // Dev mode: relative to CWD
        std::path::PathBuf::from("resources/cameras/canonical.json"),
        // macOS app bundle: adjacent to executable
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../Resources/resources/cameras/canonical.json")))
            .unwrap_or_default(),
        // Fallback: next to executable
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("resources/cameras/canonical.json")))
            .unwrap_or_default(),
    ];

    for path in &candidates {
        if path.exists() {
            match load_bundled_profiles(conn, path) {
                Ok(count) => {
                    if count > 0 {
                        eprintln!("Loaded {} bundled camera profiles from {}", count, path.display());
                    }
                    return count;
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load bundled profiles from {}: {}", path.display(), e);
                }
            }
        }
    }

    0
}

/// Serializable entry from canonical.json
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundledProfileEntry {
    name: String,
    version: Option<i32>,
    match_rules: Option<MatchRules>,
    transform_rules: Option<TransformRules>,
}
