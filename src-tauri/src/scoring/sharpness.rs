// Phase 4: Sharpness/blur detection
// Uses FFmpeg's blurdetect filter (per Phase 4 spec 1.3 and 4.5)

use std::path::Path;
use std::process::Command;
use anyhow::Result;
use serde::Deserialize;

use crate::constants::{
    SHARPNESS_BLUR_THRESHOLD, SHARPNESS_SHARP_THRESHOLD,
    R_SHARP_BLURRY, R_SHARP_OK, R_SHARP_GOOD, R_SHARP_SHORT, R_SHARP_UNAVAIL
};
use crate::tools::ffmpeg_path;

// Blur sampling constants (per spec 3.3)
const BLUR_SAMPLE_INTERVAL_SECS: f64 = 2.0;  // Sample every 2 seconds
const BLUR_MAX_VALUE: f64 = 100.0;           // Normalize blur values

#[derive(Debug, Deserialize)]
struct FFprobeOutput {
    frames: Option<Vec<FrameData>>,
}

#[derive(Debug, Deserialize)]
struct FrameData {
    tags: Option<FrameTags>,
}

#[derive(Debug, Deserialize)]
struct FrameTags {
    #[serde(rename = "lavfi.blur")]
    blur: Option<String>,
}

/// Analyze sharpness using blurdetect filter (per spec 1.3)
/// Returns (score 0-1, optional reason) where higher = sharper
pub fn analyze_sharpness(video_path: &Path, duration_ms: i64, verbose: bool) -> Result<(f64, Option<String>)> {
    // Very short clips - limited analysis
    if duration_ms < 1000 {
        return Ok((0.5, Some(R_SHARP_SHORT.to_string())));
    }

    let duration_secs = duration_ms as f64 / 1000.0;

    // Calculate sample rate to get ~30 samples max (per spec)
    let sample_count = (duration_secs / BLUR_SAMPLE_INTERVAL_SECS).min(30.0) as u32;
    let fps = if sample_count > 0 {
        sample_count as f64 / duration_secs
    } else {
        0.5
    };

    // Try primary method: ffprobe with blurdetect filter (per spec 1.3)
    let filter = format!(
        "movie={},fps={},blurdetect",
        video_path.to_string_lossy().replace('\'', "'\\''"),
        fps
    );

    let output = Command::new(crate::tools::ffprobe_path())
        .args([
            "-f", "lavfi",
            "-i", &filter,
            "-show_entries", "frame_tags=lavfi.blur",
            "-of", "json",
            "-v", "quiet",
        ])
        .output()?;

    if output.status.success() {
        let json_str = String::from_utf8_lossy(&output.stdout);
        let parsed: FFprobeOutput = serde_json::from_str(&json_str)
            .unwrap_or(FFprobeOutput { frames: None });

        // Extract blur values from frames
        let blur_values: Vec<f64> = parsed.frames
            .unwrap_or_default()
            .iter()
            .filter_map(|f| {
                f.tags.as_ref()
                    .and_then(|t| t.blur.as_ref())
                    .and_then(|s| s.parse::<f64>().ok())
            })
            .collect();

        if !blur_values.is_empty() {
            return compute_sharpness_result(&blur_values, verbose);
        }
    }

    // Fallback: use ffmpeg with blurdetect filter
    analyze_sharpness_fallback(video_path, fps, verbose)
}

