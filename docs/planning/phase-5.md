Dad Cam - Phase 5 Implementation Guide

Version: 1.0
Target Audience: Developers new to video concatenation pipelines

---

Overview

Phase 5 builds the Auto-Edit Engine, also known as "VHS Mode." This is the heart of Dad Cam's unique value proposition: one button generates a nostalgic long-form movie from your footage. This is NOT a full video editor - it is an automatic compilation generator.

When complete, you can:
- Create export recipes with different selection modes (By Date, By Event, By Favorites, All)
- Generate VHS-style films with crossfades and audio smoothing
- Apply LUTs for nostalgic color grading
- Add optional date/text overlays
- Store every export run for reproducibility
- Re-render previous exports with identical results

Prerequisites:
- Phase 1, 2, 3, and 4 complete and working
- Test library with scored clips (Phase 4)
- Understanding of Phase 1 job system
- FFmpeg available via `tools.rs` resolver (from Phase 1)
- Basic understanding of FFmpeg filter graphs

---

What We're Building

Phase 5 adds automatic film generation:

```
User Selects Mode
    |
    v  (By Date / By Event / By Favorites / All)
Clip Selection
    |
    v  Scoring + User Tags
Clip Ordering
    |
    v  Pacing Rules
Segment Assembly
    |-- Crossfade transitions
    |-- Audio smoothing (J/L cuts)
    |-- Date overlays (optional)
    |-- LUT application (optional)
    |
    v  FFmpeg concat pipeline
Rendered Output (.mp4 / .mov)
    |
    v  Stored in export_runs
Reproducible Export
```

Core concepts:

1. **Export Recipe**: A saved configuration that defines HOW to generate a film (mode, settings, LUT, etc.)

2. **Export Run**: A specific execution of a recipe, storing which clips were included and in what order

3. **VHS Mode**: The automatic edit style - crossfades, audio smoothing, date overlays, nostalgic LUT

---

Part 1: Understanding the Pipeline

1.1 Selection Modes

Dad Cam offers four modes for selecting which clips go into a film:

**By Date**: Clips from a specific date range, ordered chronologically
- User picks start date and end date
- Uses recorded_at timestamp from clips
- Good for "Summer 2019" or "Birthday Party" compilations

**By Event**: Clips grouped by folder/event, ordered chronologically
- Uses Phase 1 event grouping (folder-based + time-gap)
- Each source folder = one event
- Good for "Trip to Grandma's" compilations

**By Favorites**: Only clips tagged as favorites
- Uses Phase 3 favorite tags
- Ordered by recorded_at or score
- Good for "Best Moments" compilations

**All Clips**: Every clip in the library above a score threshold
- Uses Phase 4 scoring to filter
- Default threshold: 0.5
- Good for "Complete Archive" films

1.2 Pacing and Ordering Rules

Once clips are selected, they need to be ordered and paced:

**Ordering options**:
- `chronological`: By recorded_at timestamp (default)
- `score_desc`: Best clips first
- `score_asc`: Build to a climax
- `shuffle`: Random order (for variety)

**Pacing options**:
- `full`: Use entire clip duration
- `trimmed`: Use first N seconds of each clip (e.g., 10 seconds)
- `best_segment`: Use the highest-scored segment (requires Phase 4 segment scoring, deferred)

For v1, we implement `chronological` ordering with `full` pacing.

1.3 Transition Types

VHS Mode uses crossfade transitions between clips:

**Crossfade (default)**:
- 0.5 second overlap
- Video: xfade filter with fade effect
- Audio: acrossfade filter

**Hard cut (optional)**:
- No transition
- Clips placed back-to-back

FFmpeg filter for crossfade between two clips:
```bash
ffmpeg -i clip1.mp4 -i clip2.mp4 \
  -filter_complex "[0:v][1:v]xfade=transition=fade:duration=0.5:offset=OFFSET[v]; \
                   [0:a][1:a]acrossfade=d=0.5[a]" \
  -map "[v]" -map "[a]" output.mp4
```

1.4 Audio Smoothing (J/L Cuts)1.3.1 Clip Normalization (Required in v1)

Real-world libraries contain clips with mixed formats (VFR, different resolutions, missing audio, odd channel layouts).  
**Before** building crossfades, Dad Cam must normalize each input into a consistent intermediate stream:

**Canonical intermediate (for editing/filtergraph stability):**
- Video: H.264, constant FPS (target_fps), yuv420p, SAR=1
- Audio: 48kHz stereo (or generated silence if missing)

**Per-clip normalization filters (conceptual):**
- Video: `scale` (or `scale=-2:TARGET_H`), `fps=TARGET_FPS`, `format=yuv420p`, `setsar=1`
- Audio: `aresample=48000`, `aformat=channel_layouts=stereo`
- Missing audio: synthesize `anullsrc` with `d=clip_duration`

**Offset safety:**  
When computing `xfade` offsets, clamp to `max(0, clip_duration - transition_duration)` so short clips don’t produce negative offsets.

This normalization is the difference between “works on test data” and “works on wedding footage from 2009, iPhones, GoPros, and camcorders.”

---



J/L cuts make transitions feel natural by offsetting audio and video:

- **J-cut**: Audio from next clip starts before video (audio leads)
- **L-cut**: Audio from current clip continues into next video (audio trails)

For VHS Mode v1, we use simple crossfades. J/L cuts are documented for future enhancement.

1.5 Date Overlays

Optional text burned into the video showing the recording date:

```
December 25, 2019
```

FFmpeg drawtext filter:
```bash
-vf "drawtext=text='December 25, 2019':fontsize=24:fontcolor=white:x=20:y=h-40"
```

Options:
- Position: bottom-left (default), bottom-right, top-left, top-right
- Font size: small (18), medium (24), large (32)
- Format: "Month Day, Year" or "YYYY-MM-DD"
- Duration: first 3 seconds of each clip, or always visible

1.6 LUT Application

LUTs (Look-Up Tables) apply color grading for nostalgic looks:

- VHS preset: Slightly desaturated, warm highlights, crushed blacks
- Film preset: Higher contrast, film grain simulation
- None: Original colors

FFmpeg LUT filter:
```bash
-vf "lut3d=vhs_look.cube"
```

LUT files (.cube format) are stored in `.dadcam/luts/` and bundled with the app.

---

Part 2: Database Schema

2.1 Add Export Tables (Migration)

Add this migration to `src-tauri/src/db/migrations.rs`:

```rust
// Add to MIGRATIONS array:

// Migration 3: Export tables (Phase 5)
r#"
-- Export recipes (saved configurations)
CREATE TABLE export_recipes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,

    -- Recipe identity
    name TEXT NOT NULL,
    description TEXT,

    -- Selection mode
    mode TEXT NOT NULL CHECK (mode IN ('by_date', 'by_event', 'by_favorites', 'all')),

    -- Mode-specific filters (JSON)
    -- by_date: { "start_date": "2019-01-01", "end_date": "2019-12-31" }
    -- by_event: { "event_ids": [1, 2, 3] }
    -- by_favorites: {}
    -- all: { "min_score": 0.5 }
    filters TEXT NOT NULL DEFAULT '{}',

    -- Ordering
    ordering TEXT NOT NULL DEFAULT 'chronological'
        CHECK (ordering IN ('chronological', 'score_desc', 'score_asc', 'shuffle')),

    -- Pacing
    pacing TEXT NOT NULL DEFAULT 'full'
        CHECK (pacing IN ('full', 'trimmed')),
    max_clip_duration_secs INTEGER, -- for trimmed pacing

    -- Transitions
    transition_type TEXT NOT NULL DEFAULT 'crossfade'
        CHECK (transition_type IN ('crossfade', 'hard_cut')),
    transition_duration_ms INTEGER NOT NULL DEFAULT 500,

    -- Overlays
    show_date_overlay INTEGER NOT NULL DEFAULT 0,
    date_overlay_position TEXT DEFAULT 'bottom_left',
    date_overlay_format TEXT DEFAULT 'month_day_year',
    date_overlay_duration_secs INTEGER DEFAULT 3,

    -- Style
    lut_id TEXT, -- NULL = no LUT, or filename like 'vhs_look.cube'

    -- Output settings
    output_preset TEXT NOT NULL DEFAULT 'share'
        CHECK (output_preset IN ('share', 'archive')),

    -- Metadata
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Export runs (execution history)
CREATE TABLE export_runs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recipe_id INTEGER NOT NULL REFERENCES export_recipes(id) ON DELETE CASCADE,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,

    -- Run identity
    name TEXT NOT NULL, -- auto-generated or user-set

    -- Snapshot of recipe settings at run time (for reproducibility)
    recipe_snapshot TEXT NOT NULL, -- JSON copy of recipe settings

    -- Snapshot of inputs used (for deterministic re-render)
    -- Store clip_ids plus the source asset identity used at render time (hash_full if present, else hash_fast+size).
    inputs_snapshot TEXT NOT NULL DEFAULT '[]',

    -- Output
    output_asset_id INTEGER REFERENCES assets(id) ON DELETE SET NULL,
    output_path TEXT, -- relative path in .dadcam/exports/

    -- Stats
    total_clips INTEGER NOT NULL DEFAULT 0,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,

    -- Status
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'rendering', 'completed', 'failed', 'cancelled')),
    progress INTEGER DEFAULT 0, -- 0-100
    error_message TEXT,

    -- FFmpeg command used (for debugging/reproducibility)
    ffmpeg_command TEXT,

    -- Toolchain + pipeline (reproducibility)
    pipeline_version INTEGER NOT NULL,
    ffmpeg_version TEXT,          -- `ffmpeg -version` first line
    ffprobe_version TEXT,         -- `ffprobe -version` first line
    normalized_settings TEXT,     -- JSON: target_fps, target_res, target_audio_rate, channel_layout, etc.

    -- LUT identity (detect drift)
    luts_manifest_b3 TEXT,        -- BLAKE3 hash of luts-manifest.json used at render time

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    completed_at TEXT
);

-- Export run items (which clips in what order)
CREATE TABLE export_run_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id INTEGER NOT NULL REFERENCES export_runs(id) ON DELETE CASCADE,
    clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,

    -- Order in the sequence (0-indexed)
    sequence_order INTEGER NOT NULL,

    -- Timing
    start_offset_ms INTEGER NOT NULL DEFAULT 0, -- where this clip starts in the output
    duration_ms INTEGER NOT NULL, -- how long this clip plays

    -- Segment info (for trimmed pacing)
    clip_start_ms INTEGER NOT NULL DEFAULT 0, -- where in the source clip to start
    clip_end_ms INTEGER, -- where in the source clip to end (NULL = end of clip)

    -- Why this clip was included
    inclusion_reason TEXT, -- 'score_above_threshold', 'favorite', 'in_date_range', etc.

    UNIQUE(run_id, sequence_order)
);

-- Indexes for performance
CREATE INDEX idx_export_recipes_library ON export_recipes(library_id);
CREATE INDEX idx_export_runs_recipe ON export_runs(recipe_id);
CREATE INDEX idx_export_runs_library ON export_runs(library_id);
CREATE INDEX idx_export_runs_status ON export_runs(status);
CREATE INDEX idx_export_run_items_run ON export_run_items(run_id);
CREATE INDEX idx_export_run_items_clip ON export_run_items(clip_id);

-- LUT registry (optional, can also use filesystem)
CREATE TABLE luts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL, -- e.g., 'vhs_look.cube'
    description TEXT,
    is_bundled INTEGER NOT NULL DEFAULT 0, -- 1 = shipped with app
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Insert default LUTs
INSERT INTO luts (name, filename, description, is_bundled) VALUES
    ('VHS Look', 'vhs_look.cube', 'Warm, slightly desaturated, nostalgic video look', 1),
    ('Film Stock', 'film_stock.cube', 'Higher contrast with subtle grain simulation', 1);
"#,
```

2.2 Schema Design Notes

**export_recipes table:**
- One recipe can be run multiple times (different days, different clips added)
- `filters` is JSON to support mode-specific options without schema changes
- `recipe_snapshot` in runs captures exact settings at run time

**export_runs table:**
- Links to recipe but stores snapshot for reproducibility
- `ffmpeg_command` stores the exact command for debugging
- Status tracks render progress

**export_run_items table:**
- Preserves exact clip order and timing
- `inclusion_reason` helps users understand why clips were selected
- `clip_start_ms` / `clip_end_ms` support trimmed pacing

2.3 Schema Query Helpers

Add to `src-tauri/src/db/schema.rs`:

```rust
// ----- Export Recipes -----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecipe {
    pub id: i64,
    pub library_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub mode: String,
    pub filters: serde_json::Value,
    pub ordering: String,
    pub pacing: String,
    pub max_clip_duration_secs: Option<i64>,
    pub transition_type: String,
    pub transition_duration_ms: i64,
    pub show_date_overlay: bool,
    pub date_overlay_position: Option<String>,
    pub date_overlay_format: Option<String>,
    pub date_overlay_duration_secs: Option<i64>,
    pub lut_id: Option<String>,
    pub output_preset: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRun {
    pub id: i64,
    pub recipe_id: i64,
    pub library_id: i64,
    pub name: String,

    // Reproducibility
    pub recipe_snapshot: serde_json::Value,
    pub inputs_snapshot: serde_json::Value,
    pub pipeline_version: i64,
    pub ffmpeg_version: Option<String>,
    pub ffprobe_version: Option<String>,
    pub normalized_settings: serde_json::Value,
    pub luts_manifest_b3: Option<String>,

    // Output
    pub output_asset_id: Option<i64>,
    pub output_path: Option<String>,

    // Stats
    pub total_clips: i64,
    pub total_duration_ms: i64,

    // Status
    pub status: String,
    pub progress: Option<i64>,
    pub error_message: Option<String>,
    pub ffmpeg_command: Option<String>,

    // Timestamps
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRunItem {
    pub id: i64,
    pub run_id: i64,
    pub clip_id: i64,
    pub sequence_order: i64,
    pub start_offset_ms: i64,
    pub duration_ms: i64,
    pub clip_start_ms: i64,
    pub clip_end_ms: Option<i64>,
    pub inclusion_reason: Option<String>,
}

/// Create a new export recipe
pub fn create_export_recipe(
    conn: &Connection,
    library_id: i64,
    name: &str,
    mode: &str,
    filters: &serde_json::Value,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO export_recipes (library_id, name, mode, filters)
           VALUES (?1, ?2, ?3, ?4)"#,
        params![library_id, name, mode, filters.to_string()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get export recipe by ID
pub fn get_export_recipe(conn: &Connection, id: i64) -> Result<Option<ExportRecipe>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, library_id, name, description, mode, filters, ordering, pacing,
                  max_clip_duration_secs, transition_type, transition_duration_ms,
                  show_date_overlay, date_overlay_position, date_overlay_format,
                  date_overlay_duration_secs, lut_id, output_preset, created_at, updated_at
           FROM export_recipes WHERE id = ?1"#
    )?;

    let result = stmt.query_row(params![id], |row| {
        let filters_str: String = row.get(5)?;
        let filters: serde_json::Value = serde_json::from_str(&filters_str).unwrap_or_default();

        Ok(ExportRecipe {
            id: row.get(0)?,
            library_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            mode: row.get(4)?,
            filters,
            ordering: row.get(6)?,
            pacing: row.get(7)?,
            max_clip_duration_secs: row.get(8)?,
            transition_type: row.get(9)?,
            transition_duration_ms: row.get(10)?,
            show_date_overlay: row.get::<_, i64>(11)? != 0,
            date_overlay_position: row.get(12)?,
            date_overlay_format: row.get(13)?,
            date_overlay_duration_secs: row.get(14)?,
            lut_id: row.get(15)?,
            output_preset: row.get(16)?,
            created_at: row.get(17)?,
            updated_at: row.get(18)?,
        })
    });

    match result {
        Ok(recipe) => Ok(Some(recipe)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List all export recipes for a library
pub fn list_export_recipes(conn: &Connection, library_id: i64) -> Result<Vec<ExportRecipe>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, library_id, name, description, mode, filters, ordering, pacing,
                  max_clip_duration_secs, transition_type, transition_duration_ms,
                  show_date_overlay, date_overlay_position, date_overlay_format,
                  date_overlay_duration_secs, lut_id, output_preset, created_at, updated_at
           FROM export_recipes WHERE library_id = ?1 ORDER BY updated_at DESC"#
    )?;

    let rows = stmt.query_map(params![library_id], |row| {
        let filters_str: String = row.get(5)?;
        let filters: serde_json::Value = serde_json::from_str(&filters_str).unwrap_or_default();

        Ok(ExportRecipe {
            id: row.get(0)?,
            library_id: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            mode: row.get(4)?,
            filters,
            ordering: row.get(6)?,
            pacing: row.get(7)?,
            max_clip_duration_secs: row.get(8)?,
            transition_type: row.get(9)?,
            transition_duration_ms: row.get(10)?,
            show_date_overlay: row.get::<_, i64>(11)? != 0,
            date_overlay_position: row.get(12)?,
            date_overlay_format: row.get(13)?,
            date_overlay_duration_secs: row.get(14)?,
            lut_id: row.get(15)?,
            output_preset: row.get(16)?,
            created_at: row.get(17)?,
            updated_at: row.get(18)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Update export recipe settings
pub fn update_export_recipe(
    conn: &Connection,
    id: i64,
    updates: &ExportRecipeUpdate,
) -> Result<()> {
    // Build dynamic UPDATE query based on which fields are set
    let mut sql = String::from("UPDATE export_recipes SET updated_at = datetime('now')");
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref name) = updates.name {
        sql.push_str(", name = ?");
        params_vec.push(Box::new(name.clone()));
    }
    if let Some(ref ordering) = updates.ordering {
        sql.push_str(", ordering = ?");
        params_vec.push(Box::new(ordering.clone()));
    }
    if let Some(ref transition_type) = updates.transition_type {
        sql.push_str(", transition_type = ?");
        params_vec.push(Box::new(transition_type.clone()));
    }
    if let Some(transition_duration_ms) = updates.transition_duration_ms {
        sql.push_str(", transition_duration_ms = ?");
        params_vec.push(Box::new(transition_duration_ms));
    }
    if let Some(show_date_overlay) = updates.show_date_overlay {
        sql.push_str(", show_date_overlay = ?");
        params_vec.push(Box::new(if show_date_overlay { 1i64 } else { 0i64 }));
    }
    if let Some(ref lut_id) = updates.lut_id {
        sql.push_str(", lut_id = ?");
        params_vec.push(Box::new(lut_id.clone()));
    }
    if let Some(ref output_preset) = updates.output_preset {
        sql.push_str(", output_preset = ?");
        params_vec.push(Box::new(output_preset.clone()));
    }

    sql.push_str(" WHERE id = ?");
    params_vec.push(Box::new(id));

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, params_refs.as_slice())?;

    Ok(())
}

#[derive(Debug, Default)]
pub struct ExportRecipeUpdate {
    pub name: Option<String>,
    pub ordering: Option<String>,
    pub transition_type: Option<String>,
    pub transition_duration_ms: Option<i64>,
    pub show_date_overlay: Option<bool>,
    pub lut_id: Option<String>,
    pub output_preset: Option<String>,
}

/// Create a new export run (reproducible)
pub fn create_export_run(
    conn: &Connection,
    recipe_id: i64,
    library_id: i64,
    name: &str,
    recipe_snapshot: &serde_json::Value,
    inputs_snapshot: &serde_json::Value, // list of clip_ids + source identity at run time
    normalized_settings: &serde_json::Value, // canonical res/fps/audio settings for this run
    luts_manifest_b3: Option<&str>,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO export_runs (
                recipe_id, library_id, name,
                recipe_snapshot, inputs_snapshot,
                pipeline_version, normalized_settings, luts_manifest_b3
           )
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
        params![
            recipe_id,
            library_id,
            name,
            recipe_snapshot.to_string(),
            inputs_snapshot.to_string(),
            constants::PIPELINE_VERSION,
            normalized_settings.to_string(),
            luts_manifest_b3,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get export run by ID
pub fn get_export_run(conn: &Connection, id: i64) -> Result<Option<ExportRun>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, recipe_id, library_id, name,
                  recipe_snapshot, inputs_snapshot, pipeline_version,
                  ffmpeg_version, ffprobe_version, normalized_settings, luts_manifest_b3,
                  output_asset_id, output_path,
                  total_clips, total_duration_ms,
                  status, progress, error_message, ffmpeg_command,
                  created_at, started_at, completed_at
           FROM export_runs WHERE id = ?1"#
    )?;

    let result = stmt.query_row(params![id], |row| {
        let snapshot_str: String = row.get(4)?;
        let recipe_snapshot: serde_json::Value = serde_json::from_str(&snapshot_str).unwrap_or_default();

        // Parse reproducibility JSON blobs
        let inputs_str: String = row.get(5)?;
        let inputs_snapshot: serde_json::Value = serde_json::from_str(&inputs_str).unwrap_or_else(|_| serde_json::json!([]));

        let norm_str: String = row.get(9)?;
        let normalized_settings: serde_json::Value = serde_json::from_str(&norm_str).unwrap_or_default();

        Ok(ExportRun {
            id: row.get(0)?,
            recipe_id: row.get(1)?,
            library_id: row.get(2)?,
            name: row.get(3)?,

            recipe_snapshot,
            inputs_snapshot,
            pipeline_version: row.get(6)?,
            ffmpeg_version: row.get(7)?,
            ffprobe_version: row.get(8)?,
            normalized_settings,
            luts_manifest_b3: row.get(10)?,

            output_asset_id: row.get(11)?,
            output_path: row.get(12)?,

            total_clips: row.get(13)?,
            total_duration_ms: row.get(14)?,

            status: row.get(15)?,
            progress: row.get(16)?,
            error_message: row.get(17)?,
            ffmpeg_command: row.get(18)?,

            created_at: row.get(19)?,
            started_at: row.get(20)?,
            completed_at: row.get(21)?,
        })
    });

    match result {
        Ok(run) => Ok(Some(run)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Update export run status and progress
pub fn update_export_run_status(
    conn: &Connection,
    id: i64,
    status: &str,
    progress: Option<i64>,
    error_message: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();

    match status {
        "rendering" => {
            conn.execute(
                r#"UPDATE export_runs SET status = ?1, progress = ?2, started_at = ?3
                   WHERE id = ?4"#,
                params![status, progress, now, id],
            )?;
        }
        "completed" => {
            conn.execute(
                r#"UPDATE export_runs SET status = ?1, progress = 100, completed_at = ?2
                   WHERE id = ?3"#,
                params![status, now, id],
            )?;
        }
        "failed" => {
            conn.execute(
                r#"UPDATE export_runs SET status = ?1, error_message = ?2, completed_at = ?3
                   WHERE id = ?4"#,
                params![status, error_message, now, id],
            )?;
        }
        _ => {
            conn.execute(
                r#"UPDATE export_runs SET status = ?1, progress = ?2
                   WHERE id = ?3"#,
                params![status, progress, id],
            )?;
        }
    }

    Ok(())
}

/// Add item to export run
pub fn add_export_run_item(
    conn: &Connection,
    run_id: i64,
    clip_id: i64,
    sequence_order: i64,
    start_offset_ms: i64,
    duration_ms: i64,
    clip_start_ms: i64,
    clip_end_ms: Option<i64>,
    inclusion_reason: Option<&str>,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO export_run_items
           (run_id, clip_id, sequence_order, start_offset_ms, duration_ms,
            clip_start_ms, clip_end_ms, inclusion_reason)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
        params![run_id, clip_id, sequence_order, start_offset_ms, duration_ms,
                clip_start_ms, clip_end_ms, inclusion_reason],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Get all items for an export run
pub fn get_export_run_items(conn: &Connection, run_id: i64) -> Result<Vec<ExportRunItem>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, run_id, clip_id, sequence_order, start_offset_ms, duration_ms,
                  clip_start_ms, clip_end_ms, inclusion_reason
           FROM export_run_items WHERE run_id = ?1 ORDER BY sequence_order"#
    )?;

    let rows = stmt.query_map(params![run_id], |row| {
        Ok(ExportRunItem {
            id: row.get(0)?,
            run_id: row.get(1)?,
            clip_id: row.get(2)?,
            sequence_order: row.get(3)?,
            start_offset_ms: row.get(4)?,
            duration_ms: row.get(5)?,
            clip_start_ms: row.get(6)?,
            clip_end_ms: row.get(7)?,
            inclusion_reason: row.get(8)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// List export runs for a library
pub fn list_export_runs(
    conn: &Connection,
    library_id: i64,
    limit: i64,
) -> Result<Vec<ExportRun>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, recipe_id, library_id, name, recipe_snapshot, output_asset_id,
                  output_path, total_clips, total_duration_ms, status, progress,
                  error_message, ffmpeg_command, created_at, started_at, completed_at
           FROM export_runs WHERE library_id = ?1
           ORDER BY created_at DESC LIMIT ?2"#
    )?;

    let rows = stmt.query_map(params![library_id, limit], |row| {
        let snapshot_str: String = row.get(4)?;
        let recipe_snapshot: serde_json::Value = serde_json::from_str(&snapshot_str).unwrap_or_default();

        // Parse reproducibility JSON blobs
        let inputs_str: String = row.get(5)?;
        let inputs_snapshot: serde_json::Value = serde_json::from_str(&inputs_str).unwrap_or_else(|_| serde_json::json!([]));

        let norm_str: String = row.get(9)?;
        let normalized_settings: serde_json::Value = serde_json::from_str(&norm_str).unwrap_or_default();

        Ok(ExportRun {
            id: row.get(0)?,
            recipe_id: row.get(1)?,
            library_id: row.get(2)?,
            name: row.get(3)?,

            recipe_snapshot,
            inputs_snapshot,
            pipeline_version: row.get(6)?,
            ffmpeg_version: row.get(7)?,
            ffprobe_version: row.get(8)?,
            normalized_settings,
            luts_manifest_b3: row.get(10)?,

            output_asset_id: row.get(11)?,
            output_path: row.get(12)?,

            total_clips: row.get(13)?,
            total_duration_ms: row.get(14)?,

            status: row.get(15)?,
            progress: row.get(16)?,
            error_message: row.get(17)?,
            ffmpeg_command: row.get(18)?,

            created_at: row.get(19)?,
            started_at: row.get(20)?,
            completed_at: row.get(21)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

---

Part 3: Constants and Dependencies

3.1 Add Export Constants

Add to `src-tauri/src/constants.rs`:

```rust
// Export / VHS Mode settings
pub const EXPORT_VERSION: u32 = 1;

// Transition defaults
pub const TRANSITION_DURATION_MS: i64 = 500;  // 0.5 second crossfades
pub const TRANSITION_TYPE_DEFAULT: &str = "crossfade";

// Date overlay defaults
pub const DATE_OVERLAY_POSITION_DEFAULT: &str = "bottom_left";
pub const DATE_OVERLAY_FORMAT_DEFAULT: &str = "month_day_year";
pub const DATE_OVERLAY_DURATION_SECS: i64 = 3;
pub const DATE_OVERLAY_FONT_SIZE: u32 = 24;
pub const DATE_OVERLAY_FONT_COLOR: &str = "white";
pub const DATE_OVERLAY_SHADOW_COLOR: &str = "black@0.5";

