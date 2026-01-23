Dad Cam — Phase 0 Guide (Contracts and Non-Negotiables)

Goal: Lock down all architectural decisions BEFORE writing code.

---

What is Phase 0?

Phase 0 is not coding. It is decision-making.

Every question that could cause a rewrite later gets answered now. The output is documentation, not software.

---

Deliverables

1. contracts.md (root level)
   - Plain-language policy document
   - The "constitution" - changes require explicit reopening
   - See Contracts Checklist below

2. changelog.md (root level)
   - Version history
   - Source of truth for version number
   - Start at 0.1.0

3. techguide.md (root level, skeleton)
   - Manual for the app
   - Core logic descriptions
   - CLI commands (populated in Phase 1)
   - Placeholder sections for now

4. Schema design (section in this file)
   - Table names and relationships
   - Logical model, not full SQL yet

5. Constants definition (section in this file)
   - Hardcoded values that implement contracts
   - Becomes code in Phase 1

---

Contracts Checklist

These must all be documented in contracts.md:

[x] Library root model (one folder, .dadcam/ structure)
[x] Ingest modes (copy vs reference)
[x] Hashing strategy (BLAKE3, chunked + full)
[x] Dedup rules (same hash = same clip, user override)
[x] Verification policy (hash verify after copy, default ON)
[x] Sidecar policy (preserve all, never flatten structures)
[x] Timestamp precedence (metadata > folder > filesystem)
[x] Event grouping rules (folder-based + time-gap)
[x] Pipeline versioning (version bump invalidates derived assets)
[x] Supported formats (anything ffmpeg supports)
[x] Camera profile format (JSON)
[x] Outlier handling (audio-only and images accepted)
[x] Originals preservation (never delete, non-negotiable)
[x] No cloud dependency (fully offline, non-negotiable)
[x] No NLE lock-in (exports are standard formats, non-negotiable)
[x] Cross-platform requirement (Mac/Win/Linux, non-negotiable)
[x] Crash-safety guarantee (resumable ingest, non-negotiable)
[x] Database choice (SQLite, single file)
[x] External tools (ffmpeg, ffprobe, exiftool - bundled)
[x] Clip model definition (primary file vs multi-file bundle) + sidecar mapping rules
[x] Fast-hash scheme versioning (store scheme/method alongside hash_fast)
[x] Derived asset parameter recording (so invalidation is correct)
[x] Job idempotency + deterministic derived output paths
[x] Path normalization rule for DB (POSIX separators; runtime conversion per OS)
[x] Timezone semantics for recorded_at (UTC + offset/estimated flags)


---

Phase 0 Lock-ins (Addendum)

These are explicit “no-rewrite” decisions that must be reflected in contracts.md and the Phase 1 implementation.

1) Clip Model (v1)
- A “clip” represents ONE primary media file (video/audio/image).
- Sidecars and companion files are preserved and mapped to the clip (see Sidecar Mapping below).
- Multi-file camera structures (e.g., AVCHD/BDMV folders) are ingested as:
  - Primary media files: the playable media files (e.g., .MTS) become clips
  - All other files/folders are preserved as sidecars and kept in structure

2) Hashing (fast + full) and Scheme Versioning
- hash_fast is computed as: BLAKE3(first 1MB + last 1MB + file_size_bytes), concatenated in that order.
- hash_full is BLAKE3(full file) and may be deferred.
- Store hash_fast_scheme so we can change the method in the future without ambiguity.
  - v1 scheme name: first_last_size_v1

3) Path Normalization (DB Storage)
- Paths stored in the DB are always relative to the library root and use POSIX separators (“/”).
- Runtime converts to platform-native paths for filesystem access.

4) recorded_at Time Semantics
- Store recorded_at as UTC.
- If camera metadata includes a timezone offset, store it in recorded_at_offset_minutes.
- If timezone is unknown or inferred, set recorded_at_is_estimated = true.

5) Derived Assets Must Record Build Parameters
- Every derived asset row records:
  - pipeline_version used to generate it
  - derived_params (JSON) that includes presets/options (e.g., proxy preset, resolution, CRF, LUT id, deinterlace flag)
- Staleness decisions are made by comparing current (pipeline_version + params) to stored values.

6) Job Idempotency and Deterministic Outputs
- All jobs must be safe to retry.
- Derived output paths are deterministic based on (clip_id + role + params hash).
- Before doing work, a job checks for an existing derived asset with matching role+params hash and reuses it if present.

