Dad Cam App — Technical Guide

This is the manual for the app. Core logic, CLI commands, and implementation details.

Version: 0.1.132

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
    sidecars/          # Per-clip metadata JSON
  originals/           # Copied source files (when using copy mode)
```

Reference mode: originals stay in their original location, only .dadcam/ is created.

---

Dual Database Architecture (Library Fix)

Dad Cam uses two SQLite databases with distinct responsibilities.
See docs/planning/libraryfix.md for the full spec.
See contracts.md #19-22 for the architectural contracts.

App DB (`~/.dadcam/app.db`):
- User-global. Survives library deletion/moves.
- Tables:
  - bundled_profiles: Shipped camera profiles (slug = stable ID)
  - user_profiles: User-created profiles (uuid = stable ID)
  - camera_devices: Registered physical cameras (uuid, USB fingerprints, serial, profile assignment)
  - libraries: Library registry (UUID, path, label, last_opened, pinned, missing)
  - app_settings: KV table for all app settings (ui_mode, features, theme, title offset, etc.)
  - profile_staging: Dev menu staging area for profile authoring
  - entitlements: Optional license persistence
- Initialized once at startup via ensure_app_db_initialized()
- Migrations: A1 (camera tables), A2 (libraries + settings + upgrade support), A3 (entitlements)

Library DB (`<library>/.dadcam/dadcam.db`):
- Project-local. Portable with library folder.
- Tables: clips, events, event_clips, clip_scores, clip_score_overrides, export_history, vhs_edits, library_meta, plus asset/job tables
- library_meta stores library_uuid (source of truth for library identity)
- Initialized on library open/create via ensure_library_db_initialized()
- Migrations: L0 (library_meta), L6 (stable camera refs on clips), L7 (VHS recipes)

Connection pattern (contract #22):
- No long-lived connections stored in Tauri State
- DbState stores path only (Mutex<Option<PathBuf>>)
- Each command opens short-lived connections via open_app_db_connection() or DbState::connect()
- Background workers open their own connections

Stable camera references (contract #20):
- Clips reference cameras via {camera_profile_type, camera_profile_ref, camera_device_uuid}
- profile_type: 'bundled' | 'user' | 'none'
- profile_ref: slug (bundled) or uuid (user)
- No name-based or integer FK references

Library UUID identity (contract #21):
- Library identified by UUID stored in library_meta table
- On open: read UUID from library DB, generate if missing
- App DB registry keyed by library_uuid
- Relink validates UUID matches before updating path

Bundled profiles sync:
- Source: resources/cameras/bundled_profiles.json
- sync_bundled_profiles() does full replace at startup (idempotent)
- Bundled profiles matched by slug, user profiles by uuid

Legacy migrations (one-time at startup):
- Tauri Store settings -> App DB KV (tauri_store_migrated flag)
- ~/.dadcam/custom_cameras.json -> App DB camera_devices (renamed to .migrated)
- L6 backfill: legacy integer camera IDs -> stable refs in clip rows

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
- DbState: Stores library path in Mutex<Option<PathBuf>> (not a connection -- see contract #22)
- Each command opens short-lived connections via DbState::connect()
- Commands: open_library, close_library, get_clips, get_clip, toggle_tag, set_tag
- Phase 7 Commands: validate_reference_path, create_reference_library, create_batch_ingest, get_batch_progress, start_relink_scan, get_relink_candidates, apply_relink_candidate, get_offline_clips, list_codec_presets, create_codec_preset, get_clip_volume_info
- Phase 8 Commands: get_ml_analysis, analyze_clip_ml, get_ml_analysis_status, record_clip_view, train_personalized_scoring, get_best_clips_ml
- Library Fix Commands: list_bundled_profiles, list_user_profiles, create_user_profile, update_user_profile, delete_user_profile, list_camera_devices, register_camera_device, match_camera, import_camera_db, export_camera_db, stage_profile, list_staged, validate_staged, publish_staged, discard_staged, list_registry_libraries
- Import Verification Commands: get_session_status, get_session_by_job, export_audit_report, wipe_source_files
- Settings Commands (App DB): get_app_settings, save_app_settings, get_mode, set_mode, add_recent_library, remove_recent_library, get_recent_libraries, validate_library_path
- All responses use camelCase (serde rename_all)

Key Components:
- ClipGrid: Virtualized grid using TanStack Virtual rowVirtualizer
- ClipThumbnail: Individual clip card with hover detection
- SpriteHover: Mouse-position-based sprite frame display
- VideoPlayer: HTML5 video with keyboard shortcuts
- FilterBar: Filter buttons, search, date range, sort controls
- LibraryView: Main library browser container
- SettingsView: Full-page settings with left nav (200px) + content (640px max) per Braun D.5.8. Sections: General, Features (Advanced), Cameras (Advanced + flag), About
- LibraryDashboard: Pro mode multi-library selection grid

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

App Modes and Settings (Phase 9)

Two user modes with persistent settings stored in App DB KV table (app_settings).

Modes:
- Simple: Single project, auto-open last project, light theme only, no feature toggles
- Advanced: Multi-project dashboard, full feature toggles, theme toggle, dev menu

Settings storage (App DB app_settings KV table):
- ui_mode: "simple" | "advanced"
- features: JSON map (screenGrabs, faceDetection, bestClips, camerasTab)
- title_card_offset_seconds: "5"
- simple_default_library_uuid: UUID or ""
- theme: "light" | "dark"
- firstRunCompleted: "true" | "false"
- dev_menu: JSON (titleStartSeconds, jlBlendMs, scoreWeights, watermarkText)
- license_state_cache: JSON (licenseType, isActive, daysRemaining) | null
- tauri_store_migrated: "true" (set after one-time Tauri Store migration)

Recent projects are derived from the libraries registry table (App DB), not a settings array.

Legacy migration (one-time at startup):
- Tauri Store v2 settings copied to App DB KV on first launch after upgrade
- Recent projects resolved to library UUIDs and registered in App DB
- tauri_store_migrated flag prevents re-migration
- Old Tauri Store file kept as backup (not deleted)

Theme System:
- Light mode: default for all users
- Dark mode: available in Advanced mode only via Settings > General > Theme toggle
- Applied via `document.documentElement.classList.toggle('dark-mode')`
- CSS variables switch between light/dark palettes on :root vs :root.dark-mode
- Simple mode users always get light theme (no toggle shown)

Feature Flags (Advanced mode only):
- screenGrabs: Export still frames (default: true)
- faceDetection: Face detection during scoring (default: true in Advanced, false in Simple)
- bestClips: Auto-identify top clips (default: true)
- camerasTab: Show cameras section in Settings (default: true in Advanced, false in Simple)

First-Run Wizard:
- Shown when firstRunCompleted is false
- Step 1: Choose Simple or Advanced mode
- Step 2 (Simple): Pick project folder, creates default project
- Step 2 (Advanced): Navigates to projects dashboard
- Sets firstRunCompleted = true on finish

Tauri Commands:
- get_app_settings: Load settings with v1->v2 migration
- save_app_settings: Persist all settings
- get_mode / set_mode: Mode getter/setter (set_mode also resets feature flags)
- add_recent_library / remove_recent_library: Manage recent projects list
- get_recent_libraries: List recent projects
- validate_library_path: Check if .dadcam/dadcam.db exists at path

---

Theme CSS Variables

Light mode (default, :root):
```css
--color-canvas: #FAFAF8;
--color-surface: #FFFFFF;
--color-surface-elevated: #FFFFFF;
--color-text: rgba(10, 10, 11, 0.87);
--color-text-secondary: rgba(10, 10, 11, 0.60);
--color-text-muted: rgba(10, 10, 11, 0.38);
--color-border: #E5E5E5;
--color-border-emphasis: #D4D4D4;
```

Dark mode (:root.dark-mode):
```css
--color-canvas: #0a0a0b;
--color-surface: #111113;
--color-surface-elevated: #1A1A1C;
--color-text: rgba(250, 250, 248, 0.87);
--color-text-secondary: rgba(250, 250, 248, 0.60);
--color-text-muted: rgba(250, 250, 248, 0.38);
--color-border: #1f1f23;
--color-border-emphasis: #3A3A3E;
```

---

Dev Menu (Phase 9)

Access: Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win/Linux), or Settings > About > click version 7 times.

Sections:
1. Formulas: titleStartSeconds (default 5), jlBlendMs (default 500), score weights with sliders, watermark text override
2. Camera Database: Full profile table + device table, import JSON (native file picker), export JSON (native save dialog)
3. License Tools: View license state, inline key activation with validation feedback, generate rental keys (batch 1-100), clear license
4. Debug: Live log viewer (job-progress events, last 200 lines), FFmpeg/ffprobe/exiftool version check, database stats, clear caches, export database (native save dialog), export EXIF dump (clip ID + save dialog), raw SQL (dev key only)

Tauri Commands:
- test_ffmpeg: Check tool versions
- clear_caches: Remove proxies/thumbs/sprites
- export_database: WAL checkpoint + copy DB
- export_exif_dump: Per-clip EXIF JSON
- execute_raw_sql: Gated to dev license only
- generate_rental_keys: Batch key generation (1-100)
- get_db_stats: Clip/asset/event/job counts + DB file size

---

Licensing System (Phase 9)

Offline license validation. No phone home (contract 13).

License Types:
- trial: 14-day free trial, starts on first launch
- purchased: DCAM-P- prefix, permanent
- rental: DCAM-R- prefix, for camera rentals
- dev: DCAM-D- prefix, full access

Key Validation:
- Algorithm: BLAKE3 keyed hash
- Storage: System keychain (keyring crate)
- Trial date: Also stored in keychain (survives settings deletion)

Soft Lock After Trial Expiry:
- CAN: view, browse, play clips; export originals (file copy); export rendered with watermark + 720p cap
- CANNOT: import new footage; run scoring/face detection/best clips; register cameras to custom DB

Feature Gating:
- import: Blocked when trial expired
- scoring: Blocked when trial expired
- camera_registration: Blocked when trial expired
- raw_sql: Dev license only

Tauri Commands:
- get_license_state: Current license info
- activate_license: Validate and store key
- deactivate_license: Remove key, revert to trial
- is_feature_allowed: Check specific feature gate

---

VHS Export (Phase 9)

Full video export pipeline with J & L audio blending and optional title overlay.

Selection Modes: all, favorites, date_range, event, score_threshold
Ordering: chronological, score_desc, score_asc, shuffle

J & L Audio Blending:
- J-cut: Audio from next clip starts before video transition
- L-cut: Audio from current clip continues after video transition
- Default blend duration: 500ms (configurable in dev menu)
- FFmpeg: xfade (video crossfade) + acrossfade (audio crossfade)

Opening Title:
- Input: Plain text field in export dialog (optional)
- Timing: Starts at titleStartSeconds (default 5, configurable in dev menu)
- Duration: 3 seconds (fade in 0.5s, hold 2s, fade out 0.5s)
- FFmpeg: drawtext filter, centered, semi-transparent background

Watermark (trial exports):
- Text: "Dad Cam Trial" centered bottom
- Resolution cap: 1280x720 max

Export History:
- Stored in export_history table (Migration 4)
- Fields: selection_mode, ordering, title_text, resolution, is_watermarked, status, duration_ms, file_size_bytes, clip_count

Tauri Commands:
- start_vhs_export: Run full pipeline (select, build, render)
- get_export_history: List past exports
- cancel_export: Cancel running export

---

Camera System (Library Fix)

Camera profiles and devices live in App DB (survive library deletion).
Cameras UI is a section inside Settings (Advanced mode, gated by camerasTab feature flag). Works with no library open.
See docs/planning/libraryfix.md for full spec.

Matching Priority (spec 7.2):
1. Registered device match (USB fingerprint -> device UUID -> assigned profile)
2. User profile match (match_rules engine)
3. Bundled profile match (match_rules engine)
4. Generic fallback (no transform)

Match Rules (spec 7.3):
- JSON object: keys are ANDed, arrays within keys are ORed
- Supported keys: make, model, codec, container, folderPattern (regex), resolution constraints, frameRate, scanType
- String compares case-insensitive

Tie-break (spec 7.4):
1. Higher version
2. Higher specificity score (make+model=5, folderPattern=3, codec+container=3, resolution=2, frameRate=1)
3. Stable sort by profile_ref (deterministic)

Bundled Profiles:
- Source: resources/cameras/bundled_profiles.json (shipped with app)
- App DB table: bundled_profiles (slug = stable ID)
- sync_bundled_profiles() does full replace at startup (idempotent)

User Profiles:
- App DB table: user_profiles (uuid = stable ID)
- CRUD: create/list/get/update/delete

Camera Devices:
- App DB table: camera_devices (uuid, profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints JSON)
- USB fingerprint capture: macOS (system_profiler -xml), Windows (PowerShell Get-CimInstance), Linux (/sys/bus/usb/devices/)
- All USB detection wrapped in catch_unwind (best-effort, never blocks user)
- Root hubs filtered, fingerprints deduplicated
- Gated by licensing (camera_registration feature)

Stable Clip References:
- Clips store: camera_profile_type, camera_profile_ref, camera_device_uuid
- No integer FK references (legacy columns kept but not used for new matching)
- L6 migration backfills legacy integer IDs to stable refs

Profile Staging (Dev Menu):
- profile_staging table in App DB for authoring workflow
- Stage, validate, publish, discard cycle
- Must pass tests before publish

Tauri Commands:
- list_bundled_profiles / list_user_profiles: Profile listing
- create_user_profile / update_user_profile / delete_user_profile: User profile CRUD
- list_camera_devices / register_camera_device: Device management
- match_camera: Match clip against App DB profiles (spec 7.2 priority)
- import_camera_db / export_camera_db: JSON import/export
- stage_profile / list_staged / validate_staged / publish_staged / discard_staged: Staging workflow

---

Import Dialog (Phase 9)

GUI import flow with progress tracking and camera detection.

Three Phases:
1. Setup: Folder picker, event assignment (existing or new)
2. Progress: Real-time progress via job-progress Tauri events
3. Summary: Total processed/skipped/failed, camera breakdown (Advanced mode)

Features:
- Cancel support via cancel_job command
- Event linking (existing or create new during import)
- Camera auto-detection results displayed in Advanced mode

Sidecar Files:
- Written to .dadcam/sidecars/<clip_id>.json during ingest
- Schema v0.2.0: MetadataSnapshot, CameraMatchSnapshot, IngestTimestamps, DerivedAssetPaths

---

Import Verification (Gold Standard)

Bulletproof import pipeline ensuring every byte is verified before source deletion is allowed.
See docs/planning/importplan.md for the full specification.

Pipeline (per file):
1. Discover: Walk source, filter video extensions, record file stats
2. Pre-copy stat check: Re-stat source file, abort if size/mtime changed since discovery
3. Streaming copy: Read source in 8MB chunks, feed each chunk to BLAKE3 hasher AND write to temp file
4. Fsync + close: Flush temp file to disk
5. Read-back verify: Re-read temp file in chunks, compute fresh BLAKE3 hash, compare to copy-time hash
6. Atomic rename: Rename temp file to final destination (no partial files visible)
7. Manifest update: Record source_hash, dest_hash, result per file

Key invariants:
- Never loads full file into RAM (streaming 8MB chunks)
- Temp files use `.dadcam_tmp_` prefix (crash cleanup: any leftover temp = incomplete)
- Read-back hash must match copy-time hash or file is marked failed
- Dedup verification: when fast hash matches existing clip, full BLAKE3 proves identity (not just first+last MB)

Ingest Sessions (Migration 9):
- ingest_sessions table: tracks source_root, status, manifest frozen timestamp, rescan results, safe_to_wipe_at
- ingest_manifest table: per-file row with relative_path, source_hash, dest_hash, result, error_code, error_detail
- Status flow: pending -> running -> completed/failed
- safe_to_wipe_at: NULL until rescan proves manifest matches source exactly

Rescan Gate:
- After all files processed, re-walks source directory
- Compares discovered files against frozen manifest
- If any new/missing/changed files found: rescan fails, safe_to_wipe_at stays NULL
- Only when manifest matches source exactly: safe_to_wipe_at is set

Wipe Workflow:
- Hard-gated: refuses if safe_to_wipe_at is NULL
- Reads manifest entries in deterministic order
- Deletes each source file individually, records success/failure per file
- Returns WipeReport with total_files, deleted, failed counts + per-file entries
- Tauri command: wipe_source_files(session_id, output_dir?)

Device Ejection Detection:
- During ingest loop: checks source_root.exists() between each file
- If source disappears mid-import: marks session failed, emits DEVICE_DISCONNECTED error
- All remaining pending/copying manifest entries marked failed with DEVICE_DISCONNECTED code
- During rescan: checks source_root.exists() before walking, fails explicitly if disconnected
- Clear error message: "Source device disconnected. Session is NOT safe to wipe."

Audit Artifacts (exported to audit_session_<id>/ directory):
- session.json: Full session record with status and timestamps
- manifest.jsonl: One line per manifest entry
- results.jsonl: Per-file copy/verify outcomes
- rescan.jsonl: Rescan walk results
- rescan_diff.json: Differences found during rescan (new/missing/changed files)
- wipe_report.json: Per-file wipe outcomes (success/failure)

Tauri Commands:
- get_session_status: Get session verification status (total, verified, failed, pending, safe_to_wipe, sidecar_total, sidecar_failed)
- get_session_by_job: Look up session by ingest job ID
- export_audit_report: Export all audit artifacts to directory
- wipe_source_files: Execute wipe workflow (hard-gated on safe_to_wipe_at)

---

Sidecar Import Verification (Gold Standard)

Sidecar files (THM, XML, XMP, SRT, LRF, IDX) follow the exact same gold-standard import pipeline as primary media files. Implemented in v0.1.129-0.1.131 per docs/planning/sidecar-importplan.md.

See Contract #7: "Sidecars travel with their parent video."

Database (Migration 10):
- Two new columns on ingest_manifest_entries:
  - entry_type: TEXT NOT NULL DEFAULT 'media' CHECK (IN ('media', 'sidecar'))
  - parent_entry_id: INTEGER REFERENCES ingest_manifest_entries(id) (nullable)
- Indexes: idx_manifest_entry_type, idx_manifest_parent
- Backward compat: existing rows get entry_type='media', parent_entry_id=NULL

Discovery:
- discover_all_sidecars(source_path, media_files): walks source, returns paired sidecars (matched by stem per directory) and orphan sidecars (no matching media stem)
- discover_all_eligible_files(source_path): returns media + sidecar paths combined (used by rescan)
- is_sidecar_file(path): checks extension against SIDECAR_EXTENSIONS constant

Manifest Building:
- Phase 1: Insert media entries (entry_type='media', lower IDs = processed first)
- Phase 2: Insert paired sidecar entries (entry_type='sidecar', parent_entry_id = media entry ID)
- Phase 3: Insert orphan sidecar entries (entry_type='sidecar', parent_entry_id = NULL)
- manifest_hash covers all entries (media + sidecars)

Copy Pipeline (process_sidecar_entry):
- Same algorithm as media: re-stat > temp copy > stream hash > fsync > read-back hash > compare > atomic rename
- Creates sidecar asset with type='sidecar', full BLAKE3 hash, verified_method='copy_readback'
- Dedup: fast hash lookup, full hash verification before accepting dedup
- Links sidecar asset to parent clip via link_sidecar_to_parent_clip()
- Failed sidecar = hard failure (blocks SAFE TO WIPE), not a warning
- Processing order: all media entries first (clip records exist), then sidecar entries

Rescan Gate:
- Uses discover_all_eligible_files() (media + sidecars combined)
- Missing or new sidecar on source blocks SAFE TO WIPE identically to media
- All manifest entries (media AND sidecar) must be copied_verified or dedup_verified

Wipe Workflow:
- wipe_source_files() iterates ALL manifest entries (no entry_type filter)
- Sidecar paths included in wipe manifest and wipe_report.json

Audit Artifacts:
- manifest.jsonl: includes entry_type and parent_entry_id per entry
- results.jsonl: includes entry_type and parent_entry_id per entry
- rescan.jsonl: includes sidecar files (via discover_all_eligible_files)
- rescan_diff.json: diffs cover sidecars (must be empty for SAFE)

UX:
- Progress: sidecar count included in total, emits separate "copying_sidecars" phase
- Summary: "X clips imported + Y sidecars" display
- Errors: distinguishes "X media failed" from "Y sidecars failed"
- SessionVerificationStatus: includes sidecar_total and sidecar_failed counts
- IngestResponse: includes sidecarCount and sidecarFailed fields (Rust + TypeScript)

Error Handling:
- Sidecar read-back failure: marks entry failed, blocks SAFE TO WIPE
- Sidecar disappeared between manifest and copy: marks entry changed, blocks SAFE TO WIPE
- Sidecar I/O failure: captures error_code (SIDECAR_COPY_FAILED) + error_detail
- Parent media success does NOT imply sidecar success (each independent)

---

End of Technical Guide
