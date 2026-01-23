Dad Cam - Phase 6 Implementation Guide

Version: 1.0
Target Audience: Developers new to desktop app export workflows

---

Overview

Phase 6 makes exports first-class, professional, and trustworthy. While Phase 5 built the Auto-Edit engine (VHS Mode) backend, Phase 6 focuses on the user experience: real-time progress tracking, export history, failure recovery, and polished UI.

When complete, you can:
- Watch export progress in real-time with accurate percentage and ETA
- Browse export history with thumbnails and metadata
- Re-run any previous export with one click
- Resume or cancel in-progress exports
- See clear error messages when exports fail
- Open the export folder directly from the app
- Choose between Share (H.264) and Archive (ProRes) presets with confidence

Prerequisites:
- Phase 1-5 complete and working
- Test library with export_recipes and at least one completed export_run
- Understanding of Phase 1 job system and Phase 5 export pipeline
- FFmpeg available via `tools.rs` resolver (from Phase 1)
- Basic understanding of Tauri events for IPC

---

What We're Building

Phase 6 adds polish and reliability to the export system:

```
Export Request (from Phase 5)
    |
    v  Job System
Export Job Running
    |
    v  Progress Events (NEW)
Real-Time Progress UI
    |-- Percentage bar
    |-- Time elapsed / ETA
    |-- Current operation label
    |-- Cancel button
    |
    v  Completion/Failure
Export History Entry
    |
    v  History UI (NEW)
Browseable Export List
    |-- Thumbnail preview
    |-- Re-run button
    |-- Open folder button
    |-- Delete export
```

Core concepts:

1. **Progress Events**: Job system emits events that the UI listens to via Tauri's event system

2. **Export History**: The export_runs table becomes browseable with rich UI

3. **Failure Recovery**: Exports can be cancelled, resumed (where possible), and errors are clearly displayed

4. **Professional Output**: Users trust the export system because they can see exactly what happened

---

Part 1: Understanding the Export UI Requirements

1.1 Progress UI Requirements

The user needs to know:
- Is the export running? (clear visual state)
- How far along is it? (percentage)
- What is it doing right now? (current operation)
- How long will it take? (estimated time remaining)
- Can I stop it? (cancel button)

Progress sources from FFmpeg:
```
frame=  500 fps=30 q=23.0 size=   10240kB time=00:00:16.67 bitrate=5025.8kbits/s speed=1.0x
```

We parse:
- `frame=` and total frames for percentage
- `time=` for elapsed duration
- `speed=` for ETA calculation

1.2 Export History Requirements

Users want to:
- See all past exports in a list
- Know when each was created
- See export settings (mode, LUT, preset)
- Preview what the export looks like (thumbnail)
- Re-run an export (regenerate with same settings)
- Open the output file or folder
- Delete exports they no longer need

1.3 Failure Recovery Requirements

When things go wrong:
- Clear error message (not raw FFmpeg stderr)
- Option to retry
- Logs available for debugging
- No corrupt partial files left behind

1.4 State Machine for Export Runs

```
pending --> rendering --> completed
    |           |
    |           v
    +-------> failed
    |           |
    +-------> cancelled
```

Status transitions:
- `pending` -> `rendering`: Job starts processing
- `rendering` -> `completed`: FFmpeg finishes successfully
- `rendering` -> `failed`: FFmpeg error or timeout
- `rendering` -> `cancelled`: User cancels
- `pending` -> `cancelled`: User cancels before start

---

Part 2: Database Schema Additions

2.1 Export Progress Tracking (Migration)

Add this migration to `src-tauri/src/db/migrations.rs`:

```rust
// Add to MIGRATIONS array:

// Migration 4: Export progress tracking (Phase 6)
r#"
-- Add progress tracking columns to export_runs
ALTER TABLE export_runs ADD COLUMN current_operation TEXT;
ALTER TABLE export_runs ADD COLUMN frames_total INTEGER;
ALTER TABLE export_runs ADD COLUMN frames_completed INTEGER DEFAULT 0;
ALTER TABLE export_runs ADD COLUMN last_progress_at TEXT;

-- Export output metadata (for history display)
ALTER TABLE export_runs ADD COLUMN output_size_bytes INTEGER;
ALTER TABLE export_runs ADD COLUMN output_duration_ms INTEGER;
ALTER TABLE export_runs ADD COLUMN thumbnail_path TEXT;

-- Index for history queries
CREATE INDEX IF NOT EXISTS idx_export_runs_created ON export_runs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_export_runs_status_created ON export_runs(status, created_at DESC);
"#,
```

2.2 Schema Design Notes

**Progress columns:**
- `current_operation`: Human-readable label ("Encoding clip 3 of 15", "Applying transitions")
- `frames_total`: Total frames to encode (calculated from clip durations)
- `frames_completed`: Current frame count from FFmpeg output
- `last_progress_at`: Timestamp of last progress update (for stale detection)

**Output metadata:**
- `output_size_bytes`: Final file size (for display)
- `output_duration_ms`: Final video duration (for verification)
- `thumbnail_path`: Generated thumbnail of the export (for history display)

2.3 Schema Query Helpers

Add to `src-tauri/src/db/schema.rs`:

```rust
// ----- Export Progress -----

/// Update export run progress
pub fn update_export_progress(
    conn: &Connection,
    run_id: i64,
    current_operation: &str,
    frames_completed: i64,
    progress_percent: i64,
) -> Result<()> {
    conn.execute(
        r#"UPDATE export_runs SET
             current_operation = ?1,
             frames_completed = ?2,
             progress = ?3,
             last_progress_at = datetime('now')
           WHERE id = ?4"#,
        params![current_operation, frames_completed, progress_percent, run_id],
    )?;
    Ok(())
}

/// Set total frames for progress calculation
pub fn set_export_frames_total(
    conn: &Connection,
    run_id: i64,
    frames_total: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE export_runs SET frames_total = ?1 WHERE id = ?2",
        params![frames_total, run_id],
    )?;
    Ok(())
}

/// Complete export with output metadata
pub fn complete_export_run(
    conn: &Connection,
    run_id: i64,
    output_path: &str,
    output_size_bytes: i64,
    output_duration_ms: i64,
    thumbnail_path: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        r#"UPDATE export_runs SET
             status = 'completed',
             progress = 100,
             output_path = ?1,
             output_size_bytes = ?2,
             output_duration_ms = ?3,
             thumbnail_path = ?4,
             completed_at = ?5
           WHERE id = ?6"#,
        params![output_path, output_size_bytes, output_duration_ms, thumbnail_path, now, run_id],
    )?;
    Ok(())
}

/// Get export runs for history display
pub fn get_export_history(
    conn: &Connection,
    library_id: i64,
    limit: i64,
    offset: i64,
    status_filter: Option<&str>,
) -> Result<Vec<ExportRunWithDetails>> {
    let base_sql = r#"
        SELECT er.id, er.recipe_id, er.name, er.status, er.progress,
               er.output_path, er.output_size_bytes, er.output_duration_ms,
               er.thumbnail_path, er.total_clips, er.total_duration_ms,
               er.error_message, er.created_at, er.completed_at,
               ex.name as recipe_name, ex.mode, ex.output_preset, ex.lut_id
        FROM export_runs er
        JOIN export_recipes ex ON er.recipe_id = ex.id
        WHERE er.library_id = ?1
    "#;

    let sql = if let Some(status) = status_filter {
        format!("{} AND er.status = ?4 ORDER BY er.created_at DESC LIMIT ?2 OFFSET ?3", base_sql)
    } else {
        format!("{} ORDER BY er.created_at DESC LIMIT ?2 OFFSET ?3", base_sql)
    };

    let mut stmt = conn.prepare(&sql)?;

    let rows = if let Some(status) = status_filter {
        stmt.query_map(params![library_id, limit, offset, status], map_export_row)?
    } else {
        stmt.query_map(params![library_id, limit, offset], map_export_row)?
    };

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn map_export_row(row: &rusqlite::Row) -> rusqlite::Result<ExportRunWithDetails> {
    Ok(ExportRunWithDetails {
        id: row.get(0)?,
        recipe_id: row.get(1)?,
        name: row.get(2)?,
        status: row.get(3)?,
        progress: row.get(4)?,
        output_path: row.get(5)?,
        output_size_bytes: row.get(6)?,
        output_duration_ms: row.get(7)?,
        thumbnail_path: row.get(8)?,
        total_clips: row.get(9)?,
        total_duration_ms: row.get(10)?,
        error_message: row.get(11)?,
        created_at: row.get(12)?,
        completed_at: row.get(13)?,
        recipe_name: row.get(14)?,
        mode: row.get(15)?,
        output_preset: row.get(16)?,
        lut_id: row.get(17)?,
    })
}

/// Extended export run data for history display
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRunWithDetails {
    pub id: i64,
    pub recipe_id: i64,
    pub name: String,
    pub status: String,
    pub progress: Option<i64>,
    pub output_path: Option<String>,
    pub output_size_bytes: Option<i64>,
    pub output_duration_ms: Option<i64>,
    pub thumbnail_path: Option<String>,
    pub total_clips: i64,
    pub total_duration_ms: i64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    // Recipe details for display
    pub recipe_name: String,
    pub mode: String,
    pub output_preset: String,
    pub lut_id: Option<String>,
}
```

---

Part 3: Progress UI System

3.1 Understanding Tauri Events

