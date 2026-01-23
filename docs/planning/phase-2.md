Dad Cam - Phase 2 Implementation Guide

Version: 1.0
Target Audience: Developers new to Rust/Tauri

---

Overview

Phase 2 builds the preview pipeline. Every clip becomes watchable and scrubbable instantly through generated proxies, thumbnails, and sprite sheets.

When complete, you can:
- Generate H.264 720p proxy videos for smooth playback
- Generate poster frame thumbnails for each clip
- Generate sprite sheets for hover scrubbing
- Regenerate derived assets when pipeline version changes
- Queue and run preview generation jobs from CLI

Prerequisites:
- Phase 1 complete and working
- Phase 1 `tools.rs` resolver present (ffmpeg/ffprobe/exiftool bundled sidecars)
- FFmpeg and ffprobe installed (or bundled via ffmpeg-sidecar)
- Test library with ingested clips
- Understanding of Phase 1 job system


---

Tool Resolution (No PATH Assumptions)

Phase 0 requires bundling ffmpeg/ffprobe/exiftool as sidecars. Phase 1 adds `src-tauri/src/tools.rs`, which resolves tools in this order:
1) `DADCAM_*_PATH` env override
2) sidecar next to the executable
3) macOS app `Contents/Resources/`
4) PATH fallback (dev-only)

Phase 2 code snippets call `crate::tools::ffmpeg_path()` (and never hardcode `"ffmpeg"`).
---

What We're Building

Phase 2 adds three new job types to the existing job system:

1. **Proxy Job**: Creates H.264 720p video for smooth playback
2. **Thumb Job**: Creates JPG poster frame for grid display
3. **Sprite Job**: Creates tiled JPG strip for hover scrubbing

Each derived asset:
- Is stored in `.dadcam/proxies/`, `.dadcam/thumbs/`, or `.dadcam/sprites/`
- Is linked to its clip via `clip_assets` table
- Records `pipeline_version` and `derived_params` for invalidation tracking

---

Part 1: Understanding the Preview Pipeline

1.1 Why Proxies?

Original footage from dad cams is often:
- Interlaced (requires deinterlacing)
- Variable frame rate (causes playback issues)
- High bitrate (slow to decode)
- Unusual codecs (AVCHD, DV, etc.)

Proxies solve this by converting everything to a common format:
- H.264 codec (universal hardware decoding)
- 720p resolution (fast to render, good enough for preview)
- Constant frame rate (smooth playback)
- AAC audio (universal support)

1.2 Thumbnail Strategy

Thumbnails are poster frames shown in the grid view:
- One JPG per clip
- Extracted from video at 10% duration (avoids black frames at start)
- Stored at source aspect ratio, max 480px wide

1.3 Sprite Sheet Strategy

Sprite sheets enable hover scrubbing without loading video:
- Tiled strip of frames extracted at ~1 fps
- Each tile is 160px wide
- Frames arranged horizontally
- CSS/JS can calculate which tile to show based on hover position

1.4 Pipeline Versioning

When we change how proxies/thumbs/sprites are generated (new codec settings, new thumbnail algorithm, etc.), we bump `PIPELINE_VERSION`. This invalidates all derived assets built with older versions.

The staleness check compares:
- `pipeline_version` in constants vs stored in asset
- `derived_params` hash (settings used to generate)

---

Part 2: FFmpeg Command Patterns

2.1 FFmpeg for Proxies

The proxy generation command pattern:

```bash
ffmpeg -i input.mts \
  -vf "yadif=mode=1,scale=-2:720" \
  -c:v libx264 -preset medium -crf 23 \
  -r 30 \
  -c:a aac -b:a 128k \
  -movflags +faststart \
  output.mp4
```

Breakdown:
- `-vf "yadif=mode=1,scale=-2:720"`: Deinterlace (if needed) and scale to 720p height
- `-c:v libx264 -preset medium -crf 23`: H.264 encoding with good quality/size balance
- `-r 30`: Force constant 30fps
- `-c:a aac -b:a 128k`: AAC audio at 128kbps
- `-movflags +faststart`: Move metadata to start for streaming

2.2 FFmpeg for Thumbnails

```bash
ffmpeg -i input.mts \
  -ss 00:00:05 \
  -vframes 1 \
  -vf "scale='min(480,iw)':'-1'" \
  output.jpg
```

Breakdown:
- `-ss 00:00:05`: Seek to 5 seconds (or 10% of duration)
- `-vframes 1`: Extract single frame
- `-vf "scale='min(480,iw)':'-1'"`: Scale to max 480px wide, maintain aspect

2.3 FFmpeg for Sprite Sheets

```bash
ffmpeg -i input.mts \
  -vf "fps=1,scale=160:-1,tile=60x1" \
  output.jpg
```

Breakdown:
- `fps=1`: Extract 1 frame per second
- `scale=160:-1`: Scale each frame to 160px wide
- `tile=60x1`: Arrange 60 frames in a single row

For longer videos, we may need multiple sprite sheets or limit frame count.

---

Part 3: Proxy Generator Module

3.1 Create the Preview Module

Create `src-tauri/src/preview/mod.rs`:

```rust
pub mod proxy;
pub mod thumb;
pub mod sprite;

use std::path::{Path, PathBuf};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::constants::{
    PIPELINE_VERSION, PROXY_CODEC, PROXY_RESOLUTION, PROXY_CRF,
    THUMB_FORMAT, THUMB_QUALITY, SPRITE_FPS, SPRITE_TILE_WIDTH,
    DADCAM_FOLDER, DERIVED_PARAMS_HASH_ALGO,
};

/// Parameters used to generate a derived asset
/// These params are hashed to detect when regeneration is needed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedParams {
    pub pipeline_version: u32,
    pub preset: String,
    /// Camera profile ID - changes trigger regeneration (per development-plan.md)
    pub camera_profile_id: Option<i64>,
    /// Source file hash - changes trigger regeneration (per development-plan.md)
    pub source_hash: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl DerivedParams {
    /// Create params for proxy generation
    /// camera_profile_id and source_hash are included to trigger regeneration when they change
    pub fn for_proxy(
        deinterlace: bool,
        lut_id: Option<i64>,
        camera_profile_id: Option<i64>,
        source_hash: Option<String>,
    ) -> Self {
        Self {
            pipeline_version: PIPELINE_VERSION,
            preset: "proxy_720p".to_string(),
            camera_profile_id,
            source_hash,
            extra: serde_json::json!({
                "codec": PROXY_CODEC,
                "resolution": PROXY_RESOLUTION,
                "crf": PROXY_CRF,
                "deinterlace": deinterlace,
                "lut_id": lut_id,
            }),
        }
    }

    /// Create params for thumbnail generation
    pub fn for_thumb(camera_profile_id: Option<i64>, source_hash: Option<String>) -> Self {
        Self {
            pipeline_version: PIPELINE_VERSION,
            preset: "thumb_480".to_string(),
            camera_profile_id,
            source_hash,
            extra: serde_json::json!({
                "format": THUMB_FORMAT,
                "quality": THUMB_QUALITY,
                "max_width": 480,
            }),
        }
    }

    /// Create params for sprite generation
    pub fn for_sprite(
        duration_ms: i64,
        camera_profile_id: Option<i64>,
        source_hash: Option<String>,
    ) -> Self {
        // Calculate frame count (1 fps, max 120 frames)
        let duration_secs = duration_ms / 1000;
        let frame_count = duration_secs.min(120);

        Self {
            pipeline_version: PIPELINE_VERSION,
            preset: "sprite_160".to_string(),
            camera_profile_id,
            source_hash,
            extra: serde_json::json!({
                "fps": SPRITE_FPS,
                "tile_width": SPRITE_TILE_WIDTH,
                "frame_count": frame_count,
            }),
        }
    }

    /// Compute a hash of these params for comparison
    pub fn hash(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        let hash = blake3::hash(json.as_bytes());
        hash.to_hex()[..16].to_string() // Short hash for filename
    }

    /// Convert to JSON string for storage
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Get the output path for a derived asset
pub fn get_derived_path(
    library_root: &Path,
    clip_id: i64,
    role: &str,
    params: &DerivedParams,
    extension: &str,
) -> PathBuf {
    let subdir = match role {
        "proxy" => "proxies",
        "thumb" => "thumbs",
        "sprite" => "sprites",
        _ => "derived",
    };

    let params_hash = params.hash();
    let filename = format!("{}_{}.{}", clip_id, params_hash, extension);

    library_root
        .join(DADCAM_FOLDER)
        .join(subdir)
        .join(filename)
}

/// Convert a library-absolute path to a relative path for DB storage
pub fn to_relative_path(library_root: &Path, absolute_path: &Path) -> String {
    absolute_path
        .strip_prefix(library_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| absolute_path.to_string_lossy().to_string())
}
```

