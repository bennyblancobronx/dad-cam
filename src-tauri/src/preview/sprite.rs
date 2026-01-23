// Sprite sheet generation - Phase 2
//
// Creates tiled JPG strips for hover scrubbing.
// Each frame is 160px wide, one frame per second.
// Long videos generate multiple pages (max 60 frames per page).

use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};

use crate::constants::{SPRITE_FPS, SPRITE_TILE_WIDTH, SPRITE_MAX_FRAMES, SPRITE_PAGE_COLS, THUMB_QUALITY};

/// Options for sprite generation.
#[derive(Debug, Clone)]
pub struct SpriteOptions {
    pub tile_width: u32,
    pub fps: u32,
    pub max_frames: u32,
    pub frames_per_page: u32,
}

impl Default for SpriteOptions {
    fn default() -> Self {
        Self {
            tile_width: SPRITE_TILE_WIDTH,
            fps: SPRITE_FPS,
            max_frames: SPRITE_MAX_FRAMES,
            frames_per_page: SPRITE_PAGE_COLS,
        }
    }
}

/// Sprite sheet metadata (for VTT generation).
#[derive(Debug, Clone)]
pub struct SpriteInfo {
    pub frame_count: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    pub interval_ms: u64,
}

/// Multi-page sprite metadata.
#[derive(Debug, Clone)]
pub struct SpritePageInfo {
    pub page_index: u32,
    pub page_count: u32,
    pub frame_count_total: u32,
    pub frames_this_page: u32,
    pub start_frame: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    pub rows: u32,
    pub interval_ms: u64,
}

/// Calculate sprite sheet layout for a single page (legacy compatibility).
pub fn calculate_sprite_layout(
    duration_ms: i64,
    options: &SpriteOptions,
) -> SpriteInfo {
    let duration_secs = (duration_ms as f64 / 1000.0).ceil() as u32;

    // Calculate frame count (1 per second, capped at max)
    let frame_count = duration_secs.min(options.max_frames).max(1);

    // Calculate columns (aim for roughly 10 columns)
    let columns = 10u32.min(frame_count);

    // Estimate tile height based on 16:9 aspect ratio
    let tile_height = (options.tile_width as f64 * 9.0 / 16.0) as u32;

    // Interval between frames in ms
    let interval_ms = if frame_count > 1 {
        (duration_ms as u64) / (frame_count as u64 - 1)
    } else {
        duration_ms as u64
    };

    SpriteInfo {
        frame_count,
        tile_width: options.tile_width,
        tile_height,
        columns,
        interval_ms,
    }
}

/// Calculate multi-page sprite layout.
pub fn calculate_paged_sprite_layout(
    duration_ms: i64,
    options: &SpriteOptions,
) -> Vec<SpritePageInfo> {
    let duration_secs = (duration_ms as f64 / 1000.0).ceil() as u32;
    let frame_count_total = duration_secs.min(options.max_frames).max(1);

    // Calculate page count
    let page_count = (frame_count_total + options.frames_per_page - 1) / options.frames_per_page;

    // Tile height based on 16:9 aspect ratio
    let tile_height = (options.tile_width as f64 * 9.0 / 16.0) as u32;

    // Interval between frames in ms
    let interval_ms = if frame_count_total > 1 {
        (duration_ms as u64) / (frame_count_total as u64)
    } else {
        duration_ms as u64
    };

    let mut pages = Vec::with_capacity(page_count as usize);

    for page_index in 0..page_count {
        let start_frame = page_index * options.frames_per_page;
        let frames_this_page = (frame_count_total - start_frame).min(options.frames_per_page);

        // Columns: use 10 columns or less if fewer frames
        let columns = 10u32.min(frames_this_page);
        let rows = (frames_this_page + columns - 1) / columns;

        pages.push(SpritePageInfo {
            page_index,
            page_count,
            frame_count_total,
            frames_this_page,
            start_frame,
            tile_width: options.tile_width,
            tile_height,
            columns,
            rows,
            interval_ms,
        });
    }

    pages
}

