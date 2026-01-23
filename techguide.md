Dad Cam App — Technical Guide

This is the manual for the app. Core logic, CLI commands, and implementation details.

Version: 0.1.25

---

Overview

Dad Cam is a cross-platform video library for dad cam footage. It ingests, organizes, previews, scores, and auto-edits footage from old-school digital cameras.

Core loop: Ingest > Index > Preview > Pick Best > Auto-Edit > Export

See about.md for product definition.
See contracts.md for architectural decisions.

---

Tech Stack

Framework: Tauri 2.0
Frontend: React + TypeScript
Backend: Rust

Architecture:
```
Frontend (React/TS)
    |
    v Tauri Commands
Rust Backend
    |-- rusqlite (database)
    |-- blake3 (hashing)
    |-- ffmpeg-sidecar (video processing)
```

Key dependencies:
- rusqlite (bundled): SQLite database access
- blake3: BLAKE3 hashing (3+ GB/s with SIMD)
- ffmpeg-sidecar: FFmpeg/ffprobe wrapper with auto-download
- exiftool: Bundled as Tauri sidecar

Rust backend owns:
- All database operations
- All hashing operations
- All ffmpeg/exiftool calls

Frontend handles:
- UI rendering
- User interactions
- Calls Rust via Tauri commands

---

Library Structure

A Dad Cam library is a single folder containing:

```
my-library/
  .dadcam/
    dadcam.db          # SQLite database (source of truth)
    proxies/           # H.264 720p preview videos
    thumbs/            # Poster frame JPGs
    sprites/           # Hover scrub sprite sheets
    exports/           # Rendered output files
  originals/           # Copied source files (when using copy mode)
```

Reference mode: originals stay in their original location, only .dadcam/ is created.

---

CLI Commands

Phase 1 - Core:
- dadcam init <library_root> - Initialize a new library
- dadcam ingest <path> - Ingest footage into library
- dadcam list - List all clips
- dadcam show <clip_id> - Show clip details
- dadcam jobs - List all jobs
- dadcam relink-scan <path> - Find missing originals

Phase 2 - Previews:
- dadcam preview [--type <proxy|thumb|sprite|all>] [--clip <id>] [--force]
- dadcam preview-status [--missing-only]
- dadcam invalidate [--type <all>] [--confirm]
- dadcam cleanup [--scope <derived|orphans|all>] [--dedup] [--max-size-gb <n>] [--confirm]
- dadcam check-tools [--download]

Phase 4 - Scoring:
- dadcam score [--clip <id>] [--force] [--verbose]
- dadcam score-status [--missing-only]
- dadcam best-clips [--threshold 0.6] [--limit 20]
- dadcam score-override <clip_id> <promote|demote|pin|clear> [--value] [--note]

Phase 5 - Export:
- dadcam recipe-create <name> --mode <by_date|by_event|by_favorites|all> [filters]
- dadcam recipe-list
- dadcam recipe-show <recipe_id>
- dadcam export <recipe_id> [--preview] [--draft] [--name <string>]
- dadcam export-list [--limit 20]
- dadcam export-show <run_id> [--clips]
- dadcam export-rerun <run_id>

Phase 6 - Export System:
- dadcam export-history [--status <completed|failed|cancelled>] [--limit 20]
- dadcam export-details <run_id> [--logs]
- dadcam export-open <run_id> [--folder]

Phase 7 - Pro Mode:
- dadcam init-reference --path <library_root> --name <name> - Create reference-mode library (NAS workflow)
- dadcam batch-ingest <source1> [source2...] --library <path> --name <name> - Batch ingest from multiple sources
- dadcam batch-export <recipe_id1> [recipe_id2...] --library <path> --name <name> - Batch export multiple recipes
- dadcam relink <scan_path> --library <path> [--auto-apply] [--min-confidence 0.95] - Scan and relink offline clips
- dadcam list-offline --library <path> - List clips with missing originals
- dadcam list-presets - List all codec presets (system and custom)
- dadcam create-preset <name> --video-codec <codec> [--crf <n>] [--bitrate <rate>] [--audio-codec <codec>] [--container <fmt>] [--description <text>] - Create custom codec preset
- dadcam delete-preset <name> - Delete custom codec preset (system presets protected)
- dadcam volume-info --library <path> [--clip-id <id>] - Show volume information for clips

