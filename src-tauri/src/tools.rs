// External tool resolver for bundled ffmpeg/ffprobe/exiftool
//
// Dad Cam policy (contracts.md): all tools must be bundled with the app.
// Resolution order:
// 1) Environment variable override (DADCAM_FFPROBE_PATH, etc.)
// 2) Sidecar next to the executable
// 3) macOS app bundle Resources fallback
// 4) PATH fallback (dev-only convenience)

use std::env;
use std::path::PathBuf;

/// Get the directory containing the current executable
fn exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
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

    // 2) Check sidecar next to executable
    if let Some(dir) = exe_dir() {
        let candidate = dir.join(&filename);
        if candidate.exists() {
            return candidate;
        }

        // 3) macOS app bundle layout:
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

    // 4) Fall back to PATH (dev-only convenience)
    PathBuf::from(default_name)
}

/// Get path to ffprobe binary
pub fn ffprobe_path() -> PathBuf {
    resolve_tool("DADCAM_FFPROBE_PATH", "ffprobe")
}

/// Get path to ffmpeg binary
pub fn ffmpeg_path() -> PathBuf {
    resolve_tool("DADCAM_FFMPEG_PATH", "ffmpeg")
}

/// Get path to exiftool binary
pub fn exiftool_path() -> PathBuf {
    resolve_tool("DADCAM_EXIFTOOL_PATH", "exiftool")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_tool_fallback() {
        // Without env var set, should return the default name for PATH lookup
        let path = resolve_tool("DADCAM_TEST_NONEXISTENT", "testcmd");
        assert_eq!(path, PathBuf::from("testcmd"));
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
}