/// Generate a sprite sheet from a video file (single page for short videos).
pub fn generate_sprite_sheet(
    source_path: &Path,
    output_path: &Path,
    duration_ms: i64,
    options: &SpriteOptions,
) -> Result<SpriteInfo> {
    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let info = calculate_sprite_layout(duration_ms, options);

    // Use temp file for atomic write
    let tmp_path = output_path.with_extension("tmp.jpg");

    // Build complex filter:
    // 1. Extract 1 frame per second
    // 2. Scale to tile width
    // 3. Tile into a single image
    let scale_height = (options.tile_width as f64 * 9.0 / 16.0) as u32;
    let filter = format!(
        "fps=1,scale={}:{},tile={}x{}",
        options.tile_width,
        scale_height,
        info.columns,
        (info.frame_count + info.columns - 1) / info.columns // rows
    );

    // FFmpeg quality for JPEG
    let q_value = ((100 - THUMB_QUALITY) as f32 / 100.0 * 30.0 + 1.0) as u32;

    // Limit to max frames by specifying -vframes
    let vframes = info.frame_count.to_string();

    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-i", source_path.to_str().unwrap(),
        "-vf", &filter,
        "-vframes", &vframes,
        "-q:v", &q_value.to_string(),
        tmp_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg sprite generation failed: {}", stderr));
    }

    // Atomic rename
    std::fs::rename(&tmp_path, output_path)?;

    // Verify file
    if !output_path.exists() {
        return Err(anyhow!("Sprite file was not created"));
    }

    let size = std::fs::metadata(output_path)?.len();
    if size == 0 {
        let _ = std::fs::remove_file(output_path);
        return Err(anyhow!("Sprite file is empty"));
    }

    Ok(info)
}

/// Generate multi-page sprite sheets for long videos.
/// Returns a vector of (output_path, page_info) tuples.
pub fn generate_paged_sprite_sheets(
    source_path: &Path,
    output_base_path: &Path,
    duration_ms: i64,
    options: &SpriteOptions,
) -> Result<Vec<(std::path::PathBuf, SpritePageInfo)>> {
    let pages = calculate_paged_sprite_layout(duration_ms, options);

    // If only one page, use the simple path
    if pages.len() == 1 {
        let info = generate_sprite_sheet(source_path, output_base_path, duration_ms, options)?;
        let page_info = SpritePageInfo {
            page_index: 0,
            page_count: 1,
            frame_count_total: info.frame_count,
            frames_this_page: info.frame_count,
            start_frame: 0,
            tile_width: info.tile_width,
            tile_height: info.tile_height,
            columns: info.columns,
            rows: (info.frame_count + info.columns - 1) / info.columns,
            interval_ms: info.interval_ms,
        };
        return Ok(vec![(output_base_path.to_path_buf(), page_info)]);
    }

    // Ensure output directory exists
    if let Some(parent) = output_base_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut results = Vec::with_capacity(pages.len());
    let scale_height = (options.tile_width as f64 * 9.0 / 16.0) as u32;
    let q_value = ((100 - THUMB_QUALITY) as f32 / 100.0 * 30.0 + 1.0) as u32;

    for page in &pages {
        // Generate output path with page index
        let stem = output_base_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "sprite".to_string());
        let ext = output_base_path
            .extension()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "jpg".to_string());

        let page_filename = format!("{}_p{}.{}", stem, page.page_index, ext);
        let page_path = output_base_path.parent()
            .map(|p| p.join(&page_filename))
            .unwrap_or_else(|| std::path::PathBuf::from(&page_filename));

        let tmp_path = page_path.with_extension("tmp.jpg");

        // Calculate frame range for this page
        let end_frame = page.start_frame + page.frames_this_page;

        // Build filter with frame selection
        let filter = format!(
            "fps=1,select='between(n\\,{}\\,{})',scale={}:{},tile={}x{}",
            page.start_frame,
            end_frame - 1,
            options.tile_width,
            scale_height,
            page.columns,
            page.rows
        );

        let mut cmd = Command::new(crate::tools::ffmpeg_path());

        cmd.args([
            "-y",
            "-i", source_path.to_str().unwrap(),
            "-vf", &filter,
            "-frames:v", "1",
            "-q:v", &q_value.to_string(),
            tmp_path.to_str().unwrap(),
        ]);

        let output = cmd.output()?;

        if !output.status.success() {
            let _ = std::fs::remove_file(&tmp_path);
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("FFmpeg sprite page {} generation failed: {}", page.page_index, stderr));
        }

        // Atomic rename
        std::fs::rename(&tmp_path, &page_path)?;

        // Verify
        if !page_path.exists() || std::fs::metadata(&page_path)?.len() == 0 {
            let _ = std::fs::remove_file(&page_path);
            return Err(anyhow!("Sprite page {} is empty or missing", page.page_index));
        }

        results.push((page_path, page.clone()));
    }

    Ok(results)
}

