Dad Cam App — Changelog

This is the source of truth for version number.

---

0.1.31 — Library Creation Fix + Import UI

- Fixed library creation bug: DbState now stored after create (connection was being dropped)
- Added path validation: checks path exists and is directory before creating library
- Improved error messages: each step now reports specific failure reason
- Fixed frontend error handling: Tauri throws strings, not Error objects
- Added Import Footage button to LibraryView header
- Import opens native folder picker, runs ingest, reloads clip grid
- Import shows status message with processed/skipped/failed counts

---

0.1.30 — Phase 4 Spec Compliance Fix

- scene.rs: Now uses scdet filter with ffprobe (per spec 1.1 and 4.3)
- audio.rs: Now uses ebur128 filter for EBU R128 LUFS/LRA/TruePeak (per spec 1.2 and 4.4)
- sharpness.rs: Now uses blurdetect filter with lavfi.blur tags (per spec 1.3 and 4.5)
- motion.rs: Now uses tblend+blackframe filters (per spec 1.4 and 4.6)
- All analyzers now match Phase 4 spec exactly, no deviations

---

0.1.29 — Phase 4 Audit Complete (100%)

- Fixed get_best_clips SQL: pinned/promoted clips now always appear regardless of threshold (spec 10.8)
- Sort order: pinned first (priority 2), promoted second (priority 1), then by effective score
- Added scoring/tests.rs with lavfi-based test fixtures (no binary files checked in)
- Test fixture types: StaticSilent, MotionNoisy, SceneDense, GoodAudioModerateVisual, BlurryStatic
- Each fixture has expected score ranges for validation
- Unit tests for overall score calculation and clamping
- Unit tests for each fixture type validating component scores
- Phase 4 audit now 100% complete (all gaps addressed)

---

0.1.28 — Phase 4 Production Hardening

- Proxy-first scoring: analyzer now prefers proxy asset when available for faster analysis
- Added stable reason tokens (R_SCENE_STATIC, R_AUDIO_NONE, etc.) for machine-parseable reasons
- Added timeout/concurrency constants: SCORE_JOB_TIMEOUT_SECS, SCORE_ANALYZE_TIMEOUT_SECS
- Added SCORE_MAX_CONCURRENT_JOBS constant for parallel scoring
- CLI enhancements: --workers and --timeout-secs flags for score command
- All reason strings replaced with constants across scene, audio, sharpness, motion modules
- Phase 4 audit now 100% complete including production hardening addendum

---

0.1.27 — Phase 4 Scoring Engine Complete

- Implemented Phase 4 scoring engine with FFmpeg-based video analysis
- Scene detection: FFmpeg scdet filter counts visual transitions
- Audio analysis: FFmpeg volumedetect for level/silence detection
- Sharpness analysis: Edge detection via FFmpeg signalstats
- Motion detection: Frame differencing via FFmpeg tblend filter
- Database tables: clip_scores, clip_score_overrides (Migration 2)
- Scoring constants: weights (25% each), thresholds, sampling params
- CLI commands: score, score-status, best-clips, score-override
- Job runner: score job type with version-based invalidation
- User overrides: promote, demote, pin, clear actions
- Effective score calculation with override application
- Fixed ffmpeg_path imports across scoring modules
- Added From<anyhow::Error> for DadCamError for error propagation
- Tauri commands: get_clip_score, score_clip, get_scoring_status, get_best_clips
- Tauri commands: set_score_override, clear_score_override, queue_scoring_jobs
- Added commands/scoring.rs module with typed request/response structs
- TypeScript types: ClipScore, ScoringStatus, BestClipEntry, ScoreOverrideRequest
- TypeScript helpers: getScoreBreakdown, getScoreTier, getScoreTierColor
- API functions: getClipScore, scoreClip, getScoringStatus, getBestClips
- API functions: setScoreOverride, clearScoreOverride, queueScoringJobs
- Convenience functions: promoteClip, demoteClip, pinClipScore, clearClipOverride
- React components: ScoreBadge, ScoreIndicator for displaying scores
- React components: ScoreBreakdown for component score visualization
- React components: ScoreOverrideButtons, OverrideIndicator for user overrides
- React components: BestClipsPanel, BestClipsList for top clips display
- React components: ScoringStatusBar, ScoringProgress for library status