Phase 8 - ML and Intelligence:
- dadcam ml-analyze [--clip <id>] [--force] [--verbose] - Run ML analysis (face, emotion, speech, motion)
- dadcam ml-status [--missing-only] - Show ML analysis progress
- dadcam train-scoring [--verbose] - Train personalized scoring model from user feedback
- dadcam best-clips-ml [--threshold 0.6] [--limit 20] - List best clips using combined ML + heuristic scoring

---

Database Schema

(Populated in Phase 1)

See docs/planning/phase-0.md for logical schema design.

---

Job System

(Populated in Phase 1)

Job types:
- ingest: Discover, copy/reference, hash, extract metadata
- proxy: Generate H.264 720p preview
- thumb: Generate poster frame
- sprite: Generate hover scrub sprite sheet
- hash_full: Full BLAKE3 hash (background)
- score: Run heuristic scoring
- export: Render final output
- batch_ingest: Process multiple ingest sources sequentially (Phase 7)
- batch_export: Render multiple export recipes sequentially (Phase 7)
- relink_scan: Scan path and match against offline clips (Phase 7)
- ml_analysis: Run ML analysis on clip (face, emotion, speech, motion) (Phase 8)

Jobs are durable, resumable, and crash-safe.

---

Camera Profiles

(Populated in Phase 1)

Camera profiles are JSON files that describe:
- Match rules (how to detect this camera)
- Transform rules (deinterlace, color, etc.)

Profiles are versioned. Changing a profile version invalidates derived assets.

---

Hashing

Algorithm: BLAKE3
Crate: blake3 (Rust, official implementation)
Performance: 3+ GB/s single-threaded with SIMD

Fast hash (during ingest):
- First 1MB + last 1MB + file size
- Used for quick dedup
- Runs synchronously during copy

Full hash (background job):
- Entire file
- Used for verification and relink
- Runs as queued job after ingest

---

Metadata Extraction

ffprobe: codec, resolution, duration, fps, audio streams
exiftool: dates, camera make/model, GPS (if present)

Timestamp precedence:
1. Embedded metadata
2. Folder name parsing
3. Filesystem modified date

---

Proxy Generation

Output: H.264, 720p, CFR, AAC audio
Purpose: Smooth playback in app without decoding originals

FFmpeg command pattern:
```
ffmpeg -i input.mts \
  -vf "yadif=mode=1,scale=-2:720" \
  -c:v libx264 -preset medium -crf 23 \
  -r 30 -c:a aac -b:a 128k \
  -movflags +faststart output.mp4
```

Features:
- Auto-deinterlace for interlaced sources (yadif filter)
- Scale to 720p height, maintain aspect ratio
- Constant 30fps for smooth playback
- Optional LUT application for preview look
- Audio-only clips get m4a proxy

See docs/planning/phase-2.md for implementation details

---

Thumbnail Generation

Output: JPG poster frame per clip
Selection: Frame at 10% of duration (avoids black frames at start)

FFmpeg command pattern:
```
ffmpeg -ss 00:00:05 -i input.mts \
  -vframes 1 -vf "scale='min(480,iw)':-1" \
  -q:v 2 output.jpg
```

Features:
- Max 480px wide, maintain aspect ratio
- Quality setting from THUMB_QUALITY constant
- Handles images as well as video

See docs/planning/phase-2.md for implementation details

---

Sprite Sheet Generation

Output: Tiled JPG strip for hover scrubbing
FPS: 1 frame per second
Tile width: 160px
Max frames: 600 (10 minutes of video)
Frames per page: 60 (paging for long videos)

FFmpeg command pattern:
```
ffmpeg -i input.mts \
  -vf "fps=1,scale=160:90,tile=10x6" \
  -vframes 60 -q:v 2 output.jpg
```

Features:
- Tiled grid of frames (10 columns, up to 6 rows per page)
- Multi-page sprites for videos longer than 60 seconds
- Metadata JSON stored alongside (fps, tile_width, tile_height, frame_count, columns, rows, interval_ms, page_index, page_count)
- WebVTT file generated for video player scrub preview
- CSS/JS calculates tile position from hover percentage
- Only generated for video clips (not audio/images)

See docs/planning/phase-2.md for implementation details

---

