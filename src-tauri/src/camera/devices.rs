// Camera device management (physical rental units / "Dad Cams")

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use crate::error::Result;

/// A physical camera device (rental unit / "Dad Cam")
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraDevice {
    pub id: i64,
    pub uuid: String,
    pub profile_id: Option<i64>,
    pub serial_number: Option<String>,
    pub fleet_label: Option<String>,
    pub usb_fingerprints: Vec<String>,
    pub rental_notes: Option<String>,
    pub created_at: String,
}

/// Parameters for registering a new camera device
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewCameraDevice {
    pub profile_id: Option<i64>,
    pub serial_number: Option<String>,
    pub fleet_label: Option<String>,
    pub usb_fingerprints: Vec<String>,
    pub rental_notes: Option<String>,
}

/// Get all camera devices from the database
pub fn get_all_devices(conn: &Connection) -> Result<Vec<CameraDevice>> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, profile_id, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
         FROM camera_devices ORDER BY fleet_label, created_at"
    )?;

    let devices = stmt.query_map([], |row| {
        let fps_str: String = row.get(5)?;
        Ok(CameraDevice {
            id: row.get(0)?,
            uuid: row.get(1)?,
            profile_id: row.get(2)?,
            serial_number: row.get(3)?,
            fleet_label: row.get(4)?,
            usb_fingerprints: serde_json::from_str(&fps_str).unwrap_or_default(),
            rental_notes: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(devices)
}

/// Insert a new camera device, returns the created device
pub fn insert_device(conn: &Connection, device: &NewCameraDevice) -> Result<CameraDevice> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let fps_json = serde_json::to_string(&device.usb_fingerprints)?;

    conn.execute(
        "INSERT INTO camera_devices (uuid, profile_id, serial_number, fleet_label, usb_fingerprints, rental_notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            uuid,
            device.profile_id,
            device.serial_number,
            device.fleet_label,
            fps_json,
            device.rental_notes,
        ],
    )?;

    let id = conn.last_insert_rowid();
    let created_at: String = conn.query_row(
        "SELECT created_at FROM camera_devices WHERE id = ?1",
        [id],
        |row| row.get(0),
    )?;

    Ok(CameraDevice {
        id,
        uuid,
        profile_id: device.profile_id,
        serial_number: device.serial_number.clone(),
        fleet_label: device.fleet_label.clone(),
        usb_fingerprints: device.usb_fingerprints.clone(),
        rental_notes: device.rental_notes.clone(),
        created_at,
    })
}

/// Find a device by USB fingerprint
pub fn find_device_by_usb_fingerprint(conn: &Connection, fingerprint: &str) -> Result<Option<CameraDevice>> {
    // Search JSON array for matching fingerprint
    let mut stmt = conn.prepare(
        "SELECT id, uuid, profile_id, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
         FROM camera_devices
         WHERE usb_fingerprints LIKE ?1 ESCAPE '\\'"
    )?;

    // Escape special LIKE characters in the fingerprint before searching
    let escaped_fp = fingerprint
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let fp_pattern = format!("%\"{}\"%", escaped_fp);
    let mut devices: Vec<CameraDevice> = stmt.query_map([&fp_pattern], |row| {
        let fps_str: String = row.get(5)?;
        Ok(CameraDevice {
            id: row.get(0)?,
            uuid: row.get(1)?,
            profile_id: row.get(2)?,
            serial_number: row.get(3)?,
            fleet_label: row.get(4)?,
            usb_fingerprints: serde_json::from_str(&fps_str).unwrap_or_default(),
            rental_notes: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    // Verify exact match in the parsed JSON array
    if let Some(pos) = devices.iter().position(|d| d.usb_fingerprints.contains(&fingerprint.to_string())) {
        Ok(Some(devices.remove(pos)))
    } else {
        Ok(None)
    }
}

/// Find a device by serial number
pub fn find_device_by_serial(conn: &Connection, serial: &str) -> Result<Option<CameraDevice>> {
    let mut stmt = conn.prepare(
        "SELECT id, uuid, profile_id, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
         FROM camera_devices
         WHERE serial_number = ?1"
    )?;

    let device = stmt.query_row([serial], |row| {
        let fps_str: String = row.get(5)?;
        Ok(CameraDevice {
            id: row.get(0)?,
            uuid: row.get(1)?,
            profile_id: row.get(2)?,
            serial_number: row.get(3)?,
            fleet_label: row.get(4)?,
            usb_fingerprints: serde_json::from_str(&fps_str).unwrap_or_default(),
            rental_notes: row.get(6)?,
            created_at: row.get(7)?,
        })
    });

    match device {
        Ok(d) => Ok(Some(d)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Update the camera_device_id on a clip
pub fn update_clip_camera_device(conn: &Connection, clip_id: i64, device_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET camera_device_id = ?1 WHERE id = ?2",
        params![device_id, clip_id],
    )?;
    Ok(())
}

/// Get the path to ~/.dadcam/custom_cameras.json
fn custom_cameras_json_path() -> Option<std::path::PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.home_dir().join(".dadcam").join("custom_cameras.json"))
}

/// Save all camera devices from the database to ~/.dadcam/custom_cameras.json.
/// Best-effort: failures are logged but never block the caller.
pub fn save_devices_to_json(conn: &Connection) -> Result<()> {
    let path = match custom_cameras_json_path() {
        Some(p) => p,
        None => return Ok(()),
    };

    // Ensure ~/.dadcam/ directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| crate::error::DadCamError::Io(e))?;
    }

    let devices = get_all_devices(conn)?;
    let json = serde_json::to_string_pretty(&devices)
        .map_err(|e| crate::error::DadCamError::Json(e))?;
    std::fs::write(&path, json)
        .map_err(|e| crate::error::DadCamError::Io(e))?;

    Ok(())
}

/// Load camera devices from ~/.dadcam/custom_cameras.json into the database.
/// Skips devices whose UUID already exists. Returns count of newly inserted devices.
/// Best-effort: if the file is missing or invalid, returns 0.
pub fn load_devices_from_json(conn: &Connection) -> u32 {
    let path = match custom_cameras_json_path() {
        Some(p) if p.exists() => p,
        _ => return 0,
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let devices: Vec<CameraDevice> = match serde_json::from_str(&content) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Warning: Failed to parse custom_cameras.json: {}", e);
            return 0;
        }
    };

    let mut inserted = 0u32;
    for device in devices {
        // Check if UUID already exists in this library's DB
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM camera_devices WHERE uuid = ?1)",
            [&device.uuid],
            |row| row.get(0),
        ).unwrap_or(true);

        if !exists {
            let fps_json = serde_json::to_string(&device.usb_fingerprints).unwrap_or_else(|_| "[]".to_string());
            match conn.execute(
                "INSERT INTO camera_devices (uuid, profile_id, serial_number, fleet_label, usb_fingerprints, rental_notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    device.uuid,
                    device.profile_id,
                    device.serial_number,
                    device.fleet_label,
                    fps_json,
                    device.rental_notes,
                ],
            ) {
                Ok(_) => inserted += 1,
                Err(e) => eprintln!("Warning: Failed to import device {}: {}", device.uuid, e),
            }
        }
    }

    inserted
}

