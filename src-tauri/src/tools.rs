// External tool resolver for bundled ffmpeg/ffprobe/exiftool
//
// Dad Cam policy (contracts.md): all tools must be bundled with the app.
// Resolution order:
// 1) Environment variable override (DADCAM_FFPROBE_PATH, etc.)
// 2) ffmpeg-sidecar managed binaries (auto-downloaded)
// 3) Sidecar next to the executable
// 4) macOS app bundle Resources fallback
// 5) PATH fallback (dev-only convenience)

use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Cached paths to avoid repeated resolution
static FFMPEG_PATH: OnceLock<PathBuf> = OnceLock::new();
static FFPROBE_PATH: OnceLock<PathBuf> = OnceLock::new();
static EXIFTOOL_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Get the directory containing the current executable
fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

/// Check if ffmpeg-sidecar has been initialized and binaries are available
fn get_ffmpeg_sidecar_path(tool: &str) -> Option<PathBuf> {
    // Try to get path from ffmpeg-sidecar
    match tool {
        "ffmpeg" => {
            // ffmpeg-sidecar provides the path if binaries are downloaded
            let path = ffmpeg_sidecar::paths::ffmpeg_path();
            if path.exists() {
                return Some(path);
            }
        }
        "ffprobe" => {
            let path = ffmpeg_sidecar::ffprobe::ffprobe_path();
            if path.exists() {
                return Some(path);
            }
        }
        _ => {}
    }
    None
}

/// Resolve a bundled sidecar tool path.
fn resolve_tool(env_key: &str, default_name: &str) -> PathBuf {
    // 1) Check environment variable override
    if let Ok(v) = env::var(env_key) {
        let p = PathBuf::from(&v);
        if p.exists() {
            return p;
        }
    }

    // Add .exe on Windows
    let mut filename = default_name.to_string();
    if cfg!(windows) && !filename.to_lowercase().ends_with(".exe") {
        filename.push_str(".exe");
    }

    // 2) Check ffmpeg-sidecar managed binaries (for ffmpeg/ffprobe)
    if default_name == "ffmpeg" || default_name == "ffprobe" {
        if let Some(path) = get_ffmpeg_sidecar_path(default_name) {
            return path;
        }
    }

    // 3) Check sidecar next to executable
    if let Some(dir) = exe_dir() {
        let candidate = dir.join(&filename);
        if candidate.exists() {
            return candidate;
        }

        // 4) macOS app bundle layout:
        //    Dad Cam.app/Contents/MacOS/<exe>
        //    Dad Cam.app/Contents/Resources/<sidecars>
        if let Some(contents_dir) = dir.parent() {
            let resources = contents_dir.join("Resources").join(&filename);
            if resources.exists() {
                return resources;
            }
        }

        // Also check bin/ subdirectory (common bundling pattern)
        let bin_candidate = dir.join("bin").join(&filename);
        if bin_candidate.exists() {
            return bin_candidate;
        }
    }

    // 5) Fall back to PATH (dev-only convenience)
    PathBuf::from(default_name)
}

/// Get path to ffprobe binary
pub fn ffprobe_path() -> PathBuf {
    FFPROBE_PATH.get_or_init(|| resolve_tool("DADCAM_FFPROBE_PATH", "ffprobe")).clone()
}

/// Get path to ffmpeg binary
pub fn ffmpeg_path() -> PathBuf {
    FFMPEG_PATH.get_or_init(|| resolve_tool("DADCAM_FFMPEG_PATH", "ffmpeg")).clone()
}

/// Get path to exiftool binary
pub fn exiftool_path() -> PathBuf {
    EXIFTOOL_PATH.get_or_init(|| resolve_tool("DADCAM_EXIFTOOL_PATH", "exiftool")).clone()
}