7) Sidecar Mapping (DB Representation)
- Sidecars are represented as assets and linked to the owning clip via clip_assets with role="sidecar".
- The primary file is the clip’s original_asset_id and is also represented in clip_assets with role="primary".



---

Schema Design (Logical Model)

Tables needed for Phase 1:

libraries
- id (primary key)
- root_path (absolute path to library folder)
- name (user-facing name)
- ingest_mode (copy | reference)
- created_at
- settings (JSON blob for future knobs)

assets
- id
- library_id (FK)
- type (original | proxy | thumb | sprite | export)
- path (relative to library root, POSIX “/” separators)
- source_uri (absolute last-seen source path/URI for reference mode, nullable)
- size_bytes
- hash_fast (chunked BLAKE3)
- hash_fast_scheme (e.g., first_last_size_v1)
- hash_full (full BLAKE3, nullable)
- verified_at (nullable timestamp)
- pipeline_version (nullable; set for derived assets)
- derived_params (JSON, nullable; set for derived assets)
- created_at

clips
- id
- library_id (FK)
- original_asset_id (FK to assets)  # primary media file
- camera_profile_id (FK, nullable)
- media_type (video | audio | image)
- title (auto-generated or user-set)
- duration_ms (nullable for images)
- width
- height
- fps (nullable for images/audio)
- codec
- recorded_at (UTC)
- recorded_at_offset_minutes (nullable)
- recorded_at_is_estimated (boolean)
- timestamp_source (metadata | folder | filesystem)
- created_at

clip_assets
- clip_id (FK)
- asset_id (FK)
- role (primary | proxy | thumb | sprite | sidecar)

camera_profiles
- id
- name (e.g., "Canon HV20")
- version
- match_rules (JSON)
- transform_rules (JSON)

tags
- id
- name (favorite | bad | archived | custom)
- is_system (boolean)

clip_tags
- clip_id (FK)
- tag_id (FK)
- created_at

jobs
- id
- type (ingest | proxy | thumb | sprite | export | hash_full | score)
- status (pending | running | completed | failed | cancelled)
- clip_id (FK, nullable)
- asset_id (FK, nullable)
- priority
- attempts
- last_error
- progress (0-100, nullable)
- created_at
- started_at
- completed_at

job_logs
- id
- job_id (FK)
- level (info | warn | error)
- message
- created_at

volumes
- id
- serial (disk serial if available)
- label (volume name)
- last_seen_at

fingerprints
- id
- clip_id (FK)
- type (size_duration | sample_hash | full_hash)
- value
- created_at
Constraints & Index Intent (Phase 1 Implementation Notes)

These are logical constraints; implement as UNIQUE constraints / indexes in SQLite in Phase 1.

- assets: UNIQUE(library_id, path)
- tags: UNIQUE(name)
- clip_tags: UNIQUE(clip_id, tag_id)
- clip_assets: UNIQUE(clip_id, role, asset_id)  # allow multiple sidecars; enforce one primary/proxy/thumb/sprite per params hash later
- jobs: consider UNIQUE(type, clip_id, asset_id, pipeline_version) for “one active job per work unit” (optional)



---

Constants to Define

These become code in Phase 1:

PIPELINE_VERSION = 1
DEFAULT_INGEST_MODE = "copy"
HASH_ALGORITHM = "blake3"
HASH_CHUNK_SIZE = 1048576  # 1MB
HASH_FAST_SCHEME = "first_last_size_v1"
PATH_DB_SEPARATOR = "/"
RECORDED_AT_STORAGE = "utc"
DERIVED_PARAMS_HASH_ALGO = "blake3"
TIMESTAMP_PRECEDENCE = ["metadata", "folder", "filesystem"]
EVENT_TIME_GAP_HOURS = 4
PROXY_CODEC = "h264"
PROXY_RESOLUTION = 720
PROXY_CRF = 23
THUMB_FORMAT = "jpg"
THUMB_QUALITY = 85
SPRITE_FPS = 1
SPRITE_TILE_WIDTH = 160
DB_FILENAME = "dadcam.db"
DADCAM_FOLDER = ".dadcam"
CAMERA_PROFILE_FORMAT = "json"
SUPPORTED_FORMATS = "ffmpeg-native"  # anything ffmpeg accepts
OUTLIER_TYPES = ["audio", "image"]  # accepted but flagged

---

Research Findings (Completed)