Tauri provides bidirectional event communication between Rust backend and JavaScript frontend.

Backend emits:
```rust
app_handle.emit("export-progress", payload)?;
```

Frontend listens:
```typescript
import { listen } from '@tauri-apps/api/event';

const unlisten = await listen('export-progress', (event) => {
  console.log('Progress:', event.payload);
});
```

3.2 Progress Event Types

Define progress event payloads in `src-tauri/src/export/events.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Progress event emitted during export
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgressEvent {
    pub run_id: i64,
    pub status: ExportStatus,
    pub progress_percent: u8,
    pub current_operation: String,
    pub frames_completed: Option<i64>,
    pub frames_total: Option<i64>,
    pub elapsed_secs: f64,
    pub estimated_remaining_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    Starting,
    Encoding,
    ApplyingTransitions,
    ApplyingLut,
    Finalizing,
    Completed,
    Failed,
    Cancelled,
}

/// Final result event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCompletedEvent {
    pub run_id: i64,
    pub success: bool,
    pub output_path: Option<String>,
    pub output_size_bytes: Option<i64>,
    pub duration_ms: Option<i64>,
    pub error_message: Option<String>,
}

impl ExportProgressEvent {
    pub fn starting(run_id: i64) -> Self {
        Self {
            run_id,
            status: ExportStatus::Starting,
            progress_percent: 0,
            current_operation: "Starting export...".to_string(),
            frames_completed: None,
            frames_total: None,
            elapsed_secs: 0.0,
            estimated_remaining_secs: None,
        }
    }

    pub fn encoding(
        run_id: i64,
        frames_completed: i64,
        frames_total: i64,
        elapsed_secs: f64,
    ) -> Self {
        let progress_percent = if frames_total > 0 {
            ((frames_completed as f64 / frames_total as f64) * 100.0) as u8
        } else {
            0
        };

        let estimated_remaining = if frames_completed > 0 && progress_percent > 0 {
            let rate = frames_completed as f64 / elapsed_secs;
            let remaining_frames = frames_total - frames_completed;
            Some(remaining_frames as f64 / rate)
        } else {
            None
        };

        Self {
            run_id,
            status: ExportStatus::Encoding,
            progress_percent,
            current_operation: format!(
                "Encoding frame {} of {}",
                frames_completed, frames_total
            ),
            frames_completed: Some(frames_completed),
            frames_total: Some(frames_total),
            elapsed_secs,
            estimated_remaining_secs: estimated_remaining,
        }
    }

    pub fn completed(run_id: i64, elapsed_secs: f64) -> Self {
        Self {
            run_id,
            status: ExportStatus::Completed,
            progress_percent: 100,
            current_operation: "Export completed".to_string(),
            frames_completed: None,
            frames_total: None,
            elapsed_secs,
            estimated_remaining_secs: Some(0.0),
        }
    }

    pub fn failed(run_id: i64, error: &str) -> Self {
        Self {
            run_id,
            status: ExportStatus::Failed,
            progress_percent: 0,
            current_operation: format!("Failed: {}", error),
            frames_completed: None,
            frames_total: None,
            elapsed_secs: 0.0,
            estimated_remaining_secs: None,
        }
    }
}
```

3.3 FFmpeg Progress Parser

**Required dependency** - Add to `src-tauri/Cargo.toml`:
```toml
lazy_static = "1.4"  # For regex caching
```

Create `src-tauri/src/export/progress_parser.rs`:

```rust
use anyhow::Result;
use regex::Regex;
use std::time::Duration;

/// Parsed FFmpeg progress line
#[derive(Debug, Clone, Default)]
pub struct FfmpegProgress {
    pub frame: Option<i64>,
    pub fps: Option<f64>,
    pub time: Option<Duration>,
    pub speed: Option<f64>,
    pub size_bytes: Option<i64>,
}

lazy_static::lazy_static! {
    static ref FRAME_RE: Regex = Regex::new(r"frame=\s*(\d+)").unwrap();
    static ref FPS_RE: Regex = Regex::new(r"fps=\s*([\d.]+)").unwrap();
    static ref TIME_RE: Regex = Regex::new(r"time=(\d+):(\d+):(\d+)\.(\d+)").unwrap();
    static ref SPEED_RE: Regex = Regex::new(r"speed=\s*([\d.]+)x").unwrap();
    static ref SIZE_RE: Regex = Regex::new(r"size=\s*(\d+)kB").unwrap();
}

impl FfmpegProgress {
    /// Parse a line of FFmpeg stderr output
    pub fn parse(line: &str) -> Option<Self> {
        // FFmpeg progress lines contain "frame=" and "time="
        if !line.contains("frame=") || !line.contains("time=") {
            return None;
        }

        let mut progress = Self::default();

        if let Some(caps) = FRAME_RE.captures(line) {
            progress.frame = caps.get(1).and_then(|m| m.as_str().parse().ok());
        }

        if let Some(caps) = FPS_RE.captures(line) {
            progress.fps = caps.get(1).and_then(|m| m.as_str().parse().ok());
        }

        if let Some(caps) = TIME_RE.captures(line) {
            let hours: u64 = caps.get(1).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let mins: u64 = caps.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let secs: u64 = caps.get(3).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let centis: u64 = caps.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);

            progress.time = Some(Duration::from_secs(
                hours * 3600 + mins * 60 + secs
            ) + Duration::from_millis(centis * 10));
        }

        if let Some(caps) = SPEED_RE.captures(line) {
            progress.speed = caps.get(1).and_then(|m| m.as_str().parse().ok());
        }

        if let Some(caps) = SIZE_RE.captures(line) {
            progress.size_bytes = caps.get(1).and_then(|m| {
                m.as_str().parse::<i64>().ok().map(|kb| kb * 1024)
            });
        }

        // Only return if we got at least frame info
        if progress.frame.is_some() {
            Some(progress)
        } else {
            None
        }
    }
}

/// Calculate total frames for export
pub fn calculate_total_frames(clips: &[(i64, f64)], fps: f64) -> i64 {
    let total_duration_secs: f64 = clips.iter()
        .map(|(duration_ms, _)| *duration_ms as f64 / 1000.0)
        .sum();

    (total_duration_secs * fps) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_line() {
        let line = "frame=  500 fps=30.0 q=23.0 size=   10240kB time=00:00:16.67 bitrate=5025.8kbits/s speed=1.0x";

        let progress = FfmpegProgress::parse(line).unwrap();

        assert_eq!(progress.frame, Some(500));
        assert_eq!(progress.fps, Some(30.0));
        assert_eq!(progress.speed, Some(1.0));
        assert_eq!(progress.size_bytes, Some(10240 * 1024));
    }

    #[test]
    fn test_parse_non_progress_line() {
        let line = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'input.mp4':";

        let progress = FfmpegProgress::parse(line);

        assert!(progress.is_none());
    }
}
```

3.4 Event Emitter Integration

Update `src-tauri/src/export/renderer.rs` to emit progress events:

```rust
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use tauri::AppHandle;

use super::events::{ExportProgressEvent, ExportCompletedEvent};
use super::progress_parser::FfmpegProgress;

/// Run FFmpeg with progress tracking
pub fn run_ffmpeg_with_progress(
    app_handle: &AppHandle,
    run_id: i64,
    ffmpeg_path: &str,
    args: &[&str],
    total_frames: i64,
) -> Result<(), String> {
    let start_time = Instant::now();

    // Emit starting event
    let _ = app_handle.emit("export-progress", ExportProgressEvent::starting(run_id));

    // Start FFmpeg process
    let mut child = Command::new(ffmpeg_path)
        .args(args)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;

    let stderr = child.stderr.take()
        .ok_or_else(|| "Failed to capture FFmpeg stderr".to_string())?;

    // Create channel for progress updates
    let (tx, rx) = mpsc::channel::<FfmpegProgress>();

    // Spawn thread to read stderr
    let reader_thread = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(line) = line {
                if let Some(progress) = FfmpegProgress::parse(&line) {
                    let _ = tx.send(progress);
                }
            }
        }
    });

    // Process progress updates
    let app_handle_clone = app_handle.clone();
    let progress_thread = thread::spawn(move || {
        let mut last_emit = Instant::now();
        let emit_interval = std::time::Duration::from_millis(250); // 4 updates/second max

        while let Ok(progress) = rx.recv() {
            // Throttle UI updates
            if last_emit.elapsed() >= emit_interval {
                if let Some(frame) = progress.frame {
                    let elapsed = start_time.elapsed().as_secs_f64();
                    let event = ExportProgressEvent::encoding(
                        run_id,
                        frame,
                        total_frames,
                        elapsed,
                    );
                    let _ = app_handle_clone.emit("export-progress", event);
                    last_emit = Instant::now();
                }
            }
        }
    });

    // Wait for FFmpeg to complete
    let status = child.wait()
        .map_err(|e| format!("FFmpeg process error: {}", e))?;

    // Wait for threads to finish
    let _ = reader_thread.join();
    let _ = progress_thread.join();

    let elapsed = start_time.elapsed().as_secs_f64();

    if status.success() {
        let _ = app_handle.emit("export-progress", ExportProgressEvent::completed(run_id, elapsed));
        Ok(())
    } else {
        let error_msg = format!("FFmpeg exited with code: {:?}", status.code());
        let _ = app_handle.emit("export-progress", ExportProgressEvent::failed(run_id, &error_msg));
        Err(error_msg)
    }
}
```