3.2 Create the Proxy Module

Create `src-tauri/src/preview/proxy.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};

use crate::constants::{PROXY_RESOLUTION, PROXY_CRF};
use crate::metadata::ffprobe::MediaInfo;

/// Options for proxy generation
#[derive(Debug, Clone)]
pub struct ProxyOptions {
    pub deinterlace: bool,
    pub target_fps: u32,
    pub lut_path: Option<String>,
}

impl Default for ProxyOptions {
    fn default() -> Self {
        Self {
            deinterlace: false,
            target_fps: 30,
            lut_path: None,
        }
    }
}

/// Determine if a video needs deinterlacing based on metadata
pub fn needs_deinterlace(media_info: &MediaInfo) -> bool {
    // Common interlaced formats
    if let Some(ref codec) = media_info.codec {
        let codec_lower = codec.to_lowercase();
        // MPEG-2 and DV are often interlaced
        if codec_lower.contains("mpeg2") || codec_lower.contains("dvvideo") {
            return true;
        }
    }

    // Check for interlaced resolution patterns
    if let Some(height) = media_info.height {
        // 1080i, 480i are common interlaced formats
        if height == 1080 || height == 480 || height == 576 {
            // Could be interlaced - safer to deinterlace
            // In production, check field_order from ffprobe
            return true;
        }
    }

    false
}

/// Generate a proxy video from the source file
pub fn generate_proxy(
    source_path: &Path,
    output_path: &Path,
    options: &ProxyOptions,
) -> Result<()> {
    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Build video filter chain
    let mut vf_filters = Vec::new();

    // Deinterlace if needed
    if options.deinterlace {
        vf_filters.push("yadif=mode=1".to_string());
    }

    // Scale to target resolution (720p height, maintain aspect)
    vf_filters.push(format!("scale=-2:{}", PROXY_RESOLUTION));

    // Apply LUT if provided
    if let Some(ref lut) = options.lut_path {
        vf_filters.push(format!("lut3d={}", lut));
    }

    let vf_string = vf_filters.join(",");

    // Build ffmpeg command
    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",                          // Overwrite output
        "-i", source_path.to_str().unwrap(),
        "-vf", &vf_string,
        "-c:v", "libx264",
        "-preset", "medium",
        "-crf", &PROXY_CRF.to_string(),
        "-r", &options.target_fps.to_string(),
        "-c:a", "aac",
        "-b:a", "128k",
        "-movflags", "+faststart",
        output_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg proxy generation failed: {}", stderr));
    }

    Ok(())
}

/// Generate a proxy for audio-only files (just re-encode audio)
pub fn generate_audio_proxy(
    source_path: &Path,
    output_path: &Path,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-i", source_path.to_str().unwrap(),
        "-c:a", "aac",
        "-b:a", "128k",
        output_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg audio proxy failed: {}", stderr));
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_deinterlace_mpeg2() {
        let info = MediaInfo {
            codec: Some("mpeg2video".to_string()),
            height: Some(480),
            ..Default::default()
        };
        assert!(needs_deinterlace(&info));
    }

    #[test]
    fn test_needs_deinterlace_h264() {
        let info = MediaInfo {
            codec: Some("h264".to_string()),
            height: Some(720),
            ..Default::default()
        };
        // 720p is usually progressive
        assert!(!needs_deinterlace(&info));
    }
}
```

3.3 Add Default for MediaInfo

Add this to `src-tauri/src/metadata/ffprobe.rs`:

```rust
impl Default for MediaInfo {
    fn default() -> Self {
        Self {
            duration_ms: None,
            width: None,
            height: None,
            fps: None,
            codec: None,
            audio_codec: None,
            has_audio: false,
            is_video: false,
            is_audio_only: false,
            is_image: false,
            creation_time: None,
        }
    }
}
```

---

Part 4: Thumbnail Generator Module

4.1 Create the Thumbnail Module

Create `src-tauri/src/preview/thumb.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};

use crate::constants::THUMB_QUALITY;

/// Options for thumbnail generation
#[derive(Debug, Clone)]
pub struct ThumbOptions {
    pub max_width: u32,
    pub seek_percent: f64,  // Where to extract frame (0.0 to 1.0)
}

impl Default for ThumbOptions {
    fn default() -> Self {
        Self {
            max_width: 480,
            seek_percent: 0.1,  // 10% into the video
        }
    }
}

/// Generate a thumbnail from a video file
pub fn generate_thumbnail(
    source_path: &Path,
    output_path: &Path,
    duration_ms: Option<i64>,
    options: &ThumbOptions,
) -> Result<()> {
    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Calculate seek time
    let seek_seconds = duration_ms
        .map(|d| (d as f64 / 1000.0) * options.seek_percent)
        .unwrap_or(1.0)  // Default to 1 second if no duration
        .max(0.1);       // At least 0.1 seconds in

    let seek_time = format_duration(seek_seconds);

    // Build scale filter
    let scale_filter = format!(
        "scale='min({},iw)':-1",
        options.max_width
    );

    // Build ffmpeg command
    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-ss", &seek_time,           // Seek before input (faster)
        "-i", source_path.to_str().unwrap(),
        "-vframes", "1",             // Single frame
        "-vf", &scale_filter,
        "-q:v", &((100 - THUMB_QUALITY) / 3).to_string(), // FFmpeg quality scale
        output_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg thumbnail generation failed: {}", stderr));
    }

    // Verify file was created
    if !output_path.exists() {
        return Err(anyhow!("Thumbnail file was not created"));
    }

    Ok(())
}

/// Generate a thumbnail from an image file (just resize)
pub fn generate_image_thumbnail(
    source_path: &Path,
    output_path: &Path,
    options: &ThumbOptions,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let scale_filter = format!(
        "scale='min({},iw)':-1",
        options.max_width
    );

    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-i", source_path.to_str().unwrap(),
        "-vf", &scale_filter,
        "-q:v", &((100 - THUMB_QUALITY) / 3).to_string(),
        output_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg image thumbnail failed: {}", stderr));
    }

    Ok(())
}

/// Format seconds as HH:MM:SS.mmm for ffmpeg
fn format_duration(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let secs = seconds % 60.0;
    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0.0), "00:00:00.000");
        assert_eq!(format_duration(5.5), "00:00:05.500");
        assert_eq!(format_duration(65.25), "00:01:05.250");
        assert_eq!(format_duration(3661.0), "01:01:01.000");
    }
}
```

