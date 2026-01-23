Dad Cam — Full Development Plan (Bullet-Proof)
Phase 0 — Contracts & Non-Negotiables

Goal: Decide the rules once so you don’t rewrite later.

Decisions / Specs (write these down)

Library Root model:

One folder per library

.dadcam/ contains DB + derived assets

Ingest modes

Default: Copy into library

Advanced: Reference in place (pro mode)

Dedup + hashing

During ingest: chunked BLAKE3 (fast)

Optional background job: full BLAKE3 later

Dedup rules (same signature = same clip, with safe override)

Verification

Optional hash verify after copy (toggle; default ON for pro)

Sidecar policy

Preserve unknown files + camera folder structures (especially AVCHD-like)

Time policy

Timestamp precedence: metadata → folder structure → filesystem

“By Event” rule: folder-based + optional time-gap grouping

Pipeline versioning

pipeline_version bumps invalidate proxies/thumbs/sprites/scoring caches

✅ Done when: these policies exist as a short doc + constants/fields in schema.

Phase 1 — Core Backend Engine (CLI Only)

Goal: A real ingestion engine with zero UI. Crash-safe. Resumable. Queryable.

Build
1) SQLite schema + migrations (the spine)

Core tables (high level):

libraries (root path + settings)

assets (files: original/proxy/thumb/sprite/export)

clips (logical video items)

clip_assets (mapping)

camera_profiles (installed profiles + versions)

tags + clip_tags (favorites/bad/etc.)

jobs (durable queue)

job_logs (optional but recommended)

volumes / mounts (best-effort volume identity)

fingerprints (size/duration/sample-hash/etc.)

2) Durable job runner (CLI)

Lease-based job claiming (TTL)

Retries with backoff

Cancellation

Progress reporting

3) Ingest CLI (discover → copy/ref → hash → DB)

Discover supported video + sidecars

Copy into originals/ OR reference original path

Store per-file ingest state so USB disconnects resume cleanly

Chunked BLAKE3 during ingest (fast)

Optional verify after copy

Log everything to DB

4) Metadata extraction (wrappers)

ffprobe wrapper → codec/resolution/duration/fps/audio streams

exiftool wrapper → “best guess” dates, make/model strings when present

5) Camera profile matcher

Profile format: JSON/TOML

Matching rules:

metadata hints

codec/container hints

folder structure hints

(future) USB fingerprint hints

Write match results to DB:

camera_profile_id

confidence score

reasons

6) Relink foundations (no UI yet)

Capture volume identity when possible

Store fingerprints to enable future relink

CLI commands (minimum)

dadcam init <library_root>

dadcam ingest <path>

dadcam list

dadcam show <clip_id>

dadcam jobs

dadcam relink-scan <path> (optional early)

✅ Done when: you can ingest a folder/card from CLI, unplug mid-way, re-run, and it resumes; SQLite reflects clean clip/asset records with metadata + camera match.

Phase 2 — Preview Pipeline (Proxies, Thumbs, Sprites)

Goal: Every clip becomes watchable and scrubbable instantly.

Build (as job types)
1) Proxy generator

Output: H.264 720p, CFR, AAC

Apply camera profile transforms (optional at proxy stage):

deinterlace if needed

LUT toggle (preview look)

Store proxy as an asset row linked to clip

2) Thumbnail generator

Poster frame per clip (or best frame heuristic)

3) Sprite sheet generator

Hover scrub sprite strips (fps = e.g. 0.5–1.0, tile layout)

4) Pipeline versioning + invalidation rules

Regenerate derived assets when:

pipeline_version changes

camera profile changes

LUT changes

proxy preset changes

source file changes

✅ Done when: every clip has (original + proxy + poster + sprite) and you can delete derived assets and they regenerate deterministically from jobs.

Phase 3 — Desktop App Shell (Viewer First)

Goal: Dad Cam becomes a fast video viewer/library for years of footage.

Build
1) App framework

Commit: Tauri 2.0 + React/TS

Bundle helper binaries (ffmpeg/ffprobe/exiftool) into app resources

2) SQLite integration layer

Read-only queries first

