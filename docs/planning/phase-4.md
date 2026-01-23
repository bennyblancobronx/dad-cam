Dad Cam - Phase 4 Implementation Guide

Version: 1.0
Target Audience: Developers new to video analysis

---

Overview

Phase 4 builds the scoring engine. Dad Cam finds "best moments" automatically using heuristics - no ML required. The system analyzes each clip for scene density, audio quality, sharpness, and motion, then produces a composite score.

When complete, you can:
- Run scoring jobs on all clips from CLI
- See each clip's overall score (0-1) and component breakdown
- View "Best Clips" filtered by threshold slider
- Promote or demote clips manually (user override)
- Regenerate scores when pipeline version changes

Prerequisites:
- Phase 1, 2, and 3 complete and working
- Test library with ingested clips that have proxies
- Understanding of Phase 1 job system
- FFmpeg and ffprobe available via `tools.rs` resolver (from Phase 1)
- `regex` crate added to Cargo.toml (for parsing FFmpeg output)

---

What We're Building

Phase 4 adds scoring analysis to every clip:

```
Clip (video file)
    |
    v  FFmpeg/ffprobe analysis
Heuristic Scores
    |-- Scene Density (scene changes per minute)
    |-- Audio Stability (loudness range consistency)
    |-- Sharpness (blur detection inverse)
    |-- Motion (movement activity level)
    |
    v  Weighted combination
Overall Score (0.0 - 1.0)
    |
    v  User can override
Final Display Score
```

Each component:
- Scene Density: Clips with interesting scene changes score higher
- Audio Stability: Consistent audio levels (not clipping, not silent) score higher
- Sharpness: In-focus footage scores higher than blurry
- Motion: Active footage scores higher than static shots

---

Part 1: Understanding the Heuristics

1.1 Scene Change Density

Measures how many scene changes occur per minute. Footage with varied content (multiple scenes, cuts, movement) typically contains more "interesting" moments.

FFmpeg provides the `scdet` filter for scene detection:

```bash
ffprobe -f lavfi -i "movie=input.mp4,scdet=t=10" \
  -show_entries frame_tags=lavfi.scd.score \
  -of json
```

The `scdet` filter outputs:
- `lavfi.scd.score`: Scene change score for each frame (0-100)
- `lavfi.scd.time`: Timestamp when scene change detected

Scoring logic:
- Count frames where score > threshold (default 10)
- Divide by clip duration in minutes
- Normalize to 0-1 range (0-10 changes/min maps to 0-1)

1.2 Audio Loudness Stability

Measures consistency of audio levels using EBU R128 loudness analysis. Good audio has consistent perceived loudness without clipping or silence gaps.

FFmpeg provides the `ebur128` filter for loudness analysis:

```bash
ffmpeg -i input.mp4 \
  -af "ebur128=peak=true:framelog=verbose" \
  -f null - 2>&1 | grep -E "(I:|LRA:|True peak)"
```

Key metrics:
- `I` (Integrated Loudness): Overall loudness in LUFS
- `LRA` (Loudness Range): Dynamic range in LU
- `True peak`: Maximum sample value in dBTP

Scoring logic:
- Penalize clips with no audio (score 0.3)
- Penalize extreme loudness (< -30 LUFS or > -10 LUFS)
- Reward moderate LRA (4-12 LU is typical for good content)
- Penalize true peaks above -1 dBTP (clipping risk)

1.3 Sharpness (Blur Detection)

Measures how in-focus the footage is. Sharp footage indicates intentional filming; blurry footage often means camera shake or focus issues.

FFmpeg provides the `blurdetect` filter:

```bash
ffprobe -f lavfi -i "movie=input.mp4,blurdetect" \
  -show_entries frame_tags=lavfi.blur \
  -of json
```

The filter outputs a blur value per frame:
- Higher values = more blur
- Lower values = sharper image

Scoring logic:
- Sample frames at regular intervals (e.g., every 2 seconds)
- Average the blur values
- Invert and normalize: sharpness = 1.0 - (avg_blur / max_blur)

1.4 Motion Detection

Measures movement activity. Active footage with people moving, camera panning, or action typically contains more interesting moments than static shots.

We use frame differencing with the `blend` filter:

```bash
ffprobe -f lavfi \
  -i "movie=input.mp4,tblend=all_mode=difference,blackframe=amount=98" \
  -show_entries frame_tags=lavfi.blackframe.pblack \
  -of json
```

Alternative: Use `mestimate` filter metadata for motion vectors.

Scoring logic:
- Calculate average frame difference
- Higher difference = more motion
- Normalize to 0-1 range

---

Part 2: Database Schema

2.1 Add Scoring Tables (Migration)

Add this migration to `src-tauri/src/db/migrations.rs`:

```rust
// Add to MIGRATIONS array:

// Migration 2: Scoring tables
r#"
-- Clip scores table (machine-generated)
CREATE TABLE clip_scores (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    clip_id INTEGER NOT NULL UNIQUE REFERENCES clips(id) ON DELETE CASCADE,

    -- Overall score (weighted combination)
    overall_score REAL NOT NULL CHECK (overall_score >= 0 AND overall_score <= 1),

    -- Component scores (each 0-1)
    scene_score REAL NOT NULL DEFAULT 0,
    audio_score REAL NOT NULL DEFAULT 0,
    sharpness_score REAL NOT NULL DEFAULT 0,
    motion_score REAL NOT NULL DEFAULT 0,

    -- Reasons array (JSON)
    reasons TEXT NOT NULL DEFAULT '[]',

    -- Versioning for invalidation
    pipeline_version INTEGER NOT NULL,
    scoring_version INTEGER NOT NULL DEFAULT 1,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- User score overrides (human preference)
CREATE TABLE clip_score_overrides (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    clip_id INTEGER NOT NULL UNIQUE REFERENCES clips(id) ON DELETE CASCADE,

    -- Override type: 'promote' adds to score, 'demote' subtracts, 'pin' sets exact
    override_type TEXT NOT NULL CHECK (override_type IN ('promote', 'demote', 'pin')),

    -- For 'pin' type, this is the exact score. For promote/demote, this is the adjustment.
    override_value REAL NOT NULL DEFAULT 0.2,

    -- Optional note from user
    note TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Indexes
CREATE INDEX idx_clip_scores_overall ON clip_scores(overall_score DESC);
CREATE INDEX idx_clip_scores_clip ON clip_scores(clip_id);
CREATE INDEX idx_clip_scores_version ON clip_scores(pipeline_version);
CREATE INDEX idx_clip_score_overrides_clip ON clip_score_overrides(clip_id);
"#,
```

2.2 Schema Design Notes

**clip_scores table:**
- One row per clip (unique constraint on clip_id)
- Stores both overall and component scores for transparency
- `reasons` is a JSON array explaining why the score is what it is
- `pipeline_version` matches the global constant for invalidation
- `scoring_version` allows independent scoring algorithm updates

**clip_score_overrides table:**
- Separate table keeps machine scores pure
- Three override types:
  - `promote`: Add 0.2 (default) to machine score
  - `demote`: Subtract 0.2 from machine score
  - `pin`: Set exact score (for "always include" or "never include")
- User overrides survive re-scoring

2.3 Schema Query Helpers

Add to `src-tauri/src/db/schema.rs`:

```rust
// ----- Clip Scores -----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipScore {
    pub id: i64,
    pub clip_id: i64,
    pub overall_score: f64,
    pub scene_score: f64,
    pub audio_score: f64,
    pub sharpness_score: f64,
    pub motion_score: f64,
    pub reasons: Vec<String>,
    pub pipeline_version: u32,
    pub scoring_version: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipScoreOverride {
    pub id: i64,
    pub clip_id: i64,
    pub override_type: String,
    pub override_value: f64,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Insert or update a clip score
pub fn upsert_clip_score(
    conn: &Connection,
    clip_id: i64,
    overall: f64,
    scene: f64,
    audio: f64,
    sharpness: f64,
    motion: f64,
    reasons: &[String],
    pipeline_version: u32,
    scoring_version: u32,
) -> Result<i64> {
    let reasons_json = serde_json::to_string(reasons)?;

    conn.execute(
        r#"INSERT INTO clip_scores
           (clip_id, overall_score, scene_score, audio_score, sharpness_score,
            motion_score, reasons, pipeline_version, scoring_version, updated_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))
           ON CONFLICT(clip_id) DO UPDATE SET
             overall_score = excluded.overall_score,
             scene_score = excluded.scene_score,
             audio_score = excluded.audio_score,
             sharpness_score = excluded.sharpness_score,
             motion_score = excluded.motion_score,
             reasons = excluded.reasons,
             pipeline_version = excluded.pipeline_version,
             scoring_version = excluded.scoring_version,
             updated_at = datetime('now')"#,
        params![clip_id, overall, scene, audio, sharpness, motion,
                reasons_json, pipeline_version, scoring_version],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get clip score by clip ID
pub fn get_clip_score(conn: &Connection, clip_id: i64) -> Result<Option<ClipScore>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, clip_id, overall_score, scene_score, audio_score,
                  sharpness_score, motion_score, reasons, pipeline_version,
                  scoring_version, created_at, updated_at
           FROM clip_scores WHERE clip_id = ?1"#
    )?;

    let result = stmt.query_row(params![clip_id], |row| {
        let reasons_json: String = row.get(7)?;
        let reasons: Vec<String> = serde_json::from_str(&reasons_json).unwrap_or_default();

        Ok(ClipScore {
            id: row.get(0)?,
            clip_id: row.get(1)?,
            overall_score: row.get(2)?,
            scene_score: row.get(3)?,
            audio_score: row.get(4)?,
            sharpness_score: row.get(5)?,
            motion_score: row.get(6)?,
            reasons,
            pipeline_version: row.get(8)?,
            scoring_version: row.get(9)?,
            created_at: row.get(10)?,
            updated_at: row.get(11)?,
        })
    });

    match result {
        Ok(score) => Ok(Some(score)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Get effective score (machine score adjusted by user override)
pub fn get_effective_score(conn: &Connection, clip_id: i64) -> Result<Option<f64>> {
    let row: Option<(f64, Option<String>, Option<f64>)> = conn.query_row(
        r#"SELECT
             cs.overall_score,
             cso.override_type,
             cso.override_value
           FROM clip_scores cs
           LEFT JOIN clip_score_overrides cso ON cs.clip_id = cso.clip_id
           WHERE cs.clip_id = ?1"#,
        params![clip_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional()?;

    match row {
        Some((machine_score, override_type, override_value)) => {
            let effective = match (override_type.as_deref(), override_value) {
                (Some("promote"), Some(v)) => (machine_score + v).min(1.0),
                (Some("demote"), Some(v)) => (machine_score - v).max(0.0),
                (Some("pin"), Some(v)) => v,
                _ => machine_score,
            };
            Ok(Some(effective))
        }
        None => Ok(None),
    }
}

/// Set or update user override
pub fn set_score_override(
    conn: &Connection,
    clip_id: i64,
    override_type: &str,
    override_value: f64,
    note: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"INSERT INTO clip_score_overrides
           (clip_id, override_type, override_value, note, updated_at)
           VALUES (?1, ?2, ?3, ?4, datetime('now'))
           ON CONFLICT(clip_id) DO UPDATE SET
             override_type = excluded.override_type,
             override_value = excluded.override_value,
             note = excluded.note,
             updated_at = datetime('now')"#,
        params![clip_id, override_type, override_value, note],
    )?;
    Ok(())
}

/// Remove user override (revert to machine score)
pub fn remove_score_override(conn: &Connection, clip_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM clip_score_overrides WHERE clip_id = ?1",
        params![clip_id],
    )?;
    Ok(())
}

/// Get clips by effective score threshold
pub fn get_clips_by_score(
    conn: &Connection,
    library_id: i64,
    min_score: f64,
    limit: i64,
    offset: i64,
) -> Result<Vec<(i64, f64)>> {
    let mut stmt = conn.prepare(
        r#"SELECT
             c.id,
             CASE
               WHEN cso.override_type = 'pin' THEN cso.override_value
               WHEN cso.override_type = 'promote' THEN MIN(cs.overall_score + cso.override_value, 1.0)
               WHEN cso.override_type = 'demote' THEN MAX(cs.overall_score - cso.override_value, 0.0)
               ELSE cs.overall_score
             END as effective_score
           FROM clips c
           JOIN clip_scores cs ON c.id = cs.clip_id
           LEFT JOIN clip_score_overrides cso ON c.id = cso.clip_id
           WHERE c.library_id = ?1
           HAVING effective_score >= ?2
           ORDER BY effective_score DESC
           LIMIT ?3 OFFSET ?4"#
    )?;

    let rows = stmt.query_map(params![library_id, min_score, limit, offset], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

---

Part 3: Dependencies and Constants

3.1 Add Required Dependency

Add to `src-tauri/Cargo.toml`:

```toml
# Add to [dependencies]
regex = "1.10"
```

The `regex` crate is used for parsing FFmpeg output (LUFS values, blur metrics, etc.).

3.2 Tool Resolution (Reminder)

All FFmpeg/ffprobe calls use the tool resolver from Phase 1 (`src-tauri/src/tools.rs`):

```rust
// These are already defined in tools.rs from Phase 1
crate::tools::ffmpeg_path()   // Returns path to bundled ffmpeg
crate::tools::ffprobe_path()  // Returns path to bundled ffprobe
```

This ensures the bundled binaries are used, not system-installed versions.

3.3 Add Scoring Constants

Add to `src-tauri/src/constants.rs`:

```rust
// Scoring engine
pub const SCORING_VERSION: u32 = 1;

// Score weights (must sum to 1.0)
pub const SCORE_WEIGHT_SCENE: f64 = 0.25;
pub const SCORE_WEIGHT_AUDIO: f64 = 0.25;
pub const SCORE_WEIGHT_SHARPNESS: f64 = 0.25;
pub const SCORE_WEIGHT_MOTION: f64 = 0.25;

// Scene detection
pub const SCENE_DETECT_THRESHOLD: f64 = 10.0;  // scdet threshold (0-100)
pub const SCENE_MAX_PER_MINUTE: f64 = 10.0;     // Normalize: 10 scenes/min = score 1.0

// Audio analysis
pub const AUDIO_TARGET_LUFS: f64 = -23.0;       // EBU R128 broadcast standard
pub const AUDIO_ACCEPTABLE_RANGE: f64 = 10.0;   // +/- 10 LUFS from target
pub const AUDIO_LRA_MIN: f64 = 4.0;             // Minimum acceptable LRA
pub const AUDIO_LRA_MAX: f64 = 15.0;            // Maximum acceptable LRA

// Blur detection
pub const BLUR_SAMPLE_INTERVAL_SECS: f64 = 2.0; // Sample every 2 seconds
pub const BLUR_MAX_VALUE: f64 = 100.0;          // Normalize blur values

// Motion detection
pub const MOTION_SAMPLE_INTERVAL_SECS: f64 = 1.0;
pub const MOTION_STATIC_THRESHOLD: f64 = 0.02;  // Below this = static shot

// User override defaults
pub const OVERRIDE_PROMOTE_DEFAULT: f64 = 0.2;
pub const OVERRIDE_DEMOTE_DEFAULT: f64 = 0.2;
```

---

Part 4: Scoring Module Implementation

4.1 Create the Scoring Module

Create `src-tauri/src/scoring/mod.rs`:

```rust
pub mod analyzer;
pub mod scene;
pub mod audio;
pub mod sharpness;
pub mod motion;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::constants::{
    SCORING_VERSION, PIPELINE_VERSION,
    SCORE_WEIGHT_SCENE, SCORE_WEIGHT_AUDIO,
    SCORE_WEIGHT_SHARPNESS, SCORE_WEIGHT_MOTION,
};

/// Complete scoring result for a clip
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoringResult {
    pub overall_score: f64,
    pub scene_score: f64,
    pub audio_score: f64,
    pub sharpness_score: f64,
    pub motion_score: f64,
    pub reasons: Vec<String>,
    pub pipeline_version: u32,
    pub scoring_version: u32,
}

impl ScoringResult {
    /// Calculate overall score from components using weights
    pub fn calculate_overall(
        scene: f64,
        audio: f64,
        sharpness: f64,
        motion: f64,
    ) -> f64 {
        let weighted =
            scene * SCORE_WEIGHT_SCENE +
            audio * SCORE_WEIGHT_AUDIO +
            sharpness * SCORE_WEIGHT_SHARPNESS +
            motion * SCORE_WEIGHT_MOTION;

        // Clamp to valid range
        weighted.max(0.0).min(1.0)
    }

    /// Build a scoring result from component scores
    pub fn from_components(
        scene: f64,
        audio: f64,
        sharpness: f64,
        motion: f64,
        mut reasons: Vec<String>,
    ) -> Self {
        let overall = Self::calculate_overall(scene, audio, sharpness, motion);

        // Add summary reason based on overall score
        if overall >= 0.8 {
            reasons.push("High quality clip with good visual and audio characteristics".to_string());
        } else if overall >= 0.5 {
            reasons.push("Moderate quality clip".to_string());
        } else if overall >= 0.3 {
            reasons.push("Below average clip quality".to_string());
        } else {
            reasons.push("Low quality or static clip".to_string());
        }

        Self {
            overall_score: overall,
            scene_score: scene,
            audio_score: audio,
            sharpness_score: sharpness,
            motion_score: motion,
            reasons,
            pipeline_version: PIPELINE_VERSION,
            scoring_version: SCORING_VERSION,
        }
    }
}

