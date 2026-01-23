// File discovery for ingest

use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use crate::constants::{VIDEO_EXTENSIONS, AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, SIDECAR_EXTENSIONS};
use crate::error::Result;

/// Discover all media files in a directory
pub fn discover_media_files(source_path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if source_path.is_file() {
        // Single file
        if is_media_file(source_path) {
            files.push(source_path.to_path_buf());
        }
    } else if source_path.is_dir() {
        // Walk directory
        for entry in WalkDir::new(source_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && is_media_file(path) {
                files.push(path.to_path_buf());
            }
        }
    }

    // Sort by path for consistent ordering
    files.sort();

    Ok(files)
}

/// Discover sidecar files for a media file
pub fn discover_sidecars(media_path: &Path) -> Vec<PathBuf> {
    let mut sidecars = Vec::new();

    let stem = match media_path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return sidecars,
    };

    let parent = match media_path.parent() {
        Some(p) => p,
        None => return sidecars,
    };

    // Look for files with same stem but sidecar extension
    for ext in SIDECAR_EXTENSIONS {
        let sidecar_path = parent.join(format!("{}.{}", stem, ext));
        if sidecar_path.exists() {
            sidecars.push(sidecar_path);
        }

        // Also check uppercase extension
        let sidecar_path_upper = parent.join(format!("{}.{}", stem, ext.to_uppercase()));
        if sidecar_path_upper.exists() && !sidecars.contains(&sidecar_path_upper) {
            sidecars.push(sidecar_path_upper);
        }
    }

    sidecars
}

/// Check if a file is a media file based on extension
pub fn is_media_file(path: &Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };

    VIDEO_EXTENSIONS.contains(&ext.as_str())
        || AUDIO_EXTENSIONS.contains(&ext.as_str())
        || IMAGE_EXTENSIONS.contains(&ext.as_str())
}

/// Check if a file is a sidecar file
pub fn is_sidecar_file(path: &Path) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };

    SIDECAR_EXTENSIONS.contains(&ext.as_str())
}

/// Check if path is within an AVCHD/BDMV structure
pub fn is_avchd_structure(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();
    path_str.contains("/avchd/") || path_str.contains("/bdmv/") || path_str.contains("/private/")
}

/// Get the AVCHD root directory for a file
pub fn get_avchd_root(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent()?;

    while let Some(parent) = current.parent() {
        let dir_name = current.file_name()?.to_str()?.to_lowercase();
        if dir_name == "avchd" || dir_name == "bdmv" || dir_name == "private" {
            return Some(current.to_path_buf());
        }
        current = parent;
    }

    None
}

/// Volume information (serial, label, mount point)
#[derive(Debug, Clone)]
pub struct VolumeInfo {
    pub serial: Option<String>,
    pub label: Option<String>,
    pub mount_point: Option<String>,
}

/// Get volume information for a path (cross-platform)
pub fn get_volume_info(path: &Path) -> VolumeInfo {
    #[cfg(target_os = "macos")]
    {
        get_volume_info_macos(path)
    }
    #[cfg(target_os = "windows")]
    {
        get_volume_info_windows(path)
    }
    #[cfg(target_os = "linux")]
    {
        get_volume_info_linux(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        VolumeInfo { serial: None, label: None, mount_point: None }
    }
}

#[cfg(target_os = "macos")]
fn get_volume_info_macos(path: &Path) -> VolumeInfo {
    use std::process::Command;

    // Get mount point using df
    let mount_point = Command::new("df")
        .arg(path)
        .output()
        .ok()
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.lines().nth(1)
                .and_then(|line| line.split_whitespace().last())
                .map(String::from)
        });

    // Get volume label from mount point
    let label = mount_point.as_ref().and_then(|mp| {
        std::path::Path::new(mp)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
    });

    // Try to get volume UUID using diskutil
    let serial = mount_point.as_ref().and_then(|mp| {
        Command::new("diskutil")
            .args(["info", mp])
            .output()
            .ok()
            .and_then(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.lines()
                    .find(|line| line.contains("Volume UUID:"))
                    .and_then(|line| line.split(':').nth(1))
                    .map(|s| s.trim().to_string())
            })
    });

    VolumeInfo { serial, label, mount_point }
}

#[cfg(target_os = "windows")]
fn get_volume_info_windows(path: &Path) -> VolumeInfo {
    use std::process::Command;

    // Get drive letter
    let drive = path.components().next()
        .and_then(|c| {
            let s = c.as_os_str().to_string_lossy();
            if s.len() >= 2 { Some(s[..2].to_string()) } else { None }
        });

    let mount_point = drive.clone();

    if let Some(ref drv) = drive {
        // Use wmic to get volume info
        let output = Command::new("wmic")
            .args(["volume", "where", &format!("DriveLetter='{}'", drv), "get", "SerialNumber,Label", "/format:list"])
            .output()
            .ok();

        if let Some(o) = output {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let mut serial = None;
            let mut label = None;

            for line in stdout.lines() {
                if let Some(val) = line.strip_prefix("Label=") {
                    let v = val.trim();
                    if !v.is_empty() { label = Some(v.to_string()); }
                }
                if let Some(val) = line.strip_prefix("SerialNumber=") {
                    let v = val.trim();
                    if !v.is_empty() { serial = Some(v.to_string()); }
                }
            }

            return VolumeInfo { serial, label, mount_point };
        }
    }

    VolumeInfo { serial: None, label: None, mount_point }
}

#[cfg(target_os = "linux")]
fn get_volume_info_linux(path: &Path) -> VolumeInfo {
    use std::process::Command;

    // Find device for path using df
    let device = Command::new("df")
        .arg(path)
        .output()
        .ok()
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.lines().nth(1)
                .and_then(|line| line.split_whitespace().next())
                .map(String::from)
        });

    // Get mount point
    let mount_point = Command::new("df")
        .arg(path)
        .output()
        .ok()
        .and_then(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.lines().nth(1)
                .and_then(|line| line.split_whitespace().last())
                .map(String::from)
        });

    let (serial, label) = if let Some(ref dev) = device {
        // Get UUID using blkid
        let uuid = Command::new("blkid")
            .args(["-s", "UUID", "-o", "value", dev])
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        // Get label using blkid
        let lbl = Command::new("blkid")
            .args(["-s", "LABEL", "-o", "value", dev])
            .output()
            .ok()
            .and_then(|o| {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            });

        (uuid, lbl)
    } else {
        (None, None)
    };

    VolumeInfo { serial, label, mount_point }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_media_file() {
        assert!(is_media_file(Path::new("video.mp4")));
        assert!(is_media_file(Path::new("video.MTS")));
        assert!(is_media_file(Path::new("audio.mp3")));
        assert!(is_media_file(Path::new("image.jpg")));
        assert!(!is_media_file(Path::new("document.txt")));
        assert!(!is_media_file(Path::new("file.xml")));
    }

    #[test]
    fn test_is_sidecar_file() {
        assert!(is_sidecar_file(Path::new("video.thm")));
        assert!(is_sidecar_file(Path::new("video.XML")));
        assert!(is_sidecar_file(Path::new("video.srt")));
        assert!(!is_sidecar_file(Path::new("video.mp4")));
    }
}