3.5 Constants for Progress

Add to `src-tauri/src/constants.rs`:

```rust
// Progress tracking
pub const PROGRESS_UPDATE_INTERVAL_MS: u64 = 250;  // 4 updates per second
pub const PROGRESS_STALE_TIMEOUT_SECS: u64 = 30;   // Consider stale if no update for 30s
pub const EXPORT_OUTPUT_FPS: f64 = 30.0;           // Standard output framerate
```

3.6 FFmpeg Command References

For progress tracking to work correctly, both export presets must output progress to stderr.

**H.264 Share Preset** (from Phase 5):
```bash
ffmpeg -y -f concat -safe 0 -i concat_list.txt \
  -c:v libx264 -preset medium -crf 23 \
  -c:a aac -b:a 128k \
  -movflags +faststart \
  -progress pipe:2 \
  output.mp4
```

**ProRes Archive Preset**:
```bash
ffmpeg -y -f concat -safe 0 -i concat_list.txt \
  -c:v prores_ks -profile:v 3 \
  -vendor apl0 -bits_per_mb 8000 \
  -pix_fmt yuv422p10le \
  -c:a pcm_s16le \
  -progress pipe:2 \
  output.mov
```

ProRes profile options:
- Profile 0: ProRes 422 Proxy (lowest quality, smallest)
- Profile 1: ProRes 422 LT (low quality)
- Profile 2: ProRes 422 (standard quality)
- Profile 3: ProRes 422 HQ (high quality, recommended for archive)
- Profile 4: ProRes 4444 (highest quality, preserves alpha)

Note: The `-progress pipe:2` flag ensures FFmpeg outputs progress stats to stderr for parsing.

3.7 Phase 5 Integration Bridge

This section shows how the Phase 5 export job system integrates with Phase 6 progress tracking.

Update the export job handler in `src-tauri/src/jobs/export_job.rs` to use progress tracking:

```rust
use crate::export::renderer::run_ffmpeg_with_progress;
use crate::export::progress_parser::calculate_total_frames;
use crate::db::schema::{set_export_frames_total, update_export_progress};

/// Execute export job with Phase 6 progress tracking
pub fn execute_export_job(
    app_handle: &AppHandle,
    conn: &Connection,
    run_id: i64,
    recipe: &ExportRecipe,
    clips: &[ClipForExport],
) -> Result<(), ExportError> {
    // 1. Calculate total frames for progress tracking
    let clip_durations: Vec<(i64, f64)> = clips
        .iter()
        .map(|c| (c.duration_ms, EXPORT_OUTPUT_FPS))
        .collect();
    let total_frames = calculate_total_frames(&clip_durations, EXPORT_OUTPUT_FPS);

    // Store in database for progress calculation
    set_export_frames_total(conn, run_id, total_frames)?;

    // 2. Build FFmpeg args based on preset
    let ffmpeg_path = resolve_ffmpeg()?;
    let args = match recipe.output_preset.as_str() {
        "archive" => build_prores_args(&concat_file, &output_path),
        _ => build_h264_args(&concat_file, &output_path),  // "share" default
    };

    // 3. Run FFmpeg with progress tracking (Phase 6)
    // This emits progress events that the UI listens to
    run_ffmpeg_with_progress(
        app_handle,
        run_id,
        &ffmpeg_path,
        &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        total_frames,
    )?;

    Ok(())
}

fn build_h264_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-f".to_string(), "concat".to_string(),
        "-safe".to_string(), "0".to_string(),
        "-i".to_string(), input.to_string_lossy().to_string(),
        "-c:v".to_string(), "libx264".to_string(),
        "-preset".to_string(), "medium".to_string(),
        "-crf".to_string(), "23".to_string(),
        "-c:a".to_string(), "aac".to_string(),
        "-b:a".to_string(), "128k".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-progress".to_string(), "pipe:2".to_string(),
        output.to_string_lossy().to_string(),
    ]
}

fn build_prores_args(input: &Path, output: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-f".to_string(), "concat".to_string(),
        "-safe".to_string(), "0".to_string(),
        "-i".to_string(), input.to_string_lossy().to_string(),
        "-c:v".to_string(), "prores_ks".to_string(),
        "-profile:v".to_string(), "3".to_string(),
        "-vendor".to_string(), "apl0".to_string(),
        "-bits_per_mb".to_string(), "8000".to_string(),
        "-pix_fmt".to_string(), "yuv422p10le".to_string(),
        "-c:a".to_string(), "pcm_s16le".to_string(),
        "-progress".to_string(), "pipe:2".to_string(),
        output.to_string_lossy().to_string(),
    ]
}
```

Key integration points:
1. Phase 5 `ExportRecipe` provides clip selection and settings
2. Phase 6 `calculate_total_frames` computes expected frame count
3. Phase 6 `run_ffmpeg_with_progress` executes FFmpeg with event emission
4. Frontend listens via `listenToExportProgress` from Part 4.2

---

Part 4: Export History UI Components

4.1 TypeScript Types

Add to `src/types/exports.ts`:

```typescript
export interface ExportRunWithDetails {
  id: number;
  recipeId: number;
  name: string;
  status: 'pending' | 'rendering' | 'completed' | 'failed' | 'cancelled';
  progress: number | null;
  outputPath: string | null;
  outputSizeBytes: number | null;
  outputDurationMs: number | null;
  thumbnailPath: string | null;
  totalClips: number;
  totalDurationMs: number;
  errorMessage: string | null;
  createdAt: string;
  completedAt: string | null;
  // Recipe details
  recipeName: string;
  mode: 'by_date' | 'by_event' | 'by_favorites' | 'all';
  outputPreset: 'share' | 'archive';
  lutId: string | null;
}

export interface ExportProgressEvent {
  runId: number;
  status: 'starting' | 'encoding' | 'applying_transitions' | 'applying_lut' | 'finalizing' | 'completed' | 'failed' | 'cancelled';
  progressPercent: number;
  currentOperation: string;
  framesCompleted: number | null;
  framesTotal: number | null;
  elapsedSecs: number;
  estimatedRemainingSecs: number | null;
}

export interface ExportHistoryFilters {
  status?: 'completed' | 'failed' | 'cancelled' | 'all';
  limit: number;
  offset: number;
}
```

4.2 API Functions

Add to `src/api/exports.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import type { ExportRunWithDetails, ExportProgressEvent, ExportHistoryFilters } from '../types/exports';

// ----- Export History -----

export async function getExportHistory(
  filters: ExportHistoryFilters
): Promise<ExportRunWithDetails[]> {
  return invoke('get_export_history', {
    limit: filters.limit,
    offset: filters.offset,
    statusFilter: filters.status === 'all' ? null : filters.status,
  });
}

export async function getExportRun(runId: number): Promise<ExportRunWithDetails | null> {
  return invoke('get_export_run', { runId });
}

export async function deleteExportRun(runId: number): Promise<void> {
  return invoke('delete_export_run', { runId });
}

export async function cancelExportRun(runId: number): Promise<void> {
  return invoke('cancel_export_run', { runId });
}

// ----- Progress Listening -----

export async function listenToExportProgress(
  callback: (event: ExportProgressEvent) => void
): Promise<UnlistenFn> {
  return listen<ExportProgressEvent>('export-progress', (event) => {
    callback(event.payload);
  });
}

// ----- File Operations -----

export async function openExportFolder(runId: number): Promise<void> {
  return invoke('open_export_folder', { runId });
}

export async function openExportFile(runId: number): Promise<void> {
  return invoke('open_export_file', { runId });
}

export async function revealInFinder(path: string): Promise<void> {
  return invoke('reveal_in_finder', { path });
}

// ----- Utility Functions -----

export function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
  }
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function formatTimeAgo(dateString: string): string {
  const date = new Date(dateString);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / (1000 * 60));
  const diffHours = Math.floor(diffMs / (1000 * 60 * 60));
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffMins < 1) return 'Just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString();
}

// ----- Re-run Export -----

export async function rerunExport(runId: number): Promise<number> {
  // Returns the new run ID
  return invoke('rerun_export', { runId });
}
```

4.3 Export History View Component

Create `src/components/ExportHistoryView.tsx`:

```typescript
import { useState, useEffect, useCallback } from 'react';
import type { ExportRunWithDetails, ExportHistoryFilters } from '../types/exports';
import { getExportHistory, deleteExportRun, cancelExportRun, openExportFolder, openExportFile, formatDuration, formatFileSize, formatTimeAgo } from '../api/exports';
import { ExportProgressCard } from './ExportProgressCard';
import { rerunExport } from '../api/exports';

interface ExportHistoryViewProps {
  onExportClick?: (run: ExportRunWithDetails) => void;
}

export function ExportHistoryView({ onExportClick }: ExportHistoryViewProps) {
  const [exports, setExports] = useState<ExportRunWithDetails[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<'all' | 'completed' | 'failed' | 'cancelled'>('all');

  const loadExports = useCallback(async () => {
    setIsLoading(true);
    try {
      const history = await getExportHistory({
        status: statusFilter,
        limit: 50,
        offset: 0,
      });
      setExports(history);
    } catch (err) {
      console.error('Failed to load export history:', err);
    } finally {
      setIsLoading(false);
    }
  }, [statusFilter]);

  useEffect(() => {
    loadExports();
  }, [loadExports]);

  const handleDelete = async (runId: number) => {
    if (!confirm('Delete this export? The output file will also be removed.')) {
      return;
    }
    try {
      await deleteExportRun(runId);
      setExports(prev => prev.filter(e => e.id !== runId));
    } catch (err) {
      console.error('Failed to delete export:', err);
    }
  };

  const handleCancel = async (runId: number) => {
    try {
      await cancelExportRun(runId);
      loadExports();
    } catch (err) {
      console.error('Failed to cancel export:', err);
    }
  };

  const handleRerun = async (runId: number) => {
    try {
      await rerunExport(runId);
      loadExports();
    } catch (err) {
      console.error('Failed to rerun export:', err);
    }
  };

  const handleOpenFolder = async (runId: number) => {
    try {
      await openExportFolder(runId);
    } catch (err) {
      console.error('Failed to open folder:', err);
    }
  };

  const handleOpenFile = async (runId: number) => {
    try {
      await openExportFile(runId);
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  };

  const getStatusBadgeStyle = (status: string): React.CSSProperties => {
    const baseStyle: React.CSSProperties = {
      padding: '2px 8px',
      borderRadius: '4px',
      fontSize: '11px',
      fontWeight: 'bold',
      textTransform: 'uppercase' as const,
    };

    switch (status) {
      case 'completed':
        return { ...baseStyle, backgroundColor: '#22c55e', color: 'white' };
      case 'rendering':
        return { ...baseStyle, backgroundColor: '#3b82f6', color: 'white' };
      case 'failed':
        return { ...baseStyle, backgroundColor: '#ef4444', color: 'white' };
      case 'cancelled':
        return { ...baseStyle, backgroundColor: '#6b7280', color: 'white' };
      case 'pending':
        return { ...baseStyle, backgroundColor: '#eab308', color: 'black' };
      default:
        return baseStyle;
    }
  };

  const getModeLabel = (mode: string): string => {
    switch (mode) {
      case 'by_date': return 'By Date';
      case 'by_event': return 'By Event';
      case 'by_favorites': return 'Favorites';
      case 'all': return 'All Clips';
      default: return mode;
    }
  };

  const getPresetLabel = (preset: string): string => {
    switch (preset) {
      case 'share': return 'Share (H.264)';
      case 'archive': return 'Archive (ProRes)';
      default: return preset;
    }
  };

  // Separate in-progress exports from history
  const inProgressExports = exports.filter(e => e.status === 'rendering' || e.status === 'pending');
  const historyExports = exports.filter(e => e.status !== 'rendering' && e.status !== 'pending');

  return (
    <div style={containerStyle}>
      {/* Header */}
      <div style={headerStyle}>
        <h2 style={titleStyle}>Export History</h2>
        <div style={filterContainerStyle}>
          {(['all', 'completed', 'failed', 'cancelled'] as const).map(status => (
            <button
              key={status}
              onClick={() => setStatusFilter(status)}
              style={{
                ...filterButtonStyle,
                backgroundColor: statusFilter === status ? '#4a9eff' : '#333',
              }}
            >
              {status.charAt(0).toUpperCase() + status.slice(1)}
            </button>
          ))}
        </div>
      </div>

      {/* In Progress Section */}
      {inProgressExports.length > 0 && (
        <div style={sectionStyle}>
          <h3 style={sectionTitleStyle}>In Progress</h3>
          {inProgressExports.map(exp => (
            <ExportProgressCard
              key={exp.id}
              exportRun={exp}
              onCancel={() => handleCancel(exp.id)}
            />
          ))}
        </div>
      )}

      {/* History Section */}
      <div style={sectionStyle}>
        {isLoading ? (
          <div style={loadingStyle}>Loading exports...</div>
        ) : historyExports.length === 0 ? (
          <div style={emptyStyle}>No exports yet. Create a recipe and run an export to get started.</div>
        ) : (
          <div style={listStyle}>
            {historyExports.map(exp => (
              <div key={exp.id} style={exportCardStyle}>
                {/* Thumbnail */}
                <div style={thumbnailContainerStyle}>
                  {exp.thumbnailPath ? (
                    <img
                      src={`asset://${exp.thumbnailPath}`}
                      alt={exp.name}
                      style={thumbnailStyle}
                    />
                  ) : (
                    <div style={thumbnailPlaceholderStyle}>No Preview</div>
                  )}
                </div>

                {/* Info */}
                <div style={infoContainerStyle}>
                  <div style={infoHeaderStyle}>
                    <span style={nameStyle}>{exp.name}</span>
                    <span style={getStatusBadgeStyle(exp.status)}>{exp.status}</span>
                  </div>

                  <div style={metaStyle}>
                    <span>{getModeLabel(exp.mode)}</span>
                    <span style={dotStyle}>|</span>
                    <span>{getPresetLabel(exp.outputPreset)}</span>
                    {exp.lutId && (
                      <>
                        <span style={dotStyle}>|</span>
                        <span>LUT: {exp.lutId}</span>
                      </>
                    )}
                  </div>

                  <div style={statsStyle}>
                    <span>{exp.totalClips} clips</span>
                    <span style={dotStyle}>|</span>
                    <span>{formatDuration(exp.totalDurationMs)}</span>
                    {exp.outputSizeBytes && (
                      <>
                        <span style={dotStyle}>|</span>
                        <span>{formatFileSize(exp.outputSizeBytes)}</span>
                      </>
                    )}
                  </div>

                  {exp.errorMessage && (
                    <div style={errorStyle}>Error: {exp.errorMessage}</div>
                  )}

                  <div style={timeStyle}>{formatTimeAgo(exp.createdAt)}</div>
                </div>

                {/* Actions */}
                <div style={actionsStyle}>
                  {exp.status === 'completed' && (
                    <>
                      <button
                        onClick={() => handleOpenFile(exp.id)}
                        style={actionButtonStyle}
                        title="Open video file"
                      >
                        Play
                      </button>
                      <button
                        onClick={() => handleOpenFolder(exp.id)}
                        style={actionButtonStyle}
                        title="Open in Finder/Explorer"
                      >
                        Folder
                      </button>
                    </>
                  )}
                  <button
                    onClick={() => handleRerun(exp.id)}
                    style={actionButtonStyle}
                    title="Re-run this export with same settings"
                  >
                    Re-run
                  </button>
                  <button
                    onClick={() => handleDelete(exp.id)}
                    style={{ ...actionButtonStyle, color: '#ef4444' }}
                    title="Delete export"
                  >
                    Delete
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// Styles
const containerStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  backgroundColor: '#1a1a1a',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  padding: '16px 20px',
  borderBottom: '1px solid #333',
};

const titleStyle: React.CSSProperties = {
  margin: 0,
  fontSize: '18px',
  fontWeight: 'bold',
  color: '#fff',
};

const filterContainerStyle: React.CSSProperties = {
  display: 'flex',
  gap: '8px',
};

const filterButtonStyle: React.CSSProperties = {
  padding: '6px 12px',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  fontSize: '12px',
  cursor: 'pointer',
};

const sectionStyle: React.CSSProperties = {
  padding: '16px 20px',
};

const sectionTitleStyle: React.CSSProperties = {
  margin: '0 0 12px 0',
  fontSize: '14px',
  fontWeight: 'bold',
  color: '#888',
  textTransform: 'uppercase',
};

const loadingStyle: React.CSSProperties = {
  padding: '40px',
  textAlign: 'center',
  color: '#666',
};

const emptyStyle: React.CSSProperties = {
  padding: '40px',
  textAlign: 'center',
  color: '#888',
};

const listStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '12px',
};

const exportCardStyle: React.CSSProperties = {
  display: 'flex',
  gap: '16px',
  padding: '16px',
  backgroundColor: '#2a2a2a',
  borderRadius: '8px',
  border: '1px solid #333',
};

const thumbnailContainerStyle: React.CSSProperties = {
  flexShrink: 0,
  width: '120px',
  height: '68px',
  borderRadius: '4px',
  overflow: 'hidden',
  backgroundColor: '#111',
};

const thumbnailStyle: React.CSSProperties = {
  width: '100%',
  height: '100%',
  objectFit: 'cover',
};

const thumbnailPlaceholderStyle: React.CSSProperties = {
  width: '100%',
  height: '100%',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  color: '#666',
  fontSize: '11px',
};

const infoContainerStyle: React.CSSProperties = {
  flex: 1,
  display: 'flex',
  flexDirection: 'column',
  gap: '4px',
};

const infoHeaderStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
};

const nameStyle: React.CSSProperties = {
  fontSize: '14px',
  fontWeight: 'bold',
  color: '#fff',
};

const metaStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#888',
  display: 'flex',
  gap: '6px',
};

const statsStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#666',
  display: 'flex',
  gap: '6px',
};

const dotStyle: React.CSSProperties = {
  color: '#444',
};

const errorStyle: React.CSSProperties = {
  fontSize: '11px',
  color: '#ef4444',
  marginTop: '4px',
};

const timeStyle: React.CSSProperties = {
  fontSize: '11px',
  color: '#555',
  marginTop: 'auto',
};

const actionsStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '4px',
  alignItems: 'flex-end',
};

const actionButtonStyle: React.CSSProperties = {
  padding: '4px 12px',
  border: 'none',
  borderRadius: '4px',
  backgroundColor: '#333',
  color: '#fff',
  fontSize: '12px',
  cursor: 'pointer',
  minWidth: '70px',
};
```

4.4 Export Progress Card Component

Create `src/components/ExportProgressCard.tsx`:

```typescript
import { useState, useEffect } from 'react';
import type { ExportRunWithDetails, ExportProgressEvent } from '../types/exports';
import { listenToExportProgress, formatDuration } from '../api/exports';