1. BLAKE3 Library
   - DECISION: Rust `blake3` crate
   - Performance: 3+ GB/s with SIMD + multithreading (official implementation)
   - Node.js WASM binding is ~2000x slower, no SIMD support
   - All hashing happens in Rust backend
   - Crate: https://crates.io/crates/blake3

2. ffmpeg/ffprobe Bundling
   - DECISION: `ffmpeg-sidecar` crate + Tauri sidecar
   - Tauri supports sidecar binaries with `-$TARGET_TRIPLE` naming
   - `ffmpeg-sidecar` provides cross-platform auto-download (<100MB)
   - Can download at first launch or bundle with app
   - Crate: https://crates.io/crates/ffmpeg-sidecar
   - Docs: https://v2.tauri.app/develop/sidecar/

3. exiftool Bundling
   - DECISION: Tauri sidecar (standalone build per platform)
   - Standalone builds exist for Windows/Mac/Linux (no Perl dependency)
   - Same sidecar pattern as ffmpeg

4. SQLite Library
   - DECISION: `rusqlite` with `bundled` feature
   - Statically links SQLite (no system dependency)
   - Sync API, widely used, stable
   - All DB operations happen in Rust backend
   - Crate: https://crates.io/crates/rusqlite

5. Rust vs Node DB Ownership
   - DECISION: Rust owns the database
   - Hashing is in Rust (BLAKE3 performance)
   - ffmpeg calls are in Rust (sidecar)
   - SQLite access is faster from Rust
   - Frontend calls Rust via Tauri commands
   - No need for better-sqlite3 in Node

6. Cross-Platform Path Handling
   - DECISION: `std::path` + relative paths in DB
   - Store relative paths in DB (relative to library root)
   - Convert to absolute only when accessing filesystem
   - Rust std::path::PathBuf handles platform differences
   - Windows quirks: drive letters, reserved names (CON, PRN, AUX)

7. Volume Detection
   - CONTRACT ADDITION: Volume identity is **best-effort** across OSes and mounts.
   - If serial/label cannot be determined (permissions, FUSE, flaky mounts), Dad Cam must still function using paths + fingerprints.
   - UI must degrade gracefully: show `Unknown volume` and allow manual relink by search.

   - DECISION: Platform-specific implementation (Phase 7)
   - Windows: GetVolumeInformation API
   - macOS: diskutil
   - Linux: /dev/disk/by-id/
   - Consider `nusb` crate for USB device serial (pure Rust)
   - Crate: https://lib.rs/crates/nusb

8. Existing Tools Researched
   - Video Hub App: Open source, cross-platform, similar concept
   - Wwidd: Browser-based video tagger, handles thousands of files
   - Fast Video Cataloger: Commercial, good UX reference
   - FINDING: None target "dad cam" footage or have auto-edit
   - Dad Cam fills a unique gap

9. Tech Stack Confirmation
   - Framework: Tauri 2.0 + React/TS
   - Backend: Rust (owns DB, hashing, ffmpeg calls)
   - Frontend: React/TS (UI only, calls Rust via Tauri commands)
   - Architecture:
     ```
     Frontend (React/TS)
         |
         v Tauri Commands
     Rust Backend
         |-- rusqlite (DB)
         |-- blake3 (hashing)
         |-- ffmpeg-sidecar (video processing)
     ```

---

Phase 0 Checklist

Documentation:
[x] contracts.md written and reviewed
[x] changelog.md created (start at 0.1.0)
[x] techguide.md skeleton created

Schema and Constants:
[x] Schema tables reviewed and approved
[x] Constants list finalized

Research:
[x] BLAKE3 library chosen (Rust blake3 crate)
[x] ffmpeg bundling strategy decided (ffmpeg-sidecar + Tauri sidecar)
[x] exiftool bundling strategy decided (Tauri sidecar)
[x] SQLite library chosen (rusqlite with bundled feature)
[x] Rust vs Node DB ownership decided (Rust owns DB)
[x] Cross-platform path strategy documented (std::path + relative paths)
[x] Existing tools researched (none fill this gap)

---

Done When

- contracts.md exists at project root (all boxes checked above)
- changelog.md exists at project root (v0.1.0)
- techguide.md skeleton exists at project root
- This file (phase-0.md) has all checkboxes marked
- No open questions about architecture remain
- A new developer can read these docs and start Phase 1 without asking "but what should X do?"

---

Next: Phase 1 (Core Backend Engine - CLI Only)