// Output presets
pub const OUTPUT_PRESET_SHARE_CODEC: &str = "libx264";
pub const OUTPUT_PRESET_SHARE_CRF: u32 = 23;
pub const OUTPUT_PRESET_SHARE_AUDIO_CODEC: &str = "aac";
pub const OUTPUT_PRESET_SHARE_AUDIO_BITRATE: &str = "192k";

pub const OUTPUT_PRESET_ARCHIVE_CODEC: &str = "prores_ks";
pub const OUTPUT_PRESET_ARCHIVE_PROFILE: &str = "3";  // ProRes 422 HQ
pub const OUTPUT_PRESET_ARCHIVE_AUDIO_CODEC: &str = "pcm_s16le";

// Clip selection defaults
pub const ALL_MODE_MIN_SCORE_DEFAULT: f64 = 0.5;
pub const MAX_CLIPS_PER_EXPORT: usize = 500;  // Safety limit

// Rendering
pub const EXPORT_RENDER_TIMEOUT_SECS: u64 = 3600;  // 1 hour max per export
pub const EXPORT_WORKERS_DEFAULT: usize = 1;  // Serial exports

// Output paths
pub const EXPORTS_FOLDER: &str = "exports";
pub const LUTS_FOLDER: &str = "luts";
```

3.2 LUT Files (Required Assets)

LUT files (.cube format) define color transformations. The app bundles default LUTs in the resources folder.

**LUT Directory Setup:**

Create the directory structure:
```bash
mkdir -p src-tauri/resources/luts
```

3.2.1 LUT Manifest and Verification (Required)

To make LUT usage reproducible and tamper-detectable, Dad Cam maintains a manifest:

`src-tauri/resources/luts/luts-manifest.json`
```json
{
  "version": 1,
  "generated_at": "2026-01-23T00:00:00Z",
  "luts": {
    "vhs_look.cube": { "bytes": 12345, "blake3": "..." },
    "film_stock.cube": { "bytes": 23456, "blake3": "..." }
  }
}
```

Rules:
- The manifest is bundled with the app alongside the `.cube` files.
- On app start (or library init), validate each LUT file:
  - size matches `bytes`
  - BLAKE3 matches `blake3`
- Store the **BLAKE3 of the manifest** in `export_runs.luts_manifest_b3` at render time.

This guarantees “re-render identical output” stays true even if LUT files are edited or replaced.



**Option 1: Generate LUTs with Python Script**

Create `scripts/generate_luts.py`:

```python
#!/usr/bin/env python3
"""Generate .cube LUT files for Dad Cam VHS Mode."""

import os

def generate_identity_lut(size=17):
    """Generate a base identity LUT (no color change)."""
    lut = []
    for b in range(size):
        for g in range(size):
            for r in range(size):
                lut.append((r / (size - 1), g / (size - 1), b / (size - 1)))
    return lut

def apply_vhs_transform(r, g, b):
    """Apply VHS look: warm highlights, crushed blacks, slight desaturation."""
    # Desaturate slightly (10%)
    gray = 0.299 * r + 0.587 * g + 0.114 * b
    r = r * 0.9 + gray * 0.1
    g = g * 0.9 + gray * 0.1
    b = b * 0.9 + gray * 0.1

    # Warm shift (add red/yellow to highlights)
    r = min(1.0, r * 1.05 + 0.02)
    g = min(1.0, g * 1.02)
    b = max(0.0, b * 0.95 - 0.02)

    # Crush blacks (lift shadows slightly)
    r = r * 0.92 + 0.04
    g = g * 0.92 + 0.04
    b = b * 0.92 + 0.04

    return (r, g, b)

def apply_film_transform(r, g, b):
    """Apply film look: higher contrast, slight color shift."""
    # S-curve contrast
    def s_curve(x):
        return 0.5 + 0.5 * ((2 * x - 1) ** 3) if x > 0.5 else 0.5 - 0.5 * ((1 - 2 * x) ** 3)

    r = s_curve(r)
    g = s_curve(g)
    b = s_curve(b)

    # Slight blue shadow tint
    b = min(1.0, b + (1 - r) * 0.03)

    return (r, g, b)

def write_cube_file(path, title, lut, size):
    """Write a .cube LUT file."""
    with open(path, 'w') as f:
        f.write(f'# {title}\n')
        f.write(f'TITLE "{title}"\n')
        f.write(f'LUT_3D_SIZE {size}\n')
        f.write('DOMAIN_MIN 0.0 0.0 0.0\n')
        f.write('DOMAIN_MAX 1.0 1.0 1.0\n')
        f.write('\n')
        for r, g, b in lut:
            f.write(f'{r:.6f} {g:.6f} {b:.6f}\n')

def main():
    size = 17  # Standard LUT size
    output_dir = 'src-tauri/resources/luts'
    os.makedirs(output_dir, exist_ok=True)

    # Generate VHS Look
    base_lut = generate_identity_lut(size)
    vhs_lut = [apply_vhs_transform(r, g, b) for r, g, b in base_lut]
    write_cube_file(
        f'{output_dir}/vhs_look.cube',
        'VHS Look',
        vhs_lut,
        size
    )
    print(f'Generated vhs_look.cube ({len(vhs_lut)} entries)')

    # Generate Film Stock
    film_lut = [apply_film_transform(r, g, b) for r, g, b in base_lut]
    write_cube_file(
        f'{output_dir}/film_stock.cube',
        'Film Stock',
        film_lut,
        size
    )
    print(f'Generated film_stock.cube ({len(film_lut)} entries)')

if __name__ == '__main__':
    main()
```

Run the script to generate LUTs:
```bash
python scripts/generate_luts.py
```

**Option 2: Download Free LUTs**

Use free .cube LUTs from reputable sources:
- https://lutify.me/free-luts/ (various film looks)
- https://www.freepresets.com/product/free-luts-cinematic/ (cinematic)

Download, rename to `vhs_look.cube` and `film_stock.cube`, and place in `src-tauri/resources/luts/`.

**Option 3: Identity LUT (No Color Grading)**

For testing without actual color grading, create an identity LUT that passes colors through unchanged.

Create `src-tauri/resources/luts/identity.cube`:
```
# Identity LUT - No color change
TITLE "Identity"
LUT_3D_SIZE 2
DOMAIN_MIN 0.0 0.0 0.0
DOMAIN_MAX 1.0 1.0 1.0

0.000000 0.000000 0.000000
1.000000 0.000000 0.000000
0.000000 1.000000 0.000000
1.000000 1.000000 0.000000
0.000000 0.000000 1.000000
1.000000 0.000000 1.000000
0.000000 1.000000 1.000000
1.000000 1.000000 1.000000
```

**Important Notes:**
- LUT files are included in the Tauri bundle via `tauri.conf.json` resources
- If `lut_id = NULL` in the recipe, the render pipeline skips LUT application
- Users can add custom LUTs by placing .cube files in `.dadcam/luts/` in their library

---

Part 4: Clip Selection Module

4.1 Create the Selection Module

Create `src-tauri/src/export/mod.rs`:

```rust
pub mod selector;
pub mod assembler;
pub mod renderer;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::schema::{ExportRecipe, ExportRun, ExportRunItem};

/// Clip selected for export with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedClip {
    pub clip_id: i64,
    pub asset_path: String,         // Path to source file (proxy or original)
    pub duration_ms: i64,
    pub recorded_at: String,
    pub score: Option<f64>,
    pub inclusion_reason: String,
}

/// Export assembly plan (before rendering)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPlan {
    pub recipe_id: i64,
    pub clips: Vec<SelectedClip>,
    pub total_duration_ms: i64,
    pub transition_duration_ms: i64,
    pub output_filename: String,
}
```

4.2 Create the Selector Module

Create `src-tauri/src/export/selector.rs`:

```rust
use std::collections::HashSet;
use anyhow::{Result, anyhow};
use rusqlite::Connection;

use crate::db::schema::{self, ExportRecipe};
use crate::constants::{ALL_MODE_MIN_SCORE_DEFAULT, MAX_CLIPS_PER_EXPORT};
use super::SelectedClip;

/// Select clips based on recipe mode and filters
pub fn select_clips(
    conn: &Connection,
    recipe: &ExportRecipe,
    library_id: i64,
) -> Result<Vec<SelectedClip>> {
    let clips = match recipe.mode.as_str() {
        "by_date" => select_by_date(conn, recipe, library_id)?,
        "by_event" => select_by_event(conn, recipe, library_id)?,
        "by_favorites" => select_by_favorites(conn, library_id)?,
        "all" => select_all(conn, recipe, library_id)?,
        _ => return Err(anyhow!("Unknown selection mode: {}", recipe.mode)),
    };

    // Apply ordering
    let ordered = apply_ordering(clips, &recipe.ordering);

    // Apply safety limit
    let limited: Vec<SelectedClip> = ordered
        .into_iter()
        .take(MAX_CLIPS_PER_EXPORT)
        .collect();

    Ok(limited)
}

