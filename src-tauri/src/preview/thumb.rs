// Thumbnail generation - Phase 2
//
// Creates JPG poster frames for grid display.
// Extracts frame from 10% into the video to avoid black frames.

use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};

use crate::constants::THUMB_QUALITY;

/// Options for thumbnail generation.
#[derive(Debug, Clone)]
pub struct ThumbOptions {
    pub max_width: u32,
    pub seek_percent: f64, // Where to extract frame (0.0 to 1.0)
}

impl Default for ThumbOptions {
    fn default() -> Self {
        Self {
            max_width: 480,
            seek_percent: 0.1, // 10% into the video
        }
    }
}

/// Generate a thumbnail from a video file.
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

    // Use temp file for atomic write
    let tmp_path = output_path.with_extension("tmp.jpg");

    // Calculate seek time
    let seek_seconds = duration_ms
        .map(|d| (d as f64 / 1000.0) * options.seek_percent)
        .unwrap_or(1.0) // Default to 1 second if no duration
        .max(0.1);      // At least 0.1 seconds in

    let seek_time = format_duration(seek_seconds);

    // Build scale filter
    let scale_filter = format!(
        "scale='min({},iw)':-1",
        options.max_width
    );

    // FFmpeg quality scale is 1-31 where 1 is best
    // Convert our 0-100 quality to ffmpeg scale
    let q_value = ((100 - THUMB_QUALITY) as f32 / 100.0 * 30.0 + 1.0) as u32;

    // Build ffmpeg command
    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-ss", &seek_time,           // Seek before input (faster)
        "-i", source_path.to_str().unwrap(),
        "-vframes", "1",             // Single frame
        "-vf", &scale_filter,
        "-q:v", &q_value.to_string(),
        tmp_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg thumbnail generation failed: {}", stderr));
    }

    // Atomic rename
    std::fs::rename(&tmp_path, output_path)?;

    // Verify file was created
    if !output_path.exists() {
        return Err(anyhow!("Thumbnail file was not created"));
    }

    let size = std::fs::metadata(output_path)?.len();
    if size == 0 {
        let _ = std::fs::remove_file(output_path);
        return Err(anyhow!("Thumbnail file is empty"));
    }

    Ok(())
}

/// Generate a thumbnail from an image file (just resize).
pub fn generate_image_thumbnail(
    source_path: &Path,
    output_path: &Path,
    options: &ThumbOptions,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Use temp file for atomic write
    let tmp_path = output_path.with_extension("tmp.jpg");

    let scale_filter = format!(
        "scale='min({},iw)':-1",
        options.max_width
    );

    let q_value = ((100 - THUMB_QUALITY) as f32 / 100.0 * 30.0 + 1.0) as u32;

    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-i", source_path.to_str().unwrap(),
        "-vf", &scale_filter,
        "-q:v", &q_value.to_string(),
        tmp_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg image thumbnail failed: {}", stderr));
    }

    // Atomic rename
    std::fs::rename(&tmp_path, output_path)?;

    if !output_path.exists() || std::fs::metadata(output_path)?.len() == 0 {
        let _ = std::fs::remove_file(output_path);
        return Err(anyhow!("Image thumbnail is empty or missing"));
    }

    Ok(())
}

/// Generate a placeholder thumbnail for audio files.
/// Creates a simple waveform visualization.
pub fn generate_audio_thumbnail(
    source_path: &Path,
    output_path: &Path,
    options: &ThumbOptions,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp_path = output_path.with_extension("tmp.jpg");

    // Generate waveform visualization
    let filter = format!(
        "showwavespic=s={}x{}:colors=0x333333",
        options.max_width,
        options.max_width * 9 / 16 // 16:9 aspect ratio
    );

    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-i", source_path.to_str().unwrap(),
        "-filter_complex", &filter,
        "-frames:v", "1",
        tmp_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg audio thumbnail failed: {}", stderr));
    }

    std::fs::rename(&tmp_path, output_path)?;

    if !output_path.exists() || std::fs::metadata(output_path)?.len() == 0 {
        let _ = std::fs::remove_file(output_path);
        return Err(anyhow!("Audio thumbnail is empty or missing"));
    }

    Ok(())
}

/// Format seconds as HH:MM:SS.mmm for ffmpeg.
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

    #[test]
    fn test_default_options() {
        let opts = ThumbOptions::default();
        assert_eq!(opts.max_width, 480);
        assert!((opts.seek_percent - 0.1).abs() < 0.001);
    }
}