/// Generate a WebVTT file for sprite sheet.
/// This is used by video players for thumbnail preview on scrub.
pub fn generate_vtt(
    sprite_filename: &str,
    duration_ms: i64,
    info: &SpriteInfo,
) -> String {
    let mut vtt = String::from("WEBVTT\n\n");

    for i in 0..info.frame_count {
        let start_ms = (i as u64) * info.interval_ms;
        let end_ms = ((i + 1) as u64) * info.interval_ms;
        let end_ms = end_ms.min(duration_ms as u64);

        // Calculate position in sprite sheet
        let col = i % info.columns;
        let row = i / info.columns;
        let x = col * info.tile_width;
        let y = row * info.tile_height;

        // Format timestamps as HH:MM:SS.mmm
        let start_time = format_vtt_time(start_ms);
        let end_time = format_vtt_time(end_ms);

        // Write cue
        vtt.push_str(&format!(
            "{} --> {}\n{}#xywh={},{},{},{}\n\n",
            start_time,
            end_time,
            sprite_filename,
            x, y, info.tile_width, info.tile_height
        ));
    }

    vtt
}

/// Generate a WebVTT file for multi-page sprite sheets.
pub fn generate_paged_vtt(
    pages: &[(std::path::PathBuf, SpritePageInfo)],
    duration_ms: i64,
) -> String {
    let mut vtt = String::from("WEBVTT\n\n");

    for (page_path, page) in pages {
        let sprite_filename = page_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "sprite.jpg".to_string());

        for i in 0..page.frames_this_page {
            let global_frame = page.start_frame + i;
            let start_ms = (global_frame as u64) * page.interval_ms;
            let end_ms = ((global_frame + 1) as u64) * page.interval_ms;
            let end_ms = end_ms.min(duration_ms as u64);

            // Calculate position within this page's sprite sheet
            let col = i % page.columns;
            let row = i / page.columns;
            let x = col * page.tile_width;
            let y = row * page.tile_height;

            let start_time = format_vtt_time(start_ms);
            let end_time = format_vtt_time(end_ms);

            vtt.push_str(&format!(
                "{} --> {}\n{}#xywh={},{},{},{}\n\n",
                start_time,
                end_time,
                sprite_filename,
                x, y, page.tile_width, page.tile_height
            ));
        }
    }

    vtt
}

/// Format milliseconds as VTT timestamp (HH:MM:SS.mmm).
fn format_vtt_time(ms: u64) -> String {
    let hours = ms / (60 * 60 * 1000);
    let minutes = (ms % (60 * 60 * 1000)) / (60 * 1000);
    let seconds = (ms % (60 * 1000)) / 1000;
    let millis = ms % 1000;
    format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
}

/// Sprite metadata for JSON persistence (per phase-2.md spec).
/// Stored alongside sprite image for UI consumption.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpriteMetadata {
    pub fps: u32,
    pub tile_width: u32,
    pub tile_height: u32,
    pub frame_count: u32,
    pub columns: u32,
    pub rows: u32,
    pub interval_ms: u64,
    /// For paged sprites: which page this is (0-indexed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_index: Option<u32>,
    /// For paged sprites: total page count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_count: Option<u32>,
}

impl From<&SpriteInfo> for SpriteMetadata {
    fn from(info: &SpriteInfo) -> Self {
        Self {
            fps: 1, // Default 1 fps
            tile_width: info.tile_width,
            tile_height: info.tile_height,
            frame_count: info.frame_count,
            columns: info.columns,
            rows: (info.frame_count + info.columns - 1) / info.columns,
            interval_ms: info.interval_ms,
            page_index: None,
            page_count: None,
        }
    }
}