/// Attempt to capture USB fingerprint for the current system.
/// Best-effort: returns None if detection fails for any reason.
/// Wrapped in catch_unwind to prevent panics in platform-specific
/// code from crashing the app (contract 16: crash safety).
pub fn capture_usb_fingerprint() -> Option<Vec<String>> {
    std::panic::catch_unwind(|| {
        capture_usb_fingerprint_inner()
    })
    .unwrap_or(None)
}

fn capture_usb_fingerprint_inner() -> Option<Vec<String>> {
    #[cfg(target_os = "macos")]
    {
        capture_usb_fingerprint_macos()
    }
    #[cfg(target_os = "windows")]
    {
        capture_usb_fingerprint_windows()
    }
    #[cfg(target_os = "linux")]
    {
        capture_usb_fingerprint_linux()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
fn capture_usb_fingerprint_macos() -> Option<Vec<String>> {
    use std::process::Command;

    // Use -xml for stable, parseable plist output (plain text format is fragile across macOS versions)
    let output = Command::new("system_profiler")
        .args(["SPUSBDataType", "-xml"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut fingerprints = Vec::new();

    // Parse vendor_id and product_id from plist XML.
    // The XML contains <key>vendor_id</key><string>0x1234</string> pairs.
    // We use simple line-based scanning rather than a full XML parser to avoid
    // adding a dependency for this best-effort feature.
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut i = 0;
    let mut current_vendor = String::new();

    while i < lines.len() {
        let line = lines[i];

        if line == "<key>vendor_id</key>" {
            if let Some(next) = lines.get(i + 1) {
                current_vendor = extract_plist_string(next).unwrap_or_default();
                i += 2;
                continue;
            }
        } else if line == "<key>product_id</key>" {
            if let Some(next) = lines.get(i + 1) {
                let product = extract_plist_string(next).unwrap_or_default();
                if !current_vendor.is_empty() && !product.is_empty() {
                    // Filter: skip USB hubs (vendor 0x05ac product 0x8006/etc are Apple hubs)
                    // and root hubs (product_id 0x0000). Only keep real peripherals.
                    if product != "0x0000" {
                        fingerprints.push(format!("{}:{}", current_vendor, product));
                    }
                }
                i += 2;
                continue;
            }
        } else if line == "<key>serial_num</key>" {
            if let Some(next) = lines.get(i + 1) {
                let serial = extract_plist_string(next).unwrap_or_default();
                if !serial.is_empty() {
                    fingerprints.push(format!("serial:{}", serial));
                }
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    fingerprints.dedup();
    if fingerprints.is_empty() {
        None
    } else {
        Some(fingerprints)
    }
}

/// Extract text content from a plist <string>...</string> element.
#[cfg(target_os = "macos")]
fn extract_plist_string(line: &str) -> Option<String> {
    let s = line.trim();
    if s.starts_with("<string>") && s.ends_with("</string>") {
        Some(s[8..s.len() - 9].to_string())
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn capture_usb_fingerprint_windows() -> Option<Vec<String>> {
    use std::process::Command;

    // Use PowerShell Get-CimInstance instead of deprecated wmic.
    // Get-CimInstance is available on Windows 10+ and is the modern replacement.
    let output = Command::new("powershell")
        .args([
            "-NoProfile", "-NonInteractive", "-Command",
            "Get-CimInstance Win32_PnPEntity | Where-Object { $_.DeviceID -match 'USB\\\\VID_' } | Select-Object -ExpandProperty DeviceID",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut fingerprints = Vec::new();

    // Use regex to robustly extract VID and PID hex values of any length
    let vid_pid_re = regex::Regex::new(r"VID_([0-9A-Fa-f]+)&PID_([0-9A-Fa-f]+)").ok()?;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(caps) = vid_pid_re.captures(trimmed) {
            let vid = caps.get(1)?.as_str().to_lowercase();
            let pid = caps.get(2)?.as_str().to_lowercase();
            let fp = format!("0x{}:0x{}", vid, pid);
            if !fingerprints.contains(&fp) {
                fingerprints.push(fp);
            }
        }
    }

    if fingerprints.is_empty() {
        None
    } else {
        Some(fingerprints)
    }
}

#[cfg(target_os = "linux")]
fn capture_usb_fingerprint_linux() -> Option<Vec<String>> {
    use std::fs;

    let mut fingerprints = Vec::new();

    // Read from /sys/bus/usb/devices/
    let entries = fs::read_dir("/sys/bus/usb/devices/").ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        let vendor_path = path.join("idVendor");
        let product_path = path.join("idProduct");

        if vendor_path.exists() && product_path.exists() {
            if let (Ok(vendor), Ok(product)) = (
                fs::read_to_string(&vendor_path),
                fs::read_to_string(&product_path),
            ) {
                let vid = vendor.trim();
                let pid = product.trim();
                // Skip root hubs and empty entries (0000:0000 is a root hub)
                if !vid.is_empty() && !pid.is_empty() && !(vid == "0000" && pid == "0000") {
                    let fp = format!("0x{}:0x{}", vid, pid);
                    if !fingerprints.contains(&fp) {
                        fingerprints.push(fp);
                    }
                }
            }
        }

        // Also check serial
        let serial_path = path.join("serial");
        if serial_path.exists() {
            if let Ok(serial) = fs::read_to_string(&serial_path) {
                let s = serial.trim();
                if !s.is_empty() {
                    let fp = format!("serial:{}", s);
                    if !fingerprints.contains(&fp) {
                        fingerprints.push(fp);
                    }
                }
            }
        }
    }

    if fingerprints.is_empty() {
        None
    } else {
        Some(fingerprints)
    }
}
