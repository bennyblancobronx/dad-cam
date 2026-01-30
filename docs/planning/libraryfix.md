# Dad Cam: Library Fix (App DB + Portable Library DB) — Long‑Term Spec

> **Goal:** Camera profiles, user profiles, registered devices, library locations/recents, and app settings are **always saved inside the app** (`~/.dadcam/app.db`).  
> Library/project metadata stays **portable inside the library folder** (`<library>/.dadcam/dadcam.db`), alongside sidecars/exif/derived assets.
>
> **This doc is the source of truth** for implementation. It is written to be safe for a junior dev and a coding agent (no implied decisions).

---

## 0) Definitions (read first)

- **App DB**: `~/.dadcam/app.db` — user-global. Survives library deletion/moves.
- **Library DB**: `<library>/.dadcam/dadcam.db` — project-local. Portable with library folder.
- **Library UUID**: stable ID of a library, stored *inside the library* (source of truth).
- **Profile Type**: `'bundled' | 'user' | 'none'`
- **Profile Ref**: stable ID for a profile:
  - bundled: `profile_ref = slug`
  - user: `profile_ref = uuid`
- **Device UUID**: stable ID for a physical camera device (fleet or renter).

---

## 1) Problem

Camera profiles/devices currently live in each library DB, which causes:

1. Cameras UI breaks when no library is open
2. Deleting a library deletes user profiles/device registrations
3. Redundant profiles per library
4. Fragile `~/.dadcam/custom_cameras.json` sync hack
5. No clean bundled profile updates with app versions
6. App can’t remember library locations/recents between runs

---

## 2) Scope

### In scope
**Move to App DB**
- Bundled camera profiles (shipped with app)
- User camera profiles (created/edited by user)
- Registered camera devices (fleet + renter)
- **Library registry (paths, labels, recents, last-opened, pinned, missing)**
- **App settings** (Simple/Advanced UI mode, feature flags, offsets like title start)
- (Optional persistence only) entitlements state (trial/rental unlock)

**Stay inside Library DB**
- Clips, events, dates
- Sidecars / exif dumps / derived asset references
- (Recommended) VHS edit recipe + outputs (rebuildable exports)

### Explicit non-goals (skip for now)
- FireWire ingest, VHS/MiniDV capture
- Multi-view camera support
- Cloud sync

---

## 3) Locked Decisions (best long-term options)

These are *not optional*. Do not implement “a different version”.

### 3.1 One engine; Simple vs Advanced is UI-only
There is **one** data model and command set.
- Simple UI: hides complexity (single “default library/project”, cameras tab hidden, features off)
- Advanced UI: exposes cameras, multiple libraries, feature toggles

Implementation: capability mask stored in `app_settings` and used only by UI routing.

### 3.2 Stable identity everywhere (no name-based foreign keys)
Never reference profiles by display name.

- Bundled profiles: `slug` is stable ID forever.
- User profiles: `uuid` is stable ID forever.
- `name` is display-only and may change freely.

### 3.3 Library identity is stored in the library (source of truth)
Single source of truth: `library_meta` table in **Library DB** stores `library_uuid`.

On create/open:
1. Open `<library>/.dadcam/dadcam.db`
2. Ensure `library_meta` exists
3. Read `library_uuid`
4. If missing: generate UUID v4, write once, and return it

The App DB stores only UX registry keyed by `library_uuid`.

### 3.4 SQLite connections: do not share `rusqlite::Connection` across threads
`rusqlite::Connection` is not `Send`. The safest long-term pattern:

- Do **not** store a `Connection` in Tauri `State` (even behind a mutex).
- Do store only paths/config in `State` (or compute paths on demand).
- Each command/task opens its own short-lived connections:
  - UI command opens App DB connection, does quick work, closes.
  - Background ingest/processing opens its own App DB + Library DB connections inside worker thread.

This avoids deadlocks and thread-safety footguns.

> Future optimization (optional later): add a pool (deadpool-sqlite). Not required for v1.

### 3.5 Always initialize/migrate DBs exactly once per process
To avoid races when multiple commands start at once:
- On app startup, run `ensure_app_db_initialized()` (creates DB + runs migrations).
- When opening a library, run `ensure_library_db_initialized()` for that library.

After that, commands can open connections without running migrations.