/// Check if a clip score needs regeneration
pub fn is_score_stale(
    existing_pipeline_version: u32,
    existing_scoring_version: u32,
) -> bool {
    existing_pipeline_version < PIPELINE_VERSION ||
    existing_scoring_version < SCORING_VERSION
}
```

4.2 Create the Analyzer Module

Create `src-tauri/src/scoring/analyzer.rs`:

```rust
use std::path::Path;
use anyhow::{Result, anyhow};

use super::{ScoringResult, scene, audio, sharpness, motion};
use crate::metadata::ffprobe::MediaInfo;

/// Analyze a clip and produce scoring result
pub fn analyze_clip(
    source_path: &Path,
    media_info: &MediaInfo,
) -> Result<ScoringResult> {
    let mut reasons = Vec::new();

    // Get duration for time-based calculations
    let duration_secs = media_info.duration_ms
        .map(|ms| ms as f64 / 1000.0)
        .unwrap_or(0.0);

    if duration_secs < 1.0 {
        return Ok(ScoringResult::from_components(
            0.0, 0.0, 0.0, 0.0,
            vec!["Clip too short for analysis".to_string()],
        ));
    }

    // Analyze each component
    let scene_result = scene::analyze_scenes(source_path, duration_secs);
    let scene_score = match scene_result {
        Ok((score, reason)) => {
            reasons.push(reason);
            score
        }
        Err(e) => {
            reasons.push(format!("Scene analysis failed: {}", e));
            0.5 // Default to neutral on failure
        }
    };

    let audio_result = audio::analyze_audio(source_path, media_info.has_audio);
    let audio_score = match audio_result {
        Ok((score, reason)) => {
            reasons.push(reason);
            score
        }
        Err(e) => {
            reasons.push(format!("Audio analysis failed: {}", e));
            0.3 // Default to low on failure (no audio is common)
        }
    };

    let sharpness_result = sharpness::analyze_sharpness(source_path, duration_secs);
    let sharpness_score = match sharpness_result {
        Ok((score, reason)) => {
            reasons.push(reason);
            score
        }
        Err(e) => {
            reasons.push(format!("Sharpness analysis failed: {}", e));
            0.5
        }
    };

    let motion_result = motion::analyze_motion(source_path, duration_secs);
    let motion_score = match motion_result {
        Ok((score, reason)) => {
            reasons.push(reason);
            score
        }
        Err(e) => {
            reasons.push(format!("Motion analysis failed: {}", e));
            0.5
        }
    };

    Ok(ScoringResult::from_components(
        scene_score,
        audio_score,
        sharpness_score,
        motion_score,
        reasons,
    ))
}

/// Analyze only specific components (for re-analysis)
pub fn analyze_component(
    source_path: &Path,
    component: &str,
    duration_secs: f64,
    has_audio: bool,
) -> Result<(f64, String)> {
    match component {
        "scene" => scene::analyze_scenes(source_path, duration_secs),
        "audio" => audio::analyze_audio(source_path, has_audio),
        "sharpness" => sharpness::analyze_sharpness(source_path, duration_secs),
        "motion" => motion::analyze_motion(source_path, duration_secs),
        _ => Err(anyhow!("Unknown component: {}", component)),
    }
}
```

4.3 Create Scene Analysis Module

Create `src-tauri/src/scoring/scene.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};
use serde::Deserialize;

use crate::constants::{SCENE_DETECT_THRESHOLD, SCENE_MAX_PER_MINUTE};

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

/// Analyze scene changes in a video
/// Returns (score 0-1, reason string)
pub fn analyze_scenes(source_path: &Path, duration_secs: f64) -> Result<(f64, String)> {
    if duration_secs < 1.0 {
        return Ok((0.0, "Clip too short for scene analysis".to_string()));
    }

    // Run ffprobe with scdet filter
    let filter = format!("movie={},scdet=t={}",
        source_path.to_string_lossy(),
        SCENE_DETECT_THRESHOLD
    );

    let output = Command::new(crate::tools::ffprobe_path())
        .args([
            "-f", "lavfi",
            "-i", &filter,
            "-show_entries", "frame_tags=lavfi.scd.score",
            "-of", "json",
            "-v", "quiet",
        ])
        .output()?;

    if !output.status.success() {
        // Fallback: try alternative method using select filter
        return analyze_scenes_fallback(source_path, duration_secs);
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
                .map(|score| score > SCENE_DETECT_THRESHOLD)
                .unwrap_or(false)
        })
        .count();

    let duration_mins = duration_secs / 60.0;
    let scenes_per_min = if duration_mins > 0.0 {
        scene_count as f64 / duration_mins
    } else {
        0.0
    };

    // Normalize to 0-1 (10 scenes/min = 1.0)
    let score = (scenes_per_min / SCENE_MAX_PER_MINUTE).min(1.0);

    let reason = format!(
        "Scene density: {:.1} changes/min ({} total scenes)",
        scenes_per_min, scene_count
    );

    Ok((score, reason))
}

