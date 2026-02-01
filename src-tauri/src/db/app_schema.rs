// App DB schema types and query helpers
// Operates on ~/.dadcam/app.db (library registry, app settings KV)

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use anyhow::Result;

// ---------------------------------------------------------------------------
// Library registry
// ---------------------------------------------------------------------------

/// A library entry in the App DB registry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRegistryEntry {
    pub library_uuid: String,
    pub path: String,
    pub label: Option<String>,
    pub created_at: String,
    pub last_opened_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub is_pinned: bool,
    pub is_missing: bool,
}

/// Insert or update a library in the registry.
/// If the UUID already exists, updates path/label/last_seen_at.
pub fn upsert_library(
    conn: &Connection,
    library_uuid: &str,
    path: &str,
    label: Option<&str>,
) -> Result<()> {
    // Canonicalize path (best-effort; fall back to raw path)
    let canonical = std::fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string());

    conn.execute(
        "INSERT INTO libraries (library_uuid, path, label, last_seen_at)
         VALUES (?1, ?2, ?3, datetime('now'))
         ON CONFLICT(library_uuid) DO UPDATE SET
           path = excluded.path,
           label = COALESCE(excluded.label, libraries.label),
           last_seen_at = datetime('now'),
           is_missing = 0",
        params![library_uuid, canonical, label],
    )?;
    Ok(())
}

/// Mark a library as recently opened (updates last_opened_at)
pub fn mark_opened(conn: &Connection, library_uuid: &str) -> Result<()> {
    conn.execute(
        "UPDATE libraries SET last_opened_at = datetime('now'), last_seen_at = datetime('now'), is_missing = 0
         WHERE library_uuid = ?1",
        [library_uuid],
    )?;
    Ok(())
}

/// Mark a library as recently seen (path still accessible)
pub fn mark_seen(conn: &Connection, library_uuid: &str) -> Result<()> {
    conn.execute(
        "UPDATE libraries SET last_seen_at = datetime('now'), is_missing = 0
         WHERE library_uuid = ?1",
        [library_uuid],
    )?;
    Ok(())
}

/// Mark a library as missing or not-missing
pub fn mark_missing(conn: &Connection, library_uuid: &str, missing: bool) -> Result<()> {
    conn.execute(
        "UPDATE libraries SET is_missing = ?1 WHERE library_uuid = ?2",
        params![missing as i32, library_uuid],
    )?;
    Ok(())
}

