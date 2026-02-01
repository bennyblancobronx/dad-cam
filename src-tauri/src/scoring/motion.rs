// Phase 4: Motion detection
// Uses frame differencing with tblend and blackframe filters (per Phase 4 spec 1.4 and 4.6)

use std::path::Path;
use std::process::Command;
use anyhow::Result;
use regex::Regex;

use crate::constants::{
    MOTION_LOW_THRESHOLD, MOTION_HIGH_THRESHOLD,
    R_MOTION_STATIC, R_MOTION_CALM, R_MOTION_GOOD, R_MOTION_HIGH,
    R_MOTION_CHAOTIC, R_MOTION_SHORT, R_MOTION_UNAVAIL
};
use crate::tools::ffmpeg_path;

// Motion sampling constants (per spec 3.3)
const MOTION_SAMPLE_INTERVAL_SECS: f64 = 1.0;

/// Analyze motion using frame differencing (per spec 1.4)
/// Returns (score 0-1, optional reason) where higher = more motion
pub fn analyze_motion(video_path: &Path, duration_ms: i64, verbose: bool) -> Result<(f64, Option<String>)> {
    // Very short clips - limited analysis
    if duration_ms < 2000 {
        return Ok((0.5, Some(R_MOTION_SHORT.to_string())));
    }

    let duration_secs = duration_ms as f64 / 1000.0;

    // Calculate sample rate (per spec)
    let sample_count = (duration_secs / MOTION_SAMPLE_INTERVAL_SECS).min(60.0) as u32;
    let fps = if sample_count > 1 {
        sample_count as f64 / duration_secs
    } else {
        1.0
    };

    // Use tblend to compute frame differences, then measure blackframe percentage (per spec 1.4)
    // High blackframe % after difference = low motion (frames are similar)
    let filter = format!(
        "fps={},tblend=all_mode=difference,blackframe=amount=95:threshold=24",
        fps
    );

    let output = Command::new(ffmpeg_path())
        .args([
            "-i", &video_path.to_string_lossy(),
            "-vf", &filter,
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse blackframe detection results
    let (static_frames, total_frames) = parse_blackframe_output(&stderr);

    if verbose {
        log::debug!("  Motion detection: {} static frames of {} total", static_frames, total_frames);
    }

    if total_frames == 0 {
        // Fallback: estimate from scene changes
        return analyze_motion_fallback(video_path, verbose);
    }

    // Calculate motion ratio (inverse of static ratio)
    let static_ratio = static_frames as f64 / total_frames as f64;
    let motion_ratio = 1.0 - static_ratio;

    // Compute score based on motion level (per spec scoring logic)
    let (score, reason) = compute_motion_score(motion_ratio);

    if verbose {
        log::debug!("  Motion ratio: {:.3} -> score: {:.2}", motion_ratio, score);
    }

    Ok((score, Some(reason)))
}

/// Parse FFmpeg blackframe output (per spec 4.6)
fn parse_blackframe_output(text: &str) -> (u32, u32) {
    // Count blackframe detections (static frames after differencing)
    let static_frames = text.matches("blackframe:").count() as u32;

    // Estimate total frames from frame= count in progress output
    let total_frames = Regex::new(r"frame=\s*(\d+)")
        .ok()
        .and_then(|re| {
            re.captures_iter(text)
                .last()
                .and_then(|cap| cap.get(1)?.as_str().parse::<u32>().ok())
        })
        .unwrap_or(0);

    (static_frames, total_frames.max(static_frames))
}

/// Fallback motion analysis using mestimate/scene detection (per spec)
fn analyze_motion_fallback(video_path: &Path, verbose: bool) -> Result<(f64, Option<String>)> {
    // Use select filter with scene change detection as motion proxy
    let output = Command::new(ffmpeg_path())
        .args([
            "-i", &video_path.to_string_lossy(),
            "-vf", "select='gt(scene,0.1)',metadata=print:file=-",
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Count scene changes as proxy for motion
    let changes = stderr.matches("pts_time:").count();

    if verbose {
        log::debug!("  Motion fallback: {} movement events detected", changes);
    }

    // Normalize: assume 30 changes per minute is high motion
    let score = (changes as f64 / 30.0).min(1.0);

    let reason = format!("{} (fallback)", R_MOTION_UNAVAIL);

    Ok((score, Some(reason)))
}

/// Compute motion score based on motion ratio (per spec scoring logic 1.4)
fn compute_motion_score(motion: f64) -> (f64, String) {
    // Motion values (ratio 0-1):
    // Very low (< 0.01) = static shot, boring
    // Low (0.01-0.05) = minimal motion, calm
    // Medium (0.05-0.15) = typical activity, good
    // High (0.15-0.30) = significant action, exciting
    // Very high (> 0.30) = chaotic, potentially unwatchable

    if motion < MOTION_LOW_THRESHOLD {
        // Static shot - low score but not terrible
        let score = 0.3 + (motion / MOTION_LOW_THRESHOLD) * 0.2;
        (score, R_MOTION_STATIC.to_string())
    } else if motion < 0.05 {
        // Minimal motion - calm footage
        let score = 0.5 + ((motion - MOTION_LOW_THRESHOLD) / 0.04) * 0.2;
        (score, R_MOTION_CALM.to_string())
    } else if motion < 0.15 {
        // Good motion - typical activity
        let score = 0.7 + ((motion - 0.05) / 0.10) * 0.2;
        (score.min(0.95), R_MOTION_GOOD.to_string())
    } else if motion < MOTION_HIGH_THRESHOLD {
        // High motion - exciting action
        let score = 0.9 - ((motion - 0.15) / 0.15) * 0.1;
        (score, R_MOTION_HIGH.to_string())
    } else {
        // Very high motion - potentially too chaotic
        let excess = motion - MOTION_HIGH_THRESHOLD;
        let penalty = (excess * 2.0).min(0.3);
        let score = 0.7 - penalty;
        (score.max(0.4), R_MOTION_CHAOTIC.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_blackframe_output() {
        let output = r#"
            frame=   50 fps=0.0 q=-0.0 size=N/A time=00:00:10.00
            [Parsed_blackframe_1 @ 0x7f9] blackframe:1 pblack:98 pts:1000
            [Parsed_blackframe_1 @ 0x7f9] blackframe:1 pblack:97 pts:2000
            frame=  100 fps=25.0 q=-0.0 size=N/A time=00:00:20.00
        "#;

        let (static_frames, total_frames) = parse_blackframe_output(output);
        assert_eq!(static_frames, 2);
        assert_eq!(total_frames, 100);
    }

    #[test]
    fn test_motion_score_static() {
        // 2% motion (98% static) should give low score
        let (score, reason) = compute_motion_score(0.02);
        assert!(score < 0.6, "Low motion should score lower");
        assert_eq!(reason, R_MOTION_CALM);
    }

    #[test]
    fn test_motion_score_good() {
        // 10% motion should give good score
        let (score, reason) = compute_motion_score(0.10);
        assert!(score > 0.7, "Good motion should score well");
        assert_eq!(reason, R_MOTION_GOOD);
    }

    #[test]
    fn test_motion_score_high() {
        // 20% motion should give high score
        let (score, reason) = compute_motion_score(0.20);
        assert!(score > 0.7 && score < 0.95, "High motion should score well");
        assert_eq!(reason, R_MOTION_HIGH);
    }

    #[test]
    fn test_motion_score_chaotic() {
        // 50% motion (very chaotic) should be penalized
        let (score, reason) = compute_motion_score(0.50);
        assert!(score < 0.7, "Chaotic motion should be penalized");
        assert_eq!(reason, R_MOTION_CHAOTIC);
    }

    #[test]
    fn test_motion_score_very_static() {
        // 0.5% motion (very static) should give low score
        let (score, reason) = compute_motion_score(0.005);
        assert!(score < 0.5, "Very static shot should score low");
        assert_eq!(reason, R_MOTION_STATIC);
    }
}