impl From<&SpritePageInfo> for SpriteMetadata {
    fn from(page: &SpritePageInfo) -> Self {
        Self {
            fps: 1,
            tile_width: page.tile_width,
            tile_height: page.tile_height,
            frame_count: page.frames_this_page,
            columns: page.columns,
            rows: page.rows,
            interval_ms: page.interval_ms,
            page_index: Some(page.page_index),
            page_count: Some(page.page_count),
        }
    }
}

/// Save sprite metadata to JSON file alongside the sprite image.
pub fn save_sprite_metadata(
    sprite_path: &Path,
    metadata: &SpriteMetadata,
) -> anyhow::Result<()> {
    let meta_path = sprite_path.with_extension("json");
    let json = serde_json::to_string_pretty(metadata)?;
    std::fs::write(meta_path, json)?;
    Ok(())
}

/// Load sprite metadata from JSON file.
pub fn load_sprite_metadata(sprite_path: &Path) -> anyhow::Result<SpriteMetadata> {
    let meta_path = sprite_path.with_extension("json");
    let json = std::fs::read_to_string(meta_path)?;
    let metadata: SpriteMetadata = serde_json::from_str(&json)?;
    Ok(metadata)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_sprite_layout() {
        // 30 second video
        let info = calculate_sprite_layout(30_000, &SpriteOptions::default());
        assert_eq!(info.frame_count, 30);
        assert_eq!(info.columns, 10);
        assert_eq!(info.tile_width, SPRITE_TILE_WIDTH);
    }

    #[test]
    fn test_calculate_sprite_layout_long_video() {
        // 10 minute video (600 seconds)
        let info = calculate_sprite_layout(600_000, &SpriteOptions::default());
        // Capped at max frames
        assert_eq!(info.frame_count, SPRITE_MAX_FRAMES);
    }

    #[test]
    fn test_calculate_sprite_layout_short_video() {
        // 3 second video
        let info = calculate_sprite_layout(3_000, &SpriteOptions::default());
        assert_eq!(info.frame_count, 3);
        assert_eq!(info.columns, 3); // columns <= frame_count
    }

    #[test]
    fn test_calculate_paged_sprite_layout() {
        // 2 minute video (120 seconds) - should need 2 pages
        let pages = calculate_paged_sprite_layout(120_000, &SpriteOptions::default());
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].frames_this_page, 60);
        assert_eq!(pages[1].frames_this_page, 60);
        assert_eq!(pages[0].page_count, 2);
    }

    #[test]
    fn test_calculate_paged_sprite_layout_long_video() {
        // 5 minute video (300 seconds) - should need 5 pages
        let pages = calculate_paged_sprite_layout(300_000, &SpriteOptions::default());
        assert_eq!(pages.len(), 5);
        for (i, page) in pages.iter().enumerate() {
            assert_eq!(page.page_index, i as u32);
            assert_eq!(page.page_count, 5);
        }
    }

    #[test]
    fn test_calculate_paged_sprite_layout_short_video() {
        // 30 second video - should be single page
        let pages = calculate_paged_sprite_layout(30_000, &SpriteOptions::default());
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].frames_this_page, 30);
    }

    #[test]
    fn test_format_vtt_time() {
        assert_eq!(format_vtt_time(0), "00:00:00.000");
        assert_eq!(format_vtt_time(1500), "00:00:01.500");
        assert_eq!(format_vtt_time(65_000), "00:01:05.000");
        assert_eq!(format_vtt_time(3_661_500), "01:01:01.500");
    }

    #[test]
    fn test_generate_vtt() {
        let info = SpriteInfo {
            frame_count: 3,
            tile_width: 160,
            tile_height: 90,
            columns: 3,
            interval_ms: 1000,
        };

        let vtt = generate_vtt("sprite_123.jpg", 3000, &info);
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:00:00.000 --> 00:00:01.000"));
        assert!(vtt.contains("#xywh=0,0,160,90"));
        assert!(vtt.contains("#xywh=160,0,160,90"));
    }
}