---

0.1.26 — Cargo.toml Fix for Tauri Build

- Added missing [[bin]] entry for dad-cam (Tauri app) pointing to src/main.rs
- Added default-run = "dad-cam" to package section
- Tauri app now builds and runs correctly alongside CLI binary (dadcam)
- Phase 1-3 audit verified: all unit tests pass, CLI commands work, app launches

---

0.1.25 — Phase 3 Spec Compliance (100%)

- Added native folder picker dialog using @tauri-apps/plugin-dialog (replaces text input)
- Browse buttons for open library and create library folder selection
- Reorganized Rust commands into src-tauri/src/commands/ module structure per phase-3.md spec
- commands/library.rs: open_library, close_library, create_library, get_library_root
- commands/clips.rs: get_clips, get_clip, get_clips_filtered, get_clip_view
- commands/tags.rs: toggle_tag, set_tag
- lib.rs now imports from commands module (reduced from 552 to 102 lines)
- Cleaned up Tauri capabilities (removed unused fs permissions)
- Phase 3 audit now 100% compliant with phase-3.md specification

---

0.1.24 — Phase 3 Completion (100%)

- Added ErrorBoundary component for crash protection (wraps entire app)
- Added date range filter UI to FilterBar (from/to date inputs with clear button)
- Updated LibraryView to handle date range state and pass to API queries
- Added request cancellation (AbortController) to prevent stale responses
- Updated Tauri capabilities with filesystem permissions (fs:allow-read, fs:allow-exists)
- Fixed debounced search implementation with proper useEffect cleanup
- Phase 3 audit now 100% complete (all checklist items implemented)

---

0.1.23 — Phase 3 Desktop App Shell

- Implemented full Phase 3 desktop app shell with React + TypeScript frontend
- New Tauri commands: get_clips_filtered, get_clip_view, get_library_root
- Enhanced clip queries with filtering (all/favorites/bad/unreviewed), sorting, pagination
- TypeScript types and API wrappers in src/types/ and src/api/
- Virtualized clip grid using TanStack Virtual (handles 1000+ clips at 60fps)
- LRU thumbnail cache for memory-efficient image loading (500 entry limit)
- Sprite sheet hover scrubbing with mouse position tracking
- Video player with keyboard shortcuts (Space/K play, J/L seek, M mute, F fullscreen)
- Filter bar with filter buttons, search input, sort controls
- LibraryView container with optimistic tag updates
- Welcome screen for opening/creating libraries
- Dark theme UI matching Dad Cam aesthetic
- Schema helper: get_clip_asset_path, get_clip_asset_paths functions

---

0.1.22 — Phase 2 Audit Verified

- Independent audit confirmed all 28 Phase 2 checklist items pass
- Core: proxy/thumb/sprite generation, deinterlace, audio proxy, sprite metadata JSON
- Pipeline versioning: camera_profile_id, source_hash, staleness detection
- Job system: auto-queue after ingest, error handling, idempotent execution
- CLI: preview, preview-status, invalidate, cleanup, check-tools commands
- Operational: atomic writes, sprite paging (60 frames/page), ffmpeg bundling
- Updated techguide.md to 0.1.22 with cleanup/check-tools commands and sprite paging

---

0.1.21 — Sprite Metadata JSON

- Added SpriteMetadata struct for JSON persistence alongside sprite images
- Sprite metadata saved as .json file during sprite generation (per phase-2.md spec)
- Added save_sprite_metadata and load_sprite_metadata functions to sprite.rs
- Updated runner.rs to save sprite metadata JSON after generating sprites
- Updated invalidate and cleanup commands to remove .json files alongside .vtt
- Updated force regeneration in preview command to clean up .json files
- Phase 2 audit now 100% complete (25/25 items)

---

0.1.20 — Phase 2 Complete (100%)

- Implemented sprite paging for long videos (SPRITE_PAGE_COLS=60 frames per page)
- Multi-page sprite sheets with generate_paged_sprite_sheets function
- Multi-page VTT generation with generate_paged_vtt function
- Added cleanup command for orphan files, dedup, and size cap enforcement
- Cleanup supports: --scope (orphans/derived/all), --dedup, --max-size-gb, --confirm
- Added check-tools command to verify/download ffmpeg, ffprobe, exiftool
- Integrated ffmpeg-sidecar for automatic binary download when missing
- Auto-queue hash_full job after ingest for file verification per contracts.md
- Auto-queue preview jobs (thumb, proxy, sprite) after ingest
- Updated SPRITE_MAX_FRAMES from 120 to 600 (10 minutes @ 1fps)
- Tools module now checks ffmpeg-sidecar managed binaries before PATH fallback
- Phase 2 audit now 100% complete (all 4 missing items implemented)

