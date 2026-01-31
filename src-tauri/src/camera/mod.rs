// Camera profile matching module

pub mod devices;
pub mod matcher;
pub mod bundled;
pub mod registration;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use crate::error::Result;
use crate::metadata::MediaMetadata;

/// Camera profile definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraProfile {
    pub id: i64,
    pub name: String,
    pub version: i32,
    pub match_rules: MatchRules,
    pub transform_rules: TransformRules,
}

/// Rules for matching a camera profile
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatchRules {
    pub make: Option<Vec<String>>,
    pub model: Option<Vec<String>>,
    pub codec: Option<Vec<String>>,
    pub container: Option<Vec<String>>,
    pub folder_pattern: Option<String>,
    pub resolution: Option<Resolution>,
}

/// Resolution matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// Transform rules for processing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformRules {
    pub deinterlace: Option<bool>,
    pub deinterlace_mode: Option<String>,
    pub color_space: Option<String>,
    pub lut: Option<String>,
    /// true=apply rotation correction, false=skip, None=auto-detect from metadata
    pub rotation_fix: Option<bool>,
    /// "tff", "bff", "auto", or None=auto-detect from ffprobe field_order
    pub field_order: Option<String>,
}

/// Match result with confidence score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub profile_id: i64,
    pub profile_name: String,
    pub confidence: f64,
    pub reasons: Vec<String>,
}

/// Match a clip against all camera profiles
pub fn match_camera_profile(
    conn: &Connection,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> Result<Option<MatchResult>> {
    // Get all profiles
    let profiles = get_all_profiles(conn)?;

    let mut best_match: Option<MatchResult> = None;

    for profile in profiles {
        let result = score_profile_match(&profile, metadata, source_folder);
        if result.confidence > 0.0 {
            if best_match.is_none() || result.confidence > best_match.as_ref().unwrap().confidence {
                best_match = Some(result);
            }
        }
    }

    Ok(best_match)
}

/// Score how well a profile matches the metadata
fn score_profile_match(
    profile: &CameraProfile,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> MatchResult {
    let mut confidence = 0.0;
    let mut reasons = Vec::new();
    let mut matches = 0;
    let mut total_rules = 0;

    // Check make
    if let Some(ref makes) = profile.match_rules.make {
        total_rules += 1;
        if let Some(ref camera_make) = metadata.camera_make {
            if makes.iter().any(|m| camera_make.to_lowercase().contains(&m.to_lowercase())) {
                matches += 1;
                reasons.push(format!("Make matches: {}", camera_make));
            }
        }
    }

    // Check model
    if let Some(ref models) = profile.match_rules.model {
        total_rules += 1;
        if let Some(ref camera_model) = metadata.camera_model {
            if models.iter().any(|m| camera_model.to_lowercase().contains(&m.to_lowercase())) {
                matches += 1;
                reasons.push(format!("Model matches: {}", camera_model));
            }
        }
    }

    // Check codec
    if let Some(ref codecs) = profile.match_rules.codec {
        total_rules += 1;
        if let Some(ref codec) = metadata.codec {
            if codecs.iter().any(|c| codec.to_lowercase() == c.to_lowercase()) {
                matches += 1;
                reasons.push(format!("Codec matches: {}", codec));
            }
        }
    }

    // Check container
    if let Some(ref containers) = profile.match_rules.container {
        total_rules += 1;
        if let Some(ref container) = metadata.container {
            // ffprobe format_name can be comma-separated (e.g., "mpegts,mov,mp4")
            let container_parts: Vec<&str> = container.split(',').collect();
            if containers.iter().any(|c| {
                let c_lower = c.to_lowercase();
                container_parts.iter().any(|p| p.trim().to_lowercase() == c_lower)
            }) {
                matches += 1;
                reasons.push(format!("Container matches: {}", container));
            }
        }
    }

    // Check folder pattern
    if let Some(ref pattern) = profile.match_rules.folder_pattern {
        total_rules += 1;
        if let Some(folder) = source_folder {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(folder) {
                    matches += 1;
                    reasons.push("Folder pattern matches".to_string());
                }
            }
        }
    }

    // Check resolution
    if let Some(ref res) = profile.match_rules.resolution {
        total_rules += 1;
        let width_match = res.width.map_or(true, |w| metadata.width == Some(w));
        let height_match = res.height.map_or(true, |h| metadata.height == Some(h));
        if width_match && height_match {
            matches += 1;
            reasons.push(format!("Resolution matches: {:?}x{:?}", metadata.width, metadata.height));
        }
    }

    // Calculate confidence
    if total_rules > 0 {
        confidence = matches as f64 / total_rules as f64;
    }

    MatchResult {
        profile_id: profile.id,
        profile_name: profile.name.clone(),
        confidence,
        reasons,
    }
}

/// Get all camera profiles from database
pub fn get_all_profiles(conn: &Connection) -> Result<Vec<CameraProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, version, match_rules, transform_rules FROM camera_profiles ORDER BY name"
    )?;

    let profiles = stmt.query_map([], |row| {
        let match_rules_str: String = row.get(3)?;
        let transform_rules_str: String = row.get(4)?;

        Ok(CameraProfile {
            id: row.get(0)?,
            name: row.get(1)?,
            version: row.get(2)?,
            match_rules: serde_json::from_str(&match_rules_str).unwrap_or_default(),
            transform_rules: serde_json::from_str(&transform_rules_str).unwrap_or_default(),
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(profiles)
}

/// Insert a camera profile
pub fn insert_profile(conn: &Connection, profile: &CameraProfile) -> Result<i64> {
    conn.execute(
        "INSERT INTO camera_profiles (name, version, match_rules, transform_rules)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            profile.name,
            profile.version,
            serde_json::to_string(&profile.match_rules)?,
            serde_json::to_string(&profile.transform_rules)?,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Insert default camera profiles
pub fn insert_default_profiles(conn: &Connection) -> Result<()> {
    let profiles = vec![
        CameraProfile {
            id: 0,
            name: "Sony Handycam (AVCHD)".to_string(),
            version: 1,
            match_rules: MatchRules {
                make: Some(vec!["Sony".to_string()]),
                codec: Some(vec!["h264".to_string()]),
                folder_pattern: Some(r"AVCHD|BDMV".to_string()),
                ..Default::default()
            },
            transform_rules: TransformRules {
                deinterlace: Some(true),
                deinterlace_mode: Some("yadif".to_string()),
                ..Default::default()
            },
        },
        CameraProfile {
            id: 0,
            name: "Canon DSLR".to_string(),
            version: 1,
            match_rules: MatchRules {
                make: Some(vec!["Canon".to_string()]),
                codec: Some(vec!["h264".to_string()]),
                ..Default::default()
            },
            transform_rules: TransformRules::default(),
        },
        CameraProfile {
            id: 0,
            name: "Panasonic MiniDV".to_string(),
            version: 1,
            match_rules: MatchRules {
                make: Some(vec!["Panasonic".to_string()]),
                codec: Some(vec!["dvvideo".to_string(), "dv".to_string()]),
                ..Default::default()
            },
            transform_rules: TransformRules {
                deinterlace: Some(true),
                ..Default::default()
            },
        },
    ];

    for profile in profiles {
        // Check if profile already exists
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM camera_profiles WHERE name = ?1)",
            [&profile.name],
            |row| row.get(0),
        )?;

        if !exists {
            insert_profile(conn, &profile)?;
        }
    }

    Ok(())
}