/// Fallback method using select filter for scene detection
fn analyze_scenes_fallback(source_path: &Path, duration_secs: f64) -> Result<(f64, String)> {
    // Use select filter with scene detection expression
    let output = Command::new(crate::tools::ffmpeg_path())
        .args([
            "-i", source_path.to_str().unwrap(),
            "-vf", "select='gt(scene,0.3)',showinfo",
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Count "pts_time" occurrences in showinfo output
    let scene_count = stderr.matches("pts_time:").count();

    let duration_mins = duration_secs / 60.0;
    let scenes_per_min = if duration_mins > 0.0 {
        scene_count as f64 / duration_mins
    } else {
        0.0
    };

    let score = (scenes_per_min / SCENE_MAX_PER_MINUTE).min(1.0);

    let reason = format!(
        "Scene density (fallback): {:.1} changes/min ({} detected)",
        scenes_per_min, scene_count
    );

    Ok((score, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_calculation() {
        // 5 scenes per minute should give 0.5 score
        let scenes_per_min = 5.0;
        let score = (scenes_per_min / SCENE_MAX_PER_MINUTE).min(1.0);
        assert!((score - 0.5).abs() < 0.01);
    }
}
```

4.4 Create Audio Analysis Module

Create `src-tauri/src/scoring/audio.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::Result;
use regex::Regex;

use crate::constants::{
    AUDIO_TARGET_LUFS, AUDIO_ACCEPTABLE_RANGE,
    AUDIO_LRA_MIN, AUDIO_LRA_MAX,
};

/// Analyze audio loudness using EBU R128
/// Returns (score 0-1, reason string)
pub fn analyze_audio(source_path: &Path, has_audio: bool) -> Result<(f64, String)> {
    if !has_audio {
        return Ok((0.3, "No audio track present".to_string()));
    }

    // Run ebur128 filter for loudness analysis
    let output = Command::new(crate::tools::ffmpeg_path())
        .args([
            "-i", source_path.to_str().unwrap(),
            "-af", "ebur128=peak=true",
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse integrated loudness (I:)
    let integrated_lufs = parse_lufs_value(&stderr, r"I:\s+(-?\d+\.?\d*)\s+LUFS");

    // Parse loudness range (LRA:)
    let lra = parse_lufs_value(&stderr, r"LRA:\s+(\d+\.?\d*)\s+LU");

    // Parse true peak
    let true_peak = parse_lufs_value(&stderr, r"True peak:\s+(-?\d+\.?\d*)\s+dBTP");

    // Calculate score based on metrics
    let (score, reasons) = calculate_audio_score(integrated_lufs, lra, true_peak);

    Ok((score, reasons))
}

fn parse_lufs_value(text: &str, pattern: &str) -> Option<f64> {
    let re = Regex::new(pattern).ok()?;
    re.captures(text)?
        .get(1)?
        .as_str()
        .parse::<f64>()
        .ok()
}

fn calculate_audio_score(
    integrated: Option<f64>,
    lra: Option<f64>,
    true_peak: Option<f64>,
) -> (f64, String) {
    let mut score = 1.0;
    let mut reasons = Vec::new();

    // Score integrated loudness
    if let Some(i) = integrated {
        let distance_from_target = (i - AUDIO_TARGET_LUFS).abs();
        if distance_from_target <= AUDIO_ACCEPTABLE_RANGE {
            // Good loudness range
            reasons.push(format!("Good loudness: {:.1} LUFS", i));
        } else {
            // Penalize extreme loudness
            let penalty = (distance_from_target - AUDIO_ACCEPTABLE_RANGE) / 20.0;
            score -= penalty.min(0.3);

            if i < -35.0 {
                reasons.push(format!("Very quiet audio: {:.1} LUFS", i));
            } else if i > -10.0 {
                reasons.push(format!("Very loud audio: {:.1} LUFS", i));
            }
        }
    } else {
        score -= 0.2;
        reasons.push("Could not measure loudness".to_string());
    }

    // Score LRA (loudness range)
    if let Some(range) = lra {
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

    // Score true peak
    if let Some(peak) = true_peak {
        if peak > -1.0 {
            score -= 0.15;
            reasons.push(format!("Audio clipping risk: {:.1} dBTP", peak));
        }
    }

    let final_score = score.max(0.0).min(1.0);
    let reason_str = if reasons.is_empty() {
        "Audio analysis complete".to_string()
    } else {
        reasons.join("; ")
    };

    (final_score, reason_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_score_good_loudness() {
        let (score, _) = calculate_audio_score(Some(-23.0), Some(8.0), Some(-3.0));
        assert!(score > 0.8);
    }

    #[test]
    fn test_audio_score_too_quiet() {
        let (score, reason) = calculate_audio_score(Some(-40.0), Some(8.0), Some(-10.0));
        assert!(score < 0.8);
        assert!(reason.contains("quiet"));
    }
}
```

4.5 Create Sharpness Analysis Module

Create `src-tauri/src/scoring/sharpness.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::Result;
use serde::Deserialize;

use crate::constants::{BLUR_SAMPLE_INTERVAL_SECS, BLUR_MAX_VALUE};

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

/// Analyze sharpness using blur detection
/// Returns (score 0-1, reason string) where higher = sharper
pub fn analyze_sharpness(source_path: &Path, duration_secs: f64) -> Result<(f64, String)> {
    if duration_secs < 1.0 {
        return Ok((0.5, "Clip too short for sharpness analysis".to_string()));
    }

    // Calculate sample rate to get ~30 samples max
    let sample_count = (duration_secs / BLUR_SAMPLE_INTERVAL_SECS).min(30.0) as u32;
    let fps = if sample_count > 0 {
        sample_count as f64 / duration_secs
    } else {
        0.5
    };

    // Run blurdetect filter
    let filter = format!(
        "fps={},blurdetect",
        fps
    );

    let output = Command::new(crate::tools::ffmpeg_path())
        .args([
            "-i", source_path.to_str().unwrap(),
            "-vf", &filter,
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse blur values from output
    let blur_values = parse_blur_values(&stderr);

    if blur_values.is_empty() {
        return Ok((0.5, "Could not detect blur levels".to_string()));
    }

    // Calculate average blur
    let avg_blur: f64 = blur_values.iter().sum::<f64>() / blur_values.len() as f64;

    // Convert blur to sharpness (invert and normalize)
    // Lower blur = higher sharpness
    let sharpness_score = 1.0 - (avg_blur / BLUR_MAX_VALUE).min(1.0);

    let reason = if sharpness_score >= 0.7 {
        format!("Sharp footage (blur: {:.1})", avg_blur)
    } else if sharpness_score >= 0.4 {
        format!("Moderate sharpness (blur: {:.1})", avg_blur)
    } else {
        format!("Blurry footage (blur: {:.1})", avg_blur)
    };

    Ok((sharpness_score, reason))
}

fn parse_blur_values(text: &str) -> Vec<f64> {
    let re = regex::Regex::new(r"blur:\s*(\d+\.?\d*)").unwrap();

    re.captures_iter(text)
        .filter_map(|cap| cap.get(1)?.as_str().parse::<f64>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharpness_score_calculation() {
        // Low blur (10) should give high sharpness
        let avg_blur = 10.0;
        let score = 1.0 - (avg_blur / BLUR_MAX_VALUE).min(1.0);
        assert!(score > 0.8);

        // High blur (80) should give low sharpness
        let avg_blur = 80.0;
        let score = 1.0 - (avg_blur / BLUR_MAX_VALUE).min(1.0);
        assert!(score < 0.3);
    }
}
```

4.6 Create Motion Analysis Module

Create `src-tauri/src/scoring/motion.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::Result;

use crate::constants::{MOTION_SAMPLE_INTERVAL_SECS, MOTION_STATIC_THRESHOLD};

/// Analyze motion using frame differencing
/// Returns (score 0-1, reason string) where higher = more motion
pub fn analyze_motion(source_path: &Path, duration_secs: f64) -> Result<(f64, String)> {
    if duration_secs < 2.0 {
        return Ok((0.5, "Clip too short for motion analysis".to_string()));
    }

    // Calculate sample rate
    let sample_count = (duration_secs / MOTION_SAMPLE_INTERVAL_SECS).min(60.0) as u32;
    let fps = if sample_count > 1 {
        sample_count as f64 / duration_secs
    } else {
        1.0
    };

    // Use tblend to compute frame differences, then measure blackframe percentage
    // High blackframe % after difference = low motion (frames are similar)
    let filter = format!(
        "fps={},tblend=all_mode=difference,blackframe=amount=95:threshold=24",
        fps
    );

    let output = Command::new(crate::tools::ffmpeg_path())
        .args([
            "-i", source_path.to_str().unwrap(),
            "-vf", &filter,
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse blackframe detection results
    let (static_frames, total_frames) = parse_blackframe_output(&stderr);

    if total_frames == 0 {
        // Fallback: estimate from file
        return analyze_motion_fallback(source_path);
    }

    // Calculate motion ratio (inverse of static ratio)
    let static_ratio = static_frames as f64 / total_frames as f64;
    let motion_score = 1.0 - static_ratio;

    let reason = if motion_score >= 0.7 {
        format!("High motion activity ({:.0}% active frames)", motion_score * 100.0)
    } else if motion_score >= 0.3 {
        format!("Moderate motion ({:.0}% active frames)", motion_score * 100.0)
    } else if motion_score <= MOTION_STATIC_THRESHOLD {
        "Static shot or minimal movement".to_string()
    } else {
        format!("Low motion ({:.0}% active frames)", motion_score * 100.0)
    };

    Ok((motion_score, reason))
}

fn parse_blackframe_output(text: &str) -> (u32, u32) {
    // Count blackframe detections (static frames after differencing)
    let static_frames = text.matches("blackframe:").count() as u32;

    // Estimate total frames from frame= count in progress output
    let total_frames = regex::Regex::new(r"frame=\s*(\d+)")
        .ok()
        .and_then(|re| {
            re.captures_iter(text)
                .last()
                .and_then(|cap| cap.get(1)?.as_str().parse::<u32>().ok())
        })
        .unwrap_or(0);

    (static_frames, total_frames.max(static_frames))
}

/// Fallback motion analysis using mestimate
fn analyze_motion_fallback(source_path: &Path) -> Result<(f64, String)> {
    // Use select filter with scene change detection as motion proxy
    let output = Command::new(crate::tools::ffmpeg_path())
        .args([
            "-i", source_path.to_str().unwrap(),
            "-vf", "select='gt(scene,0.1)',metadata=print:file=-",
            "-f", "null",
            "-",
        ])
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Count scene changes as proxy for motion
    let changes = stderr.matches("pts_time:").count();

    // Normalize: assume 30 changes per minute is high motion
    let score = (changes as f64 / 30.0).min(1.0);

    let reason = format!("Motion estimate (fallback): {} movement events", changes);

    Ok((score, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_score_high_activity() {
        // 20% static frames = 80% motion
        let static_ratio = 0.2;
        let motion_score = 1.0 - static_ratio;
        assert!(motion_score > 0.7);
    }

    #[test]
    fn test_motion_score_static() {
        // 95% static frames = 5% motion
        let static_ratio = 0.95;
        let motion_score = 1.0 - static_ratio;
        assert!(motion_score < 0.1);
    }
}
```

---

Part 5: Job Integration

**Important Notes:**

1. **has_audio field**: The `clips` table needs a `has_audio` column. If not present from Phase 1, add this migration:
   ```sql
   ALTER TABLE clips ADD COLUMN has_audio INTEGER DEFAULT 1;
   ```
   Update during metadata extraction in Phase 1 ingest.

2. **Job Priority**: Score jobs should run at lower priority than preview generation jobs (proxy/thumb/sprite). Use `priority = -1` for score jobs so previews complete first.

5.1 Add Score Job Handler

Add to `src-tauri/src/jobs/mod.rs`:

```rust
use crate::scoring::{analyzer, ScoringResult};
use crate::db::schema;

/// Execute a scoring job for a clip
pub fn run_score_job(
    conn: &Connection,
    clip_id: i64,
) -> Result<ScoringResult> {
    // Get clip info
    let clip = schema::get_clip(conn, clip_id)?
        .ok_or_else(|| anyhow!("Clip not found: {}", clip_id))?;

    // Choose the best scoring source (proxy-first for speed/determinism)
// - Prefer the Phase 2 H.264 proxy when available (consistent decode + fast analysis)
// - Fallback to original asset when proxy is missing
let scoring_asset = schema::get_asset_for_role(conn, clip_id, "proxy")?
    .or_else(|| schema::get_original_asset(conn, clip_id).ok().flatten())
    .ok_or_else(|| anyhow!("No asset found for clip: {}", clip_id))?;

// Get library root to resolve path
let library = schema::get_library(conn, clip.library_id)?
    .ok_or_else(|| anyhow!("Library not found"))?;

let source_path = std::path::Path::new(&library.root_path).join(&scoring_asset.path);


    // Build MediaInfo for analyzer
    let media_info = crate::metadata::ffprobe::MediaInfo {
        duration_ms: clip.duration_ms,
        width: clip.width,
        height: clip.height,
        fps: clip.fps,
        codec: clip.codec.clone(),
        has_audio: clip.has_audio.unwrap_or(false),
        is_video: clip.media_type == "video",
        is_audio_only: clip.media_type == "audio",
        is_image: clip.media_type == "image",
        ..Default::default()
    };

    // Run analysis
    let result = analyzer::analyze_clip(&source_path, &media_info)?;

    // Store result
    schema::upsert_clip_score(
        conn,
        clip_id,
        result.overall_score,
        result.scene_score,
        result.audio_score,
        result.sharpness_score,
        result.motion_score,
        &result.reasons,
        result.pipeline_version,
        result.scoring_version,
    )?;

    Ok(result)
}

/// Queue scoring jobs for all unscored clips
pub fn queue_scoring_jobs(conn: &Connection, library_id: i64) -> Result<u32> {
    let sql = r#"
        SELECT c.id FROM clips c
        LEFT JOIN clip_scores cs ON c.id = cs.clip_id
        WHERE c.library_id = ?1
        AND (cs.id IS NULL
             OR cs.pipeline_version < ?2
             OR cs.scoring_version < ?3)
    "#;

    let mut stmt = conn.prepare(sql)?;
    let clip_ids: Vec<i64> = stmt
        .query_map(
            params![library_id, PIPELINE_VERSION, SCORING_VERSION],
            |row| row.get(0),
        )?
        .filter_map(|r| r.ok())
        .collect();

    let mut queued = 0;
    for clip_id in clip_ids {
        schema::create_job(
            conn,
            "score",
            library_id,
            Some(clip_id),
            None,
            0, // normal priority
            &serde_json::json!({}),
        )?;
        queued += 1;
    }

    Ok(queued)
}
```

5.2 Update Job Runner

Update `src-tauri/src/jobs/runner.rs` to handle score jobs:

```rust
// In the job execution match statement:
match job.job_type.as_str() {
    "ingest" => run_ingest_job(conn, &job)?,
    "proxy" => run_proxy_job(conn, &job)?,
    "thumb" => run_thumb_job(conn, &job)?,
    "sprite" => run_sprite_job(conn, &job)?,
    "score" => run_score_job(conn, job.clip_id.unwrap())?,
    "hash_full" => run_hash_full_job(conn, &job)?,
    "export" => run_export_job(conn, &job)?,
    _ => return Err(anyhow!("Unknown job type: {}", job.job_type)),
}
```

---

Part 6: CLI Commands

6.1 Add Score Command

Add to `src-tauri/src/cli.rs`:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Score clips to find best moments
    Score {
        /// Library path (default: current directory)
        #[arg(long)]
        library: Option<PathBuf>,

        /// Score specific clip by ID
        #[arg(long)]
        clip: Option<i64>,

        /// Force rescore even if current
        #[arg(long)]
        force: bool,

        /// Show detailed analysis
        #[arg(long)]
        verbose: bool,
    },

    /// Show scoring status and statistics
    ScoreStatus {
        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Show only clips missing scores
        #[arg(long)]
        missing_only: bool,
    },

    /// List best clips above threshold
    BestClips {
        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Minimum score threshold (0-1, default 0.6)
        #[arg(long, default_value = "0.6")]
        threshold: f64,

        /// Maximum number of clips to show
        #[arg(long, default_value = "20")]
        limit: i64,
    },

    /// Override clip score (promote/demote/pin)
    ScoreOverride {
        /// Clip ID
        clip_id: i64,

        /// Override action
        #[arg(value_enum)]
        action: OverrideAction,

        /// Value for pin action (0-1)
        #[arg(long)]
        value: Option<f64>,

        /// Note explaining override
        #[arg(long)]
        note: Option<String>,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
enum OverrideAction {
    Promote,
    Demote,
    Pin,
    Clear,
}

fn handle_score(library: Option<PathBuf>, clip: Option<i64>, force: bool, verbose: bool) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let library = schema::get_library_by_path(&conn, &library_path.to_string_lossy())?
        .ok_or_else(|| anyhow!("Library not found"))?;

    if let Some(clip_id) = clip {
        // Score single clip
        println!("Scoring clip {}...", clip_id);
        let result = jobs::run_score_job(&conn, clip_id)?;

        println!("Overall score: {:.2}", result.overall_score);
        if verbose {
            println!("  Scene:     {:.2}", result.scene_score);
            println!("  Audio:     {:.2}", result.audio_score);
            println!("  Sharpness: {:.2}", result.sharpness_score);
            println!("  Motion:    {:.2}", result.motion_score);
            println!("Reasons:");
            for reason in &result.reasons {
                println!("  - {}", reason);
            }
        }
    } else {
        // Queue scoring jobs for all clips
        let queued = jobs::queue_scoring_jobs(&conn, library.id)?;
        println!("Queued {} scoring jobs", queued);

        // Run job processor
        jobs::process_pending_jobs(&conn)?;
    }

    Ok(())
}

fn handle_score_status(library: Option<PathBuf>, missing_only: bool) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let library = schema::get_library_by_path(&conn, &library_path.to_string_lossy())?
        .ok_or_else(|| anyhow!("Library not found"))?;

    let stats = schema::get_scoring_stats(&conn, library.id)?;

    println!("Scoring Status");
    println!("--------------");
    println!("Total clips:    {}", stats.total_clips);
    println!("Scored:         {}", stats.scored_clips);
    println!("Missing scores: {}", stats.total_clips - stats.scored_clips);
    println!("Stale scores:   {}", stats.stale_scores);
    println!("User overrides: {}", stats.override_count);
    println!();
    println!("Score distribution:");
    println!("  Excellent (0.8+): {}", stats.excellent_count);
    println!("  Good (0.6-0.8):   {}", stats.good_count);
    println!("  Fair (0.4-0.6):   {}", stats.fair_count);
    println!("  Poor (<0.4):      {}", stats.poor_count);

    Ok(())
}

fn handle_best_clips(library: Option<PathBuf>, threshold: f64, limit: i64) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let library = schema::get_library_by_path(&conn, &library_path.to_string_lossy())?
        .ok_or_else(|| anyhow!("Library not found"))?;

    let clips = schema::get_clips_by_score(&conn, library.id, threshold, limit, 0)?;

    println!("Best Clips (threshold >= {:.2})", threshold);
    println!("----------------------------");

    for (clip_id, score) in clips {
        let clip = schema::get_clip(&conn, clip_id)?;
        if let Some(c) = clip {
            println!("{:>6}  {:.2}  {}", c.id, score, c.title);
        }
    }

    Ok(())
}

fn handle_score_override(
    clip_id: i64,
    action: OverrideAction,
    value: Option<f64>,
    note: Option<String>,
    library: Option<PathBuf>,
) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    match action {
        OverrideAction::Promote => {
            let v = value.unwrap_or(OVERRIDE_PROMOTE_DEFAULT);
            schema::set_score_override(&conn, clip_id, "promote", v, note.as_deref())?;
            println!("Promoted clip {} by {:.2}", clip_id, v);
        }
        OverrideAction::Demote => {
            let v = value.unwrap_or(OVERRIDE_DEMOTE_DEFAULT);
            schema::set_score_override(&conn, clip_id, "demote", v, note.as_deref())?;
            println!("Demoted clip {} by {:.2}", clip_id, v);
        }
        OverrideAction::Pin => {
            let v = value.ok_or_else(|| anyhow!("Pin action requires --value"))?;
            schema::set_score_override(&conn, clip_id, "pin", v, note.as_deref())?;
            println!("Pinned clip {} to score {:.2}", clip_id, v);
        }
        OverrideAction::Clear => {
            schema::remove_score_override(&conn, clip_id)?;
            println!("Cleared override for clip {}", clip_id);
        }
    }

    // Show new effective score
    if let Some(effective) = schema::get_effective_score(&conn, clip_id)? {
        println!("Effective score: {:.2}", effective);
    }

    Ok(())
}
```

---

Part 7: Tauri Commands (Frontend Integration)

7.1 Add Scoring Commands

Create `src-tauri/src/commands/scoring.rs`:

```rust
use crate::db::schema;
use crate::scoring::{analyzer, ScoringResult, is_score_stale};
use crate::constants::{PIPELINE_VERSION, SCORING_VERSION};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;
use super::DbState;

/// Score data returned to frontend
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipScoreView {
    pub clip_id: i64,
    pub overall_score: f64,
    pub effective_score: f64,
    pub scene_score: f64,
    pub audio_score: f64,
    pub sharpness_score: f64,
    pub motion_score: f64,
    pub reasons: Vec<String>,
    pub has_override: bool,
    pub override_type: Option<String>,
}

/// Get score for a clip
#[tauri::command]
pub async fn get_clip_score(
    clip_id: i64,
    state: State<'_, DbState>,
) -> Result<Option<ClipScoreView>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let score = schema::get_clip_score(conn, clip_id)
        .map_err(|e| e.to_string())?;

    let effective = schema::get_effective_score(conn, clip_id)
        .map_err(|e| e.to_string())?;

    // Check for override
    let override_info: Option<(String, f64)> = conn
        .query_row(
            "SELECT override_type, override_value FROM clip_score_overrides WHERE clip_id = ?1",
            params![clip_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    match score {
        Some(s) => Ok(Some(ClipScoreView {
            clip_id: s.clip_id,
            overall_score: s.overall_score,
            effective_score: effective.unwrap_or(s.overall_score),
            scene_score: s.scene_score,
            audio_score: s.audio_score,
            sharpness_score: s.sharpness_score,
            motion_score: s.motion_score,
            reasons: s.reasons,
            has_override: override_info.is_some(),
            override_type: override_info.map(|(t, _)| t),
        })),
        None => Ok(None),
    }
}

/// Get clips filtered by score threshold
#[tauri::command]
pub async fn get_best_clips(
    min_score: f64,
    limit: i64,
    offset: i64,
    state: State<'_, DbState>,
) -> Result<Vec<i64>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let library_id: i64 = conn
        .query_row("SELECT id FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    let clips = schema::get_clips_by_score(conn, library_id, min_score, limit, offset)
        .map_err(|e| e.to_string())?;

    Ok(clips.into_iter().map(|(id, _)| id).collect())
}

/// Set score override
#[tauri::command]
pub async fn set_clip_score_override(
    clip_id: i64,
    override_type: String,
    override_value: Option<f64>,
    note: Option<String>,
    state: State<'_, DbState>,
) -> Result<f64, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let value = override_value.unwrap_or(match override_type.as_str() {
        "promote" => crate::constants::OVERRIDE_PROMOTE_DEFAULT,
        "demote" => crate::constants::OVERRIDE_DEMOTE_DEFAULT,
        _ => 0.5,
    });

    schema::set_score_override(conn, clip_id, &override_type, value, note.as_deref())
        .map_err(|e| e.to_string())?;

    // Return new effective score
    schema::get_effective_score(conn, clip_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No score found".to_string())
}

/// Clear score override
#[tauri::command]
pub async fn clear_clip_score_override(
    clip_id: i64,
    state: State<'_, DbState>,
) -> Result<f64, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    schema::remove_score_override(conn, clip_id)
        .map_err(|e| e.to_string())?;

    // Return machine score
    schema::get_clip_score(conn, clip_id)
        .map_err(|e| e.to_string())?
        .map(|s| s.overall_score)
        .ok_or_else(|| "No score found".to_string())
}

/// Get scoring statistics
#[tauri::command]
pub async fn get_scoring_stats(
    state: State<'_, DbState>,
) -> Result<ScoringStats, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let library_id: i64 = conn
        .query_row("SELECT id FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    schema::get_scoring_stats(conn, library_id)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoringStats {
    pub total_clips: i64,
    pub scored_clips: i64,
    pub stale_scores: i64,
    pub override_count: i64,
    pub excellent_count: i64,
    pub good_count: i64,
    pub fair_count: i64,
    pub poor_count: i64,
    pub avg_score: f64,
}

// Add this to schema.rs
pub fn get_scoring_stats(conn: &Connection, library_id: i64) -> Result<ScoringStats> {
    let total_clips: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clips WHERE library_id = ?1",
        params![library_id],
        |row| row.get(0),
    )?;

    let scored_clips: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_scores cs JOIN clips c ON cs.clip_id = c.id WHERE c.library_id = ?1",
        params![library_id],
        |row| row.get(0),
    )?;

    let stale_scores: i64 = conn.query_row(
        r#"SELECT COUNT(*) FROM clip_scores cs JOIN clips c ON cs.clip_id = c.id
           WHERE c.library_id = ?1 AND (cs.pipeline_version < ?2 OR cs.scoring_version < ?3)"#,
        params![library_id, PIPELINE_VERSION, SCORING_VERSION],
        |row| row.get(0),
    )?;

    let override_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_score_overrides cso JOIN clips c ON cso.clip_id = c.id WHERE c.library_id = ?1",
        params![library_id],
        |row| row.get(0),
    )?;

    let excellent_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_scores cs JOIN clips c ON cs.clip_id = c.id WHERE c.library_id = ?1 AND cs.overall_score >= 0.8",
        params![library_id],
        |row| row.get(0),
    )?;

    let good_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_scores cs JOIN clips c ON cs.clip_id = c.id WHERE c.library_id = ?1 AND cs.overall_score >= 0.6 AND cs.overall_score < 0.8",
        params![library_id],
        |row| row.get(0),
    )?;

    let fair_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_scores cs JOIN clips c ON cs.clip_id = c.id WHERE c.library_id = ?1 AND cs.overall_score >= 0.4 AND cs.overall_score < 0.6",
        params![library_id],
        |row| row.get(0),
    )?;

    let poor_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_scores cs JOIN clips c ON cs.clip_id = c.id WHERE c.library_id = ?1 AND cs.overall_score < 0.4",
        params![library_id],
        |row| row.get(0),
    )?;

    let avg_score: f64 = conn.query_row(
        "SELECT COALESCE(AVG(cs.overall_score), 0) FROM clip_scores cs JOIN clips c ON cs.clip_id = c.id WHERE c.library_id = ?1",
        params![library_id],
        |row| row.get(0),
    )?;

    Ok(ScoringStats {
        total_clips,
        scored_clips,
        stale_scores,
        override_count,
        excellent_count,
        good_count,
        fair_count,
        poor_count,
        avg_score,
    })
}
```

7.2 Register Commands

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod clips;
pub mod tags;
pub mod library;
pub mod scoring;

pub use clips::*;
pub use tags::*;
pub use library::*;
pub use scoring::*;
```

Update `src-tauri/src/lib.rs` to register the new commands:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::get_clip_score,
    commands::get_best_clips,
    commands::set_clip_score_override,
    commands::clear_clip_score_override,
    commands::get_scoring_stats,
])
```

---

Part 8: Frontend UI Components

8.1 TypeScript Types

Add to `src/types/clips.ts`:

```typescript
export interface ClipScoreView {
  clipId: number;
  overallScore: number;
  effectiveScore: number;
  sceneScore: number;
  audioScore: number;
  sharpnessScore: number;
  motionScore: number;
  reasons: string[];
  hasOverride: boolean;
  overrideType?: 'promote' | 'demote' | 'pin';
}

export interface ScoringStats {
  totalClips: number;
  scoredClips: number;
  staleScores: number;
  overrideCount: number;
  excellentCount: number;
  goodCount: number;
  fairCount: number;
  poorCount: number;
  avgScore: number;
}
```

8.2 API Functions

Add to `src/api/clips.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';
import type { ClipScoreView, ScoringStats } from '../types/clips';

export async function getClipScore(clipId: number): Promise<ClipScoreView | null> {
  return invoke('get_clip_score', { clipId });
}

export async function getBestClips(
  minScore: number,
  limit: number,
  offset: number
): Promise<number[]> {
  return invoke('get_best_clips', { minScore, limit, offset });
}

export async function setClipScoreOverride(
  clipId: number,
  overrideType: 'promote' | 'demote' | 'pin',
  overrideValue?: number,
  note?: string
): Promise<number> {
  return invoke('set_clip_score_override', { clipId, overrideType, overrideValue, note });
}

export async function clearClipScoreOverride(clipId: number): Promise<number> {
  return invoke('clear_clip_score_override', { clipId });
}

export async function getScoringStats(): Promise<ScoringStats> {
  return invoke('get_scoring_stats');
}
```

8.3 Score Badge Component

Create `src/components/ScoreBadge.tsx`:

```typescript
import type { ClipScoreView } from '../types/clips';

interface ScoreBadgeProps {
  score: ClipScoreView;
  showDetails?: boolean;
}

export function ScoreBadge({ score, showDetails = false }: ScoreBadgeProps) {
  const percentage = Math.round(score.effectiveScore * 100);

  const getScoreColor = (value: number) => {
    if (value >= 0.8) return '#22c55e'; // green
    if (value >= 0.6) return '#84cc16'; // lime
    if (value >= 0.4) return '#eab308'; // yellow
    return '#ef4444'; // red
  };

  const color = getScoreColor(score.effectiveScore);

  return (
    <div style={containerStyle}>
      <div style={{ ...badgeStyle, backgroundColor: color }}>
        {percentage}
        {score.hasOverride && (
          <span style={overrideIndicatorStyle}>
            {score.overrideType === 'promote' ? '+' :
             score.overrideType === 'demote' ? '-' : '*'}
          </span>
        )}
      </div>

      {showDetails && (
        <div style={detailsStyle}>
          <div style={componentRowStyle}>
            <span>Scene</span>
            <div style={{ ...barStyle, width: `${score.sceneScore * 100}%` }} />
          </div>
          <div style={componentRowStyle}>
            <span>Audio</span>
            <div style={{ ...barStyle, width: `${score.audioScore * 100}%` }} />
          </div>
          <div style={componentRowStyle}>
            <span>Sharp</span>
            <div style={{ ...barStyle, width: `${score.sharpnessScore * 100}%` }} />
          </div>
          <div style={componentRowStyle}>
            <span>Motion</span>
            <div style={{ ...barStyle, width: `${score.motionScore * 100}%` }} />
          </div>
        </div>
      )}
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  alignItems: 'flex-end',
};

const badgeStyle: React.CSSProperties = {
  padding: '2px 6px',
  borderRadius: '4px',
  fontSize: '11px',
  fontWeight: 'bold',
  color: 'white',
  display: 'flex',
  alignItems: 'center',
  gap: '2px',
};

const overrideIndicatorStyle: React.CSSProperties = {
  fontSize: '10px',
  opacity: 0.8,
};

const detailsStyle: React.CSSProperties = {
  marginTop: '4px',
  padding: '4px',
  backgroundColor: '#222',
  borderRadius: '4px',
  fontSize: '10px',
  width: '80px',
};

const componentRowStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  marginBottom: '2px',
};

const barStyle: React.CSSProperties = {
  height: '4px',
  backgroundColor: '#4a9eff',
  borderRadius: '2px',
  maxWidth: '40px',
};
```

8.4 Best Clips View Component

Create `src/components/BestClipsView.tsx`:

```typescript
import { useState, useEffect, useCallback } from 'react';
import type { ClipView, ClipScoreView } from '../types/clips';
import { getClips, getClipScore, getBestClips } from '../api/clips';
import { ClipGrid } from './ClipGrid';
import { ThresholdSlider } from './ThresholdSlider';

interface BestClipsViewProps {
  onClipClick: (clip: ClipView) => void;
}

export function BestClipsView({ onClipClick }: BestClipsViewProps) {
  const [threshold, setThreshold] = useState(0.6);
  const [clips, setClips] = useState<ClipView[]>([]);
  const [scores, setScores] = useState<Map<number, ClipScoreView>>(new Map());
  const [isLoading, setIsLoading] = useState(false);

  const loadBestClips = useCallback(async () => {
    setIsLoading(true);
    try {
      // Get clip IDs that meet threshold
      const clipIds = await getBestClips(threshold, 100, 0);

      // Load clip data and scores
      const response = await getClips({
        offset: 0,
        limit: 100,
        filter: 'all',
      });

      // Filter to only clips in our best list, preserve order
      const clipMap = new Map(response.clips.map(c => [c.id, c]));
      const orderedClips = clipIds
        .map(id => clipMap.get(id))
        .filter((c): c is ClipView => c !== undefined);

      setClips(orderedClips);

      // Load scores for display
      const scoreMap = new Map<number, ClipScoreView>();
      for (const clipId of clipIds) {
        const score = await getClipScore(clipId);
        if (score) {
          scoreMap.set(clipId, score);
        }
      }
      setScores(scoreMap);

    } catch (err) {
      console.error('Failed to load best clips:', err);
    } finally {
      setIsLoading(false);
    }
  }, [threshold]);

  useEffect(() => {
    loadBestClips();
  }, [loadBestClips]);

  return (
    <div style={containerStyle}>
      <div style={headerStyle}>
        <h2 style={titleStyle}>Best Clips</h2>
        <ThresholdSlider
          value={threshold}
          onChange={setThreshold}
          min={0}
          max={1}
          step={0.05}
        />
        <span style={countStyle}>
          {clips.length} clips ({Math.round(threshold * 100)}%+ score)
        </span>
      </div>

      {isLoading ? (
        <div style={loadingStyle}>Loading best clips...</div>
      ) : clips.length === 0 ? (
        <div style={emptyStyle}>
          No clips meet the current threshold. Try lowering it.
        </div>
      ) : (
        <ClipGrid
          clips={clips}
          onClipClick={onClipClick}
          onLoadMore={() => {}}
          hasMore={false}
          scores={scores}
        />
      )}
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '16px',
  padding: '12px 16px',
  backgroundColor: '#2a2a2a',
  borderBottom: '1px solid #333',
};

const titleStyle: React.CSSProperties = {
  margin: 0,
  fontSize: '16px',
  fontWeight: 'bold',
};

const countStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#888',
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
```

8.5 Threshold Slider Component

Create `src/components/ThresholdSlider.tsx`:

```typescript
interface ThresholdSliderProps {
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
}

export function ThresholdSlider({
  value,
  onChange,
  min = 0,
  max = 1,
  step = 0.05,
}: ThresholdSliderProps) {
  return (
    <div style={containerStyle}>
      <label style={labelStyle}>
        Threshold: {Math.round(value * 100)}%
      </label>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        style={sliderStyle}
      />
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
};

const labelStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#ccc',
  minWidth: '100px',
};

const sliderStyle: React.CSSProperties = {
  width: '120px',
  accentColor: '#4a9eff',
};
```

8.6 Score Override Buttons

Create `src/components/ScoreOverrideButtons.tsx`:

```typescript
import { useState } from 'react';
import type { ClipScoreView } from '../types/clips';
import { setClipScoreOverride, clearClipScoreOverride } from '../api/clips';

interface ScoreOverrideButtonsProps {
  clipId: number;
  score: ClipScoreView;
  onScoreChange: (newEffectiveScore: number) => void;
}

export function ScoreOverrideButtons({
  clipId,
  score,
  onScoreChange,
}: ScoreOverrideButtonsProps) {
  const [isLoading, setIsLoading] = useState(false);

  const handlePromote = async () => {
    setIsLoading(true);
    try {
      const newScore = await setClipScoreOverride(clipId, 'promote');
      onScoreChange(newScore);
    } catch (err) {
      console.error('Failed to promote:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleDemote = async () => {
    setIsLoading(true);
    try {
      const newScore = await setClipScoreOverride(clipId, 'demote');
      onScoreChange(newScore);
    } catch (err) {
      console.error('Failed to demote:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleClear = async () => {
    setIsLoading(true);
    try {
      const newScore = await clearClipScoreOverride(clipId);
      onScoreChange(newScore);
    } catch (err) {
      console.error('Failed to clear override:', err);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div style={containerStyle}>
      <button
        onClick={handlePromote}
        disabled={isLoading || score.overrideType === 'promote'}
        style={{
          ...buttonStyle,
          backgroundColor: score.overrideType === 'promote' ? '#22c55e' : '#333',
        }}
        title="Promote this clip (add to score)"
      >
        Promote
      </button>

      <button
        onClick={handleDemote}
        disabled={isLoading || score.overrideType === 'demote'}
        style={{
          ...buttonStyle,
          backgroundColor: score.overrideType === 'demote' ? '#ef4444' : '#333',
        }}
        title="Demote this clip (subtract from score)"
      >
        Demote
      </button>

      {score.hasOverride && (
        <button
          onClick={handleClear}
          disabled={isLoading}
          style={buttonStyle}
          title="Clear override (use machine score)"
        >
          Clear
        </button>
      )}
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  display: 'flex',
  gap: '8px',
};

const buttonStyle: React.CSSProperties = {
  padding: '6px 12px',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  fontSize: '12px',
  cursor: 'pointer',
};
```

8.7 Update FilterBar

Update `src/components/FilterBar.tsx` to add Best Clips filter:

```typescript
// Add to filter options:
const FILTERS = [
  { key: 'all', label: 'All Clips' },
  { key: 'favorites', label: 'Favorites' },
  { key: 'best', label: 'Best Clips' },  // NEW
  { key: 'bad', label: 'Bad' },
  { key: 'unreviewed', label: 'Unreviewed' },
];
```

---

Part 9: Testing Workflow

9.1 CLI Testing

```bash
# 1. Ensure library has clips
dadcam list --library /path/to/library

# 2. Run scoring on all clips
dadcam score --library /path/to/library

# 3. Check scoring status
dadcam score-status --library /path/to/library

# 4. View best clips
dadcam best-clips --library /path/to/library --threshold 0.6

# 5. Score specific clip with verbose output
dadcam score --library /path/to/library --clip 42 --verbose

# 6. Test override functionality
dadcam score-override 42 promote --library /path/to/library
dadcam score-override 42 demote --library /path/to/library
dadcam score-override 42 pin --value 0.9 --library /path/to/library
dadcam score-override 42 clear --library /path/to/library
```

9.2 Verification Checklist

**Database:**
- [ ] Migration creates clip_scores table correctly
- [ ] Migration creates clip_score_overrides table correctly
- [ ] Indexes are created for performance
- [ ] Upsert works (update existing scores)

**Scoring Jobs:**
- [ ] Jobs queue correctly for unscored clips
- [ ] Jobs complete without errors
- [ ] Scores are stored in database
- [ ] Stale scores are detected correctly

**Heuristics:**
- [ ] Scene detection runs on test videos
- [ ] Audio analysis handles clips with/without audio
- [ ] Sharpness detection produces reasonable values
- [ ] Motion detection differentiates static vs active clips

**User Overrides:**
- [ ] Promote increases effective score
- [ ] Demote decreases effective score
- [ ] Pin sets exact score
- [ ] Clear removes override
- [ ] Overrides survive re-scoring

**UI:**
- [ ] Best Clips view loads clips above threshold
- [ ] Threshold slider updates view in real-time
- [ ] Score badges show correct colors
- [ ] Override buttons work correctly
- [ ] Score details expand on hover/click

9.3 Performance Testing

```bash
# Time scoring job on library with 100+ clips
time dadcam score --library /path/to/library

# Expected: ~5-10 seconds per clip for full analysis
# If slower, check FFmpeg filter complexity
```

---


---

Addendum: Production Hardening (Required for 100% Shippable Phase 4)

This addendum does not change Phase 4 scope. It hardens the scoring engine so it behaves deterministically across real-world footage, different FFmpeg builds, and large libraries.

10.1 Proxy-First Scoring Rule (Required)

Score against the proxy whenever it exists.

Why:
- Proxies are consistent codec/fps/resolution (Phase 2), so analysis outputs are stable.
- Decoding originals (DV, MPEG2, VFR, interlaced oddities) creates flaky analyzer behavior.
- It is dramatically faster.

Implementation rule:
- If `clip_assets` has a role `proxy`, use that asset for scoring.
- Else, fallback to the original asset.

The `run_score_job` snippet above already implements this proxy-first rule.

10.2 Concurrency, Timeouts, and Load-Shedding (Required)

Scoring is heavier than thumbs/sprites. Do not allow unbounded parallelism.

Minimum shippable defaults:
- Score worker concurrency: `1` (serial scoring jobs)
- Score job timeout: `10 minutes` per clip
- Sampling cap: analyze at most `120 seconds` of media (evenly sampled) for long clips

Add constants:

```rust
pub const SCORE_WORKERS_DEFAULT: usize = 1;
pub const SCORE_JOB_TIMEOUT_SECS: u64 = 600;
pub const SCORE_MAX_ANALYZE_SECS: u64 = 120;
pub const SCORE_SAMPLE_STRIDE_SECS: u64 = 10; // analyze 1s every 10s (example)
```

Analysis strategy:
- For clips <= `SCORE_MAX_ANALYZE_SECS`: analyze entire clip.
- For clips longer: analyze a deterministic set of windows:
  - first 30s
  - middle 30s
  - last 30s
  - plus a few evenly spaced 10s samples
- Always store in reasons which strategy was used (`reason_token = "score_sampled"`).

Implementation note:
- For FFmpeg-based analyzers, use `-ss` + `-t` to isolate windows.
- Combine per-window component metrics using a stable reducer (mean/median + clamp).

10.3 Partial Failure Handling (Required)

Scoring must always produce an output unless the media is totally unreadable.

Rule:
- If one component analyzer fails (audio/sharpness/motion/scene), still emit a score.
- Missing components get a neutral default (0.5) and a reason token like:
  - `audio_analysis_failed`
  - `motion_analysis_failed`

Example pattern:

```rust
let mut reasons: Vec<&'static str> = Vec::new();

let scene = analyze_scene(...).unwrap_or_else(|_| { reasons.push("scene_analysis_failed"); 0.5 });
let audio = if has_audio { analyze_audio(...).unwrap_or_else(|_| { reasons.push("audio_analysis_failed"); 0.5 }) }
           else { reasons.push("no_audio"); 0.5 };

let sharp = analyze_sharpness(...).unwrap_or_else(|_| { reasons.push("sharpness_analysis_failed"); 0.5 });
let motion = analyze_motion(...).unwrap_or_else(|_| { reasons.push("motion_analysis_failed"); 0.5 });

// combine (weights are deterministic constants)
let overall = clamp01(scene*W_SCENE + audio*W_AUDIO + sharp*W_SHARP + motion*W_MOTION);
```

When the entire scoring pipeline fails (e.g., cannot open file), record:
- job failure with `last_error`
- job_logs include raw stderr/stdout from the failed tool invocation (see 10.5)

10.4 Stable Reason Tokens Contract (Required)

The DB stores:
- `reasons` as JSON (array)
- optional per-component reason strings

To keep backward compatibility, reasons MUST be stable machine tokens, not prose.

Define tokens as constants:

```rust
pub const R_NO_AUDIO: &str = "no_audio";
pub const R_SCENE_DENSE: &str = "scene_dense";
pub const R_SCENE_SPARSE: &str = "scene_sparse";
pub const R_AUDIO_UNSTABLE: &str = "audio_unstable";
pub const R_AUDIO_CLIPPING_RISK: &str = "audio_clipping_risk";
pub const R_SHARP: &str = "sharp";
pub const R_BLURRY: &str = "blurry";
pub const R_HIGH_MOTION: &str = "high_motion";
pub const R_STATIC: &str = "static";
pub const R_SCORE_SAMPLED: &str = "score_sampled";

pub const R_AUDIO_FAILED: &str = "audio_analysis_failed";
pub const R_SCENE_FAILED: &str = "scene_analysis_failed";
pub const R_SHARP_FAILED: &str = "sharpness_analysis_failed";
pub const R_MOTION_FAILED: &str = "motion_analysis_failed";
```

Frontend rule:
- The UI maps tokens -> localized human text.
- The DB never stores human sentences as identifiers.

10.5 Analyzer Parsing Contract + Debuggability (Required)

Avoid regexing brittle stderr whenever possible. When you must parse FFmpeg output:
- Parse only stable numeric formats
- Never rely on localized labels
- Keep parsing logic in one place per analyzer
- On parse failure, store raw tool output in `job_logs`

Implementation:
- Create `scoring/parsing.rs` with helpers:
  - `extract_floats(lines, prefix_tokens)`
  - `extract_key_value(lines, key)`
  - `count_matches(lines, needle)`

On any analyzer error:
- Write `job_logs` entries:
  - `info`: command line invoked (without leaking absolute paths if you prefer)
  - `warn/error`: parse failure summary
  - `info`: first N lines of stderr (cap to avoid huge DB rows)

This aligns with Phase 0’s “crash-safe + debuggable” intent for jobs (`job_logs` exist for a reason). fileciteturn16file3L55-L60

10.6 Test Fixtures That Don’t Require Bundled Media (Required)

Do not check in large binary fixtures. Generate deterministic fixtures at test time with FFmpeg’s `lavfi` sources.

Add a helper:

```rust
fn make_fixture_video(path: &Path, kind: &str) -> Result<()> {
    // kind: "static_silent", "motion_noisy", "scene_dense"
    // Use `testsrc2`, `color`, `sine`, `anoisesrc`, `drawbox` to create predictable patterns.
    // Always set a fixed duration and fps.
    Ok(())
}
```

Then tests:
- Generate fixture mp4 into a temp dir
- Run analyzers
- Assert component score ranges and reason tokens (NOT exact floats)

This makes scoring tests portable across platforms while keeping files small.

10.7 CLI Additions (Shippable Defaults)

Extend Phase 4 CLI score command with safe defaults:

- `--workers N` (default 1)
- `--timeout-secs N` (default 600)
- `--max-analyze-secs N` (default 120)
- `--force` (re-score even if fresh)

If you already have `dadcam score`, add flags without changing existing behavior: defaults preserve the original guide’s “just run score” UX.

10.8 “Best Clips” Query Must Respect Overrides (Required)

Sorting rule:
- Primary: pinned/promoted overrides
- Secondary: machine score
- Tertiary: recorded_at

This ensures user “promote/demote” never feels ignored, and matches Phase 4’s definition of success.

Implementation note:
- Compute `effective_score`:
  - if override = demote: treat as 0
  - if override = promote: add a bump (e.g., +0.2 clamp to 1.0)
  - if override = pin: always include regardless of threshold

---


Part 10: Deferred Items

The following are documented for future phases:

1. **Best Frame Selection**: Currently uses frame at 10% duration. Future improvement could analyze scores per-segment to find the "best" frame.

2. **Segment Scoring**: Score individual segments within long clips to identify best moments, not just overall clip quality.

3. **ML Enhancement (Phase 8)**: Face detection, smile detection, speech segments would improve scoring accuracy significantly.

4. **User Feedback Learning**: Track which clips users actually select for exports to refine scoring weights over time.

5. **Score Explanation UI**: Show detailed breakdown with visual graphs of why a clip scored the way it did.

---

End of Phase 4 Implementation Guide