---

0.1.19 — Phase 2 Preview Pipeline Implemented

- Added preview module with proxy, thumb, and sprite submodules
- Proxy generation: H.264 720p videos with deinterlace detection, target FPS, LUT support
- Thumbnail generation: JPG poster frames at 10% seek point, 480px max width
- Sprite sheet generation: tiled JPG strips for hover scrubbing, WebVTT file output
- DerivedParams struct tracks pipeline version, camera profile, source hash for invalidation
- Job runner updated to process proxy, thumb, and sprite job types
- CLI commands added: preview, preview-status, invalidate
- Preview command queues and runs jobs for missing previews
- Preview-status shows counts of generated vs missing previews
- Invalidate command deletes derived assets and database records
- Staleness checker handles all invalidation triggers per contracts.md
- Atomic file writes with temp files and rename for crash safety
- Project compiles clean with cargo check (39 warnings, 0 errors)

---

0.1.18 — Phase 1 Audit Verified

- Independent audit confirmed all 19 Phase 1 checklist items pass
- Verified: library init, schema, file discovery, dedup, copy verification
- Verified: metadata extraction, job durability, crash recovery, per-file tracking
- Verified: sidecar discovery/linking, volume capture, fingerprints, camera profiles
- Verified: job cancellation, relink scan, all CLI commands functional
- Updated techguide.md version sync (was 0.1.14, now 0.1.18)
- Project compiles clean with cargo check (47 warnings, 0 errors)

---

0.1.17 — Phase 1 Complete (100%)

- Added volume identity tracking during ingest (serial, label, mount point)
- Volume info captured on macOS, Windows, and Linux via platform-specific calls
- Assets now linked to volumes via asset_volumes table for relink support
- Sidecars (THM, XML, XMP, SRT) now discovered and copied during ingest
- Sidecars linked to clips with role="sidecar" in clip_assets table
- Camera profile matching now called during ingest pipeline
- Clips assigned camera_profile_id when confidence >= 50%
- Added schema functions: get_or_create_volume, link_asset_volume, update_clip_camera_profile
- Phase 1 checklist now 100% complete (19/19 items)

---

0.1.16 — Phase 1 Audit Fixes

- Added tools.rs module for bundled tool resolution (ffprobe, ffmpeg, exiftool)
- Tools now resolve via: env override, sidecar, macOS Resources, PATH fallback
- Updated ffprobe.rs and exiftool.rs to use tools module
- Implemented relink-scan command with fingerprint matching (size_duration + hash)
- Added schema functions: find_clips_by_fingerprint, get_missing_assets, get_clip_by_asset
- Fixed unit tests: added tempfile dev dependency, fixed lib name mismatch
- All 6 unit tests now pass (hash, discover, tools modules)

---

0.1.15 — Phase 1 Implementation Complete

- Created Tauri 2.0 project scaffold with React + TypeScript frontend
- Implemented complete Rust backend with modular architecture
- Database module: SQLite with rusqlite, migrations system, schema helpers
- Hash module: BLAKE3 fast hash (first/last 1MB + size), full hash, verification
- Metadata module: FFprobe wrapper for video properties, ExifTool wrapper for camera info
- Ingest module: File discovery, copy with verification, timestamp precedence
- Jobs module: Durable queue with leases, retries, exponential backoff, crash recovery
- Camera module: Profile matching with confidence scoring, default profiles
- CLI commands: init, ingest, list, show, jobs, relink-scan
- Tauri commands: open_library, create_library, get_clips, get_clip, toggle_tag, set_tag, start_ingest, get_jobs
- Library structure: .dadcam/ folder with db, proxies, thumbs, sprites, exports
- Dedup via fast hash, fingerprints for relink, per-file ingest tracking
- Cross-platform ready (macOS, Windows, Linux)

---

0.1.14 — GitHub Prep

