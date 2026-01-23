// FFprobe wrapper for metadata extraction

use std::path::Path;
use std::process::Command;
use serde::{Deserialize, Serialize};
use crate::error::{DadCamError, Result};
use crate::metadata::MediaMetadata;

#[derive(Debug, Deserialize)]
struct FFprobeOutput {
    streams: Option<Vec<FFprobeStream>>,
    format: Option<FFprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FFprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    r_frame_rate: Option<String>,
    channels: Option<i32>,
    sample_rate: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FFprobeFormat {
    duration: Option<String>,
    bit_rate: Option<String>,
    tags: Option<FFprobeTags>,
}

#[derive(Debug, Deserialize)]
struct FFprobeTags {
    creation_time: Option<String>,
}

/// Run ffprobe on a file and extract metadata
pub fn probe(path: &Path) -> Result<MediaMetadata> {
    let output = Command::new(crate::tools::ffprobe_path())
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .map_err(|e| DadCamError::FFprobe(format!("Failed to run ffprobe: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DadCamError::FFprobe(format!("ffprobe failed: {}", stderr)));
    }

    let probe_output: FFprobeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| DadCamError::FFprobe(format!("Failed to parse ffprobe output: {}", e)))?;

    let mut meta = MediaMetadata::default();

    // Extract video stream info
    if let Some(ref streams) = probe_output.streams {
        for stream in streams {
            match stream.codec_type.as_deref() {
                Some("video") => {
                    meta.codec = stream.codec_name.clone();
                    meta.width = stream.width;
                    meta.height = stream.height;
                    meta.fps = parse_frame_rate(stream.r_frame_rate.as_deref());
                    if meta.duration_ms.is_none() {
                        meta.duration_ms = parse_duration_ms(stream.duration.as_deref());
                    }
                    meta.media_type = "video".to_string();
                }
                Some("audio") => {
                    meta.audio_codec = stream.codec_name.clone();
                    meta.audio_channels = stream.channels;
                    meta.audio_sample_rate = stream.sample_rate.as_ref()
                        .and_then(|s| s.parse().ok());
                    if meta.media_type.is_empty() {
                        meta.media_type = "audio".to_string();
                    }
                }
                _ => {}
            }
        }
    }

    // Extract format info
    if let Some(ref format) = probe_output.format {
        if meta.duration_ms.is_none() {
            meta.duration_ms = parse_duration_ms(format.duration.as_deref());
        }
        meta.bitrate = format.bit_rate.as_ref().and_then(|s| s.parse().ok());

        // Try to get creation time from format tags
        if let Some(ref tags) = format.tags {
            if let Some(ref creation_time) = tags.creation_time {
                meta.recorded_at = Some(creation_time.clone());
                meta.recorded_at_source = Some("ffprobe".to_string());
            }
        }
    }

    // Default media type if not set
    if meta.media_type.is_empty() {
        meta.media_type = super::detect_media_type(path);
    }

    Ok(meta)
}

/// Parse frame rate string like "30000/1001" to f64
fn parse_frame_rate(rate_str: Option<&str>) -> Option<f64> {
    let rate_str = rate_str?;
    if let Some((num, den)) = rate_str.split_once('/') {
        let num: f64 = num.parse().ok()?;
        let den: f64 = den.parse().ok()?;
        if den > 0.0 {
            return Some(num / den);
        }
    }
    rate_str.parse().ok()
}

/// Parse duration string to milliseconds
fn parse_duration_ms(duration_str: Option<&str>) -> Option<i64> {
    let duration_str = duration_str?;
    let seconds: f64 = duration_str.parse().ok()?;
    Some((seconds * 1000.0) as i64)
}

/// Check if ffprobe is available
pub fn is_available() -> bool {
    crate::tools::is_tool_available("ffprobe")
}