Cleanup Command

Removes orphaned derived files and manages storage.

Scopes:
- orphans: Files in derived directories not linked in database
- derived: Duplicate derived assets (keep newest per clip/role)
- all: Both orphans and derived

Features:
- Dry-run by default (use --confirm to actually delete)
- --dedup flag removes duplicate derived assets per clip/role
- --max-size-gb cap deletes oldest derived assets when exceeded
- Reports freed space after cleanup

Command pattern:
```
dadcam cleanup --scope all --dedup --max-size-gb 50 --confirm
```

---

Check Tools Command

Verifies required tools (ffmpeg, ffprobe, exiftool) are available.

Features:
- Shows tool status (OK/MISSING) and path
- --download flag attempts to auto-download FFmpeg
- ExifTool must be installed manually

Command pattern:
```
dadcam check-tools --download
```

---

Scoring Engine

Heuristic scoring using FFmpeg analysis (no ML required).

Components (each weighted 25%):
- Scene change density: Uses scdet filter, measures scene changes per minute
- Audio loudness stability: Uses ebur128 filter, measures LUFS and LRA
- Sharpness: Uses blurdetect filter, inverted blur score
- Motion: Uses frame differencing (tblend), measures frame-to-frame changes

Output: Overall score 0-1 + component scores + reasons array

Database tables:
- clip_scores: Stores machine-generated scores with pipeline versioning
- clip_score_overrides: Stores user promote/demote/pin preferences

User override types:
- promote: Add 0.2 to machine score
- demote: Subtract 0.2 from machine score
- pin: Set exact score (for "always include" or "never include")

Effective score = machine score adjusted by user override

CLI commands:
- dadcam score [--clip ID] [--force] [--verbose]
- dadcam score-status [--missing-only]
- dadcam best-clips [--threshold 0.6] [--limit 20]
- dadcam score-override <clip_id> <promote|demote|pin|clear> [--value] [--note]

Invalidation: Scores regenerate when pipeline_version or scoring_version changes.

See docs/planning/phase-4.md for implementation details

---

Auto-Edit (VHS Mode)

One-button generation of nostalgic long-form movies. Not a full editor.

Selection Modes:
- by_date: Clips from a specific date range
- by_event: Clips grouped by source folder
- by_favorites: Only clips tagged as favorites
- all: All clips above score threshold (default 0.5)

Ordering Options:
- chronological: By recorded_at timestamp (default)
- score_desc: Best clips first
- score_asc: Build to climax
- shuffle: Deterministic random order

Pipeline:
1. Select clips based on mode + filters
2. Apply ordering rules
3. Concat with crossfade transitions (xfade + acrossfade filters)
4. Optional date overlays (drawtext filter)
5. Optional LUT application (lut3d filter)
6. Render to output preset

Transition Settings:
- Default duration: 500ms crossfade
- Type: crossfade (fade) or hard_cut

Output Presets:
- share: H.264, CRF 23, AAC 192k (social media)
- archive: ProRes 422 HQ, PCM audio (preservation)

Database Tables:
- export_recipes: Saved configurations (mode, filters, settings)
- export_runs: Execution history with recipe snapshots
- export_run_items: Which clips included and in what order
- luts: Registry of available color grading LUTs

CLI commands:
- dadcam recipe-create <name> --mode <mode> [options]
- dadcam recipe-list
- dadcam export <recipe_id> [--preview] [--draft]
- dadcam export-list
- dadcam export-show <run_id> [--clips]

Reproducibility:
- Recipe snapshot stored at run time
- Exact FFmpeg command logged
- Re-render produces identical output

See docs/planning/phase-5.md for implementation details

---

Export System

Professional export workflow with real-time progress tracking and history.

Output Presets:
- Share: H.264, CRF 23, AAC 192k (optimized for social media upload)
- Archive: ProRes 422 HQ, PCM audio (lossless preservation)

Progress Tracking:
- Real-time progress events via Tauri event system
- FFmpeg progress parsing (frame count, elapsed time, speed)
- Estimated time remaining calculation
- Current operation labels ("Encoding frame X of Y")

Export History UI:
- Browseable list of all past exports
- Thumbnail preview for completed exports
- Status badges (completed, failed, cancelled)
- File size and duration display
- Time-ago formatting

