// Proxy video generation - Phase 2
//
// Creates H.264 720p videos for smooth playback.
// Handles deinterlacing, scaling, and optional LUT application.

use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};

use crate::constants::{PROXY_RESOLUTION, PROXY_CRF};
use crate::metadata::MediaMetadata;

/// Options for proxy generation.
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

/// Determine if a video needs deinterlacing based on metadata.
pub fn needs_deinterlace(media: &MediaMetadata) -> bool {
    // Common interlaced formats
    if let Some(ref codec) = media.codec {
        let codec_lower = codec.to_lowercase();
        // MPEG-2 and DV are often interlaced
        if codec_lower.contains("mpeg2") || codec_lower.contains("dvvideo") {
            return true;
        }
    }

    // Check for interlaced resolution patterns
    if let Some(height) = media.height {
        // 1080i, 480i, 576i are common interlaced formats
        if height == 1080 || height == 480 || height == 576 {
            // Could be interlaced - safer to deinterlace
            // In production, check field_order from ffprobe
            return true;
        }
    }

    false
}

/// Generate a proxy video from the source file.
pub fn generate_proxy(
    source_path: &Path,
    output_path: &Path,
    options: &ProxyOptions,
) -> Result<()> {
    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Use temp file for atomic write
    let tmp_path = output_path.with_extension("tmp.mp4");

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
        tmp_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        // Clean up temp file on failure
        let _ = std::fs::remove_file(&tmp_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg proxy generation failed: {}", stderr));
    }

    // Atomic rename
    std::fs::rename(&tmp_path, output_path)?;

    // Verify output
    if !output_path.exists() {
        return Err(anyhow!("Proxy file was not created"));
    }

    let size = std::fs::metadata(output_path)?.len();
    if size == 0 {
        let _ = std::fs::remove_file(output_path);
        return Err(anyhow!("Proxy file is empty"));
    }

    Ok(())
}

/// Generate a proxy for audio-only files (just re-encode audio).
pub fn generate_audio_proxy(
    source_path: &Path,
    output_path: &Path,
) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Use temp file for atomic write
    let tmp_path = output_path.with_extension("tmp.m4a");

    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    cmd.args([
        "-y",
        "-i", source_path.to_str().unwrap(),
        "-vn", // No video
        "-c:a", "aac",
        "-b:a", "128k",
        tmp_path.to_str().unwrap(),
    ]);

    let output = cmd.output()?;

    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg audio proxy failed: {}", stderr));
    }

    // Atomic rename
    std::fs::rename(&tmp_path, output_path)?;

    // Verify output
    if !output_path.exists() || std::fs::metadata(output_path)?.len() == 0 {
        let _ = std::fs::remove_file(output_path);
        return Err(anyhow!("Audio proxy file is empty or missing"));
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_deinterlace_mpeg2() {
        let info = MediaMetadata {
            codec: Some("mpeg2video".to_string()),
            height: Some(480),
            ..Default::default()
        };
        assert!(needs_deinterlace(&info));
    }

    #[test]
    fn test_needs_deinterlace_dv() {
        let info = MediaMetadata {
            codec: Some("dvvideo".to_string()),
            height: Some(576),
            ..Default::default()
        };
        assert!(needs_deinterlace(&info));
    }

    #[test]
    fn test_needs_deinterlace_h264_720() {
        let info = MediaMetadata {
            codec: Some("h264".to_string()),
            height: Some(720),
            ..Default::default()
        };
        // 720p is usually progressive
        assert!(!needs_deinterlace(&info));
    }
}