interface ExportProgressCardProps {
  exportRun: ExportRunWithDetails;
  onCancel: () => void;
}

export function ExportProgressCard({ exportRun, onCancel }: ExportProgressCardProps) {
  const [progress, setProgress] = useState<ExportProgressEvent | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    const setupListener = async () => {
      unlisten = await listenToExportProgress((event) => {
        if (event.runId === exportRun.id) {
          setProgress(event);
        }
      });
    };

    setupListener();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, [exportRun.id]);

  const currentProgress = progress?.progressPercent ?? exportRun.progress ?? 0;
  const currentOperation = progress?.currentOperation ?? 'Starting...';
  const estimatedRemaining = progress?.estimatedRemainingSecs;

  const formatEta = (secs: number): string => {
    if (secs < 60) return `${Math.round(secs)}s remaining`;
    const mins = Math.floor(secs / 60);
    const remainingSecs = Math.round(secs % 60);
    return `${mins}m ${remainingSecs}s remaining`;
  };

  return (
    <div style={cardStyle}>
      <div style={headerStyle}>
        <span style={nameStyle}>{exportRun.name}</span>
        <button onClick={onCancel} style={cancelButtonStyle}>
          Cancel
        </button>
      </div>

      <div style={progressContainerStyle}>
        <div style={progressBarBackgroundStyle}>
          <div
            style={{
              ...progressBarFillStyle,
              width: `${currentProgress}%`,
            }}
          />
        </div>
        <span style={percentageStyle}>{currentProgress}%</span>
      </div>

      <div style={operationStyle}>{currentOperation}</div>

      {estimatedRemaining !== null && estimatedRemaining !== undefined && estimatedRemaining > 0 && (
        <div style={etaStyle}>{formatEta(estimatedRemaining)}</div>
      )}
    </div>
  );
}

const cardStyle: React.CSSProperties = {
  padding: '16px',
  backgroundColor: '#2a2a2a',
  borderRadius: '8px',
  border: '1px solid #3b82f6',
  marginBottom: '12px',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  marginBottom: '12px',
};

const nameStyle: React.CSSProperties = {
  fontSize: '14px',
  fontWeight: 'bold',
  color: '#fff',
};

const cancelButtonStyle: React.CSSProperties = {
  padding: '4px 12px',
  border: 'none',
  borderRadius: '4px',
  backgroundColor: '#ef4444',
  color: '#fff',
  fontSize: '12px',
  cursor: 'pointer',
};

const progressContainerStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '12px',
  marginBottom: '8px',
};

const progressBarBackgroundStyle: React.CSSProperties = {
  flex: 1,
  height: '8px',
  backgroundColor: '#333',
  borderRadius: '4px',
  overflow: 'hidden',
};

const progressBarFillStyle: React.CSSProperties = {
  height: '100%',
  backgroundColor: '#3b82f6',
  transition: 'width 0.3s ease-out',
};

const percentageStyle: React.CSSProperties = {
  fontSize: '14px',
  fontWeight: 'bold',
  color: '#fff',
  minWidth: '40px',
  textAlign: 'right',
};

const operationStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#888',
};

const etaStyle: React.CSSProperties = {
  fontSize: '11px',
  color: '#666',
  marginTop: '4px',
};
```

---

Part 5: Failure Recovery and Error Handling

5.1 Error Categories

Define clear error categories for user-friendly messages:

```rust
// src-tauri/src/export/errors.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportErrorCategory {
    /// FFmpeg not found or failed to start
    ToolMissing,
    /// Input file not found or unreadable
    InputNotFound,
    /// Output path not writable
    OutputWriteError,
    /// FFmpeg encoding error
    EncodingError,
    /// Out of disk space
    DiskFull,
    /// Export was cancelled by user
    Cancelled,
    /// Process timed out
    Timeout,
    /// Unknown error
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub category: ExportErrorCategory,
    pub message: String,
    pub details: Option<String>,
    pub recoverable: bool,
}

impl ExportError {
    pub fn tool_missing(tool: &str) -> Self {
        Self {
            category: ExportErrorCategory::ToolMissing,
            message: format!("{} not found. Please reinstall the application.", tool),
            details: None,
            recoverable: false,
        }
    }

    pub fn input_not_found(path: &str) -> Self {
        Self {
            category: ExportErrorCategory::InputNotFound,
            message: "One or more source clips could not be found.".to_string(),
            details: Some(format!("Missing: {}", path)),
            recoverable: false,
        }
    }

    pub fn output_write_error(path: &str, reason: &str) -> Self {
        Self {
            category: ExportErrorCategory::OutputWriteError,
            message: "Could not write output file.".to_string(),
            details: Some(format!("{}: {}", path, reason)),
            recoverable: true,
        }
    }

    pub fn encoding_error(ffmpeg_stderr: &str) -> Self {
        // Parse common FFmpeg errors for user-friendly messages
        let message = if ffmpeg_stderr.contains("No space left on device") {
            "Not enough disk space to complete export."
        } else if ffmpeg_stderr.contains("Invalid data found") {
            "One or more clips has corrupt or unsupported data."
        } else if ffmpeg_stderr.contains("does not contain any stream") {
            "One or more clips has missing video or audio streams."
        } else {
            "FFmpeg encountered an encoding error."
        };

        Self {
            category: ExportErrorCategory::EncodingError,
            message: message.to_string(),
            details: Some(truncate_stderr(ffmpeg_stderr)),
            recoverable: true,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            category: ExportErrorCategory::Cancelled,
            message: "Export was cancelled.".to_string(),
            details: None,
            recoverable: true,
        }
    }

    pub fn timeout() -> Self {
        Self {
            category: ExportErrorCategory::Timeout,
            message: "Export timed out. Try exporting fewer clips.".to_string(),
            details: None,
            recoverable: true,
        }
    }
}

fn truncate_stderr(stderr: &str) -> String {
    // Keep last 500 chars for debugging
    let max_len = 500;
    if stderr.len() > max_len {
        format!("...{}", &stderr[stderr.len() - max_len..])
    } else {
        stderr.to_string()
    }
}
```

5.2 Cleanup on Failure

When an export fails or is cancelled, clean up partial files:

```rust
// Add to src-tauri/src/export/renderer.rs

use std::fs;
use std::path::Path;