Actions:
- Play: Open export in default video player
- Folder: Open containing folder in file manager
- Re-run: Execute same export with current settings
- Delete: Remove export and output file

Failure Recovery:
- User-friendly error messages (not raw FFmpeg output)
- Automatic cleanup of partial files on failure
- Cancel support with immediate process termination
- Job logs stored for debugging

CLI Commands (Phase 6):
- dadcam export-history [--status <completed|failed|cancelled>] [--limit 20]
- dadcam export-details <run_id> [--logs]
- dadcam export-open <run_id> [--folder]

Database Additions:
- export_runs.current_operation: Current step label
- export_runs.frames_total/frames_completed: Progress tracking
- export_runs.output_size_bytes: Final file size
- export_runs.thumbnail_path: Preview thumbnail for history

Events:
- export-progress: Real-time progress updates
- export-cancel: Cancel signal from UI

See docs/planning/phase-6.md for implementation details.

---

Desktop App Architecture

(Implemented in Phase 3 - v0.1.23)

Frontend Stack:
- React 18+ with TypeScript
- TanStack Virtual for virtualized clip grid
- LRU cache for thumbnail memory management

Tauri Command Layer:
- DbState: Shared SQLite connection in Mutex
- Commands: open_library, close_library, get_clips, get_clip, toggle_tag, set_tag
- Phase 7 Commands: validate_reference_path, create_reference_library, create_batch_ingest, get_batch_progress, start_relink_scan, get_relink_candidates, apply_relink_candidate, get_offline_clips, list_codec_presets, create_codec_preset, get_clip_volume_info
- Phase 8 Commands: get_ml_analysis, analyze_clip_ml, get_ml_analysis_status, record_clip_view, train_personalized_scoring, get_best_clips_ml
- All responses use camelCase (serde rename_all)

Key Components:
- ClipGrid: Virtualized grid using TanStack Virtual rowVirtualizer
- ClipThumbnail: Individual clip card with hover detection
- SpriteHover: Mouse-position-based sprite frame display
- VideoPlayer: HTML5 video with keyboard shortcuts
- FilterBar: Filter buttons, search, date range, sort controls
- LibraryView: Main library browser container

Data Flow:
```
User Action
    |
React Component
    |
API Wrapper (invoke)
    |
Tauri Command (Rust)
    |
SQLite Query
    |
Response (JSON)
    |
React State Update
```

Virtualization Strategy:
- Only visible rows rendered (plus 3 row overscan)
- Thumbnail preloading for nearby items
- LRU cache with 500 entry limit
- Background preload queue for smooth scrolling

Sprite Scrubbing:
- Load sprite metadata JSON on hover
- Calculate frame index from mouse X position
- Update CSS background-position (no network calls)
- Progress bar shows current position

See docs/planning/phase-3.md for implementation details.

---

Pro Mode (Phase 7)

Production workflow features for professionals managing large footage libraries.

Reference Mode (NAS Workflow):
- Files stay in original location (not copied to originals/)
- Asset path field is empty, source_uri contains original location
- Volume tracking critical for relinking
- Network path detection: macOS (diskutil), Windows (UNC paths), Linux (/proc/mounts)
- Warning shown for removable media (reference mode recommended for network storage)

Batch Operations:
- Batch ingest: Queue multiple source paths, process sequentially
- Batch export: Queue multiple recipes, render sequentially
- Progress tracking: total_items, completed_items, failed_items
- Status states: pending, running, paused, completed, failed, cancelled

Relinking:
- Reconnects clips to original files when drives reconnect or files move
- Match types (by confidence):
  - full_hash: 100% confidence (full BLAKE3 match)
  - fast_hash: 95% confidence (chunked BLAKE3 match)
  - size_duration: 80% confidence (file size + duration match)
  - filename: 50% confidence (filename only match)
- Scan workflow: discover files > match against fingerprints > present candidates > apply
- Auto-apply option for high-confidence matches (default threshold: 95%)

Codec Presets:
- System presets (immutable): share, archive, web, master
- Custom presets: user-defined with full codec control
- Parameters: video_codec, video_params (JSON), audio_codec, audio_params (JSON), container
- Validation: checks codec/container compatibility before use

Volume Tracking:
- Volume info surfaced in UI (display_name, mount_point, is_network)
- Volume badge component shows source volume per clip
- Helps identify which drive/NAS a clip originated from

