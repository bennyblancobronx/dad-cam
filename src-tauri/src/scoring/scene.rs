// Phase 4: Scene change detection
// Uses FFmpeg's scdet filter to count visual transitions (per Phase 4 spec 4.3)

use std::path::Path;
use std::process::Command;
use anyhow::Result;
use serde::Deserialize;

use crate::constants::{SCENE_THRESHOLD, SCENE_MIN_CHANGES, SCENE_MAX_CHANGES,
    R_SCENE_STATIC, R_SCENE_GOOD, R_SCENE_CHAOTIC, R_SCENE_SHORT};
use crate::tools::ffprobe_path;

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
    #[serde(rename = "lavfi.scd.score")]
    scd_score: Option<String>,
}

/// Analyze scene changes in a video using scdet filter (per spec 1.1)
/// Returns (score 0-1, optional reason)
pub fn analyze_scenes(video_path: &Path, duration_ms: i64, verbose: bool) -> Result<(f64, Option<String>)> {
    // Very short clips get neutral score
    if duration_ms < 1000 {
        return Ok((0.5, Some(R_SCENE_SHORT.to_string())));
    }

    let duration_secs = duration_ms as f64 / 1000.0;

    // Run ffprobe with scdet filter (per spec 1.1)
    // scdet outputs lavfi.scd.score for each frame where scene change is detected
    let filter = format!("movie={},scdet=t={}",
        video_path.to_string_lossy().replace('\'', "'\\''"),
        (SCENE_THRESHOLD * 100.0) as i32  // scdet threshold is 0-100
    );

    let output = Command::new(ffprobe_path())
        .args([
            "-f", "lavfi",
            "-i", &filter,
            "-show_entries", "frame_tags=lavfi.scd.score",
            "-of", "json",
            "-v", "quiet",
        ])
        .output()?;

    if !output.status.success() {
        // Fallback to alternative method
        return analyze_scenes_fallback(video_path, duration_secs, verbose);
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let parsed: FFprobeOutput = serde_json::from_str(&json_str)
        .unwrap_or(FFprobeOutput { frames: None });

    // Count frames with scene change score above threshold
    let scene_count = parsed.frames
        .unwrap_or_default()
        .iter()
        .filter(|f| {
            f.tags.as_ref()
                .and_then(|t| t.scd_score.as_ref())
                .and_then(|s| s.parse::<f64>().ok())
                .map(|score| score > SCENE_THRESHOLD * 100.0)
                .unwrap_or(false)
        })
        .count() as i32;

    if verbose {
        log::debug!("  Detected {} scene changes via scdet", scene_count);
    }

    // Score based on scene count
    let (score, reason) = compute_scene_score(scene_count, duration_secs);

    Ok((score, Some(reason)))
}

/// Fallback method using select filter for scene detection
fn analyze_scenes_fallback(video_path: &Path, duration_secs: f64, verbose: bool) -> Result<(f64, Option<String>)> {
    use crate::tools::ffmpeg_path;

    // Use select filter with scene detection expression
    let output = Command::new(ffmpeg_path())
        .args([
            "-i", &video_path.to_string_lossy(),
            "-vf", &format!("select='gt(scene,{})',showinfo", SCENE_THRESHOLD),
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Count "pts_time" occurrences in showinfo output (indicates selected frames)
    let scene_count = stderr.matches("pts_time:").count() as i32;

    if verbose {
        log::debug!("  Detected {} scene changes via select filter (fallback)", scene_count);
    }

    let (score, reason) = compute_scene_score(scene_count, duration_secs);

    Ok((score, Some(format!("{} (fallback)", reason))))
}

/// Compute score based on scene count (per spec scoring logic)
fn compute_scene_score(scene_count: i32, duration_secs: f64) -> (f64, String) {
    let duration_mins = duration_secs / 60.0;
    let scenes_per_min = if duration_mins > 0.0 {
        scene_count as f64 / duration_mins
    } else {
        0.0
    };

    // Score based on scene count range
    if scene_count < SCENE_MIN_CHANGES {
        // Very few scene changes - static shot
        let score = 0.3 + (scene_count as f64 / SCENE_MIN_CHANGES as f64) * 0.2;
        (score, R_SCENE_STATIC.to_string())
    } else if scene_count > SCENE_MAX_CHANGES {
        // Too many scene changes - chaotic
        let excess = scene_count - SCENE_MAX_CHANGES;
        let penalty = (excess as f64 / 10.0).min(0.3);
        let score = 0.7 - penalty;
        (score.max(0.4), R_SCENE_CHAOTIC.to_string())
    } else {
        // Good range - variety without chaos
        // Peak score at moderate scene changes (per spec: 10 scenes/min = 1.0)
        let normalized = (scenes_per_min / 10.0).min(1.0);
        let score = 0.6 + normalized * 0.4;
        (score.min(1.0), R_SCENE_GOOD.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scene_score_static() {
        let (score, reason) = compute_scene_score(0, 60.0);
        assert!(score < 0.5, "Static shot should score low");
        assert_eq!(reason, R_SCENE_STATIC);
    }

    #[test]
    fn test_scene_score_moderate() {
        let (score, reason) = compute_scene_score(10, 60.0);
        assert!(score > 0.7, "Moderate scene changes should score well");
        assert_eq!(reason, R_SCENE_GOOD);
    }

    #[test]
    fn test_scene_score_chaotic() {
        let (score, reason) = compute_scene_score(50, 60.0);
        assert!(score < 0.8, "Chaotic scene changes should be penalized");
        assert_eq!(reason, R_SCENE_CHAOTIC);
    }

    #[test]
    fn test_scenes_per_minute_normalization() {
        // 5 scenes per minute (0.5 normalized) should give ~0.8 score
        let (score, _) = compute_scene_score(5, 60.0);
        assert!(score >= 0.7 && score <= 0.9);
    }
}