Paging + indices built for scale

3) Library UI (must be fast)

Virtualized grid (react-window / react-virtual)

Thumbnail loading strategy + LRU cache behavior

Proxy player (HTML5 video)

Hover scrubbing using sprite sheets

4) Basic library filters

All Clips

Favorites

Bad

Search (filename/date range)

“Unreviewed” (no favorite/bad yet)

5) Basic tagging interactions

Favorite toggle

Bad toggle

Optional “archive/delete” behavior stubbed (personal mode later)

✅ Done when: you can browse years of footage, click clips to play proxies smoothly, hover-scrub instantly, and tag favorites/bad without UI lag.

Phase 4 — Scoring Engine (Best Clips, Heuristics First)

Goal: App finds “best moments” without ML.

Build (jobs + UI)
1) Heuristic scoring jobs

Scene change density

Audio loudness stability / peaks

Sharpness (simple metric)

Motion (simple metric)

Output: score 0–1 + reason list

2) DB model

clip_scores table with:

overall score

component scores

reasons

pipeline version

3) UI

Best Clips view (threshold slider)

Promote / demote (user override stored separately)

✅ Done when: Best Clips feels useful and users can correct it; your system stores both machine score and human preference.

Phase 5 — Auto-Edit Engine (VHS Mode)

Goal: One button generates a nostalgic long-form movie (not an editor).

Build
1) Export Recipe model (first-class)

Tables:

export_recipes (settings knobs)

export_runs (history)

export_run_items (which clips + order + why)

Store exact command(s) used for reproducibility

2) VHS edit generator

Modes:

By Date

By Event (folder-based + optional gap grouping)

By Favorites

All

Pipeline:

select clips

apply ordering/pacing rules

concat with crossfades

audio smoothing + J/L transitions

optional date/text overlays

LUT application (style layer)

3) Preview / render strategy

Render final output from originals

Optionally render “draft” from proxies for quick iteration (pro feature later)

✅ Done when: user can generate a watchable VHS film reliably, and the run is stored so it can be re-rendered later.

Phase 6 — Export System (Share + Archive + Reliability)

Goal: Outputs become first-class, re-renderable, and robust.

Build

Export presets:

Share: H.264 (social)

Archive: ProRes (pro)

Progress UI (events from job system)

Failure recovery:

resume/cancel

clear error messages + logs

Export history UI:

outputs list

re-run button

“open export folder”

✅ Done when: exports feel professional and trustworthy; you can reproduce results and diagnose failures.

Phase 6.5 — Release Engineering (Cross-Platform Shipping)

Goal: Make it installable and maintainable across OSes.

Build / Checklist

Bundle ffmpeg/ffprobe/exiftool per platform

macOS:

signing + notarization plan

permissions (file access)

Windows:

signing plan

removable drive quirks

Linux:

package choice (AppImage/Flatpak)

Update strategy:

v1: manual update acceptable

v2: auto-updater (optional)

✅ Done when: you can produce installable builds for Mac/Win/Linux and the app can access chosen folders reliably.

Phase 7 — Pro Mode (Production Workflows)

Goal: Add pro knobs without changing architecture.

Add

Reference mode (NAS workflows)

Batch ingest + batch export

Keep originals always (enforced)

Relinking UI (built on Phase 1 foundations)

Volume identity tracking surfaced

More export control (codec knobs)

✅ Done when: your production workflow is faster than your current manual workflow.

Phase 8 — ML & Intelligence (Optional, Last)

Goal: Compound advantage, not required for launch.

Add (incrementally)

Face detection / smiles / speech segments

Better motion salience

Personalized scoring using user feedback

Model runs as background jobs; results stored like heuristics

✅ Done when: Best Clips is noticeably better than heuristics alone and improves over time per user.

Final System Shape (Truth)
UI
 │
SQLite DB (source of truth)
 │
Job System (durable + resumable)
 │
FFmpeg / ExifTool / Hashing (subprocess tools)
 │
Library Root folder (originals + derived assets)


Everything is:

inspectable

resumable

portable

cross-platform

not locked to an NLE

not dependent on cloud