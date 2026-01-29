// Dad Cam - Watermark + 720p Cap Filters
// Applied when licensing::should_watermark() returns true (expired trial).

/// Returns (drawtext_filter, scale_filter) strings for watermarked output.
/// Text: "Dad Cam Trial" centered bottom.
/// Scale cap: 1280x720 max.
pub fn watermark_filters() -> (String, String) {
    let drawtext = "drawtext=text='Dad Cam Trial':\
        fontsize=36:fontcolor=white@0.5:borderw=1:bordercolor=black@0.3:\
        x=(w-text_w)/2:y=h-60"
        .to_string();

    let scale = "scale='min(1280,iw)':'min(720,ih)':force_original_aspect_ratio=decrease,\
        pad=ceil(iw/2)*2:ceil(ih/2)*2:(ow-iw)/2:(oh-ih)/2"
        .to_string();

    (drawtext, scale)
}