Database Tables (Migration 5):
- batch_operations: Groups multiple ingest/export jobs
- batch_ingest_sources: Source paths in batch ingest
- batch_export_items: Recipes in batch export
- codec_presets: System and custom encoding presets
- relink_sessions: Relink scan history
- relink_candidates: Potential matches with confidence scores

New Tauri Commands:
- validate_reference_path: Check if path is network/removable
- create_reference_library: Create library in reference mode
- create_batch_ingest: Queue batch ingest operation
- get_batch_progress: Poll batch operation progress
- start_relink_scan: Scan path for matches
- get_relink_candidates: List candidates for session
- apply_relink_candidate: Apply single match
- get_offline_clips: List clips with missing originals
- list_codec_presets: List all presets
- create_codec_preset: Create custom preset
- get_clip_volume_info: Get volume info for clip

UI Components:
- RelinkPanel: Scan input, candidate list, apply buttons
- BatchProgress: Progress bar with status, completed/failed counts
- VolumeBadge: Small badge showing source volume (network icon for NAS)

See docs/planning/phase-7.md for implementation details.

---

ML and Intelligence (Phase 8)

Machine learning capabilities to improve clip scoring beyond heuristics.

Technology Stack:
- ONNX Runtime (ort crate): Cross-platform ML inference, 100% offline
- Models bundled with app in resources/models/

Face Detection:
- Model: BlazeFace (128x128 input, ~400KB)
- Pipeline: Extract frames at 1 FPS > Run detection > NMS > Aggregate
- Output: face_count_avg, face_count_max, face_frames_percent, face_timestamps

FFmpeg frame extraction pattern:
```
ffmpeg -i input.mts -vf "fps=1" -q:v 2 frames/frame_%04d.jpg
```

Emotion Detection:
- Model: emotion-ferplus-8.onnx (~8MB)
- Input: 64x64 grayscale face crops
- Categories: neutral, happiness, surprise, sadness, anger, disgust, fear, contempt
- Output: emotion probabilities, dominant emotion, smile_frames_percent

Voice Activity Detection:
- Model: Silero VAD v5 (~3MB)
- Input: Audio extracted at 16kHz mono
- Output: speech_percent, speech_segments (timestamps), speech_duration_ms

Motion Analysis:
- Method: FFmpeg optical flow (mpdecimate, tblend filters)
- Output: motion_flow_score, motion_stability_score, motion_activity_level

Combined Scoring:
- Heuristic score (Phase 4): 40% weight
- ML score (Phase 8): 40% weight
- Personalized boost: 20% weight
- Effective range: 0.0 to 1.0

Personalized Scoring:
- Learns from explicit feedback (favorites, bad tags)
- Learns from implicit feedback (views, watch time, skips, exports)
- Feature weights stored in scoring_models table
- Retrain via CLI or background job

Database Tables (Migration 6):
- ml_analyses: Per-clip ML results (faces, emotions, speech, motion, combined score)
- user_interactions: Implicit feedback tracking (views, watch time, skips)
- scoring_models: Learned feature weights per library

CLI Commands:
- dadcam ml-analyze [--clip ID] [--force] [--verbose]
- dadcam ml-status [--missing-only]
- dadcam train-scoring [--verbose]
- dadcam best-clips-ml [--threshold 0.6] [--limit 20]

Tauri Commands:
- get_ml_analysis: Get ML results for clip
- analyze_clip_ml: Queue ML analysis job
- get_ml_analysis_status: Get library analysis progress
- record_clip_view: Track playback for personalized scoring
- train_personalized_scoring: Retrain scoring model
- get_best_clips_ml: Get best clips with combined scoring

UI Components:
- MlScoreBadge: Score badge with face/smile/speech indicators
- MlInsightsPanel: Detailed ML results for selected clip
- MlAnalysisProgress: Library analysis progress bar
- useViewTracking: Hook for recording playback behavior

Error Handling:
- Model load failures: Graceful degradation, partial analysis continues
- Corrupt video: Each analysis runs independently, partial results stored
- Resume support: Interrupted analyses can be completed later
- User messages: Technical errors mapped to friendly descriptions

See docs/planning/phase-8.md for implementation details.

---

End of Technical Guide