---

Part 5: Sprite Sheet Generator Module

5.1 Create the Sprite Module

Create `src-tauri/src/preview/sprite.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};

use crate::constants::{SPRITE_FPS, SPRITE_TILE_WIDTH};

/// Options for sprite sheet generation
#[derive(Debug, Clone)]
pub struct SpriteOptions {
    pub fps: u32,           // Frames per second to extract
    pub tile_width: u32,    // Width of each tile in pixels
    pub max_frames: u32,    // Maximum number of frames
}

impl Default for SpriteOptions {
    fn default() -> Self {
        Self {
            fps: SPRITE_FPS,
            tile_width: SPRITE_TILE_WIDTH,
            max_frames: 120,  // 2 minutes of 1fps
        }
    }
}

/// Sprite sheet metadata (stored alongside the image)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpriteMetadata {
    pub frame_count: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub fps: u32,
    pub total_width: u32,
}

/// Generate a sprite sheet from a video file
pub fn generate_sprite_sheet(
    source_path: &Path,
    output_path: &Path,
    duration_ms: i64,
    options: &SpriteOptions,
) -> Result<SpriteMetadata> {
    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Calculate frame count based on duration
    let duration_secs = (duration_ms as f64 / 1000.0).ceil() as u32;
    let frame_count = (duration_secs / options.fps).min(options.max_frames).max(1);

    // Calculate tile dimensions
    // We'll use ffmpeg to tell us the actual height after scaling
    // For now, estimate based on common aspect ratios

    // Build video filter
    // fps: extract at target rate
    // scale: resize to tile width
    // tile: arrange in single row
    let vf_filter = format!(
        "fps=1/{},scale={}:-1,tile={}x1",
        options.fps,
        options.tile_width,
        frame_count
    );

    // Build ffmpeg command
    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-i", source_path.to_str().unwrap(),
        "-vf", &vf_filter,
        "-frames:v", "1",    // Output single image
        "-q:v", "5",         // Good quality for JPEG
        output_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg sprite generation failed: {}", stderr));
    }

    // Verify file was created
    if !output_path.exists() {
        return Err(anyhow!("Sprite sheet file was not created"));
    }

    // Get actual image dimensions to calculate tile height
    let (total_width, total_height) = get_image_dimensions(output_path)?;
    let tile_height = total_height;

    Ok(SpriteMetadata {
        frame_count,
        tile_width: options.tile_width,
        tile_height,
        fps: options.fps,
        total_width,
    })
}

/// Get image dimensions using ffprobe
fn get_image_dimensions(path: &Path) -> Result<(u32, u32)> {
    let output = Command::new(get_ffprobe_path())
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height",
            "-of", "csv=s=x:p=0",
        ])
        .arg(path)
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("Failed to get image dimensions"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split('x').collect();

    if parts.len() != 2 {
        return Err(anyhow!("Invalid dimension format: {}", stdout));
    }

    let width: u32 = parts[0].parse()?;
    let height: u32 = parts[1].parse()?;

    Ok((width, height))
}


fn get_ffprobe_path() -> String {
    "ffprobe".to_string()
}

/// Save sprite metadata to JSON file alongside the sprite image
pub fn save_sprite_metadata(
    sprite_path: &Path,
    metadata: &SpriteMetadata,
) -> Result<()> {
    let meta_path = sprite_path.with_extension("json");
    let json = serde_json::to_string_pretty(metadata)?;
    std::fs::write(meta_path, json)?;
    Ok(())
}

/// Load sprite metadata from JSON file
pub fn load_sprite_metadata(sprite_path: &Path) -> Result<SpriteMetadata> {
    let meta_path = sprite_path.with_extension("json");
    let json = std::fs::read_to_string(meta_path)?;
    let metadata: SpriteMetadata = serde_json::from_str(&json)?;
    Ok(metadata)
}
```

---

Part 6: Pipeline Versioning and Invalidation

6.1 Understanding Staleness

A derived asset is "stale" when ANY of these conditions are true:
1. The `pipeline_version` in the asset is less than `PIPELINE_VERSION` constant
2. The `derived_params` don't match current settings (includes camera_profile_id, LUT, etc.)
3. The source file has changed (hash mismatch)
4. The camera profile assigned to the clip has changed

Per development-plan.md, regeneration is triggered by:
- pipeline_version changes
- camera profile changes
- LUT changes
- proxy preset changes
- source file changes

6.2 Create the Staleness Checker

Add to `src-tauri/src/preview/mod.rs`:

```rust
use crate::db::schema::{Asset, Clip};

/// Check if a derived asset is stale and needs regeneration
/// Takes the clip to check camera_profile_id, and optionally the source asset hash
pub fn is_asset_stale(
    asset: &Asset,
    current_params: &DerivedParams,
    source_hash: Option<&str>,
) -> bool {
    // Check pipeline version
    if let Some(stored_version) = asset.pipeline_version {
        if (stored_version as u32) < current_params.pipeline_version {
            return true;  // Older version
        }
    } else {
        return true;  // No version stored
    }

    // Check params hash (includes camera_profile_id, lut_id, etc.)
    if let Some(ref stored_params) = asset.derived_params {
        match serde_json::from_str::<DerivedParams>(stored_params) {
            Ok(stored) => {
                if stored.hash() != current_params.hash() {
                    return true;  // Params changed
                }
                // Check source hash if provided (for source file changes)
                if let Some(src_hash) = source_hash {
                    if let Some(ref stored_src) = stored.source_hash {
                        if stored_src != src_hash {
                            return true;  // Source file changed
                        }
                    }
                }
            }
            Err(_) => return true,  // Can't parse stored params
        }
    } else {
        return true;  // No params stored
    }

    false  // Not stale
}

/// Find existing derived asset for a clip
pub fn find_derived_asset(
    conn: &rusqlite::Connection,
    clip_id: i64,
    role: &str,
) -> Result<Option<Asset>> {
    use rusqlite::params;

    let mut stmt = conn.prepare(
        r#"SELECT a.id, a.library_id, a.type, a.path, a.source_uri, a.size_bytes,
                  a.hash_fast, a.hash_fast_scheme, a.hash_full, a.verified_at,
                  a.pipeline_version, a.derived_params, a.created_at
           FROM assets a
           JOIN clip_assets ca ON ca.asset_id = a.id
           WHERE ca.clip_id = ?1 AND ca.role = ?2"#
    )?;

    let result = stmt.query_row(params![clip_id, role], |row| {
        Ok(Asset {
            id: row.get(0)?,
            library_id: row.get(1)?,
            asset_type: row.get(2)?,
            path: row.get(3)?,
            source_uri: row.get(4)?,
            size_bytes: row.get(5)?,
            hash_fast: row.get(6)?,
            hash_fast_scheme: row.get(7)?,
            hash_full: row.get(8)?,
            verified_at: row.get(9)?,
            pipeline_version: row.get(10)?,
            derived_params: row.get(11)?,
            created_at: row.get(12)?,
        })
    });

    match result {
        Ok(asset) => Ok(Some(asset)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
```