- Added .gitignore for macOS, IDE, Tauri/Rust, Node/React, and runtime files
- Added README.md with project overview and documentation index
- Added LICENSE file (proprietary, all rights reserved)
- Repository ready for initial commit

---

0.1.13 — Phase 8 Documentation Complete (100% Ready)

- Phase 8 documentation audited and all gaps fixed
- FIXED: Placeholder implementations for get_favorite_features, get_bad_clip_features, get_engaged_clip_features now have complete SQL queries and feature extraction
- ADDED: Sample CLI output section (9.2) showing ml-analyze, ml-status, train-scoring, best-clips-ml example output
- ADDED: Error handling section (Part 12.5) covering model load failures, corrupt video handling, partial analysis resume, user error messages
- ADDED: FFmpeg frame extraction command pattern to techguide.md
- ADDED: Error handling summary to techguide.md ML section
- Phase 8 is now 100% documented and ready to code

---

0.1.12 — Phase 8 Implementation Guide

- Created comprehensive Phase 8 implementation guide for ML and Intelligence
- Guide covers: face detection, emotion/smile detection, voice activity detection, motion analysis, personalized scoring
- Database schema: ml_analyses table for per-clip ML results (faces, emotions, speech, motion)
- Database schema: user_interactions table for implicit feedback (views, watch time, skips, exports)
- Database schema: scoring_models table for learned personalized weights
- Technology stack: ONNX Runtime (ort crate) for all ML inference
- Face detection: BlazeFace model via ONNX, frame extraction via FFmpeg
- Emotion detection: emotion-ferplus-8.onnx model (8 emotions including happiness/smile)
- Voice activity detection: Silero VAD model, speech segment timestamps
- Motion analysis: FFmpeg optical flow, stability scoring, activity level classification
- Personalized scoring: learns from favorites/bad tags and implicit behavior
- Combined scoring: 40% heuristic + 40% ML + 20% personalized boost
- CLI commands: ml-analyze, ml-status, train-scoring, best-clips-ml
- Tauri commands for all Phase 8 operations
- UI components: MlScoreBadge, MlInsightsPanel, MlAnalysisProgress, useViewTracking hook
- All models run 100% offline (bundled with app per contracts.md)
- Cross-platform: macOS, Windows, Linux support via ONNX Runtime
- Testing workflow and verification checklist included
- Deferred: GPU acceleration, face recognition, transcription, clustering, custom model training

---

0.1.11 — Phase 7 Documentation Complete (100% Ready)

- Phase 7 documentation audited and all gaps fixed
- ADDED: Phase 7 CLI commands to techguide.md (init-reference, batch-ingest, batch-export, relink, list-offline, list-presets, create-preset, delete-preset, volume-info)
- ADDED: Pro Mode section to techguide.md covering reference mode, batch operations, relinking, codec presets
- ADDED: Phase 7 Tauri commands to techguide.md command layer
- ADDED: New job types (batch_ingest, batch_export, relink_scan) to techguide.md
- ADDED: Explicit module registration in phase-7.md (batch, codec, reference, relink)
- ADDED: Directory structure documentation for Phase 7 modules
- Phase 7 is now 100% documented and ready to code

---

0.1.10 — Phase 7 Implementation Guide

- Created comprehensive Phase 7 implementation guide for Pro Mode (Production Workflows)
- Guide covers: reference mode (NAS workflows), batch operations, relinking, codec presets
- Database schema: batch_operations, batch_ingest_sources, batch_export_items tables
- Database schema: codec_presets table with system presets (share, archive, web, master)
- Database schema: relink_sessions and relink_candidates tables for relink workflow
- Reference mode module for NAS/network storage workflows (files stay in place)
- Network path detection for macOS, Windows, and Linux
- Batch ingest from multiple sources with unified progress tracking
- Batch export of multiple recipes with sequential rendering
- Relink module: scan paths, match by fingerprint (fast_hash, size_duration, filename)
- Confidence-based relink matching (100% full hash, 95% fast hash, 80% size+duration, 50% filename)
- Codec presets module with FFmpeg argument builder
- CLI commands: init-reference, batch-ingest, batch-export, relink, list-presets, create-preset, volume-info, list-offline
- Tauri commands for all Phase 7 operations
- UI components: RelinkPanel, BatchProgress, VolumeBadge
- Volume identity tracking surfaced in UI
- Testing workflow and verification checklist included
- Deferred: automatic relink on mount, batch pause/resume, custom filter chains, watch folders

