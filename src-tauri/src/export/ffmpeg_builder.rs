// Dad Cam - FFmpeg Filtergraph Builder for VHS Export
// Constructs the full FFmpeg command with conform filters, xfade/acrossfade chains,
// title overlay, and optional watermark/720p cap.

use std::path::Path;

use crate::error::{DadCamError, Result};
use super::ExportClip;
use super::watermark;

/// Default crossfade/blend duration in seconds (500ms per guide)
const DEFAULT_BLEND_SEC: f64 = 0.5;

/// Default title start offset in seconds
const DEFAULT_TITLE_START_SEC: f64 = 5.0;

/// Title overlay duration: 0.5s fade in + 2s hold + 0.5s fade out = 3s
const TITLE_DURATION: f64 = 3.0;

/// Build the full FFmpeg args list for the export.
/// Returns a Vec of string args to pass to Command.
///
/// `blend_sec`: crossfade duration (from devMenu.jlBlendMs, converted to seconds).
/// `title_start_sec`: when the title overlay begins (from devMenu.titleStartSeconds).
pub fn build_export_command(
    clips: &[ExportClip],
    library_root: &Path,
    output_path: &Path,
    title_text: Option<&str>,
    apply_watermark: bool,
    blend_sec: f64,
    title_start_sec: f64,
) -> Result<Vec<String>> {
    if clips.is_empty() {
        return Err(DadCamError::Other("No clips to export".to_string()));
    }

    // Single clip: simple transcode, no xfade needed
    if clips.len() == 1 {
        return build_single_clip_command(clips, library_root, output_path, title_text, apply_watermark, title_start_sec);
    }

    build_multi_clip_command(clips, library_root, output_path, title_text, apply_watermark, blend_sec, title_start_sec)
}

/// Build command for a single clip (no crossfades needed)
fn build_single_clip_command(
    clips: &[ExportClip],
    library_root: &Path,
    output_path: &Path,
    title_text: Option<&str>,
    apply_watermark: bool,
    title_start_sec: f64,
) -> Result<Vec<String>> {
    let clip = &clips[0];
    let input_path = library_root.join(&clip.path);
    let mut args: Vec<String> = Vec::new();

    // All inputs first
    args.extend_from_slice(&["-y".into(), "-i".into(), path_str(&input_path)?]);

    if !clip.has_audio {
        // Inject null audio source as second input (index 1)
        args.extend_from_slice(&[
            "-f".into(), "lavfi".into(),
            "-i".into(), "anullsrc=r=48000:cl=stereo".into(),
        ]);
    }

    // Build video filter chain
    let mut vfilters = vec![conform_video_filter()];

    if let Some(text) = title_text {
        vfilters.push(title_overlay_filter(text, title_start_sec, TITLE_DURATION));
    }

    if apply_watermark {
        let (wm_filters, scale_filter) = watermark::watermark_filters();
        vfilters.push(wm_filters);
        vfilters.push(scale_filter);
    }

    let vf = vfilters.join(",");
    args.extend_from_slice(&["-vf".into(), vf]);

    // Audio filter
    if clip.has_audio {
        args.extend_from_slice(&["-af".into(), conform_audio_filter()]);
    }

    // Stream mapping: required when we have a null audio source input
    if !clip.has_audio {
        args.extend_from_slice(&[
            "-map".into(), "0:v".into(),
            "-map".into(), "1:a".into(),
        ]);
    }

    // Output encoding
    args.extend(output_encoding_args());
    args.push(path_str(output_path)?);

    Ok(args)
}

/// Build command for multiple clips with xfade/acrossfade chain
fn build_multi_clip_command(
    clips: &[ExportClip],
    library_root: &Path,
    output_path: &Path,
    title_text: Option<&str>,
    apply_watermark: bool,
    blend_sec: f64,
    title_start_sec: f64,
) -> Result<Vec<String>> {
    let n = clips.len();
    let mut args: Vec<String> = vec!["-y".into()];

    // Add all inputs
    for clip in clips {
        let input_path = library_root.join(&clip.path);
        args.extend_from_slice(&["-i".into(), path_str(&input_path)?]);
    }

    // Add anullsrc input for clips without audio (as the last input)
    // We'll use a single null audio source and assign it where needed
    let null_audio_idx = n; // index of the null audio input
    let has_any_silent = clips.iter().any(|c| !c.has_audio);
    if has_any_silent {
        args.extend_from_slice(&[
            "-f".into(), "lavfi".into(),
            "-i".into(), "anullsrc=r=48000:cl=stereo".into(),
        ]);
    }

    // Build complex filtergraph
    let mut filter_parts: Vec<String> = Vec::new();

    // Step 1: Conform each video+audio stream
    for (i, clip) in clips.iter().enumerate() {
        // Video conform
        filter_parts.push(format!(
            "[{i}:v]{conform}[v{i}]",
            i = i,
            conform = conform_video_filter(),
        ));

        // Audio conform (use actual audio or null source)
        if clip.has_audio {
            filter_parts.push(format!(
                "[{i}:a]{conform}[a{i}]",
                i = i,
                conform = conform_audio_filter(),
            ));
        } else {
            filter_parts.push(format!(
                "[{null_idx}:a]acopy[a{i}]",
                null_idx = null_audio_idx,
                i = i,
            ));
        }
    }

    // Step 2: Chain xfade for video
    let video_out = build_xfade_chain(&mut filter_parts, clips, n, blend_sec)?;

    // Step 3: Chain acrossfade for audio
    let audio_out = build_acrossfade_chain(&mut filter_parts, clips, n, blend_sec)?;

    // Step 4: Apply title overlay + watermark to final video
    let mut final_video = video_out;

    if let Some(text) = title_text {
        let overlay = title_overlay_filter(text, title_start_sec, TITLE_DURATION);
        filter_parts.push(format!("[{final_video}]{overlay}[titled]"));
        final_video = "titled".to_string();
    }

    if apply_watermark {
        let (wm_filter, scale_filter) = watermark::watermark_filters();
        filter_parts.push(format!("[{final_video}]{wm},{scale}[watermarked]",
            wm = wm_filter, scale = scale_filter));
        final_video = "watermarked".to_string();
    }

    // Build the full filtergraph string
    let filtergraph = filter_parts.join(";");

    args.extend_from_slice(&["-filter_complex".into(), filtergraph]);
    args.extend_from_slice(&["-map".into(), format!("[{}]", final_video)]);
    args.extend_from_slice(&["-map".into(), format!("[{}]", audio_out)]);

    // Output encoding
    args.extend(output_encoding_args());
    args.push(path_str(output_path)?);

    Ok(args)
}