### 3.6 Dev Menu: Debug vs Authoring are separate
- **Debug:** query DBs, inspect matches, inspect sidecars/ffprobe, view derived cache
- **Authoring:** editing match/transform rules, formulas, exporting bundled profiles

Authoring must write to staging and run matcher tests before publish.

### 3.7 Matching priority favors user intent
Matching order (locked):
1. **Registered device match** (USB fingerprint → device UUID → device assigned profile if set)
2. **User profile match** (advanced users expect overrides)
3. **Bundled profile match**
4. **Generic fallback** (no transform)

### 3.8 VHS edits are deterministic & rebuildable (semantic determinism)
Store a **recipe** in Library DB:
- ordered clip list
- title text + offset
- audio blend params
- pipeline version
- optional transform overrides

Determinism definition:
- Same machine + same FFmpeg build + same pipeline version ⇒ same `output_hash` (BLAKE3 of bytes).
- Across machines ⇒ guarantee recipe hash stability + semantic equivalence (duration + frame count), not byte-identical.

---

## 4) Data Contracts

### 4.1 App-level contract
Deleting/moving a library folder must **never** delete:
- profiles
- devices
- app settings
- registry entries (they may become “missing”)

### 4.2 Library-level contract
Library is portable:
- Move folder → open on another machine → metadata persists
- Originals are never modified or deleted by the app

---

## 5) Database Schema (authoritative)

### 5.1 App DB — `~/.dadcam/app.db`

#### Versioning
Use `PRAGMA user_version` and run migrations in order.

#### Migration A1 (camera tables)
```sql
CREATE TABLE IF NOT EXISTS bundled_profiles (
  slug TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  match_rules TEXT NOT NULL DEFAULT '{}',
  transform_rules TEXT NOT NULL DEFAULT '{}',
  bundled_version INTEGER NOT NULL DEFAULT 1
);

-- Long-term schema includes uuid from day 1.
-- If upgrading from an older app that lacks uuid, Migration A2 must add/backfill it.
CREATE TABLE IF NOT EXISTS user_profiles (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  match_rules TEXT NOT NULL DEFAULT '{}',
  transform_rules TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_user_profiles_name ON user_profiles(name);

CREATE TABLE IF NOT EXISTS camera_devices (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  uuid TEXT NOT NULL UNIQUE,
  profile_type TEXT NOT NULL DEFAULT 'none'
    CHECK (profile_type IN ('bundled','user','none')),
  profile_ref TEXT NOT NULL DEFAULT '',
  serial_number TEXT,
  fleet_label TEXT,
  usb_fingerprints TEXT NOT NULL DEFAULT '[]', -- JSON array of stable fingerprint strings
  rental_notes TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_camera_devices_uuid ON camera_devices(uuid);
CREATE INDEX IF NOT EXISTS idx_camera_devices_profile ON camera_devices(profile_type, profile_ref);
```

#### Migration A2 (libraries registry + settings + upgrade support)
```sql
-- Library registry (UX)
CREATE TABLE IF NOT EXISTS libraries (
  library_uuid TEXT PRIMARY KEY NOT NULL,
  path TEXT NOT NULL,
  label TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  last_opened_at TEXT,
  last_seen_at TEXT,
  is_pinned INTEGER NOT NULL DEFAULT 0,
  is_missing INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_libraries_last_opened ON libraries(last_opened_at);

-- App settings (KV)
CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);

-- Upgrade support: if user_profiles exists without uuid column, add it.
-- SQLite can't conditionally ALTER in pure SQL reliably; do this in Rust:
-- 1) PRAGMA table_info(user_profiles)
-- 2) if uuid missing: ALTER TABLE user_profiles ADD COLUMN uuid TEXT;
-- 3) backfill uuid for every row where uuid is NULL/empty
-- 4) create unique index on uuid
```

#### Migration A3 (optional later — entitlement persistence)
```sql
CREATE TABLE IF NOT EXISTS entitlements (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Required settings keys (v1):**
- `ui_mode`: `"simple"` or `"advanced"`
- `simple_default_library_uuid`: `"<uuid>"` or `""`
- `title_card_offset_seconds`: `"5"`
- `features`: JSON map (`{"screengrabs":false,"face_detection":false,"best_clips":false}`)

---

### 5.2 Library DB — `<library>/.dadcam/dadcam.db`

#### Migration L0 (library identity — required)
```sql
CREATE TABLE IF NOT EXISTS library_meta (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);
-- required: key='library_uuid'
```

#### Migration L6 (clips gain stable camera refs)
```sql
ALTER TABLE clips ADD COLUMN camera_profile_type TEXT;
ALTER TABLE clips ADD COLUMN camera_profile_ref TEXT;
ALTER TABLE clips ADD COLUMN camera_device_uuid TEXT;