/// Check if a tool is available at the resolved path
pub fn is_tool_available(tool: &str) -> bool {
    let path = match tool {
        "ffprobe" => ffprobe_path(),
        "ffmpeg" => ffmpeg_path(),
        "exiftool" => exiftool_path(),
        _ => return false,
    };

    // If path exists as a file, it's available
    if path.exists() {
        return true;
    }

    // Otherwise try running it (for PATH fallback)
    std::process::Command::new(&path)
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ensure ffmpeg/ffprobe binaries are available.
/// Downloads them via ffmpeg-sidecar if not present.
/// Returns (ffmpeg_path, ffprobe_path) on success.
pub fn ensure_ffmpeg() -> anyhow::Result<(PathBuf, PathBuf)> {
    // First check if already available
    let ffmpeg = ffmpeg_path();
    let ffprobe = ffprobe_path();

    if is_tool_available("ffmpeg") && is_tool_available("ffprobe") {
        return Ok((ffmpeg, ffprobe));
    }

    // Try to download via ffmpeg-sidecar
    log::info!("FFmpeg not found, attempting to download via ffmpeg-sidecar...");

    ffmpeg_sidecar::download::auto_download()
        .map_err(|e| anyhow::anyhow!("Failed to download FFmpeg: {}", e))?;

    // Clear cached paths and re-resolve
    // Note: OnceLock doesn't support clearing, so we check sidecar directly
    let ffmpeg = ffmpeg_sidecar::paths::ffmpeg_path();
    let ffprobe = ffmpeg_sidecar::ffprobe::ffprobe_path();

    if !ffmpeg.exists() {
        return Err(anyhow::anyhow!("FFmpeg binary not found at {:?}", ffmpeg));
    }
    if !ffprobe.exists() {
        return Err(anyhow::anyhow!("FFprobe binary not found at {:?}", ffprobe));
    }

    log::info!("FFmpeg downloaded successfully");
    Ok((ffmpeg, ffprobe))
}

/// Ensure exiftool is available.
/// Unlike ffmpeg (which auto-downloads via ffmpeg-sidecar), exiftool must be
/// bundled at build time or installed on the system. Returns an error with a
/// clear message if missing.
pub fn ensure_exiftool() -> anyhow::Result<PathBuf> {
    let path = exiftool_path();

    if is_tool_available("exiftool") {
        return Ok(path);
    }

    Err(anyhow::anyhow!(
        "exiftool not found. Metadata extraction will be unavailable. \
         Install exiftool (https://exiftool.org) or run scripts/download-exiftool.sh before building."
    ))
}

/// Check all required tools and report status
pub fn check_tools() -> Vec<(String, bool, String)> {
    let tools = vec![
        ("ffmpeg", ffmpeg_path()),
        ("ffprobe", ffprobe_path()),
        ("exiftool", exiftool_path()),
    ];

    tools
        .into_iter()
        .map(|(name, path)| {
            let available = is_tool_available(name);
            let path_str = path.to_string_lossy().to_string();
            (name.to_string(), available, path_str)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_tool_fallback() {
        // Without env var set, should return the default name for PATH lookup
        let path = resolve_tool("DADCAM_TEST_NONEXISTENT", "testcmd");
        // Either finds via sidecar or falls back to PATH
        assert!(!path.to_string_lossy().is_empty());
    }

    #[test]
    fn test_env_override() {
        // Set a temp env var pointing to an existing file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("dadcam_test_tool");
        std::fs::write(&temp_file, "test").ok();

        std::env::set_var("DADCAM_TEST_TOOL", temp_file.to_str().unwrap());
        let path = resolve_tool("DADCAM_TEST_TOOL", "default");
        assert_eq!(path, temp_file);

        // Cleanup
        std::env::remove_var("DADCAM_TEST_TOOL");
        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_check_tools_returns_status() {
        let status = check_tools();
        assert_eq!(status.len(), 3);
        assert!(status.iter().any(|(name, _, _)| name == "ffmpeg"));
        assert!(status.iter().any(|(name, _, _)| name == "ffprobe"));
        assert!(status.iter().any(|(name, _, _)| name == "exiftool"));
    }
}