/// Build chained xfade transitions for N clips.
/// Returns the label of the final video stream.
fn build_xfade_chain(
    filter_parts: &mut Vec<String>,
    clips: &[ExportClip],
    n: usize,
    blend_sec: f64,
) -> Result<String> {
    if n == 1 {
        return Ok("v0".to_string());
    }

    // Calculate offsets: each xfade starts at (cumulative duration - blend_duration)
    let mut cumulative_sec = 0.0;
    let mut prev_label = "v0".to_string();

    for i in 1..n {
        let prev_dur_sec = clips[i - 1].duration_ms as f64 / 1000.0;
        cumulative_sec += prev_dur_sec;

        // Offset is cumulative duration minus all previous transitions minus this transition
        let offset = (cumulative_sec - blend_sec * i as f64).max(0.0);
        let out_label = format!("xv{}", i);

        filter_parts.push(format!(
            "[{prev}][v{i}]xfade=transition=fade:duration={dur}:offset={offset}[{out}]",
            prev = prev_label,
            i = i,
            dur = blend_sec,
            offset = format!("{:.3}", offset),
            out = out_label,
        ));

        prev_label = out_label;
    }

    Ok(prev_label)
}

/// Build chained acrossfade transitions for N audio streams.
/// Returns the label of the final audio stream.
fn build_acrossfade_chain(
    filter_parts: &mut Vec<String>,
    clips: &[ExportClip],
    n: usize,
    blend_sec: f64,
) -> Result<String> {
    if n == 1 {
        return Ok("a0".to_string());
    }

    let mut prev_label = "a0".to_string();

    for i in 1..n {
        let prev_dur_sec = clips[i - 1].duration_ms as f64 / 1000.0;
        let xfade_dur = blend_sec.min(prev_dur_sec / 2.0); // Don't exceed half the clip
        let out_label = format!("xa{}", i);

        filter_parts.push(format!(
            "[{prev}][a{i}]acrossfade=d={dur}:c1=tri:c2=tri[{out}]",
            prev = prev_label,
            i = i,
            dur = format!("{:.3}", xfade_dur),
            out = out_label,
        ));

        prev_label = out_label;
    }

    Ok(prev_label)
}

/// Conform filter: normalize resolution, fps, and SAR for consistent xfade input
fn conform_video_filter() -> String {
    "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2,fps=30,setsar=1".to_string()
}

/// Conform audio to 48kHz stereo
fn conform_audio_filter() -> String {
    "aresample=48000,aformat=channel_layouts=stereo".to_string()
}

/// Title overlay using drawtext filter.
/// `start_sec`: when the title appears (e.g. 5 seconds into the timeline).
/// `duration`: total title duration (3 seconds: 0.5s fade in, 2s hold, 0.5s fade out).
fn title_overlay_filter(text: &str, start_sec: f64, duration: f64) -> String {
    // Escape special characters for drawtext
    let escaped = text
        .replace('\\', "\\\\")
        .replace(':', "\\:")
        .replace("'", "\\'");

    let end_sec = start_sec + duration;
    let fade_in_end = start_sec + 0.5;
    let fade_out_start = end_sec - 0.5;

    format!(
        "drawtext=text='{text}':fontsize=48:fontcolor=white:borderw=2:bordercolor=black:\
         x=(w-text_w)/2:y=(h-text_h)/2:\
         enable='between(t,{start},{end})':\
         alpha='if(lt(t,{fi_end}),(t-{start})/0.5,if(gt(t,{fo_start}),(1-(t-{fo_start})/0.5),1))'",
        text = escaped,
        start = format!("{:.3}", start_sec),
        end = format!("{:.3}", end_sec),
        fi_end = format!("{:.3}", fade_in_end),
        fo_start = format!("{:.3}", fade_out_start),
    )
}

/// Output encoding args for H.264 MP4
fn output_encoding_args() -> Vec<String> {
    vec![
        "-c:v".into(), "libx264".into(),
        "-preset".into(), "medium".into(),
        "-crf".into(), "23".into(),
        "-c:a".into(), "aac".into(),
        "-b:a".into(), "192k".into(),
        "-movflags".into(), "+faststart".into(),
        "-shortest".into(),
    ]
}

/// Convert a Path to a String, failing on non-UTF8
fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| DadCamError::InvalidPath("Path contains non-UTF8 characters".to_string()))
}