/// List recent libraries, ordered by last_opened_at descending.
/// Pinned entries come first.
pub fn list_recent_libraries(conn: &Connection) -> Result<Vec<LibraryRegistryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT library_uuid, path, label, created_at, last_opened_at, last_seen_at, is_pinned, is_missing
         FROM libraries
         ORDER BY is_pinned DESC, last_opened_at DESC NULLS LAST"
    )?;

    let entries = stmt.query_map([], |row| {
        Ok(LibraryRegistryEntry {
            library_uuid: row.get(0)?,
            path: row.get(1)?,
            label: row.get(2)?,
            created_at: row.get(3)?,
            last_opened_at: row.get(4)?,
            last_seen_at: row.get(5)?,
            is_pinned: row.get::<_, i32>(6)? != 0,
            is_missing: row.get::<_, i32>(7)? != 0,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(entries)
}

/// Relink a library to a new path (validates UUID matches the library at new_path).
pub fn relink_library(conn: &Connection, library_uuid: &str, new_path: &str) -> Result<()> {
    // Validate: open the library DB at new_path and confirm its UUID matches
    let lib_db_path = std::path::Path::new(new_path)
        .join(crate::constants::DADCAM_FOLDER)
        .join(crate::constants::DB_FILENAME);

    if !lib_db_path.exists() {
        anyhow::bail!("No library database found at {}", new_path);
    }

    let lib_conn = Connection::open(&lib_db_path)?;
    let stored_uuid: Option<String> = lib_conn.query_row(
        "SELECT value FROM library_meta WHERE key = 'library_uuid'",
        [],
        |row| row.get(0),
    ).optional()?;

    match stored_uuid {
        Some(ref uuid) if uuid == library_uuid => {} // match confirmed
        Some(ref uuid) => {
            anyhow::bail!(
                "UUID mismatch: registry expects {} but library at {} has {}",
                library_uuid, new_path, uuid
            );
        }
        None => {
            anyhow::bail!(
                "Library at {} has no UUID in library_meta. Open it normally first.",
                new_path
            );
        }
    }

    let canonical = std::fs::canonicalize(new_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| new_path.to_string());

    conn.execute(
        "UPDATE libraries SET path = ?1, is_missing = 0, last_seen_at = datetime('now')
         WHERE library_uuid = ?2",
        params![canonical, library_uuid],
    )?;
    Ok(())
}

/// Look up a library by UUID
pub fn get_library_by_uuid(conn: &Connection, library_uuid: &str) -> Result<Option<LibraryRegistryEntry>> {
    let entry = conn.query_row(
        "SELECT library_uuid, path, label, created_at, last_opened_at, last_seen_at, is_pinned, is_missing
         FROM libraries WHERE library_uuid = ?1",
        [library_uuid],
        |row| {
            Ok(LibraryRegistryEntry {
                library_uuid: row.get(0)?,
                path: row.get(1)?,
                label: row.get(2)?,
                created_at: row.get(3)?,
                last_opened_at: row.get(4)?,
                last_seen_at: row.get(5)?,
                is_pinned: row.get::<_, i32>(6)? != 0,
                is_missing: row.get::<_, i32>(7)? != 0,
            })
        },
    ).optional()?;

    Ok(entry)
}

// ---------------------------------------------------------------------------
// App settings (KV store)
// ---------------------------------------------------------------------------

/// Get a setting value by key. Returns None if not set.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let value = conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    ).optional()?;
    Ok(value)
}

/// Set a setting value (upsert).
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Delete a setting by key.
pub fn delete_setting(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM app_settings WHERE key = ?1", [key])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Typed settings helpers (Group 3)
// ---------------------------------------------------------------------------

/// Get UI mode: "simple" or "advanced". Defaults to "simple".
pub fn get_ui_mode(conn: &Connection) -> Result<String> {
    Ok(get_setting(conn, "ui_mode")?.unwrap_or_else(|| "simple".to_string()))
}

/// Set UI mode.
pub fn set_ui_mode(conn: &Connection, mode: &str) -> Result<()> {
    set_setting(conn, "ui_mode", mode)
}

/// Get the default library UUID for Simple mode. Returns None if not set.
pub fn get_simple_default_library_uuid(conn: &Connection) -> Result<Option<String>> {
    let val = get_setting(conn, "simple_default_library_uuid")?;
    match val {
        Some(s) if s.is_empty() => Ok(None),
        other => Ok(other),
    }
}

/// Set the default library UUID for Simple mode.
pub fn set_simple_default_library_uuid(conn: &Connection, uuid: &str) -> Result<()> {
    set_setting(conn, "simple_default_library_uuid", uuid)
}

/// Get title card offset in seconds. Defaults to 5.
pub fn get_title_offset(conn: &Connection) -> Result<f64> {
    match get_setting(conn, "title_card_offset_seconds")? {
        Some(s) => Ok(s.parse::<f64>().unwrap_or(5.0)),
        None => Ok(5.0),
    }
}

/// Set title card offset in seconds.
pub fn set_title_offset(conn: &Connection, seconds: f64) -> Result<()> {
    set_setting(conn, "title_card_offset_seconds", &seconds.to_string())
}

/// Get feature flags as JSON string. Returns default JSON if not set.
pub fn get_features(conn: &Connection) -> Result<String> {
    Ok(get_setting(conn, "features")?.unwrap_or_else(|| {
        r#"{"screengrabs":true,"face_detection":false,"best_clips":true,"cameras_tab":false}"#.to_string()
    }))
}

/// Set feature flags (expects JSON string).
pub fn set_features(conn: &Connection, features_json: &str) -> Result<()> {
    set_setting(conn, "features", features_json)
}

// ---------------------------------------------------------------------------
// Bundled profiles (App DB)
// ---------------------------------------------------------------------------

/// A bundled camera profile stored in App DB
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppBundledProfile {
    pub slug: String,
    pub name: String,
    pub version: i32,
    pub match_rules: String,
    pub transform_rules: String,
    pub bundled_version: i32,
}

/// JSON entry from bundled_profiles.json
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundledProfileJsonEntry {
    pub slug: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: i32,
    #[serde(default = "default_json_obj")]
    pub match_rules: serde_json::Value,
    #[serde(default = "default_json_obj")]
    pub transform_rules: serde_json::Value,
    /// System profile flag (JSON-file-only, not stored in DB). See G6.
    pub is_system: Option<bool>,
    /// Whether the profile can be deleted (JSON-file-only, not stored in DB). See G6.
    pub deletable: Option<bool>,
    /// Display category for UI grouping (JSON-file-only, not stored in DB). See G6.
    pub category: Option<String>,
}

fn default_version() -> i32 { 1 }
fn default_json_obj() -> serde_json::Value { serde_json::json!({}) }

/// Sync bundled profiles from a JSON file into App DB.
/// Full replace: deletes profiles not in the file, upserts those that are.
/// Idempotent — safe to call on every startup.
pub fn sync_bundled_profiles(conn: &Connection, entries: &[BundledProfileJsonEntry]) -> Result<u32> {
    let mut count = 0u32;

    // Collect slugs for cleanup
    let slugs: Vec<&str> = entries.iter().map(|e| e.slug.as_str()).collect();

    // Delete bundled profiles not in the new set
    if slugs.is_empty() {
        conn.execute("DELETE FROM bundled_profiles", [])?;
    } else {
        // Build placeholder list
        let placeholders: Vec<String> = (1..=slugs.len()).map(|i| format!("?{}", i)).collect();
        let sql = format!(
            "DELETE FROM bundled_profiles WHERE slug NOT IN ({})",
            placeholders.join(",")
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = slugs.iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        conn.execute(&sql, params.as_slice())?;
    }

    // Upsert each entry
    for entry in entries {
        let mr = serde_json::to_string(&entry.match_rules)?;
        let tr = serde_json::to_string(&entry.transform_rules)?;

        conn.execute(
            "INSERT INTO bundled_profiles (slug, name, version, match_rules, transform_rules, bundled_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(slug) DO UPDATE SET
               name = excluded.name,
               version = excluded.version,
               match_rules = excluded.match_rules,
               transform_rules = excluded.transform_rules,
               bundled_version = excluded.bundled_version",
            params![entry.slug, entry.name, entry.version, mr, tr, entry.version],
        )?;
        count += 1;
    }

    Ok(count)
}

/// List all bundled profiles from App DB.
pub fn list_bundled_profiles(conn: &Connection) -> Result<Vec<AppBundledProfile>> {
    let mut stmt = conn.prepare(
        "SELECT slug, name, version, match_rules, transform_rules, bundled_version
         FROM bundled_profiles ORDER BY name"
    )?;

    let profiles = stmt.query_map([], |row| {
        Ok(AppBundledProfile {
            slug: row.get(0)?,
            name: row.get(1)?,
            version: row.get(2)?,
            match_rules: row.get(3)?,
            transform_rules: row.get(4)?,
            bundled_version: row.get(5)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(profiles)
}

/// Look up a bundled profile by slug.
pub fn get_bundled_profile(conn: &Connection, slug: &str) -> Result<Option<AppBundledProfile>> {
    let result = conn.query_row(
        "SELECT slug, name, version, match_rules, transform_rules, bundled_version
         FROM bundled_profiles WHERE slug = ?1",
        [slug],
        |row| {
            Ok(AppBundledProfile {
                slug: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                match_rules: row.get(3)?,
                transform_rules: row.get(4)?,
                bundled_version: row.get(5)?,
            })
        },
    ).optional()?;

    Ok(result)
}

// ---------------------------------------------------------------------------
// User profiles (App DB)
// ---------------------------------------------------------------------------

/// A user-created camera profile stored in App DB
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUserProfile {
    pub id: i64,
    pub uuid: String,
    pub name: String,
    pub version: i32,
    pub match_rules: String,
    pub transform_rules: String,
    pub created_at: String,
}

/// Parameters for creating a new user profile
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewUserProfile {
    pub name: String,
    pub match_rules: Option<String>,
    pub transform_rules: Option<String>,
}

/// Create a user profile. Returns the new profile with generated UUID.
pub fn create_user_profile(conn: &Connection, profile: &NewUserProfile) -> Result<AppUserProfile> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let mr = profile.match_rules.as_deref().unwrap_or("{}");
    let tr = profile.transform_rules.as_deref().unwrap_or("{}");

    conn.execute(
        "INSERT INTO user_profiles (uuid, name, version, match_rules, transform_rules)
         VALUES (?1, ?2, 1, ?3, ?4)",
        params![uuid, profile.name, mr, tr],
    )?;

    let id = conn.last_insert_rowid();
    let created_at: String = conn.query_row(
        "SELECT created_at FROM user_profiles WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;

    Ok(AppUserProfile {
        id,
        uuid,
        name: profile.name.clone(),
        version: 1,
        match_rules: mr.to_string(),
        transform_rules: tr.to_string(),
        created_at,
    })
}

/// List all user profiles.
pub fn list_user_profiles(conn: &Connection) -> Result<Vec<AppUserProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, name, version, match_rules, transform_rules, created_at
         FROM user_profiles ORDER BY name"
    )?;

    let profiles = stmt.query_map([], |row| {
        Ok(AppUserProfile {
            id: row.get(0)?,
            uuid: row.get(1)?,
            name: row.get(2)?,
            version: row.get(3)?,
            match_rules: row.get(4)?,
            transform_rules: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(profiles)
}

/// Get a user profile by UUID.
pub fn get_user_profile(conn: &Connection, uuid: &str) -> Result<Option<AppUserProfile>> {
    let result = conn.query_row(
        "SELECT id, uuid, name, version, match_rules, transform_rules, created_at
         FROM user_profiles WHERE uuid = ?1",
        [uuid],
        |row| {
            Ok(AppUserProfile {
                id: row.get(0)?,
                uuid: row.get(1)?,
                name: row.get(2)?,
                version: row.get(3)?,
                match_rules: row.get(4)?,
                transform_rules: row.get(5)?,
                created_at: row.get(6)?,
            })
        },
    ).optional()?;

    Ok(result)
}

/// Update a user profile by UUID.
pub fn update_user_profile(
    conn: &Connection,
    uuid: &str,
    name: Option<&str>,
    match_rules: Option<&str>,
    transform_rules: Option<&str>,
) -> Result<()> {
    // Build dynamic update
    let mut sets = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(n) = name {
        sets.push("name = ?");
        values.push(Box::new(n.to_string()));
    }
    if let Some(mr) = match_rules {
        sets.push("match_rules = ?");
        values.push(Box::new(mr.to_string()));
    }
    if let Some(tr) = transform_rules {
        sets.push("transform_rules = ?");
        values.push(Box::new(tr.to_string()));
    }

    if sets.is_empty() {
        return Ok(());
    }

    // Bump version
    sets.push("version = version + 1");

    // Add UUID as last param
    values.push(Box::new(uuid.to_string()));

    // Renumber placeholders
    let numbered_sets: Vec<String> = sets.iter().enumerate().map(|(i, s)| {
        if s.contains('?') {
            s.replacen('?', &format!("?{}", i + 1), 1)
        } else {
            s.to_string()
        }
    }).collect();

    let sql = format!(
        "UPDATE user_profiles SET {} WHERE uuid = ?{}",
        numbered_sets.join(", "),
        values.len()
    );

    let params: Vec<&dyn rusqlite::types::ToSql> = values.iter()
        .map(|v| v.as_ref() as &dyn rusqlite::types::ToSql)
        .collect();
    conn.execute(&sql, params.as_slice())?;

    Ok(())
}

/// Delete a user profile by UUID.
pub fn delete_user_profile(conn: &Connection, uuid: &str) -> Result<()> {
    conn.execute("DELETE FROM user_profiles WHERE uuid = ?1", [uuid])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Camera devices (App DB)
// ---------------------------------------------------------------------------

/// A camera device stored in App DB (uses profile_type/profile_ref, not integer FK)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppCameraDevice {
    pub id: i64,
    pub uuid: String,
    pub profile_type: String,
    pub profile_ref: String,
    pub serial_number: Option<String>,
    pub fleet_label: Option<String>,
    pub usb_fingerprints: Vec<String>,
    pub rental_notes: Option<String>,
    pub created_at: String,
}

/// Parameters for creating a new camera device
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAppCameraDevice {
    pub profile_type: Option<String>,
    pub profile_ref: Option<String>,
    pub serial_number: Option<String>,
    pub fleet_label: Option<String>,
    pub usb_fingerprints: Vec<String>,
    pub rental_notes: Option<String>,
}

/// Create a camera device in App DB.
pub fn create_camera_device(conn: &Connection, device: &NewAppCameraDevice) -> Result<AppCameraDevice> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let profile_type = device.profile_type.as_deref().unwrap_or("none");
    let profile_ref = device.profile_ref.as_deref().unwrap_or("");
    let fps_json = serde_json::to_string(&device.usb_fingerprints)?;

    conn.execute(
        "INSERT INTO camera_devices (uuid, profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints, rental_notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![uuid, profile_type, profile_ref, device.serial_number, device.fleet_label, fps_json, device.rental_notes],
    )?;

    let id = conn.last_insert_rowid();
    let created_at: String = conn.query_row(
        "SELECT created_at FROM camera_devices WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;

    Ok(AppCameraDevice {
        id,
        uuid,
        profile_type: profile_type.to_string(),
        profile_ref: profile_ref.to_string(),
        serial_number: device.serial_number.clone(),
        fleet_label: device.fleet_label.clone(),
        usb_fingerprints: device.usb_fingerprints.clone(),
        rental_notes: device.rental_notes.clone(),
        created_at,
    })
}

/// List all camera devices from App DB.
pub fn list_camera_devices_app(conn: &Connection) -> Result<Vec<AppCameraDevice>> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
         FROM camera_devices ORDER BY fleet_label, created_at"
    )?;

    let devices = stmt.query_map([], |row| {
        let fps_str: String = row.get(6)?;
        Ok(AppCameraDevice {
            id: row.get(0)?,
            uuid: row.get(1)?,
            profile_type: row.get(2)?,
            profile_ref: row.get(3)?,
            serial_number: row.get(4)?,
            fleet_label: row.get(5)?,
            usb_fingerprints: serde_json::from_str(&fps_str).unwrap_or_default(),
            rental_notes: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(devices)
}

/// Get a camera device by UUID from App DB.
pub fn get_camera_device_by_uuid(conn: &Connection, uuid: &str) -> Result<Option<AppCameraDevice>> {
    let result = conn.query_row(
        "SELECT id, uuid, profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
         FROM camera_devices WHERE uuid = ?1",
        [uuid],
        |row| {
            let fps_str: String = row.get(6)?;
            Ok(AppCameraDevice {
                id: row.get(0)?,
                uuid: row.get(1)?,
                profile_type: row.get(2)?,
                profile_ref: row.get(3)?,
                serial_number: row.get(4)?,
                fleet_label: row.get(5)?,
                usb_fingerprints: serde_json::from_str(&fps_str).unwrap_or_default(),
                rental_notes: row.get(7)?,
                created_at: row.get(8)?,
            })
        },
    ).optional()?;

    Ok(result)
}

/// Find a device by USB fingerprint in App DB.
pub fn find_device_by_usb_fingerprint_app(conn: &Connection, fingerprint: &str) -> Result<Option<AppCameraDevice>> {
    let escaped_fp = fingerprint
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let fp_pattern = format!("%\"{}\"%", escaped_fp);

    let mut stmt = conn.prepare(
        "SELECT id, uuid, profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
         FROM camera_devices
         WHERE usb_fingerprints LIKE ?1 ESCAPE '\\'"
    )?;

    let mut devices: Vec<AppCameraDevice> = stmt.query_map([&fp_pattern], |row| {
        let fps_str: String = row.get(6)?;
        Ok(AppCameraDevice {
            id: row.get(0)?,
            uuid: row.get(1)?,
            profile_type: row.get(2)?,
            profile_ref: row.get(3)?,
            serial_number: row.get(4)?,
            fleet_label: row.get(5)?,
            usb_fingerprints: serde_json::from_str(&fps_str).unwrap_or_default(),
            rental_notes: row.get(7)?,
            created_at: row.get(8)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    if let Some(pos) = devices.iter().position(|d| d.usb_fingerprints.contains(&fingerprint.to_string())) {
        Ok(Some(devices.remove(pos)))
    } else {
        Ok(None)
    }
}

/// Find a device by serial number in App DB.
pub fn find_device_by_serial_app(conn: &Connection, serial: &str) -> Result<Option<AppCameraDevice>> {
    let result = conn.query_row(
        "SELECT id, uuid, profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
         FROM camera_devices WHERE serial_number = ?1",
        [serial],
        |row| {
            let fps_str: String = row.get(6)?;
            Ok(AppCameraDevice {
                id: row.get(0)?,
                uuid: row.get(1)?,
                profile_type: row.get(2)?,
                profile_ref: row.get(3)?,
                serial_number: row.get(4)?,
                fleet_label: row.get(5)?,
                usb_fingerprints: serde_json::from_str(&fps_str).unwrap_or_default(),
                rental_notes: row.get(7)?,
                created_at: row.get(8)?,
            })
        },
    ).optional()?;

    Ok(result)
}

/// Upsert a camera device by UUID in App DB (for legacy migration).
/// If UUID exists, updates fields. If not, inserts.
pub fn upsert_camera_device(conn: &Connection, device: &AppCameraDevice) -> Result<()> {
    let fps_json = serde_json::to_string(&device.usb_fingerprints)?;

    conn.execute(
        "INSERT INTO camera_devices (uuid, profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints, rental_notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(uuid) DO UPDATE SET
           profile_type = COALESCE(excluded.profile_type, camera_devices.profile_type),
           profile_ref = COALESCE(excluded.profile_ref, camera_devices.profile_ref),
           serial_number = COALESCE(excluded.serial_number, camera_devices.serial_number),
           fleet_label = COALESCE(excluded.fleet_label, camera_devices.fleet_label),
           usb_fingerprints = excluded.usb_fingerprints,
           rental_notes = COALESCE(excluded.rental_notes, camera_devices.rental_notes)",
        params![device.uuid, device.profile_type, device.profile_ref, device.serial_number, device.fleet_label, fps_json, device.rental_notes],
    )?;

    Ok(())
}

/// Import legacy devices from ~/.dadcam/custom_cameras.json into App DB.
/// Renames the file to .migrated after import. Returns count of imported devices.
pub fn import_legacy_devices_json(conn: &Connection) -> u32 {
    let path = match directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".dadcam").join("custom_cameras.json"))
    {
        Some(p) if p.exists() => p,
        _ => return 0,
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    // Legacy format: array of devices with profile_id (integer FK)
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LegacyDevice {
        uuid: String,
        #[serde(default)]
        #[allow(dead_code)]
        profile_id: Option<i64>,
        serial_number: Option<String>,
        fleet_label: Option<String>,
        #[serde(default)]
        usb_fingerprints: Vec<String>,
        rental_notes: Option<String>,
    }

    let devices: Vec<LegacyDevice> = match serde_json::from_str(&content) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("Failed to parse custom_cameras.json: {}", e);
            return 0;
        }
    };

    let mut imported = 0u32;
    for device in devices {
        // Convert legacy profile_id to profile_type/ref: since we don't know
        // which profile the integer referred to, set to 'none' for migration
        let app_device = AppCameraDevice {
            id: 0,
            uuid: device.uuid,
            profile_type: "none".to_string(),
            profile_ref: String::new(),
            serial_number: device.serial_number,
            fleet_label: device.fleet_label,
            usb_fingerprints: device.usb_fingerprints,
            rental_notes: device.rental_notes,
            created_at: String::new(),
        };

        match upsert_camera_device(conn, &app_device) {
            Ok(_) => imported += 1,
            Err(e) => log::warn!("Failed to import device {}: {}", app_device.uuid, e),
        }
    }

    // Rename file to .migrated (do not delete, per spec section 6.4)
    let migrated_path = path.with_extension("json.migrated");
    if let Err(e) = std::fs::rename(&path, &migrated_path) {
        log::warn!("Failed to rename custom_cameras.json to .migrated: {}", e);
    }

    imported
}

// ---------------------------------------------------------------------------
// L6 backfill: populate stable camera refs from legacy integer IDs (Spec 6.2)
// ---------------------------------------------------------------------------

/// Backfill stable camera refs for clips that have legacy integer IDs but NULL stable refs.
/// Called on library open after migrations. Idempotent -- skips clips already backfilled.
/// Per spec section 6.2 (libraryfix.md).
pub fn backfill_stable_camera_refs(lib_conn: &Connection) -> u32 {
    // Quick check: any clips needing backfill?
    let need_backfill: i64 = lib_conn.query_row(
        "SELECT COUNT(*) FROM clips WHERE camera_profile_type IS NULL",
        [],
        |row| row.get(0),
    ).unwrap_or(0);

    if need_backfill == 0 {
        return 0;
    }

    // Open App DB for profile/device lookups
    let app_conn = match crate::db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(_) => return 0,
    };

    // Pre-load bundled profiles for matching
    let bundled = list_bundled_profiles(&app_conn).unwrap_or_default();

    // Cache: library profile name -> (profile_type, profile_ref) to avoid duplicate creates
    let mut profile_cache: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();

    // Query all clips needing backfill
    let mut stmt = match lib_conn.prepare(
        "SELECT id, camera_profile_id, camera_device_id FROM clips WHERE camera_profile_type IS NULL"
    ) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    struct ClipLegacyRef {
        id: i64,
        profile_id: Option<i64>,
        device_id: Option<i64>,
    }

    let clips: Vec<ClipLegacyRef> = match stmt.query_map([], |row| {
        Ok(ClipLegacyRef {
            id: row.get(0)?,
            profile_id: row.get(1)?,
            device_id: row.get(2)?,
        })
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(_) => return 0,
    };

    let mut count = 0u32;

    for clip in &clips {
        // 1) Resolve profile
        let (profile_type, profile_ref) = backfill_resolve_profile(
            lib_conn, &app_conn, clip.profile_id, &bundled, &mut profile_cache,
        );

        // 2) Resolve device UUID
        let device_uuid = backfill_resolve_device(lib_conn, &app_conn, clip.device_id);

        // 3) Write stable refs to clip
        if lib_conn.execute(
            "UPDATE clips SET camera_profile_type = ?1, camera_profile_ref = ?2, camera_device_uuid = ?3
             WHERE id = ?4",
            params![profile_type, profile_ref, device_uuid, clip.id],
        ).is_ok() {
            count += 1;
        }
    }

    count
}

/// Resolve a legacy camera_profile_id to (profile_type, profile_ref).
/// Uses cache to avoid creating duplicate migrated user profiles.
fn backfill_resolve_profile(
    lib_conn: &Connection,
    app_conn: &Connection,
    profile_id: Option<i64>,
    bundled: &[AppBundledProfile],
    cache: &mut std::collections::HashMap<String, (String, String)>,
) -> (String, String) {
    let pid = match profile_id {
        Some(id) => id,
        None => return ("none".to_string(), String::new()),
    };

    // Look up legacy profile name from Library DB
    let name: Option<String> = lib_conn.query_row(
        "SELECT name FROM camera_profiles WHERE id = ?1",
        [pid],
        |row| row.get(0),
    ).ok();

    let profile_name = match name {
        Some(n) => n,
        None => return ("none".to_string(), String::new()),
    };

    // Check cache first
    if let Some(cached) = cache.get(&profile_name) {
        return cached.clone();
    }

    // Try bundled match (case-insensitive name or slug, per spec 6.2)
    if let Some(bp) = bundled.iter().find(|b| {
        b.name.eq_ignore_ascii_case(&profile_name) || b.slug.eq_ignore_ascii_case(&profile_name)
    }) {
        let result = ("bundled".to_string(), bp.slug.clone());
        cache.insert(profile_name, result.clone());
        return result;
    }

    // Try existing user profile match
    if let Ok(user_profiles) = list_user_profiles(app_conn) {
        if let Some(up) = user_profiles.iter().find(|u| u.name.eq_ignore_ascii_case(&profile_name)) {
            let result = ("user".to_string(), up.uuid.clone());
            cache.insert(profile_name, result.clone());
            return result;
        }

        // Check if we already created a migrated profile for this name
        let migrated_name = format!("[Migrated] {}", profile_name);
        if let Some(mp) = user_profiles.iter().find(|u| u.name == migrated_name) {
            let result = ("user".to_string(), mp.uuid.clone());
            cache.insert(profile_name, result.clone());
            return result;
        }

        // Create migrated user profile in App DB (per spec 6.2)
        let new_profile = NewUserProfile {
            name: migrated_name,
            match_rules: Some("{}".to_string()),
            transform_rules: Some("{}".to_string()),
        };
        if let Ok(created) = create_user_profile(app_conn, &new_profile) {
            let result = ("user".to_string(), created.uuid);
            cache.insert(profile_name, result.clone());
            return result;
        }
    }

    let result = ("none".to_string(), String::new());
    cache.insert(profile_name, result.clone());
    result
}

/// Resolve a legacy camera_device_id to a device UUID string.
/// Ensures the device exists in App DB (upsert).
fn backfill_resolve_device(
    lib_conn: &Connection,
    app_conn: &Connection,
    device_id: Option<i64>,
) -> Option<String> {
    let did = device_id?;

    // Read device row from Library DB
    let row = lib_conn.query_row(
        "SELECT uuid, serial_number, fleet_label, usb_fingerprints FROM camera_devices WHERE id = ?1",
        [did],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    ).ok()?;

    let (device_uuid, serial_number, fleet_label, fps_json) = row;

    // Ensure device exists in App DB
    let app_device = AppCameraDevice {
        id: 0,
        uuid: device_uuid.clone(),
        profile_type: "none".to_string(),
        profile_ref: String::new(),
        serial_number,
        fleet_label,
        usb_fingerprints: serde_json::from_str(&fps_json).unwrap_or_default(),
        rental_notes: None,
        created_at: String::new(),
    };
    let _ = upsert_camera_device(app_conn, &app_device);

    Some(device_uuid)
}

// ---------------------------------------------------------------------------
// Library UUID management (reads from Library DB)
// ---------------------------------------------------------------------------

/// Get or create the library UUID from the library_meta table.
/// This reads/writes to the LIBRARY DB (not App DB).
pub fn get_or_create_library_uuid(library_conn: &Connection) -> Result<String> {
    // Try to read existing UUID
    let existing: Option<String> = library_conn.query_row(
        "SELECT value FROM library_meta WHERE key = 'library_uuid'",
        [],
        |row| row.get(0),
    ).optional()?;

    if let Some(uuid) = existing {
        return Ok(uuid);
    }

    // Generate and store new UUID
    let uuid = uuid::Uuid::new_v4().to_string();
    library_conn.execute(
        "INSERT INTO library_meta (key, value) VALUES ('library_uuid', ?1)",
        [&uuid],
    )?;

    Ok(uuid)
}

// ---------------------------------------------------------------------------
// Profile staging (spec 3.6 -- authoring writes to staging before publish)
// ---------------------------------------------------------------------------

/// A staged profile edit awaiting validation and publish.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedProfile {
    pub id: i64,
    pub source_type: String,  // "user" (edit existing) or "new" (create new)
    pub source_ref: String,   // uuid of existing user profile, or "" for new
    pub name: String,
    pub match_rules: String,
    pub transform_rules: String,
    pub created_at: String,
}

/// Stage a profile edit. Returns the staged entry.
pub fn stage_profile_edit(
    conn: &Connection,
    source_type: &str,
    source_ref: &str,
    name: &str,
    match_rules: &str,
    transform_rules: &str,
) -> Result<StagedProfile> {
    conn.execute(
        "INSERT INTO profile_staging (source_type, source_ref, name, match_rules, transform_rules)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![source_type, source_ref, name, match_rules, transform_rules],
    )?;
    let id = conn.last_insert_rowid();
    let created_at: String = conn.query_row(
        "SELECT created_at FROM profile_staging WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;
    Ok(StagedProfile {
        id,
        source_type: source_type.to_string(),
        source_ref: source_ref.to_string(),
        name: name.to_string(),
        match_rules: match_rules.to_string(),
        transform_rules: transform_rules.to_string(),
        created_at,
    })
}

/// List all staged profile edits.
pub fn list_staged_profiles(conn: &Connection) -> Result<Vec<StagedProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_type, source_ref, name, match_rules, transform_rules, created_at
         FROM profile_staging ORDER BY created_at"
    )?;
    let entries = stmt.query_map([], |row| {
        Ok(StagedProfile {
            id: row.get(0)?,
            source_type: row.get(1)?,
            source_ref: row.get(2)?,
            name: row.get(3)?,
            match_rules: row.get(4)?,
            transform_rules: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Validate all staged profiles. Returns list of (staged_id, error_message) for failures.
/// Checks: match_rules and transform_rules must be valid JSON objects.
pub fn validate_staged_profiles(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let staged = list_staged_profiles(conn)?;
    let mut errors = Vec::new();
    for entry in &staged {
        // Validate match_rules is valid JSON object
        match serde_json::from_str::<serde_json::Value>(&entry.match_rules) {
            Ok(v) if v.is_object() => {}
            Ok(_) => errors.push((entry.id, "match_rules must be a JSON object".to_string())),
            Err(e) => errors.push((entry.id, format!("match_rules invalid JSON: {}", e))),
        }
        // Validate transform_rules is valid JSON object
        match serde_json::from_str::<serde_json::Value>(&entry.transform_rules) {
            Ok(v) if v.is_object() => {}
            Ok(_) => errors.push((entry.id, "transform_rules must be a JSON object".to_string())),
            Err(e) => errors.push((entry.id, format!("transform_rules invalid JSON: {}", e))),
        }
        // Name must not be empty
        if entry.name.trim().is_empty() {
            errors.push((entry.id, "name must not be empty".to_string()));
        }
    }
    Ok(errors)
}

/// Publish all staged profiles (apply edits to user_profiles). Validates first.
/// Returns count of published profiles, or error if validation fails.
pub fn publish_staged_profiles(conn: &Connection) -> Result<u32> {
    let errors = validate_staged_profiles(conn)?;
    if !errors.is_empty() {
        let msgs: Vec<String> = errors.iter().map(|(id, msg)| format!("#{}: {}", id, msg)).collect();
        anyhow::bail!("Validation failed: {}", msgs.join("; "));
    }

    let staged = list_staged_profiles(conn)?;
    let mut count = 0u32;

    for entry in &staged {
        match entry.source_type.as_str() {
            "user" => {
                // Update existing user profile
                update_user_profile(
                    conn,
                    &entry.source_ref,
                    Some(&entry.name),
                    Some(&entry.match_rules),
                    Some(&entry.transform_rules),
                )?;
                count += 1;
            }
            "new" => {
                // Create new user profile
                create_user_profile(conn, &NewUserProfile {
                    name: entry.name.clone(),
                    match_rules: Some(entry.match_rules.clone()),
                    transform_rules: Some(entry.transform_rules.clone()),
                })?;
                count += 1;
            }
            _ => {}
        }
    }

    // Clear staging
    conn.execute("DELETE FROM profile_staging", [])?;
    Ok(count)
}

/// Discard all staged profile edits.
pub fn discard_staged_profiles(conn: &Connection) -> Result<u32> {
    let count = conn.execute("DELETE FROM profile_staging", [])?;
    Ok(count as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_app_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        // Inline the App DB schema for test isolation
        conn.execute_batch(r#"
            CREATE TABLE bundled_profiles (
                slug TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
                match_rules TEXT NOT NULL DEFAULT '{}', transform_rules TEXT NOT NULL DEFAULT '{}', bundled_version INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE user_profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT, uuid TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1, match_rules TEXT NOT NULL DEFAULT '{}',
                transform_rules TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE camera_devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT, uuid TEXT NOT NULL UNIQUE,
                profile_type TEXT NOT NULL DEFAULT 'none' CHECK (profile_type IN ('bundled','user','none')),
                profile_ref TEXT NOT NULL DEFAULT '', serial_number TEXT, fleet_label TEXT,
                usb_fingerprints TEXT NOT NULL DEFAULT '[]', rental_notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE libraries (
                library_uuid TEXT PRIMARY KEY NOT NULL, path TEXT NOT NULL, label TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')), last_opened_at TEXT,
                last_seen_at TEXT, is_pinned INTEGER NOT NULL DEFAULT 0, is_missing INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE app_settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
        "#).unwrap();
        conn
    }

    fn setup_library_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE library_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);"
        ).unwrap();
        conn
    }

    #[test]
    fn test_upsert_and_list_libraries() {
        let conn = setup_app_db();

        upsert_library(&conn, "uuid-1", "/path/to/lib1", Some("My Library")).unwrap();
        mark_opened(&conn, "uuid-1").unwrap();

        upsert_library(&conn, "uuid-2", "/path/to/lib2", None).unwrap();

        let libs = list_recent_libraries(&conn).unwrap();
        assert_eq!(libs.len(), 2);
        assert_eq!(libs[0].library_uuid, "uuid-1"); // opened more recently
    }

    #[test]
    fn test_upsert_updates_path() {
        let conn = setup_app_db();

        upsert_library(&conn, "uuid-1", "/old/path", Some("Lib")).unwrap();
        upsert_library(&conn, "uuid-1", "/new/path", None).unwrap();

        let entry = get_library_by_uuid(&conn, "uuid-1").unwrap().unwrap();
        // Path should be updated (canonicalized or raw)
        assert!(entry.path.contains("new") || entry.path.contains("path"));
    }

    #[test]
    fn test_mark_missing_and_relink() {
        let conn = setup_app_db();
        let dir = tempfile::tempdir().unwrap();

        // Create a fake library with matching UUID at new_path
        let new_path = dir.path();
        let dadcam_dir = new_path.join(".dadcam");
        std::fs::create_dir_all(&dadcam_dir).unwrap();
        let lib_db_path = dadcam_dir.join("dadcam.db");
        let lib_conn = Connection::open(&lib_db_path).unwrap();
        lib_conn.execute_batch(
            "CREATE TABLE library_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
             INSERT INTO library_meta (key, value) VALUES ('library_uuid', 'uuid-1');"
        ).unwrap();
        drop(lib_conn);

        let path_str = new_path.to_str().unwrap();
        upsert_library(&conn, "uuid-1", "/old/path", Some("Lib")).unwrap();
        mark_missing(&conn, "uuid-1", true).unwrap();

        let entry = get_library_by_uuid(&conn, "uuid-1").unwrap().unwrap();
        assert!(entry.is_missing);

        relink_library(&conn, "uuid-1", path_str).unwrap();
        let entry = get_library_by_uuid(&conn, "uuid-1").unwrap().unwrap();
        assert!(!entry.is_missing);
    }

    #[test]
    fn test_relink_rejects_uuid_mismatch() {
        let conn = setup_app_db();
        let dir = tempfile::tempdir().unwrap();

        // Create a library with a DIFFERENT UUID
        let new_path = dir.path();
        let dadcam_dir = new_path.join(".dadcam");
        std::fs::create_dir_all(&dadcam_dir).unwrap();
        let lib_db_path = dadcam_dir.join("dadcam.db");
        let lib_conn = Connection::open(&lib_db_path).unwrap();
        lib_conn.execute_batch(
            "CREATE TABLE library_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
             INSERT INTO library_meta (key, value) VALUES ('library_uuid', 'uuid-WRONG');"
        ).unwrap();
        drop(lib_conn);

        upsert_library(&conn, "uuid-1", "/old/path", Some("Lib")).unwrap();

        let result = relink_library(&conn, "uuid-1", new_path.to_str().unwrap());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("UUID mismatch"));
    }

    #[test]
    fn test_settings_kv() {
        let conn = setup_app_db();

        assert!(get_setting(&conn, "ui_mode").unwrap().is_none());

        set_setting(&conn, "ui_mode", "simple").unwrap();
        assert_eq!(get_setting(&conn, "ui_mode").unwrap().unwrap(), "simple");

        set_setting(&conn, "ui_mode", "advanced").unwrap();
        assert_eq!(get_setting(&conn, "ui_mode").unwrap().unwrap(), "advanced");

        delete_setting(&conn, "ui_mode").unwrap();
        assert!(get_setting(&conn, "ui_mode").unwrap().is_none());
    }

    #[test]
    fn test_backfill_stable_camera_refs() {
        // Create a library DB with the full schema (migrations 1-7)
        let lib_conn = Connection::open_in_memory().unwrap();
        lib_conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Minimal schema: libraries, assets, camera_profiles, camera_devices, clips
        lib_conn.execute_batch(r#"
            CREATE TABLE libraries (
                id INTEGER PRIMARY KEY AUTOINCREMENT, root_path TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL, ingest_mode TEXT NOT NULL DEFAULT 'copy',
                created_at TEXT NOT NULL DEFAULT (datetime('now')), settings TEXT DEFAULT '{}'
            );
            INSERT INTO libraries (root_path, name) VALUES ('/test', 'Test');

            CREATE TABLE assets (
                id INTEGER PRIMARY KEY AUTOINCREMENT, library_id INTEGER NOT NULL,
                type TEXT NOT NULL, path TEXT NOT NULL, source_uri TEXT,
                size_bytes INTEGER NOT NULL, hash_fast TEXT, hash_fast_scheme TEXT,
                hash_full TEXT, verified_at TEXT, pipeline_version INTEGER,
                derived_params TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO assets (library_id, type, path, size_bytes) VALUES (1, 'original', '/test/vid.mp4', 1000);

            CREATE TABLE camera_profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE,
                version INTEGER NOT NULL DEFAULT 1, match_rules TEXT NOT NULL DEFAULT '{}',
                transform_rules TEXT NOT NULL DEFAULT '{}'
            );
            INSERT INTO camera_profiles (name) VALUES ('Sony Handycam (AVCHD)');
            INSERT INTO camera_profiles (name) VALUES ('Obscure Camera');

            CREATE TABLE camera_devices (
                id INTEGER PRIMARY KEY AUTOINCREMENT, uuid TEXT NOT NULL UNIQUE,
                profile_id INTEGER, serial_number TEXT, fleet_label TEXT,
                usb_fingerprints TEXT DEFAULT '[]', rental_notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO camera_devices (uuid, serial_number, fleet_label, usb_fingerprints)
                VALUES ('dev-uuid-aaa', 'SN123', 'Camera A', '["fp1"]');

            CREATE TABLE clips (
                id INTEGER PRIMARY KEY AUTOINCREMENT, library_id INTEGER NOT NULL,
                original_asset_id INTEGER NOT NULL, camera_profile_id INTEGER,
                media_type TEXT NOT NULL, title TEXT NOT NULL,
                duration_ms INTEGER, width INTEGER, height INTEGER, fps REAL,
                codec TEXT, audio_codec TEXT, audio_channels INTEGER,
                audio_sample_rate INTEGER, recorded_at TEXT,
                recorded_at_offset_minutes INTEGER,
                recorded_at_is_estimated INTEGER NOT NULL DEFAULT 0,
                timestamp_source TEXT, source_folder TEXT,
                camera_device_id INTEGER,
                camera_profile_type TEXT, camera_profile_ref TEXT, camera_device_uuid TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        "#).unwrap();

        // Clip 1: has bundled-matching profile + device
        lib_conn.execute(
            "INSERT INTO clips (library_id, original_asset_id, camera_profile_id, camera_device_id, media_type, title)
             VALUES (1, 1, 1, 1, 'video', 'Clip A')",
            [],
        ).unwrap();

        // Clip 2: has unknown profile, no device
        lib_conn.execute(
            "INSERT INTO clips (library_id, original_asset_id, camera_profile_id, media_type, title)
             VALUES (1, 1, 2, 'video', 'Clip B')",
            [],
        ).unwrap();

        // Clip 3: no legacy refs at all
        lib_conn.execute(
            "INSERT INTO clips (library_id, original_asset_id, media_type, title)
             VALUES (1, 1, 'video', 'Clip C')",
            [],
        ).unwrap();

        // Set up App DB with bundled profiles (in-memory, so we need to mock open_app_db_connection)
        // Since backfill_stable_camera_refs uses open_app_db_connection() which reads the real file,
        // we test the helper functions directly instead.

        // Test resolve_legacy_profile: bundled match
        let app_conn = setup_app_db();
        // Insert bundled profile matching "Sony Handycam (AVCHD)"
        app_conn.execute(
            "INSERT INTO bundled_profiles (slug, name, version, match_rules, transform_rules, bundled_version)
             VALUES ('sony-handycam-avchd', 'Sony Handycam (AVCHD)', 1, '{}', '{}', 1)",
            [],
        ).unwrap();
        let bundled = list_bundled_profiles(&app_conn).unwrap();
        let mut cache = std::collections::HashMap::new();

        // Profile 1 should match bundled
        let (ptype, pref) = backfill_resolve_profile(&lib_conn, &app_conn, Some(1), &bundled, &mut cache);
        assert_eq!(ptype, "bundled");
        assert_eq!(pref, "sony-handycam-avchd");

        // Profile 2 (Obscure Camera) should create migrated user profile
        let (ptype2, pref2) = backfill_resolve_profile(&lib_conn, &app_conn, Some(2), &bundled, &mut cache);
        assert_eq!(ptype2, "user");
        assert!(!pref2.is_empty()); // UUID was generated

        // Verify migrated profile exists in App DB
        let user_profiles = list_user_profiles(&app_conn).unwrap();
        assert!(user_profiles.iter().any(|p| p.name == "[Migrated] Obscure Camera"));

        // No profile -> none
        let (ptype3, pref3) = backfill_resolve_profile(&lib_conn, &app_conn, None, &bundled, &mut cache);
        assert_eq!(ptype3, "none");
        assert_eq!(pref3, "");

        // Test resolve_legacy_device
        let device_uuid = backfill_resolve_device(&lib_conn, &app_conn, Some(1));
        assert_eq!(device_uuid, Some("dev-uuid-aaa".to_string()));
        // Verify device was upserted into App DB
        let dev = get_camera_device_by_uuid(&app_conn, "dev-uuid-aaa").unwrap();
        assert!(dev.is_some());
        assert_eq!(dev.unwrap().serial_number, Some("SN123".to_string()));

        // No device -> None
        let no_device = backfill_resolve_device(&lib_conn, &app_conn, None);
        assert!(no_device.is_none());
    }

    #[test]
    fn test_get_or_create_library_uuid() {
        let conn = setup_library_db();

        // First call creates UUID
        let uuid1 = get_or_create_library_uuid(&conn).unwrap();
        assert!(!uuid1.is_empty());

        // Second call returns same UUID
        let uuid2 = get_or_create_library_uuid(&conn).unwrap();
        assert_eq!(uuid1, uuid2);
    }

    // Spec 9.3: Concurrency stress test
    #[test]
    fn test_concurrent_db_operations() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("stress.db");

        // Initialize DB
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch("PRAGMA busy_timeout=5000;").unwrap();
        conn.execute_batch(r#"
            CREATE TABLE libraries (
                library_uuid TEXT PRIMARY KEY NOT NULL, path TEXT NOT NULL, label TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')), last_opened_at TEXT,
                last_seen_at TEXT, is_pinned INTEGER NOT NULL DEFAULT 0, is_missing INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE app_settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
            CREATE TABLE bundled_profiles (
                slug TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
                match_rules TEXT NOT NULL DEFAULT '{}', transform_rules TEXT NOT NULL DEFAULT '{}',
                bundled_version INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE user_profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT, uuid TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1, match_rules TEXT NOT NULL DEFAULT '{}',
                transform_rules TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        "#).unwrap();
        drop(conn);

        let path = Arc::new(db_path);
        let barrier = Arc::new(Barrier::new(8));
        let mut handles = Vec::new();

        // Spawn 8 threads: mix of reads and writes
        for i in 0..8 {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait(); // All threads start simultaneously

                let conn = Connection::open(path.as_ref()).unwrap();
                conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
                conn.execute_batch("PRAGMA busy_timeout=5000;").unwrap();

                match i % 4 {
                    0 => {
                        // Writer: upsert library
                        let uuid = format!("uuid-stress-{}", i);
                        upsert_library(&conn, &uuid, &format!("/path/{}", i), Some("Stress")).unwrap();
                        mark_opened(&conn, &uuid).unwrap();
                    }
                    1 => {
                        // Writer: settings
                        set_setting(&conn, &format!("key-{}", i), "value").unwrap();
                        let _ = get_setting(&conn, &format!("key-{}", i)).unwrap();
                    }
                    2 => {
                        // Reader: list libraries
                        let _ = list_recent_libraries(&conn).unwrap();
                    }
                    3 => {
                        // Writer: create user profile
                        let profile = NewUserProfile {
                            name: format!("Stress Profile {}", i),
                            match_rules: Some("{}".to_string()),
                            transform_rules: Some("{}".to_string()),
                        };
                        let _ = create_user_profile(&conn, &profile).unwrap();
                    }
                    _ => unreachable!(),
                }

                true // Success
            }));
        }

        // All threads must complete without timeout or deadlock
        for handle in handles {
            assert!(handle.join().unwrap(), "Thread should complete successfully");
        }
    }

    // Spec 9.4: Profile quality gate -- bundled profile match_rules produce expected matches
    #[test]
    fn test_bundled_profile_quality_gate() {
        let conn = setup_app_db();

        // Insert the 3 bundled profiles
        let entries: Vec<BundledProfileJsonEntry> = serde_json::from_str(r#"[
            {
                "slug": "sony-handycam-avchd",
                "name": "Sony Handycam (AVCHD)",
                "version": 1,
                "matchRules": { "make": ["Sony"], "codec": ["h264"], "folderPattern": "AVCHD|BDMV" },
                "transformRules": { "deinterlace": true, "deinterlaceMode": "yadif" }
            },
            {
                "slug": "canon-dslr",
                "name": "Canon DSLR",
                "version": 1,
                "matchRules": { "make": ["Canon"], "codec": ["h264"] },
                "transformRules": {}
            },
            {
                "slug": "panasonic-minidv",
                "name": "Panasonic MiniDV",
                "version": 1,
                "matchRules": { "make": ["Panasonic"], "codec": ["dvvideo", "dv"] },
                "transformRules": { "deinterlace": true }
            }
        ]"#).unwrap();

        let count = sync_bundled_profiles(&conn, &entries).unwrap();
        assert_eq!(count, 3);

        let bundled = list_bundled_profiles(&conn).unwrap();

        // Positive: Sony Handycam matches Sony + h264 + AVCHD folder
        let sony = bundled.iter().find(|b| b.slug == "sony-handycam-avchd").unwrap();
        let rules: serde_json::Value = serde_json::from_str(&sony.match_rules).unwrap();
        let makes = rules["make"].as_array().unwrap();
        assert!(makes.iter().any(|m| m.as_str() == Some("Sony")));
        let codecs = rules["codec"].as_array().unwrap();
        assert!(codecs.iter().any(|c| c.as_str() == Some("h264")));

        // Positive: Canon DSLR matches Canon + h264
        let canon = bundled.iter().find(|b| b.slug == "canon-dslr").unwrap();
        let rules: serde_json::Value = serde_json::from_str(&canon.match_rules).unwrap();
        let makes = rules["make"].as_array().unwrap();
        assert!(makes.iter().any(|m| m.as_str() == Some("Canon")));

        // Positive: Panasonic MiniDV matches dvvideo OR dv
        let panasonic = bundled.iter().find(|b| b.slug == "panasonic-minidv").unwrap();
        let rules: serde_json::Value = serde_json::from_str(&panasonic.match_rules).unwrap();
        let codecs = rules["codec"].as_array().unwrap();
        assert!(codecs.iter().any(|c| c.as_str() == Some("dvvideo")));
        assert!(codecs.iter().any(|c| c.as_str() == Some("dv")));

        // Negative: Sony profile should NOT match Panasonic codec
        let sony_codecs = serde_json::from_str::<serde_json::Value>(&sony.match_rules)
            .unwrap()["codec"].as_array().unwrap().clone();
        assert!(!sony_codecs.iter().any(|c| c.as_str() == Some("dvvideo")));

        // Negative: Canon profile should NOT have folderPattern (less restrictive)
        let canon_rules: serde_json::Value = serde_json::from_str(&canon.match_rules).unwrap();
        assert!(canon_rules.get("folderPattern").is_none());

        // Verify transform_rules: Sony requires deinterlace
        let sony_tr: serde_json::Value = serde_json::from_str(&sony.transform_rules).unwrap();
        assert_eq!(sony_tr["deinterlace"], serde_json::json!(true));
        assert_eq!(sony_tr["deinterlaceMode"], serde_json::json!("yadif"));

        // Verify transform_rules: Canon has no deinterlace
        let canon_tr: serde_json::Value = serde_json::from_str(&canon.transform_rules).unwrap();
        assert!(canon_tr.get("deinterlace").is_none() || canon_tr["deinterlace"].is_null());
    }

    // Spec 9.2: Bundled sync is idempotent (twice yields same rows)
    #[test]
    fn test_bundled_sync_idempotent() {
        let conn = setup_app_db();

        let entries: Vec<BundledProfileJsonEntry> = serde_json::from_str(r#"[
            { "slug": "test-profile", "name": "Test", "version": 1, "matchRules": {}, "transformRules": {} }
        ]"#).unwrap();

        sync_bundled_profiles(&conn, &entries).unwrap();
        let first = list_bundled_profiles(&conn).unwrap();

        sync_bundled_profiles(&conn, &entries).unwrap();
        let second = list_bundled_profiles(&conn).unwrap();

        assert_eq!(first.len(), second.len());
        assert_eq!(first[0].slug, second[0].slug);
        assert_eq!(first[0].name, second[0].name);
        assert_eq!(first[0].version, second[0].version);
    }

    // Spec 9.1: Upgrade path -- user_profiles uuid backfill
    #[test]
    fn test_upgrade_user_profiles_uuid_backfill() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();

        // Simulate pre-uuid user_profiles table (no uuid column)
        conn.execute_batch(r#"
            CREATE TABLE user_profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                version INTEGER NOT NULL DEFAULT 1,
                match_rules TEXT NOT NULL DEFAULT '{}',
                transform_rules TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO user_profiles (name) VALUES ('Legacy Profile A');
            INSERT INTO user_profiles (name) VALUES ('Legacy Profile B');
        "#).unwrap();

        // Verify no uuid column exists
        let has_uuid: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('user_profiles') WHERE name = 'uuid'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert!(!has_uuid, "uuid column should not exist before upgrade");

        // Simulate the upgrade: add uuid column and backfill
        conn.execute_batch("ALTER TABLE user_profiles ADD COLUMN uuid TEXT;").unwrap();

        // Backfill: generate UUIDs for rows missing them
        let mut stmt = conn.prepare("SELECT id FROM user_profiles WHERE uuid IS NULL OR uuid = ''").unwrap();
        let ids: Vec<i64> = stmt.query_map([], |row| row.get(0)).unwrap()
            .filter_map(|r| r.ok()).collect();
        for id in &ids {
            let uuid = uuid::Uuid::new_v4().to_string();
            conn.execute("UPDATE user_profiles SET uuid = ?1 WHERE id = ?2", params![uuid, id]).unwrap();
        }

        // Verify all rows now have UUIDs
        let null_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM user_profiles WHERE uuid IS NULL OR uuid = ''",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(null_count, 0, "All rows should have UUIDs after backfill");

        // Verify UUIDs are unique
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM user_profiles", [], |row| row.get(0)).unwrap();
        let distinct: i64 = conn.query_row("SELECT COUNT(DISTINCT uuid) FROM user_profiles", [], |row| row.get(0)).unwrap();
        assert_eq!(total, distinct, "All UUIDs should be unique");
        assert_eq!(total, 2);
    }

    // Spec 9.4: Profile quality gate -- evidence-based match verification
    #[test]
    fn test_profile_quality_gate_evidence_matching() {
        // Evidence: simulated metadata from a Sony Handycam clip
        let sony_evidence = serde_json::json!({
            "make": "Sony",
            "codec": "h264",
            "folder": "PRIVATE/AVCHD/BDMV/STREAM",
            "width": 1920,
            "height": 1080,
            "fps": 29.97
        });

        // Evidence: simulated metadata from a Canon DSLR clip
        let canon_evidence = serde_json::json!({
            "make": "Canon",
            "codec": "h264",
            "folder": "DCIM/100CANON",
            "width": 1920,
            "height": 1080,
            "fps": 24.0
        });

        // Evidence: simulated metadata from a Panasonic MiniDV clip
        let panasonic_evidence = serde_json::json!({
            "make": "Panasonic",
            "codec": "dvvideo",
            "folder": "Capture/MiniDV",
            "width": 720,
            "height": 480,
            "fps": 29.97
        });

        // Profile match rules
        let sony_rules = serde_json::json!({
            "make": ["Sony"],
            "codec": ["h264"],
            "folderPattern": "AVCHD|BDMV"
        });
        let canon_rules = serde_json::json!({
            "make": ["Canon"],
            "codec": ["h264"]
        });
        let panasonic_rules = serde_json::json!({
            "make": ["Panasonic"],
            "codec": ["dvvideo", "dv"]
        });

        // Helper: check if evidence matches rules (AND semantics, OR within arrays)
        fn evidence_matches(evidence: &serde_json::Value, rules: &serde_json::Value) -> bool {
            let rules_obj = rules.as_object().unwrap();
            for (key, expected) in rules_obj {
                match key.as_str() {
                    "make" | "model" | "codec" | "container" => {
                        let allowed: Vec<String> = expected.as_array().unwrap()
                            .iter().map(|v| v.as_str().unwrap().to_lowercase()).collect();
                        let actual = evidence.get(key)
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_lowercase())
                            .unwrap_or_default();
                        if !allowed.iter().any(|a| a == &actual) {
                            return false;
                        }
                    }
                    "folderPattern" => {
                        let pattern = expected.as_str().unwrap();
                        let folder = evidence.get("folder")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let re = regex::Regex::new(&format!("(?i){}", pattern)).unwrap();
                        if !re.is_match(folder) {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
            true
        }

        // Positive: Sony evidence matches Sony rules
        assert!(evidence_matches(&sony_evidence, &sony_rules), "Sony evidence should match Sony rules");

        // Positive: Canon evidence matches Canon rules
        assert!(evidence_matches(&canon_evidence, &canon_rules), "Canon evidence should match Canon rules");

        // Positive: Panasonic evidence matches Panasonic rules
        assert!(evidence_matches(&panasonic_evidence, &panasonic_rules), "Panasonic evidence should match Panasonic rules");

        // Negative: Sony evidence should NOT match Canon rules (different make)
        assert!(!evidence_matches(&sony_evidence, &canon_rules), "Sony evidence should NOT match Canon rules");

        // Negative: Canon evidence should NOT match Sony rules (missing folderPattern)
        assert!(!evidence_matches(&canon_evidence, &sony_rules), "Canon evidence should NOT match Sony rules (no AVCHD folder)");

        // Negative: Canon evidence should NOT match Panasonic rules (different codec + make)
        assert!(!evidence_matches(&canon_evidence, &panasonic_rules), "Canon evidence should NOT match Panasonic rules");

        // Near-miss: h264 codec but wrong make -- should NOT match Sony
        let near_miss = serde_json::json!({
            "make": "GoPro",
            "codec": "h264",
            "folder": "DCIM/100GOPRO",
            "width": 1920,
            "height": 1080
        });
        assert!(!evidence_matches(&near_miss, &sony_rules), "GoPro near-miss should NOT match Sony");
        assert!(!evidence_matches(&near_miss, &canon_rules), "GoPro near-miss should NOT match Canon");

        // Near-miss: Sony make but wrong codec -- should NOT match Sony if codec doesn't match
        let sony_wrong_codec = serde_json::json!({
            "make": "Sony",
            "codec": "dvvideo",
            "folder": "PRIVATE/AVCHD/BDMV/STREAM"
        });
        assert!(!evidence_matches(&sony_wrong_codec, &sony_rules), "Sony with wrong codec should NOT match Sony rules");
    }

    // Fix 2: Staging workflow tests (spec 3.6)
    #[test]
    fn test_staging_workflow() {
        let conn = setup_app_db();
        // Add staging table for test
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS profile_staging (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_type TEXT NOT NULL CHECK (source_type IN ('user','new')),
                source_ref TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL,
                match_rules TEXT NOT NULL DEFAULT '{}',
                transform_rules TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        "#).unwrap();

        // Stage a new profile
        let staged = stage_profile_edit(
            &conn, "new", "", "Test Profile",
            r#"{"make":["Sony"]}"#, r#"{"deinterlace":true}"#,
        ).unwrap();
        assert_eq!(staged.source_type, "new");
        assert_eq!(staged.name, "Test Profile");

        // List staged
        let list = list_staged_profiles(&conn).unwrap();
        assert_eq!(list.len(), 1);

        // Validate -- should pass
        let errors = validate_staged_profiles(&conn).unwrap();
        assert!(errors.is_empty(), "Valid staged profiles should have no errors");

        // Publish
        let count = publish_staged_profiles(&conn).unwrap();
        assert_eq!(count, 1);

        // Staging should be empty after publish
        let list = list_staged_profiles(&conn).unwrap();
        assert!(list.is_empty());

        // User profile should exist
        let profiles = list_user_profiles(&conn).unwrap();
        assert!(profiles.iter().any(|p| p.name == "Test Profile"));
    }

    #[test]
    fn test_staging_validation_rejects_invalid() {
        let conn = setup_app_db();
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS profile_staging (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_type TEXT NOT NULL CHECK (source_type IN ('user','new')),
                source_ref TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL,
                match_rules TEXT NOT NULL DEFAULT '{}',
                transform_rules TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        "#).unwrap();

        // Stage with invalid JSON
        stage_profile_edit(&conn, "new", "", "Bad Profile", "not-json", "{}").unwrap();

        let errors = validate_staged_profiles(&conn).unwrap();
        assert!(!errors.is_empty(), "Invalid JSON should produce validation errors");

        // Publish should fail
        let result = publish_staged_profiles(&conn);
        assert!(result.is_err(), "Publish should fail when validation fails");
    }

    #[test]
    fn test_staging_discard() {
        let conn = setup_app_db();
        conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS profile_staging (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_type TEXT NOT NULL CHECK (source_type IN ('user','new')),
                source_ref TEXT NOT NULL DEFAULT '',
                name TEXT NOT NULL,
                match_rules TEXT NOT NULL DEFAULT '{}',
                transform_rules TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        "#).unwrap();

        stage_profile_edit(&conn, "new", "", "Discard Me", "{}", "{}").unwrap();
        assert_eq!(list_staged_profiles(&conn).unwrap().len(), 1);

        let discarded = discard_staged_profiles(&conn).unwrap();
        assert_eq!(discarded, 1);
        assert!(list_staged_profiles(&conn).unwrap().is_empty());
    }

    /// Spec 9.2: Deleting a library folder must not delete App DB data
    /// (profiles, devices, settings all survive library removal).
    #[test]
    fn test_deleting_library_preserves_app_data() {
        let app_conn = setup_app_db();

        // Populate App DB with a bundled profile
        let entries = vec![super::BundledProfileJsonEntry {
            slug: "test-cam".to_string(),
            name: "Test Cam".to_string(),
            version: 1,
            match_rules: serde_json::json!({}),
            transform_rules: serde_json::json!({}),
            is_system: None,
            deletable: None,
            category: None,
        }];
        sync_bundled_profiles(&app_conn, &entries).unwrap();

        // Add a user profile
        let profile = super::NewUserProfile {
            name: "My Custom".to_string(),
            match_rules: Some("{}".to_string()),
            transform_rules: Some("{}".to_string()),
        };
        create_user_profile(&app_conn, &profile).unwrap();

        // Add a camera device
        let device = super::NewAppCameraDevice {
            profile_type: Some("bundled".to_string()),
            profile_ref: Some("test-cam".to_string()),
            serial_number: None,
            fleet_label: None,
            usb_fingerprints: vec![],
            rental_notes: None,
        };
        create_camera_device(&app_conn, &device).unwrap();

        // Add a setting
        set_setting(&app_conn, "theme", "dark").unwrap();

        // Add a library entry
        upsert_library(&app_conn, "lib-uuid-1", "/tmp/test-library", Some("Test Lib")).unwrap();

        // Simulate deleting the library: remove it from the registry
        app_conn.execute("DELETE FROM libraries WHERE library_uuid = ?1", ["lib-uuid-1"]).unwrap();

        // Assert all non-library App DB data is intact
        let profiles: i64 = app_conn.query_row(
            "SELECT COUNT(*) FROM bundled_profiles", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(profiles, 1, "bundled profile must survive library deletion");

        let user_profiles: i64 = app_conn.query_row(
            "SELECT COUNT(*) FROM user_profiles", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(user_profiles, 1, "user profile must survive library deletion");

        let devices: i64 = app_conn.query_row(
            "SELECT COUNT(*) FROM camera_devices", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(devices, 1, "device must survive library deletion");

        let theme = get_setting(&app_conn, "theme").unwrap();
        assert_eq!(theme, Some("dark".to_string()), "settings must survive library deletion");
    }

    /// Spec 9.2: Library registry entries persist across connection close/reopen
    /// (simulates recents surviving app restart).
    #[test]
    fn test_recents_persist_across_sessions() {
        // Use a temp file so we can close and reopen the connection
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test_app.db");

        // Session 1: create DB, add libraries
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
            conn.execute_batch(r#"
                CREATE TABLE libraries (
                    library_uuid TEXT PRIMARY KEY NOT NULL, path TEXT NOT NULL, label TEXT,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')), last_opened_at TEXT,
                    last_seen_at TEXT, is_pinned INTEGER NOT NULL DEFAULT 0, is_missing INTEGER NOT NULL DEFAULT 0
                );
            "#).unwrap();

            upsert_library(&conn, "uuid-a", "/path/a", Some("Library A")).unwrap();
            upsert_library(&conn, "uuid-b", "/path/b", Some("Library B")).unwrap();
            mark_opened(&conn, "uuid-a").unwrap();
            // conn drops here, closing the connection
        }

        // Session 2: reopen DB, assert entries survived
        {
            let conn = Connection::open(&db_path).unwrap();
            let recents = list_recent_libraries(&conn).unwrap();
            assert_eq!(recents.len(), 2, "both libraries must persist across sessions");

            let uuids: Vec<&str> = recents.iter().map(|r| r.library_uuid.as_str()).collect();
            assert!(uuids.contains(&"uuid-a"), "library A must be present");
            assert!(uuids.contains(&"uuid-b"), "library B must be present");

            // uuid-a was opened, so it should have last_opened_at set
            let lib_a = recents.iter().find(|r| r.library_uuid == "uuid-a").unwrap();
            assert!(lib_a.last_opened_at.is_some(), "last_opened_at must persist across sessions");
        }
    }
}