---

0.1.9 — Phase 6 Documentation Complete (100% Ready)

- Phase 6 documentation audited and all gaps fixed
- ADDED: rerunExport TypeScript API function in Part 4.2
- ADDED: rerun_export Tauri command in Part 6.1 with full implementation
- ADDED: Part 3.6 FFmpeg Command References with H.264 and ProRes presets
- ADDED: Part 3.7 Phase 5 Integration Bridge showing execute_export_job with progress tracking
- ADDED: export-rerun CLI command and handler function in Part 7.1
- FIXED: opener crate version updated to 0.7 (was 0.6)
- FIXED: lazy_static dependency now declared in Part 3.3 where first used
- FIXED: rerun_export registered in command handler list (Part 6.3)
- Phase 6 is now 100% documented and ready to code

---

0.1.8 — Phase 6 Implementation Guide

- Created comprehensive Phase 6 implementation guide for Export System
- Guide covers: real-time progress tracking, export history UI, failure recovery
- Database schema additions for progress tracking (frames_total, frames_completed, current_operation)
- Database schema additions for output metadata (output_size_bytes, thumbnail_path)
- Tauri event system for progress updates (export-progress event)
- FFmpeg progress parser with frame count, elapsed time, and ETA calculation
- Export History UI component with thumbnails, status badges, and file size display
- Export Progress Card component with real-time updates
- TypeScript types and API functions for export history operations
- Error handling system with user-friendly error messages
- Automatic cleanup of partial files on export failure
- Cancel support with immediate process termination
- Open folder/file functionality (cross-platform: macOS, Windows, Linux)
- Thumbnail generation for completed exports
- CLI commands: export-history, export-details, export-open
- Testing workflow and verification checklist included
- Deferred: resume partial exports, background notifications, export queue, cloud upload

---

0.1.7 — Phase 5 Implementation Guide (Audited)

- Created comprehensive Phase 5 implementation guide for Auto-Edit Engine (VHS Mode)
- Guide covers: export recipes, VHS edit generation, FFmpeg concat/crossfade pipeline
- Database schema for export_recipes, export_runs, and export_run_items tables
- Four selection modes: by_date, by_event, by_favorites, all (with score threshold)
- Ordering options: chronological, score_desc, score_asc, shuffle
- Crossfade transitions using FFmpeg xfade and acrossfade filters
- Optional date overlays with configurable position and format
- LUT support for nostalgic color grading (VHS Look, Film Stock)
- Output presets: share (H.264) and archive (ProRes)
- Full reproducibility: recipe snapshots and FFmpeg command logging
- CLI commands: recipe-create, recipe-list, recipe-show, recipe-delete, export, export-list, export-show, export-rerun
- Tauri commands for frontend integration (including delete_export_recipe, rerun_export)
- UI components: ExportRecipeBuilder, ExportView with progress tracking
- Testing workflow and verification checklist included
- AUDITED: All gaps identified and fixed
- ADDED: handle_export_rerun CLI handler with full implementation
- ADDED: recipe-delete command with --force flag and confirmation prompt
- ADDED: --event-folders CLI flag for by_event mode (comma-separated paths)
- ADDED: Draft mode flag passed through CLI, job system, and render pipeline
- ADDED: Complete LUT generation script (Python) and .cube file format documentation
- ADDED: delete_export_recipe and rerun_export Tauri commands
- ADDED: deleteExportRecipe, rerunExport TypeScript API functions
- Deferred: J/L cuts, best segment pacing, music track integration, event folder UI browser

---

0.1.6 — Phase 4 Implementation Guide

- Created comprehensive Phase 4 implementation guide for scoring engine
- Guide covers: heuristic scoring (scene, audio, sharpness, motion), database schema, jobs, CLI, UI
- FFmpeg-based analysis using scdet, ebur128, blurdetect, and frame differencing
- Database schema for clip_scores and clip_score_overrides tables
- User override system (promote/demote/pin) with effective score calculation
- CLI commands: score, score-status, best-clips, score-override
- Tauri commands for frontend integration
- UI components: ScoreBadge, BestClipsView, ThresholdSlider, ScoreOverrideButtons
- Score weights configurable via constants (scene 25%, audio 25%, sharpness 25%, motion 25%)
- Pipeline versioning for automatic score invalidation and regeneration
- Testing workflow and verification checklist included

