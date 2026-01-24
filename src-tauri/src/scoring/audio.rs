// Phase 4: Audio analysis
// Uses EBU R128 loudness analysis (per Phase 4 spec 1.2 and 4.4)

use std::path::Path;
use std::process::Command;
use anyhow::Result;
use regex::Regex;

use crate::constants::{
    R_AUDIO_NONE, R_AUDIO_SILENT, R_AUDIO_LOUD, R_AUDIO_GOOD,
    R_AUDIO_MODERATE, R_AUDIO_QUIET, R_AUDIO_SHORT
};
use crate::tools::ffmpeg_path;

// EBU R128 constants (per spec 3.3)
const AUDIO_TARGET_LUFS: f64 = -23.0;       // EBU R128 broadcast standard
const AUDIO_ACCEPTABLE_RANGE: f64 = 10.0;   // +/- 10 LUFS from target
const AUDIO_LRA_MIN: f64 = 4.0;             // Minimum acceptable LRA
const AUDIO_LRA_MAX: f64 = 15.0;            // Maximum acceptable LRA
const AUDIO_TRUE_PEAK_MAX: f64 = -1.0;      // Maximum true peak before clipping risk

/// Audio statistics from EBU R128 analysis
#[derive(Debug, Default)]
struct AudioStats {
    integrated_lufs: Option<f64>,  // Overall loudness (I:)
    lra: Option<f64>,              // Loudness Range (LRA:)
    true_peak: Option<f64>,        // True peak (dBTP)
    has_audio: bool,
}