/// Fallback method using ffmpeg with blurdetect
fn analyze_sharpness_fallback(video_path: &Path, fps: f64, verbose: bool) -> Result<(f64, Option<String>)> {
    // Run blurdetect filter via ffmpeg (per spec 4.5)
    let filter = format!("fps={},blurdetect", fps);

    let output = Command::new(ffmpeg_path())
        .args([
            "-i", &video_path.to_string_lossy(),
            "-vf", &filter,
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse blur values from output
    let blur_values = parse_blur_values(&stderr);

    if blur_values.is_empty() {
        return Ok((0.5, Some(R_SHARP_UNAVAIL.to_string())));
    }

    compute_sharpness_result(&blur_values, verbose)
}

/// Parse FFmpeg blurdetect output for blur values
fn parse_blur_values(text: &str) -> Vec<f64> {
    // Pattern: blur:12.34 or [blurdetect @ ...] blur:12.34
    let re = regex::Regex::new(r"blur:\s*(\d+\.?\d*)").unwrap();

    re.captures_iter(text)
        .filter_map(|cap| cap.get(1)?.as_str().parse::<f64>().ok())
        .collect()
}

/// Compute sharpness score from blur values (per spec scoring logic 1.3)
fn compute_sharpness_result(blur_values: &[f64], verbose: bool) -> Result<(f64, Option<String>)> {
    // Calculate average blur
    let avg_blur: f64 = blur_values.iter().sum::<f64>() / blur_values.len() as f64;

    if verbose {
        log::debug!("  Blur detection: avg={:.1} (sampled {} frames)", avg_blur, blur_values.len());
    }

    // Convert blur to sharpness (per spec: sharpness = 1.0 - (avg_blur / max_blur))
    // Lower blur = higher sharpness
    let (score, reason) = compute_sharpness_score(avg_blur);

    Ok((score, Some(reason)))
}

/// Compute sharpness score based on blur value (per spec scoring logic)
fn compute_sharpness_score(avg_blur: f64) -> (f64, String) {
    // Blur values:
    // Low values = sharp (few edges blurred)
    // High values = blurry (many edges blurred)
    //
    // Per spec: normalize and invert

    if avg_blur < SHARPNESS_BLUR_THRESHOLD {
        // Sharp footage (low blur)
        let score = 0.85 + (SHARPNESS_BLUR_THRESHOLD - avg_blur) / SHARPNESS_BLUR_THRESHOLD * 0.15;
        (score.min(1.0), R_SHARP_GOOD.to_string())
    } else if avg_blur < SHARPNESS_SHARP_THRESHOLD {
        // Moderate sharpness
        let range = SHARPNESS_SHARP_THRESHOLD - SHARPNESS_BLUR_THRESHOLD;
        let position = (avg_blur - SHARPNESS_BLUR_THRESHOLD) / range;
        let score = 0.85 - position * 0.35;
        (score.max(0.5), R_SHARP_OK.to_string())
    } else {
        // Blurry footage (high blur)
        let excess = avg_blur - SHARPNESS_SHARP_THRESHOLD;
        let penalty = (excess / BLUR_MAX_VALUE).min(0.3);
        let score = 0.5 - penalty;
        (score.max(0.2), R_SHARP_BLURRY.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_blur_values() {
        let output = r#"
            [Parsed_blurdetect_0 @ 0x7f9] blur:15.23
            [Parsed_blurdetect_0 @ 0x7f9] blur:18.45
            [Parsed_blurdetect_0 @ 0x7f9] blur:12.10
        "#;

        let values = parse_blur_values(output);
        assert_eq!(values.len(), 3);
        assert!((values[0] - 15.23).abs() < 0.01);
    }

    #[test]
    fn test_sharpness_score_sharp() {
        // Low blur (50) should give high sharpness
        let (score, reason) = compute_sharpness_score(50.0);
        assert!(score > 0.8, "Sharp footage should score high, got {}", score);
        assert_eq!(reason, R_SHARP_GOOD);
    }

    #[test]
    fn test_sharpness_score_blurry() {
        // High blur (600) should give low sharpness
        let (score, reason) = compute_sharpness_score(600.0);
        assert!(score < 0.5, "Blurry footage should score low, got {}", score);
        assert_eq!(reason, R_SHARP_BLURRY);
    }

    #[test]
    fn test_sharpness_score_moderate() {
        // Moderate blur (200) should give moderate sharpness
        let (score, reason) = compute_sharpness_score(200.0);
        assert!(score >= 0.5 && score <= 0.85, "Moderate blur should give moderate score, got {}", score);
        assert_eq!(reason, R_SHARP_OK);
    }

    #[test]
    fn test_sharpness_normalization() {
        // Per spec: sharpness = 1.0 - (avg_blur / max_blur)
        // With BLUR_MAX_VALUE = 100, blur of 50 should give ~0.5 raw sharpness
        let avg_blur = 50.0;
        let raw_sharpness = 1.0 - (avg_blur / BLUR_MAX_VALUE).min(1.0);
        assert!((raw_sharpness - 0.5).abs() < 0.01);
    }
}