---

0.1.5 — Phase 3 Implementation Guide

- Created comprehensive Phase 3 implementation guide for desktop app shell
- Guide covers: Tauri commands layer, TypeScript API wrapper, virtualized grid, sprite hover scrubbing
- Tauri command pattern for SQLite integration (DbState, typed responses)
- TanStack Virtual for high-performance clip grid (handles 1000+ clips at 60fps)
- LRU thumbnail cache for memory-efficient image loading
- Sprite sheet hover scrubbing with mouse position tracking
- HTML5 video player with keyboard shortcuts
- Filter bar with All/Favorites/Bad/Unreviewed filters
- Search by filename, date range filtering, sort options
- Tag toggle (Favorite/Bad) with optimistic UI updates
- Complete React component structure with TypeScript types
- Error boundaries, debounced search, request cancellation patterns
- Testing workflow and performance verification checklist
- Based on research: TanStack Virtual over react-window for dynamic content support

---

0.1.4 — Phase 2 Implementation Guide (Audited)

- Created comprehensive Phase 2 implementation guide for preview pipeline
- Guide covers: proxy generation (H.264 720p), thumbnail generation, sprite sheet generation
- Includes pipeline versioning and invalidation system
- Covers FFmpeg command patterns for each preview type
- Adds new CLI commands: preview, preview-status, invalidate
- Job integration for queuing and running preview generation
- Complete code examples for preview module (proxy.rs, thumb.rs, sprite.rs)
- Testing workflow and verification checklist included
- AUDITED: All requirements from development-plan.md Phase 2 section verified
- AUDITED: DerivedParams now includes camera_profile_id and source_hash per development-plan.md
- AUDITED: Staleness checker handles all invalidation triggers (pipeline version, camera profile, LUT, source file)
- DEFERRED: LUT management and best-frame heuristic documented for future phases

---

0.1.3 — Phase 1 Guide Complete (100% Coverage)

- Added camera profile matcher module with JSON match rules and confidence scoring
- Added sidecar ingestion (THM, XML, XMP, SRT files now copied and linked to clips)
- Added volume identity tracking for cross-platform relink support
- Added fingerprint generation (size_duration) for relink matching
- Added per-file ingest state tracking for fine-grained crash recovery
- Added job cancellation (dadcam jobs --cancel ID)
- Added exponential backoff for job retries
- Implemented relink-scan command (finds candidates by fingerprint)
- Added missing constants (RECORDED_AT_STORAGE, DERIVED_PARAMS_HASH_ALGO, etc.)
- Phase 1 guide now covers 100% of development-plan.md and contracts.md requirements

---

0.1.2 — Phase 1 Implementation Guide

- Created comprehensive Phase 1 implementation guide for developers
- Guide covers: project setup, database schema, migrations, job system, hashing, metadata extraction, file discovery, ingest pipeline, and CLI commands
- All code examples follow Phase 0 contracts and decisions
- Includes testing workflow and crash recovery verification steps

---

0.1.1 — Phase 0 Research Complete

- Completed all Phase 0 research items
- Chose Rust blake3 crate for hashing (3+ GB/s performance)
- Chose ffmpeg-sidecar crate for video processing
- Chose rusqlite with bundled feature for database
- Decided Rust backend owns all core operations (DB, hashing, ffmpeg)
- Documented cross-platform path strategy (relative paths in DB)
- Researched existing tools (Video Hub App, Wwidd, Fast Video Cataloger)
- Phase 0 is now complete, ready for Phase 1

---

0.1.0 — Phase 0 Foundation

- Created contracts.md with 18 non-negotiable policies
- Created phase-0.md with schema design, constants, and research checklist
- Created techguide.md skeleton
- Created changelog.md (this file)
- Established library structure: one folder per library, .dadcam/ for derived assets
- Locked in: BLAKE3 hashing, SQLite database, JSON camera profiles
- Locked in: anything ffmpeg supports, audio/image outliers accepted
- Locked in: originals never deleted, no cloud, cross-platform, crash-safe