/// Analyze audio loudness using EBU R128 (per spec 1.2)
/// Returns (score 0-1, optional reason)
pub fn analyze_audio(video_path: &Path, duration_ms: i64, verbose: bool) -> Result<(f64, Option<String>)> {
    // Very short clips - skip detailed analysis
    if duration_ms < 3000 {
        return Ok((0.5, Some(R_AUDIO_SHORT.to_string())));
    }

    // Run ebur128 filter for EBU R128 loudness analysis (per spec 1.2)
    let output = Command::new(ffmpeg_path())
        .args([
            "-i", &video_path.to_string_lossy(),
            "-af", "ebur128=peak=true:framelog=verbose",
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse EBU R128 statistics
    let stats = parse_ebur128_stats(&stderr);

    if verbose {
        if stats.has_audio {
            eprintln!("  Audio (EBU R128): I={:.1} LUFS, LRA={:.1} LU, Peak={:.1} dBTP",
                stats.integrated_lufs.unwrap_or(0.0),
                stats.lra.unwrap_or(0.0),
                stats.true_peak.unwrap_or(0.0));
        } else {
            eprintln!("  No audio stream detected");
        }
    }

    // Compute score based on EBU R128 metrics (per spec scoring logic)
    let (score, reason) = compute_audio_score(&stats);

    Ok((score, Some(reason)))
}

/// Parse FFmpeg ebur128 filter output for I:, LRA:, and True peak values
fn parse_ebur128_stats(output: &str) -> AudioStats {
    let mut stats = AudioStats::default();

    // Pattern: I: -23.0 LUFS (integrated loudness)
    if let Some(cap) = Regex::new(r"I:\s*(-?\d+\.?\d*)\s*LUFS")
        .ok()
        .and_then(|re| re.captures(output))
    {
        if let Ok(val) = cap[1].parse::<f64>() {
            stats.integrated_lufs = Some(val);
            stats.has_audio = true;
        }
    }

    // Pattern: LRA: 8.0 LU (loudness range)
    if let Some(cap) = Regex::new(r"LRA:\s*(\d+\.?\d*)\s*LU")
        .ok()
        .and_then(|re| re.captures(output))
    {
        if let Ok(val) = cap[1].parse::<f64>() {
            stats.lra = Some(val);
            stats.has_audio = true;
        }
    }

    // Pattern: True peak: -3.0 dBTP
    if let Some(cap) = Regex::new(r"True peak:\s*(-?\d+\.?\d*)\s*dBTP")
        .ok()
        .and_then(|re| re.captures(output))
    {
        if let Ok(val) = cap[1].parse::<f64>() {
            stats.true_peak = Some(val);
            stats.has_audio = true;
        }
    }

    stats
}

/// Compute audio score based on EBU R128 metrics (per spec scoring logic 1.2)
fn compute_audio_score(stats: &AudioStats) -> (f64, String) {
    if !stats.has_audio {
        // No audio track - low score (per spec: penalize clips with no audio)
        return (0.3, R_AUDIO_NONE.to_string());
    }

    let mut score = 1.0;
    let mut reasons = Vec::new();

    // Score integrated loudness (per spec: penalize extreme loudness)
    if let Some(i) = stats.integrated_lufs {
        let distance_from_target = (i - AUDIO_TARGET_LUFS).abs();

        if distance_from_target <= AUDIO_ACCEPTABLE_RANGE {
            // Good loudness range
            reasons.push(format!("Good loudness: {:.1} LUFS", i));
        } else {
            // Penalize extreme loudness (per spec: < -30 LUFS or > -10 LUFS)
            let penalty = (distance_from_target - AUDIO_ACCEPTABLE_RANGE) / 20.0;
            score -= penalty.min(0.3);

            if i < -35.0 {
                reasons.push(format!("Very quiet audio: {:.1} LUFS", i));
                return (score.max(0.2), R_AUDIO_SILENT.to_string());
            } else if i > -10.0 {
                reasons.push(format!("Very loud audio: {:.1} LUFS", i));
                return (score.max(0.5), R_AUDIO_LOUD.to_string());
            }
        }
    } else {
        score -= 0.2;
        reasons.push("Could not measure loudness".to_string());
    }

    // Score LRA (per spec: reward moderate LRA 4-12 LU)
    if let Some(range) = stats.lra {
        if range >= AUDIO_LRA_MIN && range <= AUDIO_LRA_MAX {
            reasons.push(format!("Good dynamic range: {:.1} LU", range));
        } else if range < AUDIO_LRA_MIN {
            score -= 0.1;
            reasons.push(format!("Compressed audio: {:.1} LU", range));
        } else {
            score -= 0.1;
            reasons.push(format!("Wide dynamic range: {:.1} LU", range));
        }
    }

    // Score true peak (per spec: penalize true peaks above -1 dBTP)
    if let Some(peak) = stats.true_peak {
        if peak > AUDIO_TRUE_PEAK_MAX {
            score -= 0.15;
            reasons.push(format!("Audio clipping risk: {:.1} dBTP", peak));
        }
    }

    let final_score = score.max(0.0).min(1.0);

    // Determine reason token based on score
    let reason = if final_score >= 0.8 {
        R_AUDIO_GOOD
    } else if final_score >= 0.6 {
        R_AUDIO_MODERATE
    } else {
        R_AUDIO_QUIET
    };

    (final_score, reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ebur128_stats() {
        let output = r#"
            [Parsed_ebur128_0 @ 0x7f9] Summary:
              Integrated loudness:
                I:         -23.0 LUFS
                Threshold: -33.0 LUFS
              Loudness range:
                LRA:         8.0 LU
                Threshold:  -43.0 LUFS
                LRA low:    -28.0 LUFS
                LRA high:   -20.0 LUFS
              True peak:
                Peak:        -3.0 dBTP
        "#;

        let stats = parse_ebur128_stats(output);
        assert!(stats.has_audio);
        assert!((stats.integrated_lufs.unwrap() - (-23.0)).abs() < 0.1);
        assert!((stats.lra.unwrap() - 8.0).abs() < 0.1);
        assert!((stats.true_peak.unwrap() - (-3.0)).abs() < 0.1);
    }

    #[test]
    fn test_audio_score_good_loudness() {
        let stats = AudioStats {
            integrated_lufs: Some(-23.0),
            lra: Some(8.0),
            true_peak: Some(-3.0),
            has_audio: true,
        };
        let (score, _) = compute_audio_score(&stats);
        assert!(score > 0.8, "Good EBU R128 metrics should score high");
    }

    #[test]
    fn test_audio_score_too_quiet() {
        let stats = AudioStats {
            integrated_lufs: Some(-40.0),
            lra: Some(8.0),
            true_peak: Some(-10.0),
            has_audio: true,
        };
        let (score, reason) = compute_audio_score(&stats);
        assert!(score < 0.5, "Very quiet audio should score low");
        assert_eq!(reason, R_AUDIO_SILENT);
    }

    #[test]
    fn test_audio_score_clipping() {
        let stats = AudioStats {
            integrated_lufs: Some(-20.0),
            lra: Some(6.0),
            true_peak: Some(0.5),  // Above -1 dBTP
            has_audio: true,
        };
        let (score, _) = compute_audio_score(&stats);
        assert!(score < 0.9, "Clipping risk should penalize score");
    }

    #[test]
    fn test_audio_score_no_audio() {
        let stats = AudioStats::default();
        let (score, reason) = compute_audio_score(&stats);
        assert!((score - 0.3).abs() < 0.01, "No audio should score 0.3");
        assert_eq!(reason, R_AUDIO_NONE);
    }
}