/// Select clips within a date range
fn select_by_date(
    conn: &Connection,
    recipe: &ExportRecipe,
    library_id: i64,
) -> Result<Vec<SelectedClip>> {
    let start_date = recipe.filters.get("start_date")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("by_date mode requires start_date filter"))?;

    let end_date = recipe.filters.get("end_date")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("by_date mode requires end_date filter"))?;

    let sql = r#"
        SELECT c.id, a.path, c.duration_ms, c.recorded_at,
               COALESCE(cs.overall_score, 0) as score
        FROM clips c
        JOIN assets a ON c.original_asset_id = a.id
        LEFT JOIN clip_scores cs ON c.id = cs.clip_id
        WHERE c.library_id = ?1
          AND date(c.recorded_at) >= date(?2)
          AND date(c.recorded_at) <= date(?3)
          AND c.media_type = 'video'
        ORDER BY c.recorded_at ASC
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![library_id, start_date, end_date], |row| {
        Ok(SelectedClip {
            clip_id: row.get(0)?,
            asset_path: row.get(1)?,
            duration_ms: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            recorded_at: row.get(3)?,
            score: row.get(4)?,
            inclusion_reason: "in_date_range".to_string(),
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Select clips from specific events
fn select_by_event(
    conn: &Connection,
    recipe: &ExportRecipe,
    library_id: i64,
) -> Result<Vec<SelectedClip>> {
    // Events are determined by source folder structure
    // For v1, we use the parent folder of the original asset as the event

    let event_folders: Vec<String> = recipe.filters.get("event_folders")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    if event_folders.is_empty() {
        return Err(anyhow!("by_event mode requires event_folders filter"));
    }

    // Build query with folder matching
    let placeholders: Vec<String> = event_folders.iter()
        .enumerate()
        .map(|(i, _)| format!("a.path LIKE ?{}", i + 2))
        .collect();

    let sql = format!(
        r#"
        SELECT c.id, a.path, c.duration_ms, c.recorded_at,
               COALESCE(cs.overall_score, 0) as score
        FROM clips c
        JOIN assets a ON c.original_asset_id = a.id
        LEFT JOIN clip_scores cs ON c.id = cs.clip_id
        WHERE c.library_id = ?1
          AND ({})
          AND c.media_type = 'video'
        ORDER BY c.recorded_at ASC
        "#,
        placeholders.join(" OR ")
    );

    let mut stmt = conn.prepare(&sql)?;

    // Build params: library_id, then folder patterns
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(library_id)];
    for folder in &event_folders {
        params.push(Box::new(format!("{}%", folder)));
    }

    let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(SelectedClip {
            clip_id: row.get(0)?,
            asset_path: row.get(1)?,
            duration_ms: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            recorded_at: row.get(3)?,
            score: row.get(4)?,
            inclusion_reason: "in_event".to_string(),
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Select only favorited clips
fn select_by_favorites(
    conn: &Connection,
    library_id: i64,
) -> Result<Vec<SelectedClip>> {
    let sql = r#"
        SELECT c.id, a.path, c.duration_ms, c.recorded_at,
               COALESCE(cs.overall_score, 0) as score
        FROM clips c
        JOIN assets a ON c.original_asset_id = a.id
        JOIN clip_tags ct ON c.id = ct.clip_id
        JOIN tags t ON ct.tag_id = t.id
        LEFT JOIN clip_scores cs ON c.id = cs.clip_id
        WHERE c.library_id = ?1
          AND t.name = 'favorite'
          AND c.media_type = 'video'
        ORDER BY c.recorded_at ASC
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![library_id], |row| {
        Ok(SelectedClip {
            clip_id: row.get(0)?,
            asset_path: row.get(1)?,
            duration_ms: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            recorded_at: row.get(3)?,
            score: row.get(4)?,
            inclusion_reason: "favorite".to_string(),
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Select all clips above score threshold
fn select_all(
    conn: &Connection,
    recipe: &ExportRecipe,
    library_id: i64,
) -> Result<Vec<SelectedClip>> {
    let min_score = recipe.filters.get("min_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(ALL_MODE_MIN_SCORE_DEFAULT);

    let sql = r#"
        SELECT c.id, a.path, c.duration_ms, c.recorded_at,
               COALESCE(cs.overall_score, 0) as score
        FROM clips c
        JOIN assets a ON c.original_asset_id = a.id
        LEFT JOIN clip_scores cs ON c.id = cs.clip_id
        WHERE c.library_id = ?1
          AND c.media_type = 'video'
          AND COALESCE(cs.overall_score, 0) >= ?2
        ORDER BY c.recorded_at ASC
    "#;

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![library_id, min_score], |row| {
        Ok(SelectedClip {
            clip_id: row.get(0)?,
            asset_path: row.get(1)?,
            duration_ms: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            recorded_at: row.get(3)?,
            score: row.get(4)?,
            inclusion_reason: format!("score_above_{}", min_score),
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Apply ordering to selected clips
fn apply_ordering(mut clips: Vec<SelectedClip>, ordering: &str) -> Vec<SelectedClip> {
    match ordering {
        "chronological" => {
            clips.sort_by(|a, b| a.recorded_at.cmp(&b.recorded_at));
        }
        "score_desc" => {
            clips.sort_by(|a, b| {
                b.score.unwrap_or(0.0)
                    .partial_cmp(&a.score.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "score_asc" => {
            clips.sort_by(|a, b| {
                a.score.unwrap_or(0.0)
                    .partial_cmp(&b.score.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        "shuffle" => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            // Deterministic shuffle based on clip IDs (for reproducibility)
            clips.sort_by(|a, b| {
                let mut hasher_a = DefaultHasher::new();
                a.clip_id.hash(&mut hasher_a);
                let hash_a = hasher_a.finish();

                let mut hasher_b = DefaultHasher::new();
                b.clip_id.hash(&mut hasher_b);
                let hash_b = hasher_b.finish();

                hash_a.cmp(&hash_b)
            });
        }
        _ => {
            // Default to chronological
            clips.sort_by(|a, b| a.recorded_at.cmp(&b.recorded_at));
        }
    }

    clips
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordering_score_desc() {
        let clips = vec![
            SelectedClip {
                clip_id: 1,
                asset_path: "a.mp4".to_string(),
                duration_ms: 1000,
                recorded_at: "2020-01-01".to_string(),
                score: Some(0.5),
                inclusion_reason: "test".to_string(),
            },
            SelectedClip {
                clip_id: 2,
                asset_path: "b.mp4".to_string(),
                duration_ms: 1000,
                recorded_at: "2020-01-02".to_string(),
                score: Some(0.9),
                inclusion_reason: "test".to_string(),
            },
        ];

        let ordered = apply_ordering(clips, "score_desc");
        assert_eq!(ordered[0].clip_id, 2); // Higher score first
        assert_eq!(ordered[1].clip_id, 1);
    }
}
```

---

Part 5: Assembly and Rendering Module

5.1 Create the Assembler Module

Create `src-tauri/src/export/assembler.rs`:

```rust
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc, NaiveDate};
use std::path::{Path, PathBuf};

use super::{SelectedClip, ExportPlan};
use crate::db::schema::ExportRecipe;
use crate::constants::TRANSITION_DURATION_MS;

/// Build an export plan from selected clips and recipe
pub fn build_export_plan(
    clips: Vec<SelectedClip>,
    recipe: &ExportRecipe,
    library_root: &Path,
) -> Result<ExportPlan> {
    if clips.is_empty() {
        return Err(anyhow!("No clips selected for export"));
    }

    let transition_duration_ms = recipe.transition_duration_ms;

    // Calculate total duration accounting for transitions
    let clips_duration: i64 = clips.iter().map(|c| c.duration_ms).sum();
    let num_transitions = (clips.len() as i64 - 1).max(0);
    let transitions_overlap = num_transitions * transition_duration_ms;
    let total_duration_ms = clips_duration - transitions_overlap;

    // Generate output filename
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let output_filename = format!("dadcam_export_{}.mp4", timestamp);

    Ok(ExportPlan {
        recipe_id: recipe.id,
        clips,
        total_duration_ms: total_duration_ms.max(0),
        transition_duration_ms,
        output_filename,
    })
}

/// Generate FFmpeg filter complex for concatenation with crossfades
pub fn build_ffmpeg_filter(
    plan: &ExportPlan,
    show_date_overlay: bool,
    date_overlay_position: &str,
    date_overlay_format: &str,
    lut_path: Option<&Path>,
) -> Result<(String, Vec<String>)> {
    let clips = &plan.clips;
    let transition_secs = plan.transition_duration_ms as f64 / 1000.0;

    if clips.len() == 1 {
        // Single clip - no transitions needed
        return build_single_clip_filter(
            &clips[0],
            show_date_overlay,
            date_overlay_position,
            date_overlay_format,
            lut_path,
        );
    }

    // Build complex filtergraph for multiple clips with crossfades
    let mut filter_parts = Vec::new();
    let mut input_args = Vec::new();

    // Add input arguments
    for (i, clip) in clips.iter().enumerate() {
        input_args.push("-i".to_string());
        input_args.push(clip.asset_path.clone());
    }

    // Build video chain with xfade transitions
    let mut video_chain = String::new();
    let mut audio_chain = String::new();
    let mut current_offset = 0.0;

    // First, prepare each input with date overlay if needed
    for (i, clip) in clips.iter().enumerate() {
        let clip_duration_secs = clip.duration_ms as f64 / 1000.0;

        if show_date_overlay {
            let date_text = format_date_overlay(&clip.recorded_at, date_overlay_format);
            let position = get_overlay_position(date_overlay_position);

            filter_parts.push(format!(
                "[{}:v]drawtext=text='{}':fontsize=24:fontcolor=white:\
                 shadowcolor=black@0.5:shadowx=2:shadowy=2:{}[v{}]",
                i, date_text, position, i
            ));
        } else {
            filter_parts.push(format!("[{}:v]null[v{}]", i, i));
        }
    }

    // Build xfade chain
    if clips.len() == 2 {
        let offset = clips[0].duration_ms as f64 / 1000.0 - transition_secs;
        filter_parts.push(format!(
            "[v0][v1]xfade=transition=fade:duration={}:offset={}[vout]",
            transition_secs, offset
        ));
        filter_parts.push(format!(
            "[0:a][1:a]acrossfade=d={}[aout]",
            transition_secs
        ));
    } else {
        // Chain multiple xfades
        let mut prev_label = "v0".to_string();
        let mut audio_prev = "[0:a]".to_string();
        let mut accumulated_duration = clips[0].duration_ms as f64 / 1000.0;

        for i in 1..clips.len() {
            let is_last = i == clips.len() - 1;
            let next_label = if is_last { "vout".to_string() } else { format!("vx{}", i) };
            let audio_next = if is_last { "[aout]".to_string() } else { format!("[ax{}]", i) };

            let offset = accumulated_duration - transition_secs;

            filter_parts.push(format!(
                "[{}][v{}]xfade=transition=fade:duration={}:offset={}{}",
                prev_label, i, transition_secs, offset, next_label
            ));

            filter_parts.push(format!(
                "{}[{}:a]acrossfade=d={}{}",
                audio_prev, i, transition_secs, audio_next
            ));

            accumulated_duration += clips[i].duration_ms as f64 / 1000.0 - transition_secs;
            prev_label = next_label;
            audio_prev = audio_next;
        }
    }

    // Apply LUT if specified
    if let Some(lut) = lut_path {
        filter_parts.push(format!(
            "[vout]lut3d={}[vfinal]",
            lut.to_string_lossy()
        ));
    }

    let filter_complex = filter_parts.join(";");
    let final_video = if lut_path.is_some() { "[vfinal]" } else { "[vout]" };

    Ok((filter_complex, input_args))
}

/// Build filter for single clip export
fn build_single_clip_filter(
    clip: &SelectedClip,
    show_date_overlay: bool,
    date_overlay_position: &str,
    date_overlay_format: &str,
    lut_path: Option<&Path>,
) -> Result<(String, Vec<String>)> {
    let input_args = vec!["-i".to_string(), clip.asset_path.clone()];

    let mut filters = Vec::new();

    if show_date_overlay {
        let date_text = format_date_overlay(&clip.recorded_at, date_overlay_format);
        let position = get_overlay_position(date_overlay_position);
        filters.push(format!(
            "drawtext=text='{}':fontsize=24:fontcolor=white:\
             shadowcolor=black@0.5:shadowx=2:shadowy=2:{}",
            date_text, position
        ));
    }

    if let Some(lut) = lut_path {
        filters.push(format!("lut3d={}", lut.to_string_lossy()));
    }

    let filter_complex = if filters.is_empty() {
        String::new()
    } else {
        filters.join(",")
    };

    Ok((filter_complex, input_args))
}

/// Format date for overlay text
fn format_date_overlay(recorded_at: &str, format: &str) -> String {
    // Parse the recorded_at timestamp
    let date = if let Ok(dt) = DateTime::parse_from_rfc3339(recorded_at) {
        dt.date_naive()
    } else if let Ok(d) = NaiveDate::parse_from_str(recorded_at, "%Y-%m-%d") {
        d
    } else {
        return "Unknown Date".to_string();
    };

    match format {
        "month_day_year" => date.format("%B %d, %Y").to_string(),
        "iso" => date.format("%Y-%m-%d").to_string(),
        "short" => date.format("%m/%d/%y").to_string(),
        _ => date.format("%B %d, %Y").to_string(),
    }
}

/// Get FFmpeg position string for date overlay
fn get_overlay_position(position: &str) -> String {
    match position {
        "bottom_left" => "x=20:y=h-40".to_string(),
        "bottom_right" => "x=w-tw-20:y=h-40".to_string(),
        "top_left" => "x=20:y=20".to_string(),
        "top_right" => "x=w-tw-20:y=20".to_string(),
        _ => "x=20:y=h-40".to_string(), // default bottom-left
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_date_overlay() {
        let date = "2019-12-25T10:30:00Z";
        assert_eq!(format_date_overlay(date, "month_day_year"), "December 25, 2019");
        assert_eq!(format_date_overlay(date, "iso"), "2019-12-25");
    }

    #[test]
    fn test_overlay_position() {
        assert_eq!(get_overlay_position("bottom_left"), "x=20:y=h-40");
        assert_eq!(get_overlay_position("top_right"), "x=w-tw-20:y=20");
    }
}
```

5.2 Create the Renderer Module

Create `src-tauri/src/export/renderer.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use anyhow::{Result, anyhow};
use rusqlite::Connection;

use super::{ExportPlan, assembler};
use crate::db::schema::{self, ExportRecipe, ExportRun};
use crate::constants::{
    OUTPUT_PRESET_SHARE_CODEC, OUTPUT_PRESET_SHARE_CRF,
    OUTPUT_PRESET_SHARE_AUDIO_CODEC, OUTPUT_PRESET_SHARE_AUDIO_BITRATE,
    OUTPUT_PRESET_ARCHIVE_CODEC, OUTPUT_PRESET_ARCHIVE_PROFILE,
    OUTPUT_PRESET_ARCHIVE_AUDIO_CODEC,
    EXPORTS_FOLDER, LUTS_FOLDER,
    EXPORT_RENDER_TIMEOUT_SECS,
};

/// Render an export plan to a video file
pub fn render_export(
    conn: &Connection,
    plan: &ExportPlan,
    recipe: &ExportRecipe,
    run_id: i64,
    library_root: &Path,
    progress_callback: Option<Box<dyn Fn(i64) + Send>>,
) -> Result<PathBuf> {
    // Update run status to rendering
    schema::update_export_run_status(conn, run_id, "rendering", Some(0), None)?;

    // Determine output path
    let exports_dir = library_root.join(".dadcam").join(EXPORTS_FOLDER);
    std::fs::create_dir_all(&exports_dir)?;

    let output_path = exports_dir.join(&plan.output_filename);

    // Determine LUT path if specified
    let lut_path = recipe.lut_id.as_ref().map(|lut_filename| {
        library_root.join(".dadcam").join(LUTS_FOLDER).join(lut_filename)
    });

    // Build FFmpeg filter complex
    let (filter_complex, input_args) = assembler::build_ffmpeg_filter(
        plan,
        recipe.show_date_overlay,
        recipe.date_overlay_position.as_deref().unwrap_or("bottom_left"),
        recipe.date_overlay_format.as_deref().unwrap_or("month_day_year"),
        lut_path.as_deref(),
    )?;

    // Build FFmpeg command
    let mut cmd = Command::new(crate::tools::ffmpeg_path());

    // Add inputs
    for arg in &input_args {
        cmd.arg(arg);
    }

    // Add filter complex if we have multiple clips or filters
    if !filter_complex.is_empty() && plan.clips.len() > 1 {
        cmd.arg("-filter_complex").arg(&filter_complex);
        cmd.arg("-map").arg("[vout]");
        cmd.arg("-map").arg("[aout]");
    } else if !filter_complex.is_empty() {
        cmd.arg("-vf").arg(&filter_complex);
    }

    // Add output codec settings based on preset
    match recipe.output_preset.as_str() {
        "archive" => {
            cmd.arg("-c:v").arg(OUTPUT_PRESET_ARCHIVE_CODEC);
            cmd.arg("-profile:v").arg(OUTPUT_PRESET_ARCHIVE_PROFILE);
            cmd.arg("-c:a").arg(OUTPUT_PRESET_ARCHIVE_AUDIO_CODEC);
        }
        _ => {
            // Default to share preset
            cmd.arg("-c:v").arg(OUTPUT_PRESET_SHARE_CODEC);
            cmd.arg("-crf").arg(OUTPUT_PRESET_SHARE_CRF.to_string());
            cmd.arg("-preset").arg("medium");
            cmd.arg("-c:a").arg(OUTPUT_PRESET_SHARE_AUDIO_CODEC);
            cmd.arg("-b:a").arg(OUTPUT_PRESET_SHARE_AUDIO_BITRATE);
        }
    }

    // Common output settings
    cmd.arg("-movflags").arg("+faststart");
    cmd.arg("-y"); // Overwrite output
    cmd.arg(&output_path);

    // Store the command for reproducibility
    let command_str = format!("{:?}", cmd);
    conn.execute(
        "UPDATE export_runs SET ffmpeg_command = ?1 WHERE id = ?2",
        rusqlite::params![command_str, run_id],
    )?;

    // Execute with progress monitoring
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    // Monitor stderr for progress
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("Failed to capture stderr"))?;
    let reader = BufReader::new(stderr);

    let total_duration_ms = plan.total_duration_ms;

    for line in reader.lines() {
        if let Ok(line) = line {
            // Parse FFmpeg progress output
            if let Some(time_str) = parse_ffmpeg_time(&line) {
                let current_ms = time_str_to_ms(&time_str);
                let progress = ((current_ms as f64 / total_duration_ms as f64) * 100.0) as i64;
                let progress = progress.min(99); // Cap at 99 until done

                if let Some(ref cb) = progress_callback {
                    cb(progress);
                }

                // Update database progress
                let _ = schema::update_export_run_status(conn, run_id, "rendering", Some(progress), None);
            }
        }
    }

    let status = child.wait()?;

    if !status.success() {
        let error_msg = format!("FFmpeg exited with status: {}", status);
        schema::update_export_run_status(conn, run_id, "failed", None, Some(&error_msg))?;
        return Err(anyhow!(error_msg));
    }

    // Verify output exists
    if !output_path.exists() {
        let error_msg = "Output file not created";
        schema::update_export_run_status(conn, run_id, "failed", None, Some(error_msg))?;
        return Err(anyhow!(error_msg));
    }

    // Update run with success
    let output_path_relative = output_path.strip_prefix(library_root)
        .unwrap_or(&output_path)
        .to_string_lossy()
        .replace('\\', "/"); // Normalize to POSIX

    conn.execute(
        "UPDATE export_runs SET output_path = ?1, status = 'completed', progress = 100, completed_at = datetime('now') WHERE id = ?2",
        rusqlite::params![output_path_relative, run_id],
    )?;

    Ok(output_path)
}

/// Parse FFmpeg time output from stderr
fn parse_ffmpeg_time(line: &str) -> Option<String> {
    // FFmpeg progress looks like: "frame= 1234 fps=30 ... time=00:01:23.45 ..."
    if let Some(idx) = line.find("time=") {
        let rest = &line[idx + 5..];
        if let Some(end) = rest.find(' ') {
            return Some(rest[..end].to_string());
        } else if rest.len() >= 11 {
            return Some(rest[..11].to_string());
        }
    }
    None
}

/// Convert time string (HH:MM:SS.ms) to milliseconds
fn time_str_to_ms(time_str: &str) -> i64 {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 3 {
        return 0;
    }

    let hours: i64 = parts[0].parse().unwrap_or(0);
    let minutes: i64 = parts[1].parse().unwrap_or(0);
    let seconds_parts: Vec<&str> = parts[2].split('.').collect();
    let seconds: i64 = seconds_parts[0].parse().unwrap_or(0);
    let ms: i64 = if seconds_parts.len() > 1 {
        let ms_str = seconds_parts[1];
        let ms_val: i64 = ms_str.parse().unwrap_or(0);
        // Normalize to 3 digits
        match ms_str.len() {
            1 => ms_val * 100,
            2 => ms_val * 10,
            _ => ms_val,
        }
    } else {
        0
    };

    (hours * 3600 + minutes * 60 + seconds) * 1000 + ms
}

/// Render using proxy files for quick preview (draft mode)
pub fn render_draft_export(
    conn: &Connection,
    plan: &ExportPlan,
    recipe: &ExportRecipe,
    run_id: i64,
    library_root: &Path,
) -> Result<PathBuf> {
    // For draft mode, we use proxy files instead of originals
    // This is much faster but lower quality

    let mut draft_plan = plan.clone();

    // Replace asset paths with proxy paths
    for clip in &mut draft_plan.clips {
        if let Some(proxy_path) = get_proxy_path(conn, clip.clip_id)? {
            clip.asset_path = proxy_path;
        }
    }

    // Modify output filename to indicate draft
    draft_plan.output_filename = draft_plan.output_filename
        .replace(".mp4", "_draft.mp4");

    render_export(conn, &draft_plan, recipe, run_id, library_root, None)
}

/// Get proxy path for a clip
fn get_proxy_path(conn: &Connection, clip_id: i64) -> Result<Option<String>> {
    let result: Option<String> = conn.query_row(
        r#"SELECT a.path FROM assets a
           JOIN clip_assets ca ON a.id = ca.asset_id
           WHERE ca.clip_id = ?1 AND ca.role = 'proxy'"#,
        rusqlite::params![clip_id],
        |row| row.get(0),
    ).optional()?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ffmpeg_time() {
        let line = "frame= 1234 fps=30.0 q=28.0 size=   12345kB time=00:01:23.45 bitrate=1234.5kbits/s";
        assert_eq!(parse_ffmpeg_time(line), Some("00:01:23.45".to_string()));
    }

    #[test]
    fn test_time_str_to_ms() {
        assert_eq!(time_str_to_ms("00:00:01.000"), 1000);
        assert_eq!(time_str_to_ms("00:01:00.000"), 60000);
        assert_eq!(time_str_to_ms("01:00:00.000"), 3600000);
        assert_eq!(time_str_to_ms("00:01:23.45"), 83450);
    }
}
```

---

Part 6: Job Integration

6.1 Add Export Job Handler

Add to `src-tauri/src/jobs/mod.rs`:

```rust
use crate::export::{selector, assembler, renderer, ExportPlan};

/// Execute an export job
pub fn run_export_job(
    conn: &Connection,
    run_id: i64,
    draft: bool,
) -> Result<std::path::PathBuf> {
    // Get the export run
    let run = schema::get_export_run(conn, run_id)?
        .ok_or_else(|| anyhow!("Export run not found: {}", run_id))?;

    // Get the recipe
    let recipe = schema::get_export_recipe(conn, run.recipe_id)?
        .ok_or_else(|| anyhow!("Recipe not found: {}", run.recipe_id))?;

    // Get library root
    let library = schema::get_library(conn, run.library_id)?
        .ok_or_else(|| anyhow!("Library not found"))?;
    let library_root = std::path::Path::new(&library.root_path);

    // Select clips based on recipe
    let clips = selector::select_clips(conn, &recipe, run.library_id)?;

    if clips.is_empty() {
        schema::update_export_run_status(conn, run_id, "failed", None, Some("No clips match selection criteria"))?;
        return Err(anyhow!("No clips match selection criteria"));
    }

    // Build export plan
    let plan = assembler::build_export_plan(clips.clone(), &recipe, library_root)?;

    // Store run items
    let mut offset_ms = 0i64;
    for (i, clip) in clips.iter().enumerate() {
        schema::add_export_run_item(
            conn,
            run_id,
            clip.clip_id,
            i as i64,
            offset_ms,
            clip.duration_ms,
            0,
            None,
            Some(&clip.inclusion_reason),
        )?;

        // Account for transition overlap
        if i > 0 {
            offset_ms += clip.duration_ms - recipe.transition_duration_ms;
        } else {
            offset_ms += clip.duration_ms;
        }
    }

    // Update run stats
    conn.execute(
        "UPDATE export_runs SET total_clips = ?1, total_duration_ms = ?2 WHERE id = ?3",
        params![clips.len() as i64, plan.total_duration_ms, run_id],
    )?;

    // Render the export (use draft mode if requested)
    if draft {
        renderer::render_draft_export(conn, &plan, &recipe, run_id, library_root)
    } else {
        renderer::render_export(conn, &plan, &recipe, run_id, library_root, None)
    }
}

/// Queue an export job for a recipe
pub fn queue_export_job(
    conn: &Connection,
    recipe_id: i64,
    name: Option<&str>,
    draft: bool,
) -> Result<i64> {
    let recipe = schema::get_export_recipe(conn, recipe_id)?
        .ok_or_else(|| anyhow!("Recipe not found"))?;

    // Create export run with recipe snapshot
    let recipe_snapshot = serde_json::to_value(&recipe)?;
    let run_name = name.map(|n| n.to_string()).unwrap_or_else(|| {
        let suffix = if draft { " (draft)" } else { "" };
        format!("{} - {}{}", recipe.name, chrono::Utc::now().format("%Y-%m-%d %H:%M"), suffix)
    });

    let run_id = schema::create_export_run(
        conn,
        recipe_id,
        recipe.library_id,
        &run_name,
        &recipe_snapshot,
    )?;

    // Create job with draft flag
    schema::create_job(
        conn,
        "export",
        recipe.library_id,
        None,
        Some(run_id),
        0, // normal priority
        &serde_json::json!({ "run_id": run_id, "draft": draft }),
    )?;

    Ok(run_id)
}
```

6.2 Update Job Runner

Update `src-tauri/src/jobs/runner.rs` to handle export jobs:

```rust
// In the job execution match statement:
match job.job_type.as_str() {
    "ingest" => run_ingest_job(conn, &job)?,
    "proxy" => run_proxy_job(conn, &job)?,
    "thumb" => run_thumb_job(conn, &job)?,
    "sprite" => run_sprite_job(conn, &job)?,
    "score" => run_score_job(conn, job.clip_id.unwrap())?,
    "hash_full" => run_hash_full_job(conn, &job)?,
    "export" => {
        let data: serde_json::Value = serde_json::from_str(&job.data)?;
        let run_id = data["run_id"].as_i64().ok_or_else(|| anyhow!("Missing run_id"))?;
        let draft = data["draft"].as_bool().unwrap_or(false);
        run_export_job(conn, run_id, draft)?;
    },
    _ => return Err(anyhow!("Unknown job type: {}", job.job_type)),
}
```

---

Part 7: CLI Commands

7.1 Add Export Commands

Add to `src-tauri/src/cli.rs`:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Create a new export recipe
    RecipeCreate {
        /// Recipe name
        name: String,

        /// Selection mode (by_date, by_event, by_favorites, all)
        #[arg(long)]
        mode: String,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Start date for by_date mode (YYYY-MM-DD)
        #[arg(long)]
        start_date: Option<String>,

        /// End date for by_date mode (YYYY-MM-DD)
        #[arg(long)]
        end_date: Option<String>,

        /// Minimum score for all mode (0-1)
        #[arg(long)]
        min_score: Option<f64>,

        /// Event folders for by_event mode (comma-separated relative paths)
        #[arg(long, value_delimiter = ',')]
        event_folders: Option<Vec<String>>,
    },

    /// List export recipes
    RecipeList {
        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,
    },

    /// Show export recipe details
    RecipeShow {
        /// Recipe ID
        recipe_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,
    },

    /// Delete an export recipe
    RecipeDelete {
        /// Recipe ID to delete
        recipe_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Run an export (generate VHS film)
    Export {
        /// Recipe ID to run
        recipe_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Custom name for this export
        #[arg(long)]
        name: Option<String>,

        /// Preview clips without rendering
        #[arg(long)]
        preview: bool,

        /// Use proxy files for quick draft
        #[arg(long)]
        draft: bool,
    },

    /// List export history
    ExportList {
        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Maximum number of exports to show
        #[arg(long, default_value = "20")]
        limit: i64,
    },

    /// Show export run details
    ExportShow {
        /// Export run ID
        run_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,

        /// Show included clips
        #[arg(long)]
        clips: bool,
    },

    /// Re-run a previous export
    ExportRerun {
        /// Export run ID to re-run
        run_id: i64,

        /// Library path
        #[arg(long)]
        library: Option<PathBuf>,
    },
}

// Handler implementations

fn handle_recipe_create(
    name: String,
    mode: String,
    library: Option<PathBuf>,
    start_date: Option<String>,
    end_date: Option<String>,
    min_score: Option<f64>,
    event_folders: Option<Vec<String>>,
) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let library = schema::get_library_by_path(&conn, &library_path.to_string_lossy())?
        .ok_or_else(|| anyhow!("Library not found"))?;

    // Build filters based on mode
    let filters = match mode.as_str() {
        "by_date" => {
            let start = start_date.ok_or_else(|| anyhow!("by_date mode requires --start-date"))?;
            let end = end_date.ok_or_else(|| anyhow!("by_date mode requires --end-date"))?;
            serde_json::json!({ "start_date": start, "end_date": end })
        }
        "by_favorites" => serde_json::json!({}),
        "all" => {
            let score = min_score.unwrap_or(0.5);
            serde_json::json!({ "min_score": score })
        }
        "by_event" => {
            let folders = event_folders.ok_or_else(|| {
                anyhow!("by_event mode requires --event-folders (comma-separated folder paths)")
            })?;
            if folders.is_empty() {
                return Err(anyhow!("by_event mode requires at least one event folder"));
            }
            serde_json::json!({ "event_folders": folders })
        }
        _ => return Err(anyhow!("Unknown mode: {}", mode)),
    };

    let recipe_id = schema::create_export_recipe(&conn, library.id, &name, &mode, &filters)?;

    println!("Created recipe {} with ID {}", name, recipe_id);

    Ok(())
}

fn handle_recipe_delete(recipe_id: i64, library: Option<PathBuf>, force: bool) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let recipe = schema::get_export_recipe(&conn, recipe_id)?
        .ok_or_else(|| anyhow!("Recipe not found"))?;

    if !force {
        println!("Delete recipe '{}' (ID: {})? This cannot be undone.", recipe.name, recipe_id);
        println!("Use --force to skip this prompt.");
        return Ok(());
    }

    // Check for existing runs using this recipe
    let runs: i64 = conn.query_row(
        "SELECT COUNT(*) FROM export_runs WHERE recipe_id = ?1",
        rusqlite::params![recipe_id],
        |row| row.get(0),
    )?;

    if runs > 0 {
        println!("Warning: {} export runs reference this recipe.", runs);
        println!("Run history will be preserved but recipe link will be broken.");
    }

    conn.execute(
        "DELETE FROM export_recipes WHERE id = ?1",
        rusqlite::params![recipe_id],
    )?;

    println!("Deleted recipe '{}' (ID: {})", recipe.name, recipe_id);

    Ok(())
}

fn handle_recipe_list(library: Option<PathBuf>) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let library = schema::get_library_by_path(&conn, &library_path.to_string_lossy())?
        .ok_or_else(|| anyhow!("Library not found"))?;

    let recipes = schema::list_export_recipes(&conn, library.id)?;

    println!("Export Recipes");
    println!("--------------");

    for recipe in recipes {
        println!("{:>4}  {}  [{}]", recipe.id, recipe.name, recipe.mode);
    }

    Ok(())
}

fn handle_export(
    recipe_id: i64,
    library: Option<PathBuf>,
    name: Option<String>,
    preview: bool,
    draft: bool,
) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let recipe = schema::get_export_recipe(&conn, recipe_id)?
        .ok_or_else(|| anyhow!("Recipe not found"))?;

    if preview {
        // Preview mode: show clips without rendering
        let clips = export::selector::select_clips(&conn, &recipe, recipe.library_id)?;

        println!("Preview: {} clips selected", clips.len());
        println!("-----------------------");

        let mut total_duration_ms = 0i64;
        for (i, clip) in clips.iter().enumerate() {
            let duration_secs = clip.duration_ms as f64 / 1000.0;
            println!("{:>3}. {} ({:.1}s) - {}",
                i + 1,
                clip.recorded_at.split('T').next().unwrap_or(&clip.recorded_at),
                duration_secs,
                clip.inclusion_reason
            );
            total_duration_ms += clip.duration_ms;
        }

        let total_secs = total_duration_ms as f64 / 1000.0;
        let estimated_output = total_secs - ((clips.len() - 1) as f64 * 0.5); // Account for transitions
        println!();
        println!("Total: {:.1}s raw, ~{:.1}s with transitions", total_secs, estimated_output);

        return Ok(());
    }

    // Queue the export job with draft flag
    let run_id = jobs::queue_export_job(&conn, recipe_id, name.as_deref(), draft)?;
    println!("Export queued (run ID: {})", run_id);

    if draft {
        println!("Rendering draft from proxy files (faster, lower quality)...");
    } else {
        println!("Rendering from original files...");
    }

    // Process the export job
    jobs::process_pending_jobs(&conn)?;

    // Show result
    if let Some(run) = schema::get_export_run(&conn, run_id)? {
        match run.status.as_str() {
            "completed" => {
                println!("Export complete: {}", run.output_path.unwrap_or_default());
                println!("Duration: {:.1}s", run.total_duration_ms as f64 / 1000.0);
                if draft {
                    println!("Note: This is a draft render. Re-run without --draft for full quality.");
                }
            }
            "failed" => {
                println!("Export failed: {}", run.error_message.unwrap_or_default());
            }
            _ => {
                println!("Export status: {}", run.status);
            }
        }
    }

    Ok(())
}

fn handle_export_list(library: Option<PathBuf>, limit: i64) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let library = schema::get_library_by_path(&conn, &library_path.to_string_lossy())?
        .ok_or_else(|| anyhow!("Library not found"))?;

    let runs = schema::list_export_runs(&conn, library.id, limit)?;

    println!("Export History");
    println!("--------------");

    for run in runs {
        let status_icon = match run.status.as_str() {
            "completed" => "[OK]",
            "failed" => "[!!]",
            "rendering" => "[..]",
            _ => "[--]",
        };

        println!("{:>4}  {}  {}  {} clips  {}",
            run.id,
            status_icon,
            run.name,
            run.total_clips,
            run.created_at.split('T').next().unwrap_or(&run.created_at)
        );
    }

    Ok(())
}

fn handle_export_show(run_id: i64, library: Option<PathBuf>, show_clips: bool) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    let run = schema::get_export_run(&conn, run_id)?
        .ok_or_else(|| anyhow!("Export run not found"))?;

    println!("Export Run: {}", run.name);
    println!("------------{}", "-".repeat(run.name.len()));
    println!("ID:       {}", run.id);
    println!("Status:   {}", run.status);
    println!("Clips:    {}", run.total_clips);
    println!("Duration: {:.1}s", run.total_duration_ms as f64 / 1000.0);
    println!("Created:  {}", run.created_at);

    if let Some(output) = &run.output_path {
        println!("Output:   {}", output);
    }

    if let Some(error) = &run.error_message {
        println!("Error:    {}", error);
    }

    if show_clips {
        println!();
        println!("Included Clips:");

        let items = schema::get_export_run_items(&conn, run_id)?;
        for item in items {
            let clip = schema::get_clip(&conn, item.clip_id)?;
            if let Some(c) = clip {
                println!("  {:>3}. {} ({:.1}s) - {}",
                    item.sequence_order + 1,
                    c.title,
                    item.duration_ms as f64 / 1000.0,
                    item.inclusion_reason.unwrap_or_default()
                );
            }
        }
    }

    Ok(())
}

fn handle_export_rerun(run_id: i64, library: Option<PathBuf>) -> Result<()> {
    let library_path = resolve_library_path(library)?;
    let conn = db::open_db(&db::get_db_path(&library_path))?;

    // Get the original run
    let original_run = schema::get_export_run(&conn, run_id)?
        .ok_or_else(|| anyhow!("Export run not found"))?;

    // Extract recipe settings from the snapshot
    let recipe_snapshot = &original_run.recipe_snapshot;

    println!("Re-running export: {}", original_run.name);
    println!("Original run ID: {}", run_id);

    // Create a new run using the stored recipe snapshot
    // This ensures we use the exact same settings as the original
    let new_run_name = format!("{} (re-run)", original_run.name);

    let new_run_id = schema::create_export_run(
        &conn,
        original_run.recipe_id,
        original_run.library_id,
        &new_run_name,
        recipe_snapshot,
    )?;

    println!("New export queued (run ID: {})", new_run_id);
    println!("Rendering from original files...");

    // Create and process the export job
    schema::create_job(
        &conn,
        "export",
        original_run.library_id,
        None,
        Some(new_run_id),
        0,
        &serde_json::json!({ "run_id": new_run_id, "use_snapshot": true }),
    )?;

    jobs::process_pending_jobs(&conn)?;

    // Show result
    if let Some(run) = schema::get_export_run(&conn, new_run_id)? {
        match run.status.as_str() {
            "completed" => {
                println!("Export complete: {}", run.output_path.unwrap_or_default());
                println!("Duration: {:.1}s", run.total_duration_ms as f64 / 1000.0);
            }
            "failed" => {
                println!("Export failed: {}", run.error_message.unwrap_or_default());
            }
            _ => {
                println!("Export status: {}", run.status);
            }
        }
    }

    Ok(())
}
```

---

Part 8: Tauri Commands (Frontend Integration)

8.1 Add Export Commands

Create `src-tauri/src/commands/export.rs`:

```rust
use crate::db::schema;
use crate::export::{selector, assembler, ExportPlan, SelectedClip};
use crate::jobs;
use serde::{Deserialize, Serialize};
use tauri::State;
use super::DbState;

/// Create a new export recipe
#[tauri::command]
pub async fn create_export_recipe(
    name: String,
    mode: String,
    filters: serde_json::Value,
    state: State<'_, DbState>,
) -> Result<i64, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let library_id: i64 = conn
        .query_row("SELECT id FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    schema::create_export_recipe(conn, library_id, &name, &mode, &filters)
        .map_err(|e| e.to_string())
}

/// Get export recipe by ID
#[tauri::command]
pub async fn get_export_recipe(
    recipe_id: i64,
    state: State<'_, DbState>,
) -> Result<Option<schema::ExportRecipe>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    schema::get_export_recipe(conn, recipe_id)
        .map_err(|e| e.to_string())
}

/// List all export recipes
#[tauri::command]
pub async fn list_export_recipes(
    state: State<'_, DbState>,
) -> Result<Vec<schema::ExportRecipe>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let library_id: i64 = conn
        .query_row("SELECT id FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    schema::list_export_recipes(conn, library_id)
        .map_err(|e| e.to_string())
}

/// Update export recipe settings
#[tauri::command]
pub async fn update_export_recipe(
    recipe_id: i64,
    updates: schema::ExportRecipeUpdate,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    schema::update_export_recipe(conn, recipe_id, &updates)
        .map_err(|e| e.to_string())
}

/// Preview clips that would be selected for export
#[tauri::command]
pub async fn preview_export_selection(
    recipe_id: i64,
    state: State<'_, DbState>,
) -> Result<Vec<SelectedClip>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let recipe = schema::get_export_recipe(conn, recipe_id)
        .map_err(|e| e.to_string())?
        .ok_or("Recipe not found")?;

    selector::select_clips(conn, &recipe, recipe.library_id)
        .map_err(|e| e.to_string())
}

/// Start an export job
#[tauri::command]
pub async fn start_export(
    recipe_id: i64,
    name: Option<String>,
    draft: Option<bool>,
    state: State<'_, DbState>,
) -> Result<i64, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    jobs::queue_export_job(conn, recipe_id, name.as_deref(), draft.unwrap_or(false))
        .map_err(|e| e.to_string())
}

/// Delete an export recipe
#[tauri::command]
pub async fn delete_export_recipe(
    recipe_id: i64,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Verify recipe exists
    schema::get_export_recipe(conn, recipe_id)
        .map_err(|e| e.to_string())?
        .ok_or("Recipe not found")?;

    conn.execute(
        "DELETE FROM export_recipes WHERE id = ?1",
        rusqlite::params![recipe_id],
    ).map_err(|e| e.to_string())?;

    Ok(())
}

/// Re-run a previous export with the same settings
#[tauri::command]
pub async fn rerun_export(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<i64, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Get the original run
    let original_run = schema::get_export_run(conn, run_id)
        .map_err(|e| e.to_string())?
        .ok_or("Export run not found")?;

    // Create a new run using the stored recipe snapshot
    let new_run_name = format!("{} (re-run)", original_run.name);

    let new_run_id = schema::create_export_run(
        conn,
        original_run.recipe_id,
        original_run.library_id,
        &new_run_name,
        &original_run.recipe_snapshot,
    ).map_err(|e| e.to_string())?;

    // Queue the export job
    schema::create_job(
        conn,
        "export",
        original_run.library_id,
        None,
        Some(new_run_id),
        0,
        &serde_json::json!({ "run_id": new_run_id, "use_snapshot": true }),
    ).map_err(|e| e.to_string())?;

    Ok(new_run_id)
}

/// Get export run status
#[tauri::command]
pub async fn get_export_run(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<Option<schema::ExportRun>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    schema::get_export_run(conn, run_id)
        .map_err(|e| e.to_string())
}

/// List export history
#[tauri::command]
pub async fn list_export_runs(
    limit: i64,
    state: State<'_, DbState>,
) -> Result<Vec<schema::ExportRun>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let library_id: i64 = conn
        .query_row("SELECT id FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| e.to_string())?;

    schema::list_export_runs(conn, library_id, limit)
        .map_err(|e| e.to_string())
}

/// Get clips included in an export run
#[tauri::command]
pub async fn get_export_run_items(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<Vec<schema::ExportRunItem>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    schema::get_export_run_items(conn, run_id)
        .map_err(|e| e.to_string())
}

/// Cancel an in-progress export
#[tauri::command]
pub async fn cancel_export(
    run_id: i64,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    schema::update_export_run_status(conn, run_id, "cancelled", None, None)
        .map_err(|e| e.to_string())
}

/// Get list of available LUTs
#[tauri::command]
pub async fn list_luts(
    state: State<'_, DbState>,
) -> Result<Vec<LutInfo>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let mut stmt = conn.prepare(
        "SELECT id, name, filename, description, is_bundled FROM luts ORDER BY name"
    ).map_err(|e| e.to_string())?;

    let rows = stmt.query_map([], |row| {
        Ok(LutInfo {
            id: row.get(0)?,
            name: row.get(1)?,
            filename: row.get(2)?,
            description: row.get(3)?,
            is_bundled: row.get::<_, i64>(4)? != 0,
        })
    }).map_err(|e| e.to_string())?;

    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LutInfo {
    pub id: i64,
    pub name: String,
    pub filename: String,
    pub description: Option<String>,
    pub is_bundled: bool,
}
```

8.2 Register Commands

Update `src-tauri/src/commands/mod.rs`:

```rust
pub mod clips;
pub mod tags;
pub mod library;
pub mod scoring;
pub mod export;

pub use clips::*;
pub use tags::*;
pub use library::*;
pub use scoring::*;
pub use export::*;
```

Update `src-tauri/src/lib.rs`:

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::create_export_recipe,
    commands::get_export_recipe,
    commands::list_export_recipes,
    commands::update_export_recipe,
    commands::delete_export_recipe,
    commands::preview_export_selection,
    commands::start_export,
    commands::get_export_run,
    commands::list_export_runs,
    commands::get_export_run_items,
    commands::cancel_export,
    commands::rerun_export,
    commands::list_luts,
])
```

---

Part 9: Frontend UI Components

9.1 TypeScript Types

Add to `src/types/export.ts`:

```typescript
export interface ExportRecipe {
  id: number;
  libraryId: number;
  name: string;
  description?: string;
  mode: 'by_date' | 'by_event' | 'by_favorites' | 'all';
  filters: {
    startDate?: string;
    endDate?: string;
    eventFolders?: string[];
    minScore?: number;
  };
  ordering: 'chronological' | 'score_desc' | 'score_asc' | 'shuffle';
  pacing: 'full' | 'trimmed';
  maxClipDurationSecs?: number;
  transitionType: 'crossfade' | 'hard_cut';
  transitionDurationMs: number;
  showDateOverlay: boolean;
  dateOverlayPosition?: string;
  dateOverlayFormat?: string;
  dateOverlayDurationSecs?: number;
  lutId?: string;
  outputPreset: 'share' | 'archive';
  createdAt: string;
  updatedAt: string;
}

export interface ExportRun {
  id: number;
  recipeId: number;
  libraryId: number;
  name: string;
  recipeSnapshot: ExportRecipe;
  outputAssetId?: number;
  outputPath?: string;
  totalClips: number;
  totalDurationMs: number;
  status: 'pending' | 'rendering' | 'completed' | 'failed' | 'cancelled';
  progress?: number;
  errorMessage?: string;
  ffmpegCommand?: string;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
}

export interface ExportRunItem {
  id: number;
  runId: number;
  clipId: number;
  sequenceOrder: number;
  startOffsetMs: number;
  durationMs: number;
  clipStartMs: number;
  clipEndMs?: number;
  inclusionReason?: string;
}

export interface SelectedClip {
  clipId: number;
  assetPath: string;
  durationMs: number;
  recordedAt: string;
  score?: number;
  inclusionReason: string;
}

export interface LutInfo {
  id: number;
  name: string;
  filename: string;
  description?: string;
  isBundled: boolean;
}
```

9.2 API Functions

Add to `src/api/export.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';
import type {
  ExportRecipe,
  ExportRun,
  ExportRunItem,
  SelectedClip,
  LutInfo,
} from '../types/export';

export async function createExportRecipe(
  name: string,
  mode: string,
  filters: Record<string, unknown>
): Promise<number> {
  return invoke('create_export_recipe', { name, mode, filters });
}

export async function getExportRecipe(recipeId: number): Promise<ExportRecipe | null> {
  return invoke('get_export_recipe', { recipeId });
}

export async function listExportRecipes(): Promise<ExportRecipe[]> {
  return invoke('list_export_recipes');
}

export async function updateExportRecipe(
  recipeId: number,
  updates: Partial<ExportRecipe>
): Promise<void> {
  return invoke('update_export_recipe', { recipeId, updates });
}

export async function deleteExportRecipe(recipeId: number): Promise<void> {
  return invoke('delete_export_recipe', { recipeId });
}

export async function previewExportSelection(recipeId: number): Promise<SelectedClip[]> {
  return invoke('preview_export_selection', { recipeId });
}

export async function startExport(
  recipeId: number,
  name?: string,
  draft: boolean = false
): Promise<number> {
  return invoke('start_export', { recipeId, name, draft });
}

export async function getExportRun(runId: number): Promise<ExportRun | null> {
  return invoke('get_export_run', { runId });
}

export async function listExportRuns(limit: number = 20): Promise<ExportRun[]> {
  return invoke('list_export_runs', { limit });
}

export async function getExportRunItems(runId: number): Promise<ExportRunItem[]> {
  return invoke('get_export_run_items', { runId });
}

export async function cancelExport(runId: number): Promise<void> {
  return invoke('cancel_export', { runId });
}

export async function rerunExport(runId: number): Promise<number> {
  return invoke('rerun_export', { runId });
}

export async function listLuts(): Promise<LutInfo[]> {
  return invoke('list_luts');
}
```

9.3 Export Recipe Builder Component

Create `src/components/ExportRecipeBuilder.tsx`:

```typescript
import { useState } from 'react';
import type { ExportRecipe } from '../types/export';
import { createExportRecipe, updateExportRecipe } from '../api/export';

interface ExportRecipeBuilderProps {
  existingRecipe?: ExportRecipe;
  onSave: (recipeId: number) => void;
  onCancel: () => void;
}

export function ExportRecipeBuilder({
  existingRecipe,
  onSave,
  onCancel,
}: ExportRecipeBuilderProps) {
  const [name, setName] = useState(existingRecipe?.name || 'New Export');
  const [mode, setMode] = useState<ExportRecipe['mode']>(existingRecipe?.mode || 'by_favorites');
  const [startDate, setStartDate] = useState(existingRecipe?.filters.startDate || '');
  const [endDate, setEndDate] = useState(existingRecipe?.filters.endDate || '');
  const [minScore, setMinScore] = useState(existingRecipe?.filters.minScore || 0.5);
  const [showDateOverlay, setShowDateOverlay] = useState(existingRecipe?.showDateOverlay || false);
  const [outputPreset, setOutputPreset] = useState(existingRecipe?.outputPreset || 'share');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const filters: Record<string, unknown> = {};

      switch (mode) {
        case 'by_date':
          if (!startDate || !endDate) {
            throw new Error('Date range required for By Date mode');
          }
          filters.start_date = startDate;
          filters.end_date = endDate;
          break;
        case 'all':
          filters.min_score = minScore;
          break;
      }

      if (existingRecipe) {
        await updateExportRecipe(existingRecipe.id, {
          name,
          showDateOverlay,
          outputPreset,
        });
        onSave(existingRecipe.id);
      } else {
        const recipeId = await createExportRecipe(name, mode, filters);
        onSave(recipeId);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save recipe');
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div style={containerStyle}>
      <h2 style={titleStyle}>
        {existingRecipe ? 'Edit Recipe' : 'Create Export Recipe'}
      </h2>

      {error && <div style={errorStyle}>{error}</div>}

      <div style={fieldStyle}>
        <label>Recipe Name</label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          style={inputStyle}
        />
      </div>

      <div style={fieldStyle}>
        <label>Selection Mode</label>
        <select
          value={mode}
          onChange={(e) => setMode(e.target.value as ExportRecipe['mode'])}
          style={inputStyle}
          disabled={!!existingRecipe}
        >
          <option value="by_favorites">Favorites Only</option>
          <option value="by_date">By Date Range</option>
          <option value="all">All Clips (by score)</option>
          <option value="by_event">By Event</option>
        </select>
      </div>

      {mode === 'by_date' && (
        <>
          <div style={fieldStyle}>
            <label>Start Date</label>
            <input
              type="date"
              value={startDate}
              onChange={(e) => setStartDate(e.target.value)}
              style={inputStyle}
            />
          </div>
          <div style={fieldStyle}>
            <label>End Date</label>
            <input
              type="date"
              value={endDate}
              onChange={(e) => setEndDate(e.target.value)}
              style={inputStyle}
            />
          </div>
        </>
      )}

      {mode === 'all' && (
        <div style={fieldStyle}>
          <label>Minimum Score: {Math.round(minScore * 100)}%</label>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            value={minScore}
            onChange={(e) => setMinScore(parseFloat(e.target.value))}
            style={inputStyle}
          />
        </div>
      )}

      <div style={fieldStyle}>
        <label style={checkboxLabelStyle}>
          <input
            type="checkbox"
            checked={showDateOverlay}
            onChange={(e) => setShowDateOverlay(e.target.checked)}
          />
          Show date overlay on clips
        </label>
      </div>

      <div style={fieldStyle}>
        <label>Output Quality</label>
        <select
          value={outputPreset}
          onChange={(e) => setOutputPreset(e.target.value as 'share' | 'archive')}
          style={inputStyle}
        >
          <option value="share">Share (H.264, smaller file)</option>
          <option value="archive">Archive (ProRes, highest quality)</option>
        </select>
      </div>

      <div style={buttonRowStyle}>
        <button onClick={onCancel} style={cancelButtonStyle}>
          Cancel
        </button>
        <button onClick={handleSave} disabled={isLoading} style={saveButtonStyle}>
          {isLoading ? 'Saving...' : 'Save Recipe'}
        </button>
      </div>
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  padding: '20px',
  backgroundColor: '#2a2a2a',
  borderRadius: '8px',
  maxWidth: '500px',
};

const titleStyle: React.CSSProperties = {
  margin: '0 0 20px 0',
  fontSize: '18px',
};

const fieldStyle: React.CSSProperties = {
  marginBottom: '16px',
};

const inputStyle: React.CSSProperties = {
  width: '100%',
  padding: '8px',
  backgroundColor: '#333',
  border: '1px solid #444',
  borderRadius: '4px',
  color: 'white',
  marginTop: '4px',
};

const checkboxLabelStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: '8px',
  cursor: 'pointer',
};

const buttonRowStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'flex-end',
  gap: '8px',
  marginTop: '20px',
};

const cancelButtonStyle: React.CSSProperties = {
  padding: '8px 16px',
  backgroundColor: '#444',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  cursor: 'pointer',
};

const saveButtonStyle: React.CSSProperties = {
  padding: '8px 16px',
  backgroundColor: '#4a9eff',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  cursor: 'pointer',
};

const errorStyle: React.CSSProperties = {
  padding: '8px',
  backgroundColor: '#ef4444',
  borderRadius: '4px',
  marginBottom: '16px',
};
```

9.4 Export View Component

Create `src/components/ExportView.tsx`:

```typescript
import { useState, useEffect, useCallback } from 'react';
import type { ExportRecipe, ExportRun, SelectedClip } from '../types/export';
import {
  listExportRecipes,
  listExportRuns,
  previewExportSelection,
  startExport,
  getExportRun,
} from '../api/export';
import { ExportRecipeBuilder } from './ExportRecipeBuilder';

export function ExportView() {
  const [recipes, setRecipes] = useState<ExportRecipe[]>([]);
  const [runs, setRuns] = useState<ExportRun[]>([]);
  const [selectedRecipe, setSelectedRecipe] = useState<ExportRecipe | null>(null);
  const [previewClips, setPreviewClips] = useState<SelectedClip[]>([]);
  const [showBuilder, setShowBuilder] = useState(false);
  const [activeRunId, setActiveRunId] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const loadData = useCallback(async () => {
    try {
      const [recipeList, runList] = await Promise.all([
        listExportRecipes(),
        listExportRuns(10),
      ]);
      setRecipes(recipeList);
      setRuns(runList);
    } catch (err) {
      console.error('Failed to load export data:', err);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Poll for active export progress
  useEffect(() => {
    if (!activeRunId) return;

    const interval = setInterval(async () => {
      const run = await getExportRun(activeRunId);
      if (run) {
        setRuns((prev) => prev.map((r) => (r.id === activeRunId ? run : r)));
        if (run.status === 'completed' || run.status === 'failed') {
          setActiveRunId(null);
        }
      }
    }, 1000);

    return () => clearInterval(interval);
  }, [activeRunId]);

  const handleSelectRecipe = async (recipe: ExportRecipe) => {
    setSelectedRecipe(recipe);
    setIsLoading(true);
    try {
      const clips = await previewExportSelection(recipe.id);
      setPreviewClips(clips);
    } catch (err) {
      console.error('Failed to preview clips:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleStartExport = async () => {
    if (!selectedRecipe) return;

    setIsLoading(true);
    try {
      const runId = await startExport(selectedRecipe.id);
      setActiveRunId(runId);
      await loadData();
    } catch (err) {
      console.error('Failed to start export:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const formatDuration = (ms: number) => {
    const secs = Math.floor(ms / 1000);
    const mins = Math.floor(secs / 60);
    const remainingSecs = secs % 60;
    return `${mins}:${remainingSecs.toString().padStart(2, '0')}`;
  };

  if (showBuilder) {
    return (
      <ExportRecipeBuilder
        onSave={() => {
          setShowBuilder(false);
          loadData();
        }}
        onCancel={() => setShowBuilder(false)}
      />
    );
  }

  return (
    <div style={containerStyle}>
      <div style={headerStyle}>
        <h2 style={titleStyle}>VHS Mode</h2>
        <button onClick={() => setShowBuilder(true)} style={newRecipeButtonStyle}>
          New Recipe
        </button>
      </div>

      <div style={contentStyle}>
        {/* Recipes Section */}
        <div style={sectionStyle}>
          <h3>Export Recipes</h3>
          {recipes.length === 0 ? (
            <p style={emptyStyle}>No recipes yet. Create one to get started.</p>
          ) : (
            <div style={listStyle}>
              {recipes.map((recipe) => (
                <div
                  key={recipe.id}
                  onClick={() => handleSelectRecipe(recipe)}
                  style={{
                    ...recipeCardStyle,
                    borderColor: selectedRecipe?.id === recipe.id ? '#4a9eff' : '#333',
                  }}
                >
                  <div style={recipeNameStyle}>{recipe.name}</div>
                  <div style={recipeModeStyle}>{recipe.mode.replace('_', ' ')}</div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Preview Section */}
        {selectedRecipe && (
          <div style={sectionStyle}>
            <h3>Preview: {selectedRecipe.name}</h3>
            {isLoading ? (
              <p>Loading preview...</p>
            ) : (
              <>
                <p style={previewSummaryStyle}>
                  {previewClips.length} clips, ~{formatDuration(
                    previewClips.reduce((sum, c) => sum + c.durationMs, 0)
                  )} total
                </p>
                <div style={previewListStyle}>
                  {previewClips.slice(0, 10).map((clip, i) => (
                    <div key={clip.clipId} style={previewClipStyle}>
                      <span>{i + 1}.</span>
                      <span>{clip.recordedAt.split('T')[0]}</span>
                      <span>{formatDuration(clip.durationMs)}</span>
                    </div>
                  ))}
                  {previewClips.length > 10 && (
                    <p style={moreClipsStyle}>
                      ...and {previewClips.length - 10} more clips
                    </p>
                  )}
                </div>
                <button
                  onClick={handleStartExport}
                  disabled={isLoading || previewClips.length === 0}
                  style={exportButtonStyle}
                >
                  Generate VHS Film
                </button>
              </>
            )}
          </div>
        )}

        {/* Export History */}
        <div style={sectionStyle}>
          <h3>Export History</h3>
          {runs.length === 0 ? (
            <p style={emptyStyle}>No exports yet.</p>
          ) : (
            <div style={listStyle}>
              {runs.map((run) => (
                <div key={run.id} style={runCardStyle}>
                  <div style={runHeaderStyle}>
                    <span style={runNameStyle}>{run.name}</span>
                    <span style={runStatusStyle(run.status)}>{run.status}</span>
                  </div>
                  {run.status === 'rendering' && (
                    <div style={progressBarContainerStyle}>
                      <div
                        style={{
                          ...progressBarStyle,
                          width: `${run.progress || 0}%`,
                        }}
                      />
                    </div>
                  )}
                  <div style={runMetaStyle}>
                    {run.totalClips} clips | {formatDuration(run.totalDurationMs)}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// Styles
const containerStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  overflow: 'hidden',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  padding: '16px',
  backgroundColor: '#2a2a2a',
  borderBottom: '1px solid #333',
};

const titleStyle: React.CSSProperties = {
  margin: 0,
  fontSize: '18px',
};

const newRecipeButtonStyle: React.CSSProperties = {
  padding: '8px 16px',
  backgroundColor: '#4a9eff',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  cursor: 'pointer',
};

const contentStyle: React.CSSProperties = {
  flex: 1,
  overflow: 'auto',
  padding: '16px',
  display: 'flex',
  flexDirection: 'column',
  gap: '24px',
};

const sectionStyle: React.CSSProperties = {
  backgroundColor: '#2a2a2a',
  borderRadius: '8px',
  padding: '16px',
};

const listStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: '8px',
};

const recipeCardStyle: React.CSSProperties = {
  padding: '12px',
  backgroundColor: '#222',
  borderRadius: '4px',
  border: '2px solid #333',
  cursor: 'pointer',
};

const recipeNameStyle: React.CSSProperties = {
  fontWeight: 'bold',
};

const recipeModeStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#888',
  textTransform: 'capitalize',
};

const previewSummaryStyle: React.CSSProperties = {
  color: '#888',
  marginBottom: '12px',
};

const previewListStyle: React.CSSProperties = {
  maxHeight: '200px',
  overflow: 'auto',
};

const previewClipStyle: React.CSSProperties = {
  display: 'flex',
  gap: '12px',
  padding: '4px 0',
  fontSize: '12px',
  color: '#ccc',
};

const moreClipsStyle: React.CSSProperties = {
  color: '#666',
  fontSize: '12px',
  marginTop: '8px',
};

const exportButtonStyle: React.CSSProperties = {
  width: '100%',
  padding: '12px',
  backgroundColor: '#22c55e',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  fontSize: '14px',
  fontWeight: 'bold',
  cursor: 'pointer',
  marginTop: '16px',
};

const runCardStyle: React.CSSProperties = {
  padding: '12px',
  backgroundColor: '#222',
  borderRadius: '4px',
};

const runHeaderStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
};

const runNameStyle: React.CSSProperties = {
  fontWeight: 'bold',
};

const runStatusStyle = (status: string): React.CSSProperties => ({
  padding: '2px 8px',
  borderRadius: '4px',
  fontSize: '11px',
  backgroundColor:
    status === 'completed' ? '#22c55e' :
    status === 'failed' ? '#ef4444' :
    status === 'rendering' ? '#eab308' :
    '#666',
});

const runMetaStyle: React.CSSProperties = {
  fontSize: '12px',
  color: '#888',
  marginTop: '4px',
};

const progressBarContainerStyle: React.CSSProperties = {
  height: '4px',
  backgroundColor: '#444',
  borderRadius: '2px',
  marginTop: '8px',
  overflow: 'hidden',
};

const progressBarStyle: React.CSSProperties = {
  height: '100%',
  backgroundColor: '#4a9eff',
  transition: 'width 0.3s',
};

const emptyStyle: React.CSSProperties = {
  color: '#666',
  textAlign: 'center',
  padding: '20px',
};
```

---

Part 10: Testing Workflow

10.1 CLI Testing

```bash
# 1. Create a test recipe (by_date mode)
dadcam recipe-create "Summer 2019" --mode by_date --start-date 2019-06-01 --end-date 2019-08-31 --library /path/to/library

# 2. Create a recipe (by_event mode with specific folders)
dadcam recipe-create "Trip to Grandmas" --mode by_event --event-folders "originals/2019-07-04,originals/2019-07-05" --library /path/to/library

# 3. List recipes
dadcam recipe-list --library /path/to/library

# 4. Show recipe details
dadcam recipe-show 1 --library /path/to/library

# 5. Preview without rendering
dadcam export 1 --preview --library /path/to/library

# 6. Run draft export (renders from proxies for quick preview)
dadcam export 1 --draft --library /path/to/library

# 7. Run full export
dadcam export 1 --name "Summer 2019 Final" --library /path/to/library

# 8. Check export history
dadcam export-list --library /path/to/library

# 9. Show export details with clip list
dadcam export-show 1 --clips --library /path/to/library

# 10. Re-run a previous export (creates new run with same settings)
dadcam export-rerun 1 --library /path/to/library

# 11. Delete a recipe (with confirmation prompt)
dadcam recipe-delete 1 --library /path/to/library

# 12. Delete a recipe (skip confirmation)
dadcam recipe-delete 2 --force --library /path/to/library
```

10.2 Verification Checklist

**Database:**
- [ ] Migration creates export_recipes table correctly
- [ ] Migration creates export_runs table correctly
- [ ] Migration creates export_run_items table correctly
- [ ] Default LUTs are inserted
- [ ] Foreign key constraints work
- [ ] Recipe deletion cascades to related runs

**Clip Selection:**
- [ ] by_date mode selects clips in date range
- [ ] by_event mode selects clips from specified event folders
- [ ] by_event mode with --event-folders filters correctly
- [ ] by_favorites mode only selects favorited clips
- [ ] all mode respects score threshold
- [ ] Ordering options work (chronological, score_desc, etc.)
- [ ] MAX_CLIPS_PER_EXPORT limit is enforced

**Export Assembly:**
- [ ] Single clip exports work
- [ ] Multi-clip exports with crossfades work
- [ ] Date overlays appear correctly
- [ ] Output presets apply correct codecs
- [ ] LUT application works correctly

**Draft Mode:**
- [ ] Draft flag is passed through CLI correctly
- [ ] Draft exports use proxy files instead of originals
- [ ] Draft exports render faster than full exports
- [ ] Draft output quality is acceptable for preview

**Export Rerun:**
- [ ] export-rerun creates new run with same recipe snapshot
- [ ] Rerun produces identical output (reproducibility)
- [ ] Rerun name includes "(re-run)" suffix

**Recipe Management:**
- [ ] recipe-delete prompts for confirmation without --force
- [ ] recipe-delete with --force deletes immediately
- [ ] Recipe deletion removes associated runs and items

**Job System:**
- [ ] Export jobs queue correctly
- [ ] Progress updates during render
- [ ] Failed exports report errors
- [ ] Cancelled exports stop cleanly
- [ ] Draft flag persists through job queue

**UI:**
- [ ] Recipe builder creates valid recipes
- [ ] Preview shows selected clips
- [ ] Export button starts render
- [ ] Draft checkbox triggers draft mode
- [ ] Progress bar updates
- [ ] History shows completed exports
- [ ] Re-run button works from history

10.3 Performance Testing

```bash
# Test with library of varying sizes
# Expected performance (rough guide):
# - 10 clips: ~30 seconds
# - 50 clips: ~3 minutes
# - 100 clips: ~6 minutes
# (Actual time depends on clip duration and hardware)

time dadcam export 1 --library /path/to/test-library
```

---

Part 11: Deferred Items

The following are documented for future phases:

1. **J/L Cuts**: Audio lead/trail for smoother transitions (requires segment-level audio analysis)

2. **Best Segment Pacing**: Use Phase 4 segment scoring to pick the best portion of each clip

3. **By Event Mode UI**: Need a folder browser to select source event folders (CLI supports --event-folders)

4. **LUT Creation**: Allow users to import custom .cube LUT files (bundled LUTs are functional)

5. **Background Rendering**: Move render to background process with notification on completion

6. **Export Templates**: Save complete recipe+settings as shareable template

7. **Music Track**: Add background music with auto-ducking

Note: Draft mode and --event-folders CLI flag are now implemented in this phase.

---

End of Phase 5 Implementation Guide


---

# Addendum: 100% Shippable Hardening (Phase 5 v1.1)

This addendum **does not expand scope** beyond VHS Mode v1. It makes the Phase 5 guide internally consistent, reproducible, and implementable without “magic gaps”.

## A) Transition OFFSET Contract (Required)

The guide previously used `offset=OFFSET` without defining OFFSET. The implementation MUST follow this rule:

### OFFSET definition

Let:

- `d[i]` = clip i duration in seconds (the *used* duration after trimming, if trimmed pacing is enabled)
- `t` = transition duration in seconds (e.g., 0.5)

Then for crossfades:

- For 2 clips: `offset = d[0] - t`
- For N clips (chain xfades):
  - Maintain `accumulated = d[0]`
  - For each transition from clip i-1 to clip i:
    - `offset_i = accumulated - t`
    - After adding clip i, update:
      - `accumulated = accumulated + d[i] - t`

This is the exact logic used in the assembler’s filter builder (`accumulated_duration` and `offset = accumulated_duration - transition_secs`). fileciteturn3file3 fileciteturn3file7

### Why this matters

If OFFSET is wrong, FFmpeg may:
- overlap the wrong regions,
- desync audio, or
- fail with “invalid argument” for xfade timing.

## B) Audio Smoothing Truth-in-Spec (Required)

VHS Mode v1 uses **simple audio crossfades** via `acrossfade` (same duration as the video xfade). J/L cuts are **deferred**.

Update wording everywhere to:

- ✅ “Audio smoothing via acrossfade crossfades”
- ⏳ “J/L cuts deferred”

This matches the pipeline narrative and the renderer implementation. fileciteturn3file2 fileciteturn3file3

## C) Draft Mode Contract (Required)

The checklist includes “Draft flag persists through job queue”, and the UI includes “Draft checkbox”. fileciteturn3file0  
To make Draft Mode real and reproducible:

### C.1 Schema update

Add columns to `export_runs`:

```sql
ALTER TABLE export_runs ADD COLUMN is_draft INTEGER NOT NULL DEFAULT 0;
ALTER TABLE export_runs ADD COLUMN source_role TEXT NOT NULL DEFAULT 'original'
    CHECK (source_role IN ('original', 'proxy'));
```

Rules:
- `is_draft=1` MUST imply `source_role='proxy'`
- `is_draft=0` defaults to `source_role='original'`

Store both in `recipe_snapshot` (see below) so re-renders are deterministic.

### C.2 Selection rule

When building the export plan:
- If `source_role='proxy'` and a proxy exists, use it
- Else, fall back to original

(Exact same “proxy-first when requested” philosophy used in Phase 4’s hardening addendum.) fileciteturn3file10

## D) LUT Reproducibility (Required)

Phase 5 correctly stores a `recipe_snapshot` for reproducibility. fileciteturn2file0  
However, LUT files can change on disk (bundled update, user replaces file), breaking perfect re-render.

### D.1 Add LUT hash

Add to `luts`:

```sql
ALTER TABLE luts ADD COLUMN sha256 TEXT;
```

And when rendering, compute LUT hash of the resolved LUT file path and store it inside `recipe_snapshot`, e.g.:

```json
{
  "lut": { "filename": "vhs_look.cube", "sha256": "..." }
}
```

If `lut_id` is NULL, snapshot should contain `"lut": null`.

## E) Filters Validation (Required)

`filters` is mode-specific JSON. fileciteturn2file0  
To prevent silent bad exports, validate filters before creating a run:

### E.1 Strongly typed structs (Rust)

```rust
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ByDateFilters { start_date: String, end_date: String }

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ByEventFilters { event_ids: Vec<i64> }

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AllFilters { min_score: Option<f64> }
```

Validation rules:
- Dates must parse (ISO-8601 date) and `start_date <= end_date`
- `event_ids` must be non-empty
- `min_score` clamps to `[0.0, 1.0]` and defaults to `ALL_MODE_MIN_SCORE_DEFAULT`

On validation failure:
- return an error to UI/CLI
- do NOT enqueue a render job

## F) Renderer Output + Job Logs (Required)

Phase 4 hardening establishes “capture raw tool output in job_logs on failure.” fileciteturn3file6  
Apply the same to exports:

- Write the full FFmpeg command (redacting absolute paths if desired)
- Store first N lines of stderr in `job_logs`
- On parse failure, store raw stderr snippet and mark run failed

This makes Phase 5 debuggable at scale.

## G) Update the “Deferred Items” section (Required)

The Phase 5 footer claims: “Draft mode and --event-folders CLI flag are now implemented in this phase.” fileciteturn3file0  
That sentence must only remain if the guide includes the schema + selection + CLI wiring (this addendum does). Otherwise, remove it.

---

# Phase 5 Verification Checklist (100% Shippable)

Use this as the definition of “Phase 5 is done”.

**Database**
- [x] export_recipes / export_runs / export_run_items tables exist and have indexes fileciteturn2file0
- [x] export_runs includes draft/source_role fields (Addendum C)
- [x] luts includes sha256 and is captured into recipe_snapshot (Addendum D)

**Assembler / FFmpeg graph**
- [x] OFFSET math is specified and matches implementation fileciteturn3file3
- [x] Audio smoothing is documented as acrossfade; J/L deferred fileciteturn3file2

**Validation**
- [x] filters JSON validated per mode before enqueue (Addendum E)

**Job system**
- [x] Export jobs queue correctly, update progress, and fail with captured stderr (Addendum F)
- [x] Cancel stops cleanly and marks run cancelled
- [x] Draft mode persists (run.is_draft + snapshot)

**UI**
- [x] Recipe builder saves valid filters per mode
- [x] Preview shows chosen clip list in deterministic order
- [x] Draft checkbox toggles source_role=proxy and is stored in run snapshot
- [x] History shows prior runs and “Re-run” reproduces identical output