/// Clean up partial export files
pub fn cleanup_failed_export(output_path: &Path) -> Result<(), std::io::Error> {
    if output_path.exists() {
        fs::remove_file(output_path)?;
    }

    // Also remove any temp files in the same directory
    if let Some(parent) = output_path.parent() {
        let stem = output_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        for entry in fs::read_dir(parent)? {
            if let Ok(entry) = entry {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                // Clean up temp files like "output_temp_001.mp4"
                if name_str.starts_with(stem) && name_str.contains("_temp_") {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    Ok(())
}
```

5.3 Cancel Support

Implement cancellation via process kill:

```rust
// Add to src-tauri/src/export/renderer.rs

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Export job with cancellation support
pub struct ExportJob {
    pub run_id: i64,
    pub cancelled: Arc<AtomicBool>,
    child_pid: Option<u32>,
}

impl ExportJob {
    pub fn new(run_id: i64) -> Self {
        Self {
            run_id,
            cancelled: Arc::new(AtomicBool::new(false)),
            child_pid: None,
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);

        // Kill the FFmpeg process if running
        #[cfg(unix)]
        if let Some(pid) = self.child_pid {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        #[cfg(windows)]
        if let Some(pid) = self.child_pid {
            // Windows process termination
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn set_child_pid(&mut self, pid: u32) {
        self.child_pid = Some(pid);
    }
}
```

5.4 Job Logs for Debugging

Store detailed logs for debugging failed exports:

```rust
// Add to src-tauri/src/db/schema.rs

/// Add log entry for export job
pub fn add_export_log(
    conn: &Connection,
    run_id: i64,
    level: &str,
    message: &str,
) -> Result<()> {
    // Find or create associated job
    let job_id: Option<i64> = conn.query_row(
        "SELECT id FROM jobs WHERE type = 'export' AND clip_id = ?1 ORDER BY id DESC LIMIT 1",
        params![run_id],
        |row| row.get(0),
    ).optional()?;

    if let Some(job_id) = job_id {
        conn.execute(
            "INSERT INTO job_logs (job_id, level, message, created_at) VALUES (?1, ?2, ?3, datetime('now'))",
            params![job_id, level, message],
        )?;
    }

    Ok(())
}

/// Get logs for an export run
pub fn get_export_logs(
    conn: &Connection,
    run_id: i64,
    limit: i64,
) -> Result<Vec<(String, String, String)>> {
    let mut stmt = conn.prepare(
        r#"SELECT jl.level, jl.message, jl.created_at
           FROM job_logs jl
           JOIN jobs j ON jl.job_id = j.id
           WHERE j.type = 'export' AND j.clip_id = ?1
           ORDER BY jl.created_at DESC
           LIMIT ?2"#
    )?;

    let rows = stmt.query_map(params![run_id, limit], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

---

Part 6: Tauri Commands

6.1 Export History Commands

Add to `src-tauri/src/commands/exports.rs`:

```rust
use crate::db::schema;
use crate::export::errors::ExportError;
use rusqlite::params;
use tauri::{AppHandle, Manager, State};
use super::DbState;

/// Get export history with details
#[tauri::command]
pub async fn get_export_history(
    limit: i64,
    offset: i64,
    status_filter: Option<String>,
    state: State<'_, DbState>,
) -> Result<Vec<schema::ExportRunWithDetails>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let library_id: i64 = conn
        .query_row("SELECT id FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    schema::get_export_history(conn, library_id, limit, offset, status_filter.as_deref())
        .map_err(|e| e.to_string())
}

/// Get single export run details
#[tauri::command]
pub async fn get_export_run_details(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<Option<schema::ExportRunWithDetails>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Query with join for details
    let mut stmt = conn.prepare(
        r#"SELECT er.id, er.recipe_id, er.name, er.status, er.progress,
                  er.output_path, er.output_size_bytes, er.output_duration_ms,
                  er.thumbnail_path, er.total_clips, er.total_duration_ms,
                  er.error_message, er.created_at, er.completed_at,
                  ex.name, ex.mode, ex.output_preset, ex.lut_id
           FROM export_runs er
           JOIN export_recipes ex ON er.recipe_id = ex.id
           WHERE er.id = ?1"#
    ).map_err(|e| e.to_string())?;

    let result = stmt.query_row(params![run_id], |row| {
        Ok(schema::ExportRunWithDetails {
            id: row.get(0)?,
            recipe_id: row.get(1)?,
            name: row.get(2)?,
            status: row.get(3)?,
            progress: row.get(4)?,
            output_path: row.get(5)?,
            output_size_bytes: row.get(6)?,
            output_duration_ms: row.get(7)?,
            thumbnail_path: row.get(8)?,
            total_clips: row.get(9)?,
            total_duration_ms: row.get(10)?,
            error_message: row.get(11)?,
            created_at: row.get(12)?,
            completed_at: row.get(13)?,
            recipe_name: row.get(14)?,
            mode: row.get(15)?,
            output_preset: row.get(16)?,
            lut_id: row.get(17)?,
        })
    });

    match result {
        Ok(run) => Ok(Some(run)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Delete export run and its output file
#[tauri::command]
pub async fn delete_export_run(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Get output path before deletion
    let output_path: Option<String> = conn
        .query_row(
            "SELECT output_path FROM export_runs WHERE id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();

    // Get library root for path resolution
    let library_root: String = conn
        .query_row("SELECT root_path FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    // Delete from database
    conn.execute("DELETE FROM export_runs WHERE id = ?1", params![run_id])
        .map_err(|e| e.to_string())?;

    // Delete output file if exists
    if let Some(rel_path) = output_path {
        let full_path = std::path::Path::new(&library_root)
            .join(crate::constants::DADCAM_FOLDER)
            .join(&rel_path);

        if full_path.exists() {
            std::fs::remove_file(&full_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

/// Cancel an in-progress export
#[tauri::command]
pub async fn cancel_export_run(
    run_id: i64,
    state: State<'_, DbState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    // Signal cancellation to the export job manager
    let _ = app_handle.emit("export-cancel", run_id);

    // Update database status
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    conn.execute(
        "UPDATE export_runs SET status = 'cancelled', completed_at = datetime('now') WHERE id = ?1",
        params![run_id],
    ).map_err(|e| e.to_string())?;

    Ok(())
}

/// Get logs for debugging a failed export
#[tauri::command]
pub async fn get_export_logs(
    run_id: i64,
    limit: i64,
    state: State<'_, DbState>,
) -> Result<Vec<(String, String, String)>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    schema::get_export_logs(conn, run_id, limit)
        .map_err(|e| e.to_string())
}

/// Re-run an export with the same recipe settings
#[tauri::command]
pub async fn rerun_export(
    run_id: i64,
    state: State<'_, DbState>,
    app_handle: AppHandle,
) -> Result<i64, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Get the original export run details
    let original: (i64, String, i64, String, String, Option<String>, Option<String>) = conn
        .query_row(
            r#"SELECT er.recipe_id, er.name, er.library_id,
                  er.recipe_snapshot, er.inputs_snapshot, er.normalized_settings, er.luts_manifest_b3
               FROM export_runs er
               WHERE er.id = ?1"#,
            params![run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|e| format!("Export not found: {}", e))?;

    let (recipe_id, original_name, library_id, recipe_snapshot, inputs_snapshot, normalized_settings, luts_manifest_b3) = original;

    // Generate new name with timestamp
    let new_name = format!(
        "{} (rerun {})",
        original_name.trim_end_matches(|c: char| c == ')' || c.is_numeric() || c == '(' || c == ' ' || c == 'r' || c == 'e' || c == 'u' || c == 'n'),
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );

    // Create new export run entry
    conn.execute(
        r#"INSERT INTO export_runs (
                recipe_id, library_id, name,
                recipe_snapshot, inputs_snapshot,
                pipeline_version, normalized_settings, luts_manifest_b3,
                status, progress, created_at
           )
           VALUES (
                ?1, ?2, ?3,
                ?4, ?5,
                ?6, ?7, ?8,
                'pending', 0, datetime('now')
           )"#,
        params![recipe_id, library_id, new_name, recipe_snapshot, inputs_snapshot, constants::PIPELINE_VERSION, normalized_settings, luts_manifest_b3],
    ).map_err(|e| e.to_string())?;

    let new_run_id = conn.last_insert_rowid();

    // Queue the export job (same as Phase 5 export flow)
    // This triggers the job system to process the new export
    let _ = app_handle.emit("export-queued", new_run_id);

    Ok(new_run_id)
}
```

6.2 File Operation Commands

```rust
/// Open the folder containing the export
#[tauri::command]
pub async fn open_export_folder(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let output_path: Option<String> = conn
        .query_row(
            "SELECT output_path FROM export_runs WHERE id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();

    let library_root: String = conn
        .query_row("SELECT root_path FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let rel_path = output_path.ok_or("Export has no output file")?;
    let full_path = std::path::Path::new(&library_root)
        .join(crate::constants::DADCAM_FOLDER)
        .join(&rel_path);

    let folder = full_path.parent().ok_or("Invalid path")?;

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(folder)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(folder)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(folder)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Open the export file in default video player
#[tauri::command]
pub async fn open_export_file(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let output_path: Option<String> = conn
        .query_row(
            "SELECT output_path FROM export_runs WHERE id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .flatten();

    let library_root: String = conn
        .query_row("SELECT root_path FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let rel_path = output_path.ok_or("Export has no output file")?;
    let full_path = std::path::Path::new(&library_root)
        .join(crate::constants::DADCAM_FOLDER)
        .join(&rel_path);

    if !full_path.exists() {
        return Err("Export file not found".to_string());
    }

    opener::open(&full_path).map_err(|e| e.to_string())
}

/// Reveal file in system file manager (select it)
#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<(), String> {
    let path = std::path::Path::new(&path);

    if !path.exists() {
        return Err("File not found".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", path.to_str().unwrap()])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .args(["/select,", path.to_str().unwrap()])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        // Linux doesn't have a standard "reveal" command
        // Open the containing folder instead
        if let Some(parent) = path.parent() {
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}
```

6.3 Register Commands

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod clips;
pub mod tags;
pub mod library;
pub mod scoring;
pub mod exports;

pub use clips::*;
pub use tags::*;
pub use library::*;
pub use scoring::*;
pub use exports::*;
```

Update `src-tauri/src/lib.rs`:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::get_export_history,
    commands::get_export_run_details,
    commands::delete_export_run,
    commands::cancel_export_run,
    commands::get_export_logs,
    commands::rerun_export,
    commands::open_export_folder,
    commands::open_export_file,
    commands::reveal_in_finder,
])
```

6.4 Add Dependencies

Add to `src-tauri/Cargo.toml`:

```toml
[dependencies]
# ... existing ...
opener = "0.7"      # Cross-platform file opening
lazy_static = "1.4" # For regex caching in progress parser
```

---

Part 7: CLI Commands

7.1 Export History CLI

Add to `src-tauri/src/cli.rs`:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// List export history
    ExportHistory {
        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Filter by status
        #[arg(long)]
        status: Option<String>,

        /// Maximum entries to show
        #[arg(long, default_value = "20")]
        limit: i64,
    },

    /// Show export details
    ExportDetails {
        /// Export run ID
        run_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Show debug logs
        #[arg(long)]
        logs: bool,
    },

    /// Open export in file manager
    ExportOpen {
        /// Export run ID
        run_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Open folder instead of file
        #[arg(long)]
        folder: bool,
    },

    /// Re-run a previous export with same settings
    ExportRerun {
        /// Export run ID to re-run
        run_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,
    },
}

fn handle_export_history(
    library: Option<PathBuf>,
    status: Option<String>,
    limit: i64,
) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let library = schema::get_library_by_path(&conn, &library_path.to_string_lossy())?
        .ok_or_else(|| anyhow!("Library not found"))?;

    let history = schema::get_export_history(
        &conn,
        library.id,
        limit,
        0,
        status.as_deref(),
    )?;

    println!("Export History");
    println!("--------------");

    if history.is_empty() {
        println!("No exports found.");
        return Ok(());
    }

    for run in history {
        let status_indicator = match run.status.as_str() {
            "completed" => "[OK]",
            "failed" => "[FAIL]",
            "cancelled" => "[CANCELLED]",
            "rendering" => "[RUNNING]",
            "pending" => "[PENDING]",
            _ => "[?]",
        };

        let size_str = run.output_size_bytes
            .map(|b| format_file_size(b))
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:>6}  {}  {}  {}  {}",
            run.id,
            status_indicator,
            run.name,
            size_str,
            run.created_at,
        );

        if let Some(err) = run.error_message {
            println!("        Error: {}", err);
        }
    }

    Ok(())
}

fn handle_export_details(
    run_id: i64,
    library: Option<PathBuf>,
    show_logs: bool,
) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let run = schema::get_export_run(&conn, run_id)?
        .ok_or_else(|| anyhow!("Export not found: {}", run_id))?;

    println!("Export Details: {}", run.name);
    println!("--------------");
    println!("ID:       {}", run.id);
    println!("Status:   {}", run.status);
    println!("Recipe:   {} (ID: {})", run.recipe_id, run.recipe_id);
    println!("Clips:    {}", run.total_clips);
    println!("Duration: {}ms", run.total_duration_ms);
    println!("Created:  {}", run.created_at);

    if let Some(completed) = run.completed_at {
        println!("Completed: {}", completed);
    }

    if let Some(output) = run.output_path {
        println!("Output:   {}", output);
    }

    if let Some(error) = run.error_message {
        println!("\nError: {}", error);
    }

    if show_logs {
        println!("\nLogs:");
        let logs = schema::get_export_logs(&conn, run_id, 50)?;
        for (level, message, time) in logs {
            println!("  [{}] {}: {}", time, level.to_uppercase(), message);
        }
    }

    Ok(())
}

fn handle_export_open(
    run_id: i64,
    library: Option<PathBuf>,
    folder: bool,
) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let run = schema::get_export_run(&conn, run_id)?
        .ok_or_else(|| anyhow!("Export not found: {}", run_id))?;

    let rel_path = run.output_path
        .ok_or_else(|| anyhow!("Export has no output file"))?;

    let full_path = library_path
        .join(crate::constants::DADCAM_FOLDER)
        .join(&rel_path);

    if !full_path.exists() {
        return Err(anyhow!("Output file not found: {}", full_path.display()));
    }

    if folder {
        let folder = full_path.parent().ok_or_else(|| anyhow!("Invalid path"))?;
        opener::open(folder)?;
    } else {
        opener::open(&full_path)?;
    }

    Ok(())
}

fn handle_export_rerun(
    run_id: i64,
    library: Option<PathBuf>,
) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    // Get original export run
    let original = schema::get_export_run(&conn, run_id)?
        .ok_or_else(|| anyhow!("Export not found: {}", run_id))?;

    let recipe_id = original.recipe_id;

    // Generate new name with timestamp
    let new_name = format!(
        "{} (rerun {})",
        original.name,
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );

    // Create new export run entry
    conn.execute(
        r#"INSERT INTO export_runs (
                recipe_id, library_id, name,
                recipe_snapshot, inputs_snapshot,
                pipeline_version, normalized_settings, luts_manifest_b3,
                status, progress, created_at
           )
           VALUES (
                ?1, ?2, ?3,
                ?4, ?5,
                ?6, ?7, ?8,
                'pending', 0, datetime('now')
           )"#,
        params![
            recipe_id,
            original.library_id,
            new_name,
            original.recipe_snapshot.to_string(),
            original.inputs_snapshot.to_string(),
            constants::PIPELINE_VERSION,
            original.normalized_settings.to_string(),
            original.luts_manifest_b3,
        ],
    )?;

    let new_run_id = conn.last_insert_rowid();

    println!("Created new export run: {} (ID: {})", new_name, new_run_id);
    println!("Run 'dadcam export {}' to execute the export.", new_run_id);

    Ok(())
}

fn format_file_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
```

---

Part 8: Export Thumbnail Generation

8.1 Generate Thumbnail on Export Completion

When an export completes, generate a thumbnail for history display:

```rust
// Add to src-tauri/src/export/renderer.rs

use crate::constants::THUMB_QUALITY;

/// Generate thumbnail from exported video
pub fn generate_export_thumbnail(
    ffmpeg_path: &str,
    video_path: &Path,
    output_path: &Path,
) -> Result<(), String> {
    // Extract frame at 10% of duration
    let output = std::process::Command::new(ffmpeg_path)
        .args([
            "-i", video_path.to_str().unwrap(),
            "-ss", "00:00:05",  // 5 seconds in
            "-vframes", "1",
            "-vf", "scale='min(320,iw)':-1",
            "-q:v", &THUMB_QUALITY.to_string(),
            "-y",
            output_path.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err("Failed to generate thumbnail".to_string());
    }

    Ok(())
}
```

8.2 Update Export Completion Flow

After successful render, generate thumbnail and update database:

```rust
// In the export job completion handler:

// Generate thumbnail
let thumb_filename = format!("{}_thumb.jpg", run_id);
let thumb_path = exports_dir.join(&thumb_filename);

if let Err(e) = generate_export_thumbnail(
    &ffmpeg_path,
    &output_path,
    &thumb_path,
) {
    // Log but don't fail - thumbnail is optional
    add_export_log(&conn, run_id, "warn", &format!("Thumbnail generation failed: {}", e))?;
}

// Get final file size
let output_size = std::fs::metadata(&output_path)
    .map(|m| m.len() as i64)
    .unwrap_or(0);

// Complete the export
complete_export_run(
    &conn,
    run_id,
    &relative_output_path,
    output_size,
    total_duration_ms,
    if thumb_path.exists() {
        Some(&format!("exports/{}", thumb_filename))
    } else {
        None
    },
)?;
```

---

Part 9: Testing Workflow

9.1 CLI Testing

```bash
# 1. Create a test recipe (from Phase 5)
dadcam recipe-create "Test Export" --mode all --library /path/to/library

# 2. Run an export
dadcam export 1 --library /path/to/library

# 3. Check export history
dadcam export-history --library /path/to/library

# 4. View export details
dadcam export-details 1 --library /path/to/library --logs

# 5. Open export in file manager
dadcam export-open 1 --library /path/to/library --folder

# 6. Re-run export
dadcam export-rerun 1 --library /path/to/library

# 7. Test cancellation (run in background, then cancel)
dadcam export 1 --library /path/to/library &
# Wait a few seconds...
# Cancel via UI or send signal
```

9.2 UI Testing Checklist

**Progress UI:**
- [ ] Progress bar updates smoothly (no jumps)
- [ ] Percentage matches actual progress
- [ ] Current operation label updates
- [ ] ETA is reasonable and updates
- [ ] Cancel button works immediately

**Export History:**
- [ ] All past exports appear in list
- [ ] Thumbnails load for completed exports
- [ ] Status badges show correct colors
- [ ] File size and duration display correctly
- [ ] Time ago displays correctly
- [ ] Filter buttons work (All/Completed/Failed)

**Actions:**
- [ ] Play button opens video in default player
- [ ] Folder button opens containing folder
- [ ] Re-run button creates new export with same settings
- [ ] Delete button removes entry and file
- [ ] Confirmation dialog appears before delete

**Error Handling:**
- [ ] Failed exports show error message
- [ ] Error message is user-friendly (not raw FFmpeg output)
- [ ] Logs are available for debugging
- [ ] Partial files are cleaned up on failure

9.3 Performance Testing

```bash
# Time progress update latency
# Should see UI updates within 250ms of FFmpeg progress

# Test with large export (100+ clips)
dadcam export 1 --library /path/to/large-library

# Monitor memory usage during export
# Should not grow unboundedly
```

---

Part 10: Verification Checklist

**Database:**
- [ ] Migration adds progress tracking columns
- [ ] Migration adds output metadata columns
- [ ] Indexes created for history queries
- [ ] Export history query returns correct data

**Progress System:**
- [ ] FFmpeg progress parser handles all line formats
- [ ] Progress events emit at correct interval
- [ ] ETA calculation is reasonable
- [ ] Events stop after completion/failure

**History UI:**
- [ ] History loads and displays correctly
- [ ] Status filter works
- [ ] Pagination works (if implemented)
- [ ] Empty state displays correctly

**File Operations:**
- [ ] Open folder works on macOS
- [ ] Open folder works on Windows
- [ ] Open folder works on Linux
- [ ] Open file works on all platforms
- [ ] Reveal in finder works

**Error Handling:**
- [ ] User-friendly error messages
- [ ] Partial file cleanup on failure
- [ ] Cancel stops FFmpeg immediately
- [ ] Logs available for debugging

**Thumbnails:**
- [ ] Generated after successful export
- [ ] Display in history list
- [ ] Graceful fallback if generation fails

---

Part 11: Deferred Items

The following are documented for future phases:

1. **Resume Partial Exports**: Save FFmpeg state to resume from crash (complex, limited FFmpeg support)

2. **Background Export Notifications**: System notifications when export completes (requires platform-specific code)

3. **Export Queue**: Multiple exports queued with priority (current: one at a time)

4. **Export Templates**: Save and share complete recipe+settings as template files

5. **Batch Export**: Export multiple recipes in sequence with summary report

6. **Cloud Upload**: Direct upload to YouTube/Vimeo/S3 after export

7. **Export Comparison**: Side-by-side comparison of different exports

---

End of Phase 6 Implementation Guide


---

# Addendum: Phase 6 to 100% (v1.1 “Trustworthy Exports”)

This addendum implements every missing “professional export system” fix:
- **Real resume** (segment-based checkpoints)
- **Defined progress math** (frames_total contract)
- **Explicit Share vs Archive presets**
- **Atomic output writes + cleanup**
- **Stalled-job detection + heartbeat**

It stays within Phase 6’s goal: **exports are first-class, reliable, and trustworthy**. fileciteturn5file0 fileciteturn5file7

---

## 0) Truth-in-Spec: “Resume” is now REAL

Phase 6 previously deferred “Resume Partial Exports”. fileciteturn5file0  
**This addendum implements resume** by rendering to **segments** and tracking segment completion in the DB.

### 0.1 Why segment rendering

FFmpeg cannot reliably “resume” a single long encode after crash. Instead:
1) render N segments (checkpointed)
2) concat segments into final output
3) if crash occurs, rerun only missing segments

This makes **resume** deterministic and robust.

---

## 1) Database: Segment Checkpoints + Heartbeat

Phase 6 already adds progress metadata columns to `export_runs`. fileciteturn4file7  
To enable resume, add segment tables + output temp fields.

### 1.1 Migration: Export segments (Phase 6.1)

Add this migration after the Phase 6 migration:

```rust
// Migration 5: Export segment checkpoints + atomic output (Phase 6.1)
r#"
-- Segment checkpoints for resumable exports
CREATE TABLE IF NOT EXISTS export_run_segments (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES export_runs(id) ON DELETE CASCADE,
  segment_index INTEGER NOT NULL,          -- 0..N-1
  clip_count INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  frames_total INTEGER NOT NULL,
  frames_completed INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending','rendering','completed','failed','cancelled')),
  tmp_output_path TEXT NOT NULL,           -- .dadcam/exports/tmp/run_{id}_seg_{i}.mp4
  final_output_path TEXT NOT NULL,         -- same as tmp, but without ".tmp" if you want two-stage
  error_message TEXT,
  started_at TEXT,
  completed_at TEXT,
  UNIQUE(run_id, segment_index)
);

-- Atomic output + recovery helpers
ALTER TABLE export_runs ADD COLUMN tmp_output_path TEXT;    -- .dadcam/exports/tmp/run_{id}.mp4
ALTER TABLE export_runs ADD COLUMN resumed_from_run_id INTEGER REFERENCES export_runs(id) ON DELETE SET NULL;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_export_segments_run ON export_run_segments(run_id);
CREATE INDEX IF NOT EXISTS idx_export_segments_status ON export_run_segments(status);
"#,
```

**Notes**
- `tmp_output_path` on the run is where the final “assembled export” is rendered before rename.
- `resumed_from_run_id` allows “Resume” to create a new run that reuses completed segments from the previous failed run (optional but very useful).

---

## 2) Progress System: Define the math (frames_total contract)

Phase 6 already computes progress percent from `frames_completed / frames_total`. fileciteturn5file7  
But `frames_total` must be defined or progress is fake.

### 2.1 Contract

Pick a **single target FPS per preset** and compute:

```
frames_total_run = sum(frames_total_segment)
frames_total_segment = ceil(segment_duration_seconds * target_fps)
```

Where:
- `segment_duration_seconds` is the sum of durations of the clip portions used in that segment
- for trimmed pacing, use the trimmed duration
- for crossfades, subtract transition overlap where applicable (Phase 5’s transition_duration_ms) so totals don’t drift.

### 2.2 Source for `frames_completed`

Parse FFmpeg lines:
`frame= ... time= ... speed= ...` fileciteturn4file7  
Update `export_run_segments.frames_completed` for the segment currently encoding, then roll up:

```
frames_completed_run =
  sum(completed segments frames_total) +
  current segment frames_completed
```

This gives smooth progress and correct ETA.

---

## 3) Presets: Share vs Archive are now fully specified

Phase 6 UI already labels presets “Share (H.264)” and “Archive (ProRes)”. fileciteturn5file2  
Now we specify the exact FFmpeg profiles.

### 3.1 Share preset (H.264, fast + small)

**Intent:** easy sharing, hardware decode everywhere.

Recommended encoding:
```bash
-c:v libx264 -preset medium -crf 20 -pix_fmt yuv420p
-c:a aac -b:a 160k
-movflags +faststart
```

Suggested `target_fps` for progress: **30** (or keep source fps if you already standardize in Phase 5).

### 3.2 Archive preset (ProRes 422 HQ, edit-friendly)

**Intent:** high quality, resilient to re-encode.

Recommended encoding:
```bash
-c:v prores_ks -profile:v 3 -pix_fmt yuv422p10le
-c:a pcm_s16le
```

Container:
- `.mov` is typical for ProRes
- or `.mkv` if you prefer, but `.mov` is the expectation.

Suggested `target_fps` for progress: **30** (or source fps if maintained).

### 3.3 Store the preset in `recipe_snapshot` (reproducibility)

Ensure `output_preset` is present in `recipe_snapshot` (Phase 5 already snapshots recipe settings). fileciteturn5file4

---

## 4) Atomic writes + cleanup (no corrupt partials)

Phase 6 checklist already includes “Partial files are cleaned up on failure.” fileciteturn5file0  
Make it a hard requirement using the same proven pattern from Phase 2. fileciteturn5file6

### 4.1 Run-level atomic output

Rules:
1) Render final file to: `.dadcam/exports/tmp/run_{id}.{ext}.tmp`
2) Verify file exists and `size_bytes > 0`
3) Rename atomically to: `.dadcam/exports/run_{id}.{ext}`
4) On failure/cancel: delete `.tmp`

### 4.2 Segment-level atomic output

Segments follow the same:
- render to `seg_{i}.{ext}.tmp`
- rename to `seg_{i}.{ext}` on success
- delete on failure

This ensures no corrupt segments are ever treated as “complete”.

---

## 5) Real Resume: Segment pipeline (implementation plan)

### 5.1 Segment sizing policy

Policy for v1.1:
- **Max segment duration:** 3–5 minutes (choose 3 min for safety)
- Or **max clips per segment:** e.g., 30 clips
- Segment boundaries must align on clip boundaries (don’t split a clip mid-way for v1)

### 5.2 Render flow

When an export run starts:
1) Build `export_run_items` (already Phase 5) fileciteturn5file4
2) Partition items into segments using policy above
3) Insert `export_run_segments` rows (pending) with tmp paths + frames_total
4) Start encoding segments sequentially via jobs:
   - update segment status: `pending -> rendering -> completed`
5) After all segments completed:
   - concat segments into final run tmp output
   - rename to final
   - compute size/duration + thumbnail

### 5.3 Resume behavior

On “Resume”:
- If a run failed, user clicks Resume:
  - Create **new run** with `resumed_from_run_id=old_id`
  - Reuse completed segment files from old run if:
    - `recipe_snapshot` matches exactly
    - segment partitioning policy matches (same boundaries)
    - all referenced segment files exist and are non-zero size
  - Only render missing segments, then assemble final.

If you want simpler v1:
- Resume in-place by reusing existing segment rows for the same run_id and continuing pending/failed segments.
- New run is nicer for history, but not required.

---

## 6) Stalled-job detection + heartbeat (no infinite spinner)

Phase 6 already adds `last_progress_at`. fileciteturn4file7

### 6.1 Heartbeat rule

While status is `rendering`:
- every progress event update `last_progress_at` (already in `update_export_progress`) fileciteturn4file7
- set a watchdog:

**If** `now - last_progress_at > 60s` (configurable) **then**
- mark run failed with error `"Export stalled (no progress updates for 60s)"`
- kill ffmpeg process if still alive
- cleanup tmp file for the current segment

### 6.2 UI behavior

If an export is `rendering` but `last_progress_at` is stale:
- show badge: “Stalled”
- show action: “Retry” and “View logs”

---

## 7) Update “Deferred Items” (Phase 6 is now honest)

Remove (or mark done):
- “Resume Partial Exports” is no longer deferred.

Keep deferred:
- background notifications
- export queue / multi-export priority
- template sharing
- batch export
- cloud upload

---

# Phase 6 Verification Checklist (100% Definition)

**Database**
- [ ] Phase 6 migration adds progress + metadata columns fileciteturn4file7
- [ ] Migration 5 adds export_run_segments + tmp_output_path + indexes
- [ ] Rollup progress queries work at scale (1000+ segments)

**Progress + ETA**
- [ ] frames_total contract implemented (preset target_fps)
- [ ] Progress percent matches frames_completed/frames_total
- [ ] ETA uses frame rate and updates smoothly fileciteturn5file7
- [ ] UI updates within 250ms of FFmpeg output fileciteturn5file0

**Atomic output**
- [ ] Segments write temp then rename
- [ ] Final output writes temp then rename
- [ ] Failed/cancelled cleans up temp files fileciteturn5file6

**Resume**
- [ ] Cancel stops current segment immediately
- [ ] Resume continues remaining segments without redoing completed ones
- [ ] Resume/Retry produces identical final output for same snapshot

**Presets**
- [ ] Share preset renders H.264 + AAC with faststart
- [ ] Archive preset renders ProRes 422 HQ + PCM audio
- [ ] Preset choice stored in recipe_snapshot for reproducibility

**Stalled detection**
- [ ] If no progress updates for 60s, run fails with clear message
- [ ] UI reflects “stalled” status and offers retry

---

End of Addendum