6.3 Database Updates for Derived Assets

Add to `src-tauri/src/db/schema.rs`:

```rust
/// Create a derived asset record
pub fn create_derived_asset(
    conn: &Connection,
    library_id: i64,
    asset_type: &str,
    path: &str,
    size_bytes: i64,
    pipeline_version: u32,
    derived_params: &str,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO assets
           (library_id, type, path, size_bytes, pipeline_version, derived_params)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
        params![
            library_id,
            asset_type,
            path,
            size_bytes,
            pipeline_version as i32,
            derived_params,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update an existing derived asset
pub fn update_derived_asset(
    conn: &Connection,
    asset_id: i64,
    path: &str,
    size_bytes: i64,
    pipeline_version: u32,
    derived_params: &str,
) -> Result<()> {
    conn.execute(
        r#"UPDATE assets
           SET path = ?2, size_bytes = ?3, pipeline_version = ?4, derived_params = ?5
           WHERE id = ?1"#,
        params![
            asset_id,
            path,
            size_bytes,
            pipeline_version as i32,
            derived_params,
        ],
    )?;
    Ok(())
}

/// Get clips that need preview generation
pub fn get_clips_needing_previews(
    conn: &Connection,
    library_id: i64,
    role: &str,
    limit: i64,
) -> Result<Vec<Clip>> {
    // Find clips that don't have the specified derived asset
    let mut stmt = conn.prepare(
        r#"SELECT c.id, c.library_id, c.original_asset_id, c.camera_profile_id,
                  c.media_type, c.title, c.duration_ms, c.width, c.height, c.fps,
                  c.codec, c.recorded_at, c.recorded_at_offset_minutes,
                  c.recorded_at_is_estimated, c.timestamp_source, c.created_at
           FROM clips c
           WHERE c.library_id = ?1
           AND NOT EXISTS (
               SELECT 1 FROM clip_assets ca
               WHERE ca.clip_id = c.id AND ca.role = ?2
           )
           ORDER BY c.created_at DESC
           LIMIT ?3"#
    )?;

    let clips = stmt.query_map(params![library_id, role, limit], |row| {
        Ok(Clip {
            id: row.get(0)?,
            library_id: row.get(1)?,
            original_asset_id: row.get(2)?,
            camera_profile_id: row.get(3)?,
            media_type: row.get(4)?,
            title: row.get(5)?,
            duration_ms: row.get(6)?,
            width: row.get(7)?,
            height: row.get(8)?,
            fps: row.get(9)?,
            codec: row.get(10)?,
            recorded_at: row.get(11)?,
            recorded_at_offset_minutes: row.get(12)?,
            recorded_at_is_estimated: row.get(13)?,
            timestamp_source: row.get(14)?,
            created_at: row.get(15)?,
        })
    })?;

    clips.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Get the original asset for a clip
pub fn get_clip_original_asset(conn: &Connection, clip_id: i64) -> Result<Option<Asset>> {
    let mut stmt = conn.prepare(
        r#"SELECT a.id, a.library_id, a.type, a.path, a.source_uri, a.size_bytes,
                  a.hash_fast, a.hash_fast_scheme, a.hash_full, a.verified_at,
                  a.pipeline_version, a.derived_params, a.created_at
           FROM assets a
           JOIN clips c ON c.original_asset_id = a.id
           WHERE c.id = ?1"#
    )?;

    let result = stmt.query_row(params![clip_id], |row| {
        Ok(Asset {
            id: row.get(0)?,
            library_id: row.get(1)?,
            asset_type: row.get(2)?,
            path: row.get(3)?,
            source_uri: row.get(4)?,
            size_bytes: row.get(5)?,
            hash_fast: row.get(6)?,
            hash_fast_scheme: row.get(7)?,
            hash_full: row.get(8)?,
            verified_at: row.get(9)?,
            pipeline_version: row.get(10)?,
            derived_params: row.get(11)?,
            created_at: row.get(12)?,
        })
    });

    match result {
        Ok(asset) => Ok(Some(asset)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
```

---

Part 7: Job Integration

7.1 Add New Job Types to Runner

Update `src-tauri/src/jobs/runner.rs` to handle preview jobs:

```rust
use crate::preview::{self, DerivedParams};
use crate::preview::proxy::{self, ProxyOptions};
use crate::preview::thumb::{self, ThumbOptions};
use crate::preview::sprite::{self, SpriteOptions};

// Add to the JobRunner impl block:

/// Process a proxy generation job
fn process_proxy_job(&self, conn: &Connection, job: &Job) -> Result<()> {
    let clip_id = job.clip_id.ok_or_else(|| anyhow::anyhow!("Proxy job missing clip_id"))?;

    // Get clip info
    let clips = schema::list_clips(conn, job.library_id.unwrap_or(0), 1)?;
    let clip = clips.into_iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| anyhow::anyhow!("Clip not found"))?;

    // Get original asset
    let original = schema::get_clip_original_asset(conn, clip_id)?
        .ok_or_else(|| anyhow::anyhow!("Original asset not found"))?;

    let original_path = self.library_root.join(&original.path);

    // Check media type - skip images, handle audio differently
    if clip.media_type == "image" {
        schema::log_job(conn, job.id, "info", "Skipping proxy for image")?;
        return Ok(());
    }

    // Determine if deinterlacing is needed
    let media_info = crate::metadata::ffprobe::probe(&original_path)?;
    let needs_deinterlace = proxy::needs_deinterlace(&media_info);

    // Build params - include camera_profile_id and source_hash for staleness tracking
    // Per development-plan.md: camera profile changes and source file changes trigger regeneration
    let params = DerivedParams::for_proxy(
        needs_deinterlace,
        None,  // lut_id - future enhancement
        clip.camera_profile_id,
        original.hash_fast.clone(),
    );

    // Check if we already have a valid proxy
    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "proxy")? {
        if !preview::is_asset_stale(&existing, &params, original.hash_fast.as_deref()) {
            schema::log_job(conn, job.id, "info", "Proxy already exists and is current")?;
            return Ok(());
        }
    }

    // Generate output path
    let extension = if clip.media_type == "audio" { "m4a" } else { "mp4" };
    let output_path = preview::get_derived_path(
        &self.library_root,
        clip_id,
        "proxy",
        &params,
        extension,
    );

    schema::log_job(conn, job.id, "info", &format!("Generating proxy: {}", output_path.display()))?;

    // Generate proxy
    if clip.media_type == "audio" {
        proxy::generate_audio_proxy(&original_path, &output_path)?;
    } else {
        let options = ProxyOptions {
            deinterlace: needs_deinterlace,
            ..Default::default()
        };
        proxy::generate_proxy(&original_path, &output_path, &options)?;
    }

    // Get file size
    let size_bytes = std::fs::metadata(&output_path)?.len() as i64;

    // Store in database
    let relative_path = preview::to_relative_path(&self.library_root, &output_path);
    let asset_id = schema::create_derived_asset(
        conn,
        clip.library_id,
        "proxy",
        &relative_path,
        size_bytes,
        params.pipeline_version,
        &params.to_json(),
    )?;

    // Link to clip
    schema::link_clip_asset(conn, clip_id, asset_id, "proxy")?;

    schema::log_job(conn, job.id, "info", "Proxy generation complete")?;

    Ok(())
}

/// Process a thumbnail generation job
fn process_thumb_job(&self, conn: &Connection, job: &Job) -> Result<()> {
    let clip_id = job.clip_id.ok_or_else(|| anyhow::anyhow!("Thumb job missing clip_id"))?;

    // Get clip info
    let clips = schema::list_clips(conn, job.library_id.unwrap_or(0), 1)?;
    let clip = clips.into_iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| anyhow::anyhow!("Clip not found"))?;

    // Get original asset
    let original = schema::get_clip_original_asset(conn, clip_id)?
        .ok_or_else(|| anyhow::anyhow!("Original asset not found"))?;

    let original_path = self.library_root.join(&original.path);

    // Build params - include camera_profile_id and source_hash for staleness tracking
    let params = DerivedParams::for_thumb(clip.camera_profile_id, original.hash_fast.clone());

    // Check for existing
    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "thumb")? {
        if !preview::is_asset_stale(&existing, &params, original.hash_fast.as_deref()) {
            schema::log_job(conn, job.id, "info", "Thumbnail already exists and is current")?;
            return Ok(());
        }
    }

    // Generate output path
    let output_path = preview::get_derived_path(
        &self.library_root,
        clip_id,
        "thumb",
        &params,
        "jpg",
    );

    schema::log_job(conn, job.id, "info", &format!("Generating thumbnail: {}", output_path.display()))?;

    // Generate thumbnail
    // Note: "Best frame" heuristic uses 10% duration. Future enhancement could analyze
    // scene changes, faces, or sharpness for better frame selection.
    let options = ThumbOptions::default();
    if clip.media_type == "image" {
        thumb::generate_image_thumbnail(&original_path, &output_path, &options)?;
    } else {
        thumb::generate_thumbnail(&original_path, &output_path, clip.duration_ms, &options)?;
    }

    // Store in database
    let size_bytes = std::fs::metadata(&output_path)?.len() as i64;
    let relative_path = preview::to_relative_path(&self.library_root, &output_path);
    let asset_id = schema::create_derived_asset(
        conn,
        clip.library_id,
        "thumb",
        &relative_path,
        size_bytes,
        params.pipeline_version,
        &params.to_json(),
    )?;

    schema::link_clip_asset(conn, clip_id, asset_id, "thumb")?;

    schema::log_job(conn, job.id, "info", "Thumbnail generation complete")?;

    Ok(())
}

/// Process a sprite sheet generation job
fn process_sprite_job(&self, conn: &Connection, job: &Job) -> Result<()> {
    let clip_id = job.clip_id.ok_or_else(|| anyhow::anyhow!("Sprite job missing clip_id"))?;

    // Get clip info
    let clips = schema::list_clips(conn, job.library_id.unwrap_or(0), 1)?;
    let clip = clips.into_iter()
        .find(|c| c.id == clip_id)
        .ok_or_else(|| anyhow::anyhow!("Clip not found"))?;

    // Skip non-video clips
    if clip.media_type != "video" {
        schema::log_job(conn, job.id, "info", "Skipping sprite for non-video clip")?;
        return Ok(());
    }

    let duration_ms = clip.duration_ms.ok_or_else(|| anyhow::anyhow!("Clip has no duration"))?;

    // Get original asset
    let original = schema::get_clip_original_asset(conn, clip_id)?
        .ok_or_else(|| anyhow::anyhow!("Original asset not found"))?;

    let original_path = self.library_root.join(&original.path);

    // Build params - include camera_profile_id and source_hash for staleness tracking
    let params = DerivedParams::for_sprite(
        duration_ms,
        clip.camera_profile_id,
        original.hash_fast.clone(),
    );

    // Check for existing
    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "sprite")? {
        if !preview::is_asset_stale(&existing, &params, original.hash_fast.as_deref()) {
            schema::log_job(conn, job.id, "info", "Sprite already exists and is current")?;
            return Ok(());
        }
    }

    // Generate output path
    let output_path = preview::get_derived_path(
        &self.library_root,
        clip_id,
        "sprite",
        &params,
        "jpg",
    );

    schema::log_job(conn, job.id, "info", &format!("Generating sprite: {}", output_path.display()))?;

    // Generate sprite
    let options = SpriteOptions::default();
    let metadata = sprite::generate_sprite_sheet(&original_path, &output_path, duration_ms, &options)?;

    // Save sprite metadata
    sprite::save_sprite_metadata(&output_path, &metadata)?;

    // Store in database
    let size_bytes = std::fs::metadata(&output_path)?.len() as i64;
    let relative_path = preview::to_relative_path(&self.library_root, &output_path);
    let asset_id = schema::create_derived_asset(
        conn,
        clip.library_id,
        "sprite",
        &relative_path,
        size_bytes,
        params.pipeline_version,
        &params.to_json(),
    )?;

    schema::link_clip_asset(conn, clip_id, asset_id, "sprite")?;

    schema::log_job(conn, job.id, "info", "Sprite generation complete")?;

    Ok(())
}

/// Run a single job based on type
fn run_job(&self, conn: &Connection, job: &Job) -> Result<()> {
    match job.job_type.as_str() {
        "ingest" => self.process_ingest_job(conn, job),
        "proxy" => self.process_proxy_job(conn, job),
        "thumb" => self.process_thumb_job(conn, job),
        "sprite" => self.process_sprite_job(conn, job),
        "hash_full" => self.process_hash_job(conn, job),
        _ => Err(anyhow::anyhow!("Unknown job type: {}", job.job_type)),
    }
}
```

7.2 Queue Preview Jobs After Ingest

Add this function to create preview jobs for newly ingested clips:

```rust
/// Queue preview generation jobs for a clip
pub fn queue_preview_jobs(conn: &Connection, clip_id: i64, library_id: i64) -> Result<()> {
    // Queue proxy job
    schema::create_job(conn, &schema::NewJob {
        job_type: "proxy".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 5,  // Medium priority
        payload: "{}".to_string(),
    })?;

    // Queue thumbnail job (higher priority - needed for UI)
    schema::create_job(conn, &schema::NewJob {
        job_type: "thumb".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 8,  // High priority
        payload: "{}".to_string(),
    })?;

    // Queue sprite job (lower priority)
    schema::create_job(conn, &schema::NewJob {
        job_type: "sprite".to_string(),
        library_id: Some(library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 3,  // Lower priority
        payload: "{}".to_string(),
    })?;

    Ok(())
}
```

---

Part 8: CLI Commands for Phase 2

8.1 Add Preview Commands

Update `src-tauri/src/cli.rs` to add preview-related commands:

```rust
#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands from Phase 1 ...

    /// Generate previews for clips
    Preview {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Type of preview to generate (proxy, thumb, sprite, all)
        #[arg(short = 't', long, default_value = "all")]
        preview_type: String,

        /// Specific clip ID (optional, generates for all if not specified)
        #[arg(short, long)]
        clip: Option<i64>,

        /// Force regeneration even if preview exists
        #[arg(short, long)]
        force: bool,

        /// Maximum number of clips to process
        #[arg(short = 'n', long, default_value = "100")]
        limit: i64,
    },

    /// Show preview status for clips
    PreviewStatus {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Show only clips missing previews
        #[arg(short, long)]
        missing_only: bool,
    },

    /// Invalidate and regenerate previews (when pipeline version changes)
    Invalidate {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,

        /// Type to invalidate (proxy, thumb, sprite, all)
        #[arg(short = 't', long, default_value = "all")]
        preview_type: String,

        /// Actually delete files (default is dry-run)
        #[arg(long)]
        confirm: bool,
    },
}
```

8.2 Implement Preview Command

Add to the `run()` function:

```rust
Commands::Preview { library, preview_type, clip, force, limit } => {
    let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
    cmd_preview(&lib_root, &preview_type, clip, force, limit)
}
Commands::PreviewStatus { library, missing_only } => {
    let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
    cmd_preview_status(&lib_root, missing_only)
}
Commands::Invalidate { library, preview_type, confirm } => {
    let lib_root = library.unwrap_or_else(|| PathBuf::from("."));
    cmd_invalidate(&lib_root, &preview_type, confirm)
}
```

8.3 Implement Command Functions

```rust
fn cmd_preview(
    library_root: &PathBuf,
    preview_type: &str,
    clip_id: Option<i64>,
    force: bool,
    limit: i64,
) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    // Determine which job types to run
    let job_types: Vec<&str> = match preview_type {
        "proxy" => vec!["proxy"],
        "thumb" => vec!["thumb"],
        "sprite" => vec!["sprite"],
        "all" => vec!["thumb", "proxy", "sprite"],  // Thumb first (fastest)
        _ => return Err(anyhow::anyhow!("Invalid preview type: {}", preview_type)),
    };

    // If specific clip, queue jobs for it
    if let Some(cid) = clip_id {
        println!("Queueing preview jobs for clip {}", cid);
        for jt in &job_types {
            schema::create_job(&conn, &schema::NewJob {
                job_type: jt.to_string(),
                library_id: Some(library.id),
                clip_id: Some(cid),
                asset_id: None,
                priority: 10,
                payload: if force { r#"{"force": true}"#.to_string() } else { "{}".to_string() },
            })?;
        }
    } else {
        // Queue jobs for clips missing previews
        for jt in &job_types {
            let clips = schema::get_clips_needing_previews(&conn, library.id, jt, limit)?;
            println!("Found {} clips needing {} generation", clips.len(), jt);

            for clip in clips {
                schema::create_job(&conn, &schema::NewJob {
                    job_type: jt.to_string(),
                    library_id: Some(library.id),
                    clip_id: Some(clip.id),
                    asset_id: None,
                    priority: 5,
                    payload: "{}".to_string(),
                })?;
            }
        }
    }

    // Run jobs
    let runner = JobRunner::new(library_root);
    for jt in &job_types {
        let completed = runner.run_until_empty(jt)?;
        println!("Completed {} {} jobs", completed, jt);
    }

    Ok(())
}

fn cmd_preview_status(library_root: &PathBuf, missing_only: bool) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    // Get all clips with their preview status
    let mut stmt = conn.prepare(
        r#"SELECT c.id, c.title, c.media_type,
                  (SELECT COUNT(*) FROM clip_assets WHERE clip_id = c.id AND role = 'proxy') as has_proxy,
                  (SELECT COUNT(*) FROM clip_assets WHERE clip_id = c.id AND role = 'thumb') as has_thumb,
                  (SELECT COUNT(*) FROM clip_assets WHERE clip_id = c.id AND role = 'sprite') as has_sprite
           FROM clips c
           WHERE c.library_id = ?1
           ORDER BY c.id"#
    )?;

    println!("{:<6} {:<30} {:<8} {:<8} {:<8} {:<8}",
        "ID", "Title", "Type", "Proxy", "Thumb", "Sprite");
    println!("{}", "-".repeat(78));

    let rows = stmt.query_map(params![library.id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i32>(3)?,
            row.get::<_, i32>(4)?,
            row.get::<_, i32>(5)?,
        ))
    })?;

    let mut total = 0;
    let mut missing = 0;

    for row in rows {
        let (id, title, media_type, proxy, thumb, sprite) = row?;

        let has_all = proxy > 0 && thumb > 0 && (sprite > 0 || media_type != "video");

        if missing_only && has_all {
            continue;
        }

        let title_short = if title.len() > 28 {
            format!("{}...", &title[..25])
        } else {
            title
        };

        let proxy_str = if proxy > 0 { "OK" } else { "MISSING" };
        let thumb_str = if thumb > 0 { "OK" } else { "MISSING" };
        let sprite_str = if media_type != "video" {
            "N/A"
        } else if sprite > 0 {
            "OK"
        } else {
            "MISSING"
        };

        println!("{:<6} {:<30} {:<8} {:<8} {:<8} {:<8}",
            id, title_short, media_type, proxy_str, thumb_str, sprite_str);

        total += 1;
        if !has_all {
            missing += 1;
        }
    }

    println!("{}", "-".repeat(78));
    println!("Total: {} clips, {} with missing previews", total, missing);

    Ok(())
}

fn cmd_invalidate(
    library_root: &PathBuf,
    preview_type: &str,
    confirm: bool,
) -> Result<()> {
    let db_path = db::get_db_path(library_root);
    let conn = db::open_db(&db_path)?;

    let root_path = library_root.canonicalize()?.to_string_lossy().to_string();
    let library = schema::get_library_by_path(&conn, &root_path)?
        .ok_or_else(|| anyhow::anyhow!("Library not found"))?;

    // Find stale assets
    let asset_types: Vec<&str> = match preview_type {
        "proxy" => vec!["proxy"],
        "thumb" => vec!["thumb"],
        "sprite" => vec!["sprite"],
        "all" => vec!["proxy", "thumb", "sprite"],
        _ => return Err(anyhow::anyhow!("Invalid preview type")),
    };

    let current_version = crate::constants::PIPELINE_VERSION;

    for asset_type in asset_types {
        let mut stmt = conn.prepare(
            r#"SELECT id, path FROM assets
               WHERE library_id = ?1 AND type = ?2
               AND (pipeline_version IS NULL OR pipeline_version < ?3)"#
        )?;

        let stale: Vec<(i64, String)> = stmt
            .query_map(params![library.id, asset_type, current_version as i32], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        println!("Found {} stale {} assets", stale.len(), asset_type);

        if !confirm {
            println!("Run with --confirm to delete these files and queue regeneration");
            continue;
        }

        for (asset_id, rel_path) in stale {
            let full_path = library_root.join(&rel_path);

            // Delete file
            if full_path.exists() {
                std::fs::remove_file(&full_path)?;
                println!("Deleted: {}", rel_path);
            }

            // Remove from clip_assets
            conn.execute(
                "DELETE FROM clip_assets WHERE asset_id = ?1",
                params![asset_id],
            )?;

            // Delete asset record
            conn.execute(
                "DELETE FROM assets WHERE id = ?1",
                params![asset_id],
            )?;
        }

        println!("Deleted stale {} assets. Run 'dadcam preview' to regenerate.", asset_type);
    }

    Ok(())
}
```