CREATE INDEX IF NOT EXISTS idx_clips_camera_profile_ref
  ON clips(camera_profile_type, camera_profile_ref);
CREATE INDEX IF NOT EXISTS idx_clips_camera_device_uuid
  ON clips(camera_device_uuid);
```

#### Migration L7 (recommended — VHS recipes)
`vhs_edits` stores **deterministic recipe definitions** (rebuildable).
The existing `export_history` table (Migration 4) stores **execution logs** (when it ran, status, output path).
They coexist: `vhs_edits.edit_uuid` can be referenced by `export_history` rows, but `export_history` is not replaced.

```sql
CREATE TABLE IF NOT EXISTS vhs_edits (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  edit_uuid TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  pipeline_version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),

  recipe_hash TEXT NOT NULL,               -- BLAKE3(canonical recipe JSON bytes)
  input_clip_ids TEXT NOT NULL,            -- canonical JSON array (order matters)
  title_text TEXT NOT NULL DEFAULT '',
  title_offset_seconds INTEGER NOT NULL DEFAULT 5,
  audio_blend_params TEXT NOT NULL,        -- canonical JSON
  transform_overrides TEXT NOT NULL,       -- canonical JSON

  output_relpath TEXT,                     -- relative to library root (portable)
  output_hash TEXT                         -- BLAKE3(output bytes) if built
);
CREATE INDEX IF NOT EXISTS idx_vhs_edits_created_at ON vhs_edits(created_at);
```

---

## 6) Migration & Backfill Rules (no guesswork)

### 6.1 Library UUID initialization (required)
When creating/opening a library:
1. Open library DB
2. Ensure `library_meta` exists (L0)
3. Read library_uuid
4. If missing:
   - generate UUID v4
   - insert it once
5. Return UUID

### 6.2 Backfill `clips.camera_*` from legacy integer IDs
During Migration L6 backfill:

**Legacy inputs (from Migration 5, added in v0.1.82)**
- `clips.camera_profile_id` (int FK) — added by Migration 5, references library-local `camera_profiles.id`
- `clips.camera_device_id` (int FK) — added by Migration 5, references library-local `camera_devices.id`
- `camera_profiles` table (library-local, columns: id, name, match_rules, transform_rules, version, created_at)
- `camera_devices` table (library-local Migration 5, columns: id, uuid, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at)
- These tables may not exist in libraries created before v0.1.82 (do not rely on them always existing)

**Locked algorithm**
For each clip:
1. Resolve / create device UUID
   - If legacy library `camera_devices` row contains a UUID: use it
   - Else: generate `device_uuid = uuidv4()`
   - Ensure App DB has `camera_devices(uuid=device_uuid)`; insert if missing
2. Resolve / create profile reference
   - If legacy profile name exists:
     - Try bundled match:
       - match if `slug == legacy_name` OR `name == legacy_name` (case-insensitive)
       - if found ⇒ `profile_type='bundled'`, `profile_ref=<slug>`
     - Else: create **migrated user profile** in App DB:
       - `uuid=uuidv4()`
       - `name="[Migrated] " + legacy_name`
       - `match_rules='{}'`, `transform_rules='{}'`, `version=1`
       - ⇒ `profile_type='user'`, `profile_ref=<uuid>`
   - If no legacy profile exists ⇒ `profile_type='none'`, `profile_ref=''`
3. Write new columns to clip:
   - `camera_device_uuid=device_uuid`
   - `camera_profile_type=profile_type`
   - `camera_profile_ref=profile_ref`

**Never drop old columns/tables**.

### 6.3 Tauri Store migration (one-time, at startup)
The app currently stores settings in `tauri-plugin-store` (added v0.1.36, schema v2 since v0.1.72).
On first launch after this migration:

1. Read existing Tauri Store file (platform-specific location, managed by tauri-plugin-store)
2. If store contains v2 settings:
   - Write `ui_mode` from `settings.mode` ("simple" or "advanced")
   - Write `features` from `settings.featureFlags` (JSON)
   - Write `title_card_offset_seconds` from `settings.devMenu.titleStartSeconds`
   - Write `simple_default_library_uuid`: resolve from `settings.defaultProjectPath` (open that library DB, read its UUID)
   - For each entry in `settings.recentProjects`: open library DB, read UUID, upsert into `libraries` table
   - Copy `settings.firstRunCompleted`, `settings.theme` into `app_settings` KV
   - Copy `settings.devMenu`, `settings.licenseStateCache` into `app_settings` KV
3. Write `app_settings` key `tauri_store_migrated` = `"true"`
4. On subsequent launches, skip if `tauri_store_migrated` is already set
5. Do **not** delete the old store file (keep as backup)
6. After migration ships and is stable, `tauri-plugin-store` dependency can be removed in a future version

### 6.4 Legacy JSON migration (`~/.dadcam/custom_cameras.json`)
One-time import at startup:
- If file exists:
  - parse devices
  - upsert into App DB `camera_devices`
  - rename file to `.migrated` (do not delete)

---

## 7) Matcher Rules (authoritative)

### 7.1 Input data to matcher (minimum)
Matcher should use any available inputs:
- USB fingerprint strings (from device enumeration)
- file path (relative to import root)
- container format
- codec
- make/model (from exif/metadata if available)
- resolution, frame rate, scan type (if available)

### 7.2 Matching order (locked)
1. Registered device match (USB fingerprint → device UUID)
   - If device has assigned `profile_type/ref` (not none) ⇒ return immediately
2. User profiles rules engine
3. Bundled profiles rules engine
4. Generic fallback (none)

### 7.3 Rule evaluation semantics (locked)
`match_rules` JSON object:
- Keys are ANDed (all specified keys must match).
- Within a key, arrays are ORed.
- String compares are case-insensitive.
- `folderPattern` is case-insensitive regex matched against relative path.

### 7.4 Tie-breakers (locked)
If multiple profiles match:
1. Higher `version`
2. Higher specificity score (Appendix A)
3. Stable sort by `profile_ref` to be deterministic

---

## 8) Implementation Plan (coding-agent checklist)

### Group 0 — Preconditions
- [ ] Save this doc at `docs/planning/libraryfix.md`
- [ ] Add `PIPELINE_VERSION` constant (increment when transforms/derived behavior changes)
- [ ] Ensure BLAKE3 hashing is available

### Group 1 — DB initialization (app + library)
- [ ] Implement `ensure_app_db_initialized()` (called once at startup)
  - create `~/.dadcam/` dir if missing
  - open app.db
  - set pragmas:
    - `PRAGMA journal_mode=WAL;`
    - `PRAGMA foreign_keys=ON;`
    - `PRAGMA busy_timeout=5000;`
  - run migrations A1/A2
- [ ] Implement `open_app_db_connection()` (opens and sets pragmas, **does not** migrate)
- [ ] Implement `ensure_library_db_initialized(library_path)` (called on library open/create)
  - open library DB
  - run L0/L6/L7 as needed
  - backfill L6 using App DB connection
- [ ] Implement `open_library_db_connection(library_path)` (no migration)

### Group 2 — Library UUID + registry
- [ ] `get_or_create_library_uuid(library_conn) -> String`
- [ ] App registry module:
  - `upsert_library(library_uuid, path, label?)`
  - `mark_opened(library_uuid)`
  - `mark_seen(library_uuid)`
  - `mark_missing(library_uuid, bool)`
  - `list_recent_libraries()`
  - `relink_library(library_uuid, new_path)` (validates UUID matches)

### Group 3 — App settings
- [ ] KV get/set
- [ ] typed helpers: ui_mode, features, title offset

### Group 4 — Cameras in App DB
- [ ] Bundled profiles:
  - `resources/cameras/bundled_profiles.json`
  - `sync_bundled_profiles(app_conn)` does full replace (idempotent)
- [ ] User profiles CRUD (uuid stable)
- [ ] Devices CRUD + legacy JSON import

### Group 5 — Library clip schema + ingest
- [ ] Update `Clip` / `NewClip` structs and all queries
- [ ] Replace any integer camera refs with `{profile_type, profile_ref, device_uuid}`
- [ ] In ingest worker:
  - open app db connection
  - open library db connection
  - run matcher
  - update clip match columns
  - write sidecar snapshot in new format

### Group 6 — Frontend wiring
- [ ] Replace integer `profileId` with `{profileType, profileRef}`
- [ ] Cameras UI works with no library open
- [ ] Recent libraries UI uses app registry
- [ ] Simple/Advanced selection stored in `app_settings` and drives UI-only toggles

### Group 7 — Dev Menu tooling
- [ ] Raw SQL tool can target App DB or Current Library DB
- [ ] Authoring tools write to staging and require tests to pass before publish

---

## 9) Tests (must exist before shipping)

### 9.1 Migration tests
- [ ] Fresh install: ensure_app_db_initialized creates app.db and tables
- [ ] Upgrade install: user_profiles without uuid ⇒ uuid column added + backfilled
- [ ] Library open: library_uuid created if missing
- [ ] Library L6 migration: columns added and populated deterministically

### 9.2 Persistence invariants (integration tests)
- [ ] Cameras screen works with no library open
- [ ] Deleting a library folder does not delete profiles/devices/settings
- [ ] Bundled sync idempotent (twice yields same rows)
- [ ] Legacy JSON import runs once and renames file
- [ ] Recents persist across restart
- [ ] Move library ⇒ marked missing ⇒ relink validates UUID then fixes path

### 9.3 Concurrency
- [ ] Stress: run parallel match + ingest + list-recents (no timeouts)

### 9.4 Profile DB quality gate
For each new bundled profile:
- [ ] Evidence metadata JSON committed
- [ ] Unit test: evidence matches intended profile
- [ ] Negative test: near-miss does not match

### 9.5 VHS recipe tests (if L7 enabled)
- [ ] Creating recipe stores canonical recipe_hash
- [ ] Rebuild on same machine yields same output_hash
- [ ] Changing formulas does not mutate stored recipes

---

## 10) Manual Acceptance (Definition of Done)

- [ ] First run: choose Simple/Advanced ⇒ persists across restart
- [ ] Open library ⇒ appears in recents + last-opened after restart
- [ ] Cameras screen: add user profile + register device ⇒ persists after restart
- [ ] Delete library folder ⇒ app-global data remains
- [ ] Import from USB camera ⇒ clips store stable refs
- [ ] Dev SQL queries App DB and Library DB
- [ ] (If enabled) VHS edit export: recipe stored, rebuildable

---

## Appendix A — Match Rules JSON Schema (v1)

`match_rules` allowed keys (all optional):
- `make`: [string]
- `model`: [string]
- `codec`: [string]
- `container`: [string]
- `folderPattern`: string (regex)
- `minWidth`, `minHeight`, `maxWidth`, `maxHeight`: number
- `frameRate`: [number] (tolerate +/- 0.01)
- `scanType`: ["interlaced"|"progressive"] (if detectable)

**Specificity score (tie-break):**
- +5 make+model
- +3 folderPattern
- +3 codec+container
- +2 resolution constraints
- +1 frameRate
Highest score wins after version tie-break.

---

## Appendix B — Transform Rules JSON Schema (v1)

`transform_rules` allowed keys:
- `deinterlace`: boolean
- `deinterlaceMode`: "yadif" | "bwdif"
- `crop`: { "top":n, "left":n, "right":n, "bottom":n }
- `stabilize`: boolean
- `audioNormalize`: boolean
- `lut`: string (id/path)
- `notes`: string (human explanation; **required for bundled profiles**)

---

## Appendix C — Path Canonicalization (cross-platform)

Store:
- `libraries.path` as absolute string.
- On write: attempt canonicalization.
  - mac/linux: `std::fs::canonicalize`
  - windows: `dunce::canonicalize`
If canonicalization fails, store raw absolute path and set `is_missing=1`.

Never identify a library by path alone. Always use `library_uuid`.

---

## Appendix D — VHS Recipe Canonicalization + Hashes

- Canonicalize JSON:
  - stable key ordering
  - arrays preserve order
  - no whitespace
- `recipe_hash = BLAKE3(canonical_recipe_json_bytes)`
- `output_hash = BLAKE3(output_file_bytes)` (only if output exists)

FFmpeg metadata stability (recommended):
- `-map_metadata -1 -map_chapters -1`
- `-metadata creation_time=0`
- constant encode params

---

## Appendix E — Superseded notes
Any prior plan that references profiles by **name** is superseded. Do not implement name-based refs.