---

Part 9: Testing Your Implementation

9.1 Build and Test

```bash
# Navigate to src-tauri
cd src-tauri

# Build
cargo build

# Run tests
cargo test

# Test CLI help
./target/debug/dadcam preview --help
./target/debug/dadcam preview-status --help
```

9.2 Test Workflow

```bash
# 1. Ensure you have a test library with ingested clips
./target/debug/dadcam list --library ~/test-library

# 2. Check preview status (should show MISSING)
./target/debug/dadcam preview-status --library ~/test-library

# 3. Generate thumbnails first (fastest)
./target/debug/dadcam preview --library ~/test-library -t thumb

# 4. Check status again (thumbs should be OK)
./target/debug/dadcam preview-status --library ~/test-library

# 5. Generate proxies (slowest)
./target/debug/dadcam preview --library ~/test-library -t proxy

# 6. Generate sprites
./target/debug/dadcam preview --library ~/test-library -t sprite

# 7. Verify all previews
./target/debug/dadcam preview-status --library ~/test-library

# 8. Check the generated files
ls -la ~/test-library/.dadcam/proxies/
ls -la ~/test-library/.dadcam/thumbs/
ls -la ~/test-library/.dadcam/sprites/
```

9.3 Test Specific Clip

```bash
# Generate previews for a single clip
./target/debug/dadcam preview --library ~/test-library --clip 1

# Force regeneration
./target/debug/dadcam preview --library ~/test-library --clip 1 --force
```

9.4 Test Invalidation

```bash
# Dry run - see what would be deleted
./target/debug/dadcam invalidate --library ~/test-library

# Actually delete and queue regeneration
./target/debug/dadcam invalidate --library ~/test-library --confirm
./target/debug/dadcam preview --library ~/test-library
```

9.5 Verify Database

```bash
# Check assets table for derived assets
sqlite3 ~/test-library/.dadcam/dadcam.db \
  "SELECT id, type, path, pipeline_version FROM assets WHERE type IN ('proxy', 'thumb', 'sprite')"

# Check clip_assets links
sqlite3 ~/test-library/.dadcam/dadcam.db \
  "SELECT ca.clip_id, ca.role, a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id WHERE ca.role != 'primary'"
```

---

Part 10: Checklist

Before moving to Phase 3, verify:

**Core Functionality:**
[ ] Preview module compiles without errors
[ ] Proxy generation creates valid H.264 720p videos
[ ] Proxy handles interlaced footage (deinterlace filter works)
[ ] Proxy handles audio-only clips (creates m4a)
[ ] Thumbnail generation creates JPG poster frames
[ ] Thumbnails extract frame from 10% into video (not black frame)
[ ] Sprite generation creates tiled JPG strips
[ ] Sprite metadata JSON is saved alongside sprite image

**Pipeline Versioning (per development-plan.md):**
[ ] Pipeline versioning stores version in database
[ ] Derived params include camera_profile_id
[ ] Derived params include source_hash (hash_fast)
[ ] Staleness check detects: old pipeline_version
[ ] Staleness check detects: camera_profile_id changes
[ ] Staleness check detects: source file hash changes
[ ] Staleness check detects: params hash changes (LUT, preset, etc.)
[ ] Invalidation command finds and deletes stale assets

**Job System:**
[ ] Preview jobs are queued after ingest
[ ] Preview jobs handle errors gracefully
[ ] Preview jobs skip if non-stale asset exists (idempotent)

**CLI:**
[ ] All CLI commands work correctly
[ ] preview-status shows accurate status
[ ] Regeneration after invalidation produces identical results

**Database:**
[ ] Database records link derived assets to clips correctly
[ ] Derived params stored as JSON in database

---

Part 10a: Deferred Enhancements

The following features are structured in the code but not fully implemented in Phase 2. They are planned for later phases:

**LUT Management (Phase 5 - VHS Mode):**
- The `lut_id` parameter exists in DerivedParams
- The proxy generator has `lut_path` option that applies LUT via ffmpeg `lut3d` filter
- Full LUT management (loading from database, LUT library) deferred to Phase 5 when VHS Mode is built

**Best-Frame Heuristic (Future Enhancement):**
- Current implementation uses simple 10% duration for thumbnail extraction
- Future enhancement could analyze:
  - Scene change detection (avoid mid-transition frames)
  - Face detection (prefer frames with faces)
  - Sharpness analysis (avoid blurry frames)
  - Motion analysis (prefer stable frames)
- This is a "nice to have" - 10% duration works well for most dad cam footage

**Transform Rules from Camera Profiles (Phase 5):**
- Camera profiles have `transform_rules` field
- Currently only `deinterlace` is auto-detected from media info
- Full transform application (color correction, aspect ratio fixes) deferred to Phase 5

---

Part 11: Operational Hardening (Make It Shippable)

This section closes the remaining “implementation guide gaps” so Phase 2 can be built and used in the real world without papering over edge cases.

12.1 Bundle FFmpeg (No System Dependency)

Right now the code samples assume `ffmpeg` / `ffprobe` are in PATH (and even includes a TODO). Replace that with **ffmpeg-sidecar** so preview generation works on end-user machines.

**Goal:** one helper that returns absolute paths to `ffmpeg` and `ffprobe`, used everywhere.

Create `src-tauri/src/ffmpeg/mod.rs`:

```rust
use anyhow::{Result, anyhow};
use std::path::PathBuf;

/// Ensure ffmpeg binaries are present and return paths.
/// Uses ffmpeg-sidecar to download/extract bundled binaries per-platform.
pub fn ensure_ffmpeg() -> Result<(PathBuf, PathBuf)> {
    // NOTE: exact APIs can vary by ffmpeg-sidecar version;
    // keep this as a thin wrapper so you only touch one file if the crate changes.
    let ffmpeg = ffmpeg_sidecar::ffmpeg::ffmpeg_path()
        .map_err(|e| anyhow!("ffmpeg not available: {e}"))?;
    let ffprobe = ffmpeg_sidecar::ffprobe::ffprobe_path()
        .map_err(|e| anyhow!("ffprobe not available: {e}"))?;
    Ok((ffmpeg, ffprobe))
}
```

Then update the generator modules:

- In `preview/proxy.rs`, replace `fn get_ffmpeg_path() -> String` with a call into `crate::ffmpeg::ensure_ffmpeg()?` (store paths once per job run, not per command).
- In `preview/sprite.rs`, do the same for both ffmpeg and ffprobe.
- In `preview/thumb.rs`, do the same for ffmpeg.

**Why this matters:** it removes an entire class of user-facing “it works on my machine” failures.

12.2 Atomic Writes (Avoid Corrupt Partials)

FFmpeg can be interrupted. Always write to a temp file and rename:

Pattern:

```rust
let tmp = output_path.with_extension("tmp");
run_ffmpeg_to(&tmp)?;
std::fs::rename(&tmp, output_path)?;
```

Also:
- If a job fails, delete any temp file.
- If a job succeeds, verify `output_path.exists()` and `metadata.len() > 0`.

12.3 Sprite Sheets for Long Videos (Paging)

The earlier sprite implementation creates **one giant row** (`tile=frame_count x 1`). That breaks for long clips (very wide JPEGs, memory spikes).

**Policy:** cap each sprite sheet to `SPRITE_PAGE_COLS` frames (ex: 60). Generate multiple pages when needed.

Add constants (in `constants.rs`):

```rust
pub const SPRITE_PAGE_COLS: u32 = 60;     // frames per sheet
pub const SPRITE_MAX_FRAMES: u32 = 600;   // overall cap per clip (10 minutes @ 1fps)
```

Update `SpriteMetadata` to support paging:

```rust
pub struct SpriteMetadata {
    pub fps: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub frame_count_total: u32,
    pub frames_per_page: u32,
    pub page_count: u32,
    pub page_index: u32,     // this file’s page
    pub total_width: u32,    // of this page image
}
```

Generation algorithm:

- `frame_count_total = min((duration_secs / fps), SPRITE_MAX_FRAMES).max(1)`
- `page_count = ceil(frame_count_total / SPRITE_PAGE_COLS)`
- For each `page_index`:
  - `frames_this_page = min(SPRITE_PAGE_COLS, frame_count_total - page_index*SPRITE_PAGE_COLS)`
  - FFmpeg filter: `fps=1/fps,scale=tile_width:-1,select='between(n,start,end)',tile=frames_this_pagex1`
  - Output file naming: `clipId_paramsHash_p{page_index}.jpg`

**Storage model (important):** you can keep `clip_assets.role = "sprite"` and attach multiple assets for the same role. The existing schema allows this because the primary key is `(clip_id, asset_id)`.

Add a helper query to list all derived assets for a role, ordered:

```sql
SELECT a.* FROM assets a
JOIN clip_assets ca ON ca.asset_id=a.id
WHERE ca.clip_id=?1 AND ca.role=?2
ORDER BY a.created_at ASC, a.path ASC;
```

In UI (Phase 3), you load sprite page metadata JSON and pick the correct page based on hover position.

12.4 Avoid Derived Asset Duplication (Update Instead of Append)

In the current sample, when an asset is stale you regenerate and always call `create_derived_asset()` + `link_clip_asset()`. That can leave:
- multiple proxies/thumbs/sprites for the same clip
- orphaned files in `.dadcam/*`

**Fix:** if an existing asset for a role is stale, overwrite it in-place (generate new file, then `update_derived_asset()`), and keep a single link.

Implementation guidance:

- If `find_derived_asset()` returns an asset:
  - Generate to a new path (params hash changes ⇒ new filename) OR generate to the same path.
  - Prefer **new path**, then:
    1) generate temp → final path atomically  
    2) delete old file at `existing.path` (best-effort)  
    3) `update_derived_asset(conn, existing.id, new_relative_path, size, PIPELINE_VERSION, params_json)`  
- If no existing asset, create + link as before.

12.5 Cleanup Command (Keep Libraries Healthy)

Add a CLI cleanup that:
1) deletes derived files not referenced by DB (orphans)
2) optionally keeps only the newest derived asset per `(clip_id, role)` (if duplication existed from earlier builds)
3) optionally enforces a size cap for derived assets

Add a new CLI command:

```rust
Cleanup {
  #[arg(short, long)] library: Option<PathBuf>,
  #[arg(long, default_value="derived")] scope: String, // derived|all
  #[arg(long)] keep_latest_per_role: bool,
  #[arg(long)] max_derived_gb: Option<u64>,
  #[arg(long)] confirm: bool,
}
```

Behavior (dry-run by default):
- Scan `.dadcam/proxies`, `.dadcam/thumbs`, `.dadcam/sprites`
- Build a set of referenced relative paths from `assets.path`
- Delete anything on disk not referenced (orphans)
- If `keep_latest_per_role`:
  - query all assets per clip/role
  - keep the most recent, delete the rest (and remove DB rows + clip_assets links)
- If `max_derived_gb`:
  - compute total derived size; if over, delete oldest derived assets first (never touch originals)

12.6 Concurrency Guardrails

FFmpeg jobs are expensive. Don’t run unlimited parallel transcodes.

Minimum shippable approach:
- one preview worker thread/process at a time (serial jobs)
- later: `--workers N` for parallelism, but default `N=1`

Also:
- treat proxy jobs as higher cost; keep thumb jobs higher priority so UI becomes usable quickly (already reflected in queue priorities).

---

Part 12: Module Registration


12.1 Update main.rs

Add the preview module to your main.rs:

```rust
mod cli;
mod constants;
mod db;
mod error;
mod hash;
mod ingest;
mod jobs;
mod metadata;
mod camera;
mod preview;  // Add this line

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
```

12.2 Update lib.rs (for Tauri)

If you have a lib.rs for Tauri commands, add:

```rust
pub mod preview;
```

---

Resources

- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)
- [FFmpeg Filters](https://ffmpeg.org/ffmpeg-filters.html)
- [H.264 Encoding Guide](https://trac.ffmpeg.org/wiki/Encode/H.264)
- [FFmpeg Scaling](https://trac.ffmpeg.org/wiki/Scaling)
- [Sprite Sheets with FFmpeg](https://superuser.com/questions/1486726/ffmpeg-create-a-sprite-sheet)

---

Next Steps

After Phase 2 is complete:
- Phase 3: Desktop App Shell (Tauri + React UI)
- The UI will display thumbnails in a grid
- Clicking a clip plays the proxy
- Hovering shows sprite-based scrubbing

See development-plan.md for the full roadmap.

---

End of Phase 2 Implementation Guide
