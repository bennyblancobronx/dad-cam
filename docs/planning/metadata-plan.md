# Gold-Standard Metadata & Camera Profile Framework

## Implementation Guide v3.2

**Status:** Planning

---

## Philosophy

The app works on ANY video file from ANY source. Camera profiles ENHANCE the experience but never GATE it. A screen recording, a dashcam clip, a file downloaded off the internet -- they all import, play, and auto-edit. The profile system is additive polish, not a requirement.

---

## Table of Contents

1. [What Was Wrong in v2.1 (Corrections)](#1-what-was-wrong-in-v21-corrections)
2. [What Actually Exists in the Codebase](#2-what-actually-exists-in-the-codebase)
3. [Gold-Standard Principles](#3-gold-standard-principles)
4. [Architecture Layers](#4-architecture-layers)
   - [Layer 0: Raw Dump Capture](#layer-0-raw-dump-capture)
   - [Layer 0b: Outlier Media Handling](#layer-0b-outlier-media-handling)
   - [Layer 1: Parsed Metadata](#layer-1-parsed-metadata)
   - [Layer 2: Generic Fallback Profile](#layer-2-generic-fallback-profile)
   - [Layer 3: Camera Database](#layer-3-camera-database)
   - [Layer 4: Registered Cameras](#layer-4-registered-cameras)
   - [Layer 5: Match Audit Trail](#layer-5-match-audit-trail)
   - [Layer 6: Metadata Extraction State Machine](#layer-6-metadata-extraction-state-machine)
   - [Layer 7: Sidecar Atomic Writes](#layer-7-sidecar-atomic-writes)
   - [Layer 8: Re-extraction and Re-matching](#layer-8-re-extraction-and-re-matching)
5. [Implementation Order (11 Steps)](#5-implementation-order)
6. [Version History](#6-version-history)

---

## 1. What Was Wrong in v2.1 (Corrections)

The v2.1 plan had 8 discrepancies with the actual codebase. Every one was found by auditing the files the plan references. If you are reading the old v2.1 plan, these corrections override it.

### C1: Two Parallel Matching Systems

**The problem:** The v2.1 plan only referenced the legacy matching system. Two actually exist.

**Reality:**
- **Legacy system:** `camera/mod.rs` + `camera/matcher.rs` operates on the Library DB `camera_profiles` table. No slugs. Equal-weight scoring. DEPRECATED.
- **Current system:** `ingest/mod.rs` `resolve_stable_camera_refs()` + `db/app_schema.rs` operates on the App DB `bundled_profiles` + `user_profiles` tables. HAS slugs. Weighted scoring. This is the one that runs during ingest.

**Fix:** All implementation steps reference `ingest/mod.rs` as the primary matching entry point. `camera/matcher.rs` is deprecated.

### C2: Wrong Fallback Location

**The problem:** The plan said to change the fallback in `camera/matcher.rs`.

**Reality:** The actual fallback is at `ingest/mod.rs:1271` in `resolve_profile_from_app_db()`, which returns `('none', '')`.

**Fix:** Step 1 targets `ingest/mod.rs:1271`.

### C3: Wrong Profile Source File

**The problem:** The plan said to seed `generic-fallback` into `canonical.json`.

**Reality:** The App DB system reads from `bundled_profiles.json` via `sync_bundled_profiles()` in `app_schema.rs`. The file `canonical.json` feeds the LEGACY Library DB system via `camera/bundled.rs`.

**Fix:** `generic-fallback` goes into `bundled_profiles.json`. `canonical.json` is the legacy source and is not the primary path.

### C4: Migration 12 Not Needed

**The problem:** The plan proposed migration 12 to add `profile_type` and `profile_ref` to `camera_devices`.

**Reality:** App DB migration A1 already has these columns:
- `camera_devices.profile_type TEXT CHECK IN ('bundled','user','none')`
- `camera_devices.profile_ref TEXT`

**Fix:** Migration 12 removed. Only `sample_exif_path` is genuinely missing (minor).

### C5: Wrong Deinterlace Type

**The problem:** The plan says generic-fallback should use `deinterlace='auto-detect'` (a string).

**Reality:** In Rust, `camera/mod.rs` `TransformRules` has:
- `deinterlace: Option<bool>` -- cannot hold a string
- `deinterlace_mode: Option<String>` -- for yadif/bwdif

**Fix:** Generic-fallback uses `deinterlace=null` (`None` in Rust). The proxy generator interprets `None` as "auto-detect from `field_order`." The JSON in `bundled_profiles.json` stores:
```json
{"deinterlace": null, "deinterlace_mode": "yadif"}
```
Meaning: if the file needs deinterlacing (per `field_order`), use yadif. No type change needed.

### C6: Weighted Scoring Already Exists

**The problem:** The plan proposed weighted scoring as a new feature.

**Reality:** `score_match_rules()` at `ingest/mod.rs:1336` already implements:
- +5 make+model
- +3 folderPattern
- +3 codec+container
- +2 resolution
- +1 frameRate

**Fix:** Step 4 does NOT add weighted scoring. Instead it adds reject rules (negative matching) and the audit trail, which are genuinely missing.

### C7: Wrong Single Source

**The problem:** The plan said to consolidate everything to `canonical.json`.

**Reality:**
- App DB system uses `bundled_profiles.json`
- Legacy Library DB system uses `canonical.json`
- These are two different files for two different systems

**Fix:** `bundled_profiles.json` is the single source of truth. Step 5 consolidates by deprecating the legacy path.

### C8: Schema Gaps Overstated

**The problem:** The plan said `camera_devices` needs `profile_slug` and `profile_type` columns.

**Reality:** App DB `camera_devices` already has `profile_type` and `profile_ref` columns (migration A1). Only `sample_exif_path` is genuinely missing.

---

## 2. What Actually Exists in the Codebase

This section documents what IS BUILT TODAY. Every implementation step references this section to know what to change versus what to build new. Read this carefully before writing any code.

### 2.1 Matching Systems

#### Current System (App DB) -- THE ONE THAT RUNS DURING INGEST

| Property | Detail |
|----------|--------|
| Entry point | `ingest/mod.rs` `resolve_stable_camera_refs()` line ~1159 |
| Profile source | App DB `bundled_profiles` table (seeded from `bundled_profiles.json`) + `user_profiles` table |
| Device source | App DB `camera_devices` table |
| Fallback | `('none', '')` at `ingest/mod.rs:1271` -- this is what we change to `('bundled', 'generic-fallback')` |

**Matching functions (all in `ingest/mod.rs`):**

1. `resolve_profile_from_app_db()` -- Priority chain: user profiles > bundled profiles > legacy name > fallback `('none', '')`
2. `match_app_profile_rules()` -- Scores user profiles, returns UUID of best match
3. `match_bundled_profile_rules()` -- Scores bundled profiles, returns slug of best match
4. `score_match_rules()` -- Weighted scoring: +5 make+model, +3 folderPattern, +3 codec+container, +2 resolution, +1 frameRate

**Device matching:** `resolve_stable_camera_refs()` checks devices by USB fingerprint (`find_device_by_usb_fingerprint_app`), then serial (`find_device_by_serial_app`), then falls through to profile matching.

#### Legacy System (Library DB) -- DEPRECATED

| Property | Detail |
|----------|--------|
| Entry point | `camera/matcher.rs` `match_camera()` |
| Profile source | Library DB `camera_profiles` table (seeded from `canonical.json` via `camera/bundled.rs`) |
| What it does | Equal-weight scoring (confidence = matches/total_rules), checks devices by integer profile_id |
| Status | DEPRECATED. Still compiled. Not called during standard ingest. May be called from legacy code paths. |

### 2.2 Profile Sources

#### `resources/cameras/bundled_profiles.json` (PRIMARY)

- **Feeds:** App DB `bundled_profiles` table via `sync_bundled_profiles()` in `app_schema.rs`
- **Format:** Array of `{slug, name, version, match_rules, transform_rules}`
- **Current entries:** 3 profiles: `sony-handycam-avchd`, `canon-dslr`, `panasonic-minidv`

#### `resources/cameras/canonical.json` (LEGACY)

- **Feeds:** Legacy Library DB `camera_profiles` table via `camera/bundled.rs`
- **Current contents:** Empty array `[]`
- **Status:** LEGACY. Not the primary path.

### 2.3 App DB Schema (What You Have to Work With)

**`bundled_profiles` table:**
```
slug TEXT PRIMARY KEY
name TEXT
version INTEGER
match_rules JSON
transform_rules JSON
bundled_version TEXT
```

**`user_profiles` table:**
```
id INTEGER
uuid TEXT UNIQUE
name TEXT
version INTEGER
match_rules JSON
transform_rules JSON
created_at TEXT
```

**`camera_devices` table:**
```
id INTEGER
uuid TEXT UNIQUE
profile_type TEXT CHECK IN ('bundled','user','none')
profile_ref TEXT
serial_number TEXT
fleet_label TEXT
usb_fingerprints JSON
rental_notes TEXT
created_at TEXT
```

IMPORTANT: `camera_devices` ALREADY has `profile_type` + `profile_ref`. No migration needed for stable refs.

### 2.4 Library DB Clips Columns

| Column | Type | Source | Notes |
|--------|------|--------|-------|
| `camera_profile_id` | INTEGER FK | Migration 1 | Legacy. Points to Library DB `camera_profiles` table. |
| `camera_device_id` | INTEGER FK | Migration 5 | Legacy. Points to Library DB `camera_devices` table. |
| `camera_profile_type` | TEXT | Migration 7 | 'bundled', 'user', or NULL. **This is the correct column.** |
| `camera_profile_ref` | TEXT | Migration 7 | Slug or UUID. **This is the correct column.** |
| `camera_device_uuid` | TEXT | Migration 7 | Device UUID. **This is the correct column.** |

Stable refs (migration 7) are the correct columns to use. Integer FKs are legacy.

### 2.5 TransformRules Rust Type

**Struct location:** `camera/mod.rs` `TransformRules`

| Field | Type | Notes |
|-------|------|-------|
| `deinterlace` | `Option<bool>` | `true` = always deinterlace, `false` = never, `None` = auto-detect. CANNOT hold a string. |
| `deinterlace_mode` | `Option<String>` | `"yadif"`, `"bwdif"`, etc. |
| `color_space` | `Option<String>` | |
| `lut` | `Option<String>` | |

### 2.6 Sidecar Writer

**File:** `ingest/sidecar.rs`

- **Write method:** `std::fs::write()` -- NOT atomic. No temp file, no fsync.
- **Current sections:** `original_file_path`, `file_hash_blake3`, `metadata_snapshot`, `camera_match`, `ingest_timestamps`, `derived_asset_paths`, `rental_audit`
- **Missing sections:** `rawExifDump`, `rawFfprobe`, `extendedMetadata`, `matchAudit`

### 2.7 ExifTool Extraction

**File:** `metadata/exiftool.rs`

- **Current method:** Requests 9 specific tags with `-TagName` flags
- **Tags:** `DateTimeOriginal`, `CreateDate`, `MediaCreateDate`, `Make`, `Model`, `SerialNumber`, `InternalSerialNumber`, `GPSLatitude`, `GPSLongitude`
- **What needs to change:** Switch to `exiftool -j -G -n` for full dump

### 2.8 FFprobe Extraction

**File:** `metadata/ffprobe.rs`

- **Current fields:** `codec_type`, `codec_name`, `width`, `height`, `r_frame_rate`, `channels`, `sample_rate`, `duration`
- **Missing fields:** `field_order`, `bits_per_raw_sample`, `color_space`, `color_primaries`, `color_transfer`, `display_aspect_ratio`, `sample_aspect_ratio`, `profile`, `level`

---

## 3. Gold-Standard Principles

These principles come directly from the import tool's design. The same patterns that make import bulletproof apply to metadata.

### 3.1 Every File Gets Processed

**Import analog:** Every file in the source directory gets a manifest entry, even sidecars and orphans.

**Metadata application:** Every clip gets metadata extracted AND a profile assigned. Zero clips left in limbo. The generic fallback guarantees 100% coverage. There is no "none" state.

### 3.2 Capture Everything, Parse Later

**Import analog:** Manifest records raw size + mtime before deciding what to do with the file.

**Metadata application:** Store the full raw exiftool and ffprobe dumps in the sidecar. Parse what we need today. When we need more fields tomorrow, re-parse from the stored dump -- never re-run the tools on the original file.

### 3.3 Two-Tier Verification

**Import analog:** Fast hash for dedup candidates, full hash for proof. Never trust fast hash alone.

**Metadata application:** Fast match (make+model) for profile candidates, full match (all rules + negative rules) for assignment. Never assign a profile on a single weak signal.

### 3.4 Audit Trail for Every Decision

**Import analog:** Every file gets a manifest entry with result, error_code, error_detail. You can reconstruct exactly what happened.

**Metadata application:** Every clip gets a `matchAudit` section in its sidecar: what metadata was available, what profiles were tested, what scores each got, why the winner won. You can reconstruct every matching decision without re-running anything.

### 3.5 Fail-Safe Defaults

**Import analog:** `safe_to_wipe` is NULL until ALL checks pass. One failure blocks the gate.

**Metadata application:** Generic fallback is SAFE. It auto-detects deinterlace and rotation from the file itself. It never applies a wrong color transform or LUT. Doing nothing wrong is better than guessing wrong.

### 3.6 State Machine with Resumability

**Import analog:** Session status: discovering -> ingesting -> rescanning -> complete. Crash at any point, resume from last known state.

**Metadata application:** Metadata extraction status per clip: pending -> extracting -> extracted -> matching -> matched -> verified. If the app crashes mid-extraction, we know exactly which clips need reprocessing.

### 3.7 Immutable Baseline

**Import analog:** `manifest_hash` is computed once from the sorted file list. Evidence of what existed at discovery time.

**Metadata application:** `rawExifDump` and `rawFfprobe` in the sidecar are the immutable baseline. They capture exactly what the tools reported at ingest time. All derived fields (sensor type, profile match) are computed FROM this baseline and can be recomputed.

---

## 4. Architecture Layers

The system is built in 9 layers. Each layer builds on the ones below it. Implement them in order.

---

### Layer 0: Raw Dump Capture

**Purpose:** Capture the COMPLETE output of both extraction tools at ingest time. Store once, never re-run. This is the foundation everything else builds on.

#### ExifTool Command

```bash
exiftool -j -G -n <file>
```

| Flag | What it does |
|------|-------------|
| `-j` | JSON output |
| `-G` | Group names (e.g., `EXIF:Make`, `QuickTime:CreateDate`) -- disambiguates duplicate tag names across groups |
| `-n` | Numeric values (no human-readable formatting) -- gives raw numbers for GPS, focal length, etc. |

#### FFprobe Command

```bash
ffprobe -v quiet -print_format json -show_format -show_streams \
  -show_entries stream=field_order,bits_per_raw_sample,color_space,color_primaries,color_transfer,display_aspect_ratio,sample_aspect_ratio,profile,level \
  <file>
```

#### Storage

Both raw outputs are stored in the sidecar JSON as `rawExifDump` (object) and `rawFfprobe` (object). These are the immutable baseline.

**Size impact:** `rawExifDump` is 2-15KB per clip. `rawFfprobe` is 1-5KB per clip. For 10,000 clips that's 30-200MB in sidecars. Acceptable.

#### Extraction Validation

- **ExifTool:** If exit code 0 and JSON parses successfully, extraction is valid. If exit code != 0 or JSON fails to parse, mark clip as `extraction_failed` with error detail. Do NOT block ingest -- clip still gets imported, just with incomplete metadata.
- **FFprobe:** Same pattern. FFprobe failure means no duration/resolution/codec. Clip still imports. If exiftool succeeded, mark as `extracted` with partial data noted in sidecar `extractionStatus` section (see G7). If both tools failed, mark as `extraction_failed`.

#### States

| State | Meaning |
|-------|---------|
| `pending` | Clip created, no extraction attempted yet |
| `extracting` | Tools are running (in-progress marker for crash recovery) |
| `extracted` | Both tools ran, raw dumps stored. May have partial data if one tool failed. |
| `extraction_failed` | Both tools failed. Clip has no metadata beyond filename and file size. Still imported. |

#### Why This Is Gold Standard

The import tool captures size+mtime at discovery time as immutable evidence. This captures the full tool output as immutable evidence. When we decide next year that we need a field we didn't think of today, it's already in the dump. No re-extraction needed.

---

### Layer 0b: Outlier Media Handling

**Purpose:** Contract #4 says audio-only and image files are ACCEPTED. This layer defines how layers 0-2 behave for non-video media. The extraction pipeline must handle these without treating them as errors.

#### Media Type Detection

**Method:** After ffprobe runs, classify by stream presence:

| Detected Type | Condition |
|---------------|-----------|
| `video` | Has video + audio streams |
| `audio` | Audio stream only (no video stream) |
| `image` | Single video frame, no duration or duration=0 |
| `unknown` | FFprobe fails but exiftool succeeds |

**DB column:** `media_type TEXT CHECK (media_type IN ('video','audio','image','unknown'))` on the clips table. Set during extraction. Default `'video'` for backwards compat.

**Sidecar field:** `mediaType` in `metadataSnapshot` section.

#### Extraction by Type

**Video (normal path):**
- ExifTool: Full dump as normal
- FFprobe: Full dump -- has video + audio streams
- Status: `extracted`

**Audio-only (e.g., MP3, WAV, M4A):**
- ExifTool: Full dump -- reads MP3/WAV/M4A tags (artist, album, title, duration). Returns valid JSON.
- FFprobe: Full dump -- returns audio stream only. No video stream. `duration`, `sample_rate`, `channels`, `audio_codec` are populated. Video fields (`width`, `height`, `fps`, `codec`, `field_order`) are null.
- Status: `extracted` (NOT `extraction_failed` -- having no video stream is expected for audio files, not an error)
- IMPORTANT: The state machine must distinguish "no video stream because audio-only" from "no video stream because ffprobe failed." `media_type='audio'` handles this.

**Image (e.g., JPEG, PNG, TIFF):**
- ExifTool: Full dump -- exiftool is STRONGEST on images (EXIF, IPTC, XMP). Returns rich data: make, model, GPS, focal length, exposure, etc.
- FFprobe: Full dump -- returns a single video stream with width/height but duration=0 or N/A. `codec_name` is `mjpeg` or `png` etc. No audio stream.
- Status: `extracted`
- Note: Images often have MORE exiftool data than video files. The raw dump is especially valuable here.

**Unknown:**
- ExifTool: May succeed or fail depending on file type
- FFprobe: Failed or returned no usable streams
- Status: `extraction_failed` OR `extracted` (if exiftool alone succeeded)
- File was still imported per contract #3. It just has minimal metadata.

#### Parsed Fields by Media Type

**Audio-only -- which fields are populated vs null-and-expected:**

| Populated | Null and Expected (not an error) |
|-----------|--------------------------------|
| `duration_ms`, `audio_codec`, `audio_channels`, `audio_sample_rate`, `bitrate`, `container`, `recorded_at` | `width`, `height`, `fps`, `codec` (video), `field_order`, `color_space`, `display_aspect_ratio` |

**Image -- which fields are populated vs null-and-expected:**

| Populated | Null and Expected (not an error) |
|-----------|--------------------------------|
| `width`, `height`, `camera_make`, `camera_model`, `serial_number`, `focal_length`, `gps_latitude`, `gps_longitude`, `recorded_at`, `color_space` | `duration_ms` (0 or null), `fps`, `audio_codec`, `audio_channels`, `field_order`, `bitrate` |

Images often have richer EXIF than video. `camera_make`/`camera_model` matching works well for images.

#### Profile Matching for Outliers

**Audio-only:** Profile matching still runs. Most `match_rules` check video-centric fields (codec, resolution, folder_pattern) so audio files will score low on everything and land on `generic-fallback`. This is correct. Generic-fallback transform_rules for audio: passthrough (no deinterlace, no rotation, no color transform). Audio proxy: copy or transcode to m4a.

**Image:** Profile matching runs. make+model rules CAN match images (a Canon DSLR photo matches `canon-dslr` profile by make+model). This is useful -- it links the photo to the same camera as the video. Transform_rules for images: no deinterlace, no rotation correction needed (image viewers handle EXIF orientation), no color transform. Thumbnail: the image itself (resized).

**Generic fallback for outliers:** Generic fallback already does passthrough for anything it doesn't understand. Audio and image files get: no transforms applied, media_type flag set, appropriate proxy/thumbnail generation. No special transform_rules needed.

#### Proxy Generation for Outliers

| Type | Proxy | Thumbnail | Sprite Sheet |
|------|-------|-----------|-------------|
| Audio-only | m4a transcode (or copy if already m4a/aac). No video proxy. | Waveform image or generic audio icon | N/A |
| Image | Resized JPEG (720p equivalent) | The image itself (resized) | N/A (single frame) |

Note: Proxy generation must check `media_type` before assuming video pipeline. This is a code path branch in the proxy generator. The `media_type` field from this layer drives it.

---

### Layer 1: Parsed Metadata

**Purpose:** Extract structured fields from the raw dumps. These are the values that go into the DB and drive the UI.

#### Fields from ExifTool Dump

**Core fields (used for matching and display):**

| Field | Source Tag | Notes |
|-------|-----------|-------|
| `recorded_at` | `DateTimeOriginal` > `CreateDate` > `MediaCreateDate` | Group-aware: prefer EXIF group over QuickTime group |
| `camera_make` | `EXIF:Make` or `QuickTime:Make` | |
| `camera_model` | `EXIF:Model` or `QuickTime:Model` | |
| `serial_number` | `EXIF:SerialNumber` > `EXIF:InternalSerialNumber` > `MakerNotes:SerialNumber` | Priority chain |

**Extended fields (stored in sidecar, not DB):**

| Field | Source Tag | Notes |
|-------|-----------|-------|
| `sensor_type` | `EXIF:ImageSensorType` | Direct, rare |
| `focal_length` | `EXIF:FocalLength` | mm, numeric from `-n` flag |
| `focal_length_35mm` | `EXIF:FocalLengthIn35mmFormat` | Computed crop factor = focal_length_35mm / focal_length |
| `scale_factor` | `Composite:ScaleFactor35efl` | Direct crop factor |
| `native_width` | `EXIF:ExifImageWidth` | Sensor native, not output |
| `native_height` | `EXIF:ExifImageHeight` | |
| `bits_per_sample` | `EXIF:BitsPerSample` | |
| `color_space` | `EXIF:ColorSpace` | |
| `white_balance` | `EXIF:WhiteBalance` | |
| `lens_model` | `EXIF:LensModel` | |
| `lens_id` | `EXIF:LensID` | |
| `megapixels` | `Composite:Megapixels` | |
| `rotation` | `Composite:Rotation` or `QuickTime:Rotation` | |
| `gps_latitude` | `EXIF:GPSLatitude` | Already numeric from `-n` flag |
| `gps_longitude` | `EXIF:GPSLongitude` | |
| `compressor_id` | `QuickTime:CompressorID` | |

#### Fields from FFprobe Dump

**Core fields:**

| Field | Source | Notes |
|-------|--------|-------|
| `duration_ms` | `format.duration` | Seconds -> ms |
| `width` | Video stream `width` | |
| `height` | Video stream `height` | |
| `fps` | Video stream `r_frame_rate` | Parse fraction (e.g., "30000/1001") |
| `codec` | Video stream `codec_name` | |
| `audio_codec` | Audio stream `codec_name` | |
| `audio_channels` | Audio stream `channels` | |
| `audio_sample_rate` | Audio stream `sample_rate` | |
| `bitrate` | `format.bit_rate` | |
| `container` | `format.format_name` | |
| `creation_time` | `format.tags.creation_time` | |

**Extended fields:**

| Field | Source | Notes |
|-------|--------|-------|
| `field_order` | Video stream `field_order` | `tt`, `bb`, `progressive`, `unknown` |
| `bits_per_raw_sample` | Video stream `bits_per_raw_sample` | |
| `color_space` | Video stream `color_space` | `bt709`, `bt601`, `bt2020`, etc. |
| `color_primaries` | Video stream `color_primaries` | |
| `color_transfer` | Video stream `color_transfer` | |
| `display_aspect_ratio` | Video stream `display_aspect_ratio` | |
| `sample_aspect_ratio` | Video stream `sample_aspect_ratio` | |
| `codec_profile` | Video stream `profile` | Baseline, Main, High for H.264 |
| `codec_level` | Video stream `level` | |

#### Storage Strategy

| Location | What Goes There |
|----------|----------------|
| DB `clips` table | Core fields only. Same columns as today. DB stays lean. |
| Sidecar `metadataSnapshot` section | Core + extended fields. This is the parsed, structured view. |
| Sidecar `extendedMetadata` section | Extended fields for sensor/lens/color data. |

#### Parsing Rules

1. **Null is fine.** Any field can be null. A clip with zero exiftool data still has ffprobe data. A clip with zero ffprobe data still has filename and size. There is always SOMETHING.
2. **Group disambiguation.** With the `-G` flag, exiftool returns `EXIF:Make` not just `Make`. When the same tag exists in multiple groups (e.g., `EXIF:CreateDate` vs `QuickTime:CreateDate`), prefer the EXIF group -- it's closer to the sensor.
3. **Numeric preference.** With the `-n` flag, GPS returns decimal degrees directly. No DMS parsing needed. FocalLength returns mm as float. Cleaner than parsing human-readable strings.

---

### Layer 2: Generic Fallback Profile

**Purpose:** The safety net. Every clip that doesn't match a specific profile gets this. It must produce CORRECT output for any video file -- camcorder, phone, screen recording, dashcam, downloaded meme, anything.

#### Profile Definition

```json
{
  "slug": "generic-fallback",
  "name": "Unknown Camera",
  "version": 1,
  "is_system": true,
  "deletable": false,
  "category": "system",
  "match_rules": {},
  "transform_rules": {
    "deinterlace": null,
    "deinterlace_mode": "yadif",
    "color_space": null,
    "lut": null
  }
}
```

#### Why match_rules is Empty

This profile is NEVER matched by rules. It is assigned by the fallback branch in `resolve_profile_from_app_db()` when no other profile scores above threshold. `score_match_rules()` returns 0.0 for empty objects, so this profile can never win by scoring. It only wins by being the fallback.

#### Transform Rules Explained

**Deinterlace (`null` / `None` in Rust):**
- Proxy generator interprets `deinterlace=null` as auto-detect.
- Read ffprobe `field_order` from raw dump.
- If `field_order` is `tt` or `bb` -> deinterlace with yadif.
- If `field_order` is `progressive` or missing -> do NOT deinterlace.
- Never guess.
- WHY: Deinterlacing progressive footage creates artifacts. Not deinterlacing interlaced footage looks terrible. Auto-detect from the file itself is the only safe choice.

**Rotation (`auto-detect`):**
- Read rotation from exiftool `Rotation` tag or ffprobe `side_data` displaymatrix.
- If present, apply rotation correction. If absent, do nothing.
- WHY: Phone footage is often recorded in portrait but played in landscape without rotation metadata applied. Respecting the tag fixes this silently.

**Color Space (`null` / passthrough):**
- Do NOT transform color. Pass through as-is.
- WHY: Wrong color transform is worse than no color transform. Without knowing the source camera, we can't know if the footage is Rec.601 (old SD) or Rec.709 (HD) or Rec.2020 (HDR). Passthrough is always safe.

**LUT:** null (no LUT applied)

**Audio:** passthrough

#### What Makes It Gold Standard

- **Do no harm.** The generic profile's job is to make the clip watchable without making anything worse.
- **Auto-detect from file.** It reads the FILE, not a lookup table. `field_order` and rotation are per-file properties. This means it works on any video from any source -- even files that have no camera metadata at all.
- **No assumptions.** It doesn't assume codec, container, resolution, era, or sensor. It only acts on signals that are in the file itself.
- **Upgradeable.** When a matching profile is later found (user adds a profile, database is updated), clips can be re-matched and upgraded. The generic assignment is never permanent.

#### Implementation Details

1. **Seeding:** Add `generic-fallback` entry to `bundled_profiles.json` (NOT `canonical.json`). It will be synced to App DB via `sync_bundled_profiles()` on startup. Must be first entry in the array.
2. **Fallback change:** In `ingest/mod.rs` `resolve_profile_from_app_db()` line 1271, change `('none', '')` to `('bundled', 'generic-fallback')`. This is a one-line change.
3. **Proxy generation:** When proxy generator reads transform_rules and sees `deinterlace=null`, it checks the clip's ffprobe raw dump for `field_order`. This replaces the current code that only deinterlaces when a profile explicitly says `deinterlace=true`.

---

### Layer 3: Camera Database

**Purpose:** Bundled profiles for known cameras. Enhances the experience with camera-specific transforms. Not required -- generic fallback handles everything the database doesn't cover.

#### Design Principles

1. **Verified only.** No profile ships unverified. Every profile in the database has been tested against real sample files. The verification workflow is mandatory.
2. **Single source.** `bundled_profiles.json` is the ONLY source for the App DB system. The legacy `canonical.json`/`camera/bundled.rs` path is deprecated.
3. **Additive value.** A profile must provide value BEYOND what generic-fallback already does. If a camera shoots progressive H.264 with correct rotation metadata, generic-fallback already handles it perfectly. Only create a profile if it adds: specific deinterlace mode, field order override, color space correction, LUT, or known sensor info.
4. **Negative rules.** Profiles can REJECT matches to prevent false positives. Example: "Canon DSLR" profile rejects `container='3gp'` (which would be Canon phone footage).

#### Profile Schema v2

```json
{
  "slug": "string -- stable forever (e.g., 'sony-hdr-cx405')",
  "name": "string -- display name, can change",
  "version": 1,
  "verified": true,
  "verified_sample": "sample_filename.mts",
  "category": "camcorder | dslr | mirrorless | action_cam | phone | capture_device | other",
  "era": "2007-2018",
  "sensor": {
    "type": "CCD | CMOS | BSI-CMOS | Stacked-CMOS | null",
    "size": "1/5.8\" | APS-C | Full-Frame | null",
    "megapixels": 2.0
  },
  "match_rules": {
    "make": ["Sony"],
    "model": ["HDR-CX"],
    "codec": ["h264"],
    "container": ["mts", "m2ts"],
    "folder_pattern": "AVCHD.*STREAM",
    "resolution": { "min_width": null, "max_width": null, "min_height": null, "max_height": null },
    "frame_rates": [29.97, 59.94],
    "compressor_id": [],
    "reject_codec": [],
    "reject_container": ["3gp"],
    "reject_model": ["iPhone"]
  },
  "transform_rules": {
    "deinterlace": true,
    "deinterlace_mode": "yadif",
    "field_order": "tff",
    "color_space": "bt709",
    "lut": null,
    "rotation_fix": false
  }
}
```

**Match rules details:**
- `make`, `model`, `codec`, `container` -- string arrays, OR logic within each array, AND across arrays
- `model` uses substring match, case-insensitive
- `folder_pattern` -- regex string
- `resolution` -- min/max bounds (any field can be null)
- `frame_rates` -- tolerance +/- 0.5
- `reject_*` -- if clip has this value, this profile does NOT match (negative matching)

#### Matching Algorithm v2

The existing scoring in `score_match_rules()` already works. Here is what is NEW:

**Phase 1 -- Reject (NEW):**
For each profile, check `reject_codec`, `reject_container`, `reject_model`. If ANY reject rule matches, skip this profile entirely. Return score 0.0.

**Phase 2 -- Score (EXISTING):**
Run existing `score_match_rules()`. Already has weighted scoring: +5 make+model, +3 folderPattern, +3 codec+container, +2 resolution, +1 frameRate.

**Phase 3 -- Threshold (NEW):**
If best score < 3.0, use generic-fallback. Store all candidates in audit.

#### Confidence Scale

| Confidence | Meaning |
|------------|---------|
| 1.0 | USB fingerprint match to registered device (layer 4) |
| 0.95 | Serial number match to registered device |
| 0.8-0.95 | Make + model + strong secondary signals (codec, resolution, folder) |
| 0.5-0.8 | Make or model + some secondary signals. Assigned but noted as partial match. |
| Below 0.5 | Too weak. Generic fallback assigned. Candidate recorded in audit trail. |
| 0.1 | Generic fallback (no candidates scored above threshold) |

#### Starter Profiles

**Tier 1 -- Most Common (build first):**

| Slug | Description |
|------|-------------|
| `sony-handycam-avchd` | 1080i AVCHD, needs deinterlace + field order. Massive dad cam population. |
| `iphone-h264` | Progressive H.264 MOV. No transforms needed but sensor/model info is valuable. |
| `iphone-hevc` | HEVC MOV. Same as above but codec differs. |
| `canon-vixia-hf` | Canon consumer camcorder line. AVCHD or MP4. |
| `panasonic-hc-v-series` | Panasonic consumer camcorder. AVCHD. |
| `dv-tape-generic` | Any `dvvideo` codec. Needs deinterlace. Catches all MiniDV regardless of brand. |
| `gopro-hero-h264` | GoPro H.264 MP4. Progressive, wide-angle. |

**Tier 2 -- Common:**
`sony-hdr-cx-series`, `canon-dslr-h264`, `nikon-dslr`, `jvc-everio-avchd`, `android-generic`, `gopro-hero-hevc`, `dji-phantom-mavic`

**Tier 3 -- Legacy:**
`sony-dcr-dvd`, `sony-dcr-trv-minidv`, `canon-zr-minidv`, `jvc-gr-minidv`, `panasonic-minidv`, `samsung-hmx`, `flip-video`, `kodak-zi-playsport`

**Tier 4 -- Digitized Analog:**
`vhs-c-digitized`, `hi8-digitized`, `composite-capture-generic`

#### Verification Workflow (Mandatory Per Profile)

1. Obtain 1-3 sample files from the target camera. Real files, not transcodes.
2. Run: `exiftool -j -G -n sample.mp4 > sample.exif.json`
3. Run: `ffprobe -v quiet -print_format json -show_format -show_streams sample.mp4 > sample.ffprobe.json`
4. Write `match_rules` from the dumps. Identify the MINIMUM set of rules that uniquely match this camera.
5. Write reject rules: what OTHER cameras share similar metadata? Add `reject_*` rules to prevent those false positives.
6. Test: run the matcher against the sample. Confirm the profile wins with score >= 3.0.
7. Cross-test: run the matcher against samples from 5+ OTHER cameras. Confirm this profile does NOT match them.
8. Determine `transform_rules`: play the original in ffplay. Does it need deinterlace? Check `field_order`. Does color look wrong? Check `color_primaries`. Record findings.
9. Test proxy generation with the `transform_rules`. Compare output quality to VLC playback of original.
10. Set `verified=true`, record sample filename, commit.

**Cross-validation:** When adding a new profile, re-run ALL existing profiles' cross-tests against the new sample. Ensures no existing profile is broken by the new addition.

**Sample storage:** Store verification samples in `test-library/` (already gitignored). NOT in `resources/` (too large for app bundle). Reference sample filename in profile's `verified_sample` field.

---

### Layer 4: Registered Cameras

**Purpose:** Physical camera units registered by pro/rental users. Highest match priority. The full flow covers: USB plug-in through database storage through backflow to existing clips.

#### What Already Exists

- `CameraDevice` struct (`devices.rs`): id, uuid, profile_id, serial_number, fleet_label, usb_fingerprints, rental_notes, created_at
- App DB `camera_devices` table (migration A1): `profile_type` + `profile_ref` -- ALREADY EXIST
- `insert_device()` -- creates device record
- `find_device_by_usb_fingerprint_app()` -- App DB device lookup by USB fingerprint
- `find_device_by_serial_app()` -- App DB device lookup by serial number
- `capture_usb_fingerprint()` -- cross-platform (macOS `system_profiler -xml`, Windows `Get-CimInstance`, Linux `/sys/bus/usb/devices/`)
- `save_devices_to_json()` / `load_devices_from_json()` -- export/import to `~/.dadcam/custom_cameras.json`
- `resolve_stable_camera_refs()` in `ingest/mod.rs` -- checks devices by USB fingerprint then serial BEFORE profile matching
- Library DB clips columns: `camera_profile_type`, `camera_profile_ref`, `camera_device_uuid` (migration 7)

#### What Is Missing

- No EXIF dump during registration (specified but not implemented)
- No auto-profile assignment from EXIF data during registration
- No backflow: registering a device does NOT re-match existing clips
- No `sample_exif_path` column on `camera_devices` (minor)

#### Registration Flow (Steps R1-R8)

**R1. USB Detection**

- Trigger: User opens Dev Menu > Register Camera > Via USB (or auto-detect on plug-in)
- Action: Call `capture_usb_fingerprint()`. Returns list of vendor:product pairs and serial strings.
- Output: `usb_fingerprints: ['0x054c:0x0b8c', 'serial:E35982']`
- Code: `devices.rs` `capture_usb_fingerprint()` -- already implemented
- Gap: None

**R2. Mount + Sample Discovery**

- Trigger: USB device detected or memory card mounted
- Action: Scan mounted volume for video files. Pick 1-3 representative samples (prefer largest file, or first file in DCIM/AVCHD structure).
- Output: `sample_paths: ['/Volumes/CAMERA_SD/AVCHD/BDMV/STREAM/00001.mts']`
- Code: `discover.rs` file discovery logic can be reused
- Gap: Need new function `discover_sample_files(mount_point)` that returns a small set of representative files without walking the entire tree.

**R3. Full EXIF + FFprobe Dump on Samples**

- Trigger: Sample files found
- Action: Run `exiftool -j -G -n` AND ffprobe on each sample. Capture full raw dumps. This is the SAME extraction used during ingest (layer 0), reused here.
- Output: For each sample: `{ rawExifDump: {...}, rawFfprobe: {...}, parsedMetadata: {...} }`
- Code: `metadata/exiftool.rs` and `metadata/ffprobe.rs` -- need to switch to full dump mode (layer 0 change)
- Gap: Current `exiftool.rs` requests only 9 tags. Layer 0 changes this to full dump. Registration flow benefits from the same change.

**R4. Auto-Profile Matching**

- Trigger: Parsed metadata available from samples
- Action: Run the matching algorithm. Use `match_bundled_profile_rules()` and `match_app_profile_rules()` from `ingest/mod.rs`.
- Output: `matchResult: { winner: 'sony-handycam-avchd', confidence: 0.85, candidates: [...] }`
- Code: `ingest/mod.rs` functions already accept profiles + metadata. These are the right functions.
- Gap: These functions don't return candidates/scores, only the winner slug. Need to extend to return all candidates for the registration UI.

**R5. Registration Form (UI)**

- Trigger: Auto-fill data ready
- Action: Present the registration form pre-filled with: make, model, serial (from EXIF), USB fingerprints (from R1), suggested profile (from R4). User reviews, edits fleet_label/notes, confirms.
- Output: `NewCameraDevice { profile_type, profile_ref, serial_number, fleet_label, usb_fingerprints, rental_notes }`
- Code: `devices.rs` `NewCameraDevice` struct -- already has the right fields
- Gap: UI not built yet. The auto-fill from EXIF is new.

**R6. Device Insert + Profile Assignment**

- Trigger: User clicks "Save Camera"
- Action: Call `insert_device()`. Store `profile_type` ('bundled' or 'user') and `profile_ref` (slug or UUID) from auto-match or user selection. If no profile matched, store `profile_type='none'`, `profile_ref=''`.
- Code: `insert_device()` + App DB `camera_devices` already has the columns (migration A1)
- Gap: `insert_device()` may need to be updated to write `profile_type`/`profile_ref`. Currently may write to legacy `profile_id` instead.

**R7. JSON Backup Export**

- Trigger: After successful insert
- Action: Call `save_devices_to_json()` to write `~/.dadcam/custom_cameras.json` as backup.
- Code: `devices.rs` `save_devices_to_json()` -- works today
- Gap: None

**R8. EXIF Registration Dump Storage**

- Trigger: After successful insert
- Action: Store the full EXIF + FFprobe dumps from sample files alongside the device record.
- Output: `~/.dadcam/device_dumps/<device_uuid>.json` containing raw dumps from samples
- Code: Nothing -- new
- Gap: New storage location and format. Same principle as sidecar raw dumps (layer 0) but for devices instead of clips.

#### Backflow to Existing Clips (Steps B1-B6)

When a new device is registered, re-check all existing clips that might belong to this camera. This is the CRITICAL connection between registration and the library.

**Trigger:** After R6 (device saved to DB). Also on demand from Dev Menu > Re-match Clips.

**B1. Identify Candidate Clips**

Query all clips in all open libraries where `camera_device_uuid IS NULL` (not yet assigned to any device).

```sql
SELECT id, camera_profile_ref, camera_make, camera_model, serial_number
FROM clips
WHERE camera_device_uuid IS NULL OR camera_device_uuid = ''
```

Check ALL unassigned clips, not just generic-fallback ones. A clip might have a bundled profile match but no device assignment.

**B2. Match by Serial Number**

For the newly registered device, check its `serial_number` against clip metadata. If `device.serial_number` matches clip's `serial_number` (from exiftool extraction), this clip came from this camera.

- Confidence: 0.95
- Note: Serial comes from EXIF captured during ingest. If ingest didn't capture serial (old pipeline before layer 0), this won't match -- that's fine.

**B3. Match by Make + Model + Profile**

If device has an assigned profile with make+model rules, check if clip's `camera_make` + `camera_model` match those rules.

- Confidence: 0.6 -- weaker because multiple units of the same model exist
- This is a SOFT match. It says "this clip COULD be from this camera model." User can confirm or reject.

**B4. Match by USB Fingerprint (ingest session)**

Check ingest_sessions for this clip's ingest job. If the ingest session has `device_serial` or `device_mount_point` that matches the registered device's USB fingerprints, the clip was ingested from this device.

- Confidence: 0.90
- Note: This works when the user ingested footage BEFORE registering the camera. The ingest session captured the USB fingerprint at ingest time.

**B5. Apply Matches**

For each matched clip, update:
- `clips.camera_device_uuid = device.uuid`
- If the device has an assigned profile AND the clip's current profile is generic-fallback, ALSO upgrade: `clips.camera_profile_type = device.profile_type`, `clips.camera_profile_ref = device.profile_ref`

Requirements:
- All updates for a single device registration in one transaction. If any update fails, roll back all.
- Re-write the clip's sidecar with updated `cameraMatch` section. Use atomic sidecar write (layer 7).
- If profile changed (was generic, now specific with different transform_rules), mark the proxy as invalid. Next access regenerates with correct transforms.

**B6. Report Results**

Return to UI: "Registered Sony HDR-CX405 (Rental Unit #7). Found 47 clips from this camera: 12 matched by serial number (high confidence), 35 matched by camera model (needs confirmation)."

User can review the model-matched clips and confirm/reject assignments. Serial-matched clips are auto-assigned.

#### Backflow on Profile Add

When `bundled_profiles.json` is updated with new profiles (app update or manual import): check all registered devices that have `profile_type='none'`. Re-run profile matching against the device's stored EXIF dump (from R8). If a new profile matches, offer to assign it to the device AND backflow to all clips from that device.

#### Backflow on Manual Re-match

**Trigger:** User clicks "Re-match All Clips" in settings or Dev Menu.

**Action:** Re-run the full matching algorithm on every clip using stored `inputSignature` from `matchAudit` (layer 5). Check devices first (highest priority), then bundled profiles, then generic fallback. Update any clips that now have better matches. This is the layer 8 re-matching job. It covers both device-based and profile-based improvements.

#### Forward Flow During Ingest (Steps F1-F4)

When importing NEW footage, check registered devices FIRST. This is the forward path (device already registered, footage coming in now).

**F1. Capture USB fingerprint at ingest start**

If ingesting from a mounted volume, call `capture_usb_fingerprint()` and store in `ingest_sessions.device_serial` / `device_mount_point`. Already captured in ingest session (migration 9). Used for safe-to-wipe tracking. The data is already there -- we just need to USE it for matching.

**F2. Check registered devices BEFORE profile matching**

In `resolve_stable_camera_refs()`, the current code already checks devices first (USB fingerprint, then serial). This is the correct priority order. Gap: Verify device lookup returns `profile_type` + `profile_ref` and that these are stored on the clip.

**F3. Store stable refs on clip**

When a device match is found, store: `camera_device_uuid = device.uuid`, `camera_profile_type = device.profile_type`, `camera_profile_ref = device.profile_ref`. If device has `profile_type='none'`, fall through to profile matching. Gap: The bridge between `device.profile_type`/`profile_ref` and `clip.camera_profile_type`/`ref` may not be fully wired.

**F4. Write to sidecar with device context**

Sidecar `cameraMatch` section includes: `deviceUuid`, `profileType`, `profileRef`. `matchAudit` records that the match came from a registered device, not profile rules. Add `matchSource` field: `registered_device_usb`, `registered_device_serial`, `registered_device_model`, `bundled_profile`, `generic_fallback`.

#### Match Priority Chain (Top to Bottom)

1. Registered device by USB fingerprint -> confidence 1.0, assigns device + device's profile
2. Registered device by serial number -> confidence 0.95, assigns device + device's profile
3. Registered device by make+model -> confidence 0.6-0.8, suggests device (user confirms)
4. Bundled profile from `bundled_profiles.json` (scoring algorithm, no device) -> confidence varies
5. Generic fallback (always succeeds) -> confidence 0.1, auto-detect transforms from file

#### Complete Lifecycle Example

A user buys 5 Sony HDR-CX405 camcorders for a rental fleet. Over 6 months:

**Month 1:** User imports footage from first client. No cameras registered yet. Clips match `sony-handycam-avchd` bundled profile (make=Sony + codec=h264 + folder=AVCHD). Confidence 0.85. Profile applied. Clips get deinterlaced.

**Month 2:** User registers all 5 cameras via USB in Dev Menu. Each gets a UUID, serial, USB fingerprint, fleet label ("Rental #1" through "#5"). Profile auto-assigned to `sony-handycam-avchd`.

**Month 2 (backflow):** App scans existing clips. 47 clips from Month 1 have serial numbers matching Rental #3. Those clips get `camera_device_uuid` assigned. Their profile was already correct so no proxy regeneration needed. But now they show "Shot on: Rental #3" in the UI.

**Month 3:** New client returns footage. During ingest, USB fingerprint matches Rental #1. All clips auto-assigned to Rental #1 with `sony-handycam-avchd` profile. 100% confidence. Zero user interaction.

**Month 4:** User adds a custom LUT for the CX405. Creates a user profile `sony-cx405-rental-lut` with the LUT path. Assigns it to all 5 devices. Backflow triggers: all clips from these devices get profile upgraded. Proxies invalidated and regenerated with LUT applied.

**Month 6:** User imports footage from a client's personal Canon camcorder (not registered). No device match. Bundled profile `canon-vixia-hf` matches. Clips get correct transforms. Device stays unassigned -- this is someone else's camera, not fleet inventory.

---

### Layer 5: Match Audit Trail

**Purpose:** Full audit trail for every matching decision. Same principle as the import manifest -- you can reconstruct exactly what happened without re-running anything.

#### Sidecar Section: `matchAudit`

```json
{
  "matchAudit": {
    "matchedAt": "2024-01-15T10:30:00Z",
    "matcherVersion": 2,
    "matchSource": "bundled_profile",
    "inputSignature": {
      "make": "Sony",
      "model": "HDR-CX405",
      "serial": "E35982",
      "codec": "h264",
      "container": "mts",
      "width": 1920,
      "height": 1080,
      "fps": 29.97,
      "fieldOrder": "tt",
      "compressorId": null,
      "folderPath": "/Volumes/SD/AVCHD/BDMV/STREAM/"
    },
    "candidates": [
      {
        "slug": "sony-handycam-avchd",
        "score": 11.0,
        "rejected": false,
        "rejectReason": null,
        "matchedRules": ["make", "model", "codec", "container", "folder_pattern"],
        "failedRules": [],
        "missingData": ["compressor_id"]
      },
      {
        "slug": "iphone-h264",
        "score": 0.0,
        "rejected": true,
        "rejectReason": "reject_container: mts not in ['mov']",
        "matchedRules": [],
        "failedRules": ["make", "model"],
        "missingData": []
      }
    ],
    "winner": {
      "slug": "sony-handycam-avchd",
      "confidence": 0.85,
      "assignmentReason": "Strong make+model+codec+container+folder match (score 11.0)"
    }
  }
}
```

**Field breakdown:**

| Field | Purpose |
|-------|---------|
| `matchedAt` | ISO8601 timestamp of when matching ran |
| `matcherVersion` | Integer so we know which algorithm produced this result |
| `matchSource` | How the match was found: `registered_device_usb`, `registered_device_serial`, `bundled_profile`, `generic_fallback` |
| `inputSignature` | The EXACT metadata values fed into the matcher. Frozen snapshot. |
| `candidates` | Every profile evaluated, with score breakdown. Not just the winner. |
| `winner` | The chosen profile with confidence and human-readable reason |

#### What This Enables

1. **Re-match on profile add:** When `bundled_profiles.json` is updated, scan all clips where `winner.slug='generic-fallback'`. Re-run matching using stored `inputSignature` (no file access needed). Upgrade clips that now match.
2. **Profile discovery:** Group all generic-fallback clips by `inputSignature.make+model`. Returns: "You have 47 clips from JVC GZ-MG330 with no profile. Want to create one?" This drives organic database growth.
3. **False positive debug:** User reports wrong camera match. Look at `matchAudit.candidates` to see all scores. Identify which rule caused the false positive. Add a reject rule to the offending profile.
4. **Matcher version tracking:** When we improve the matching algorithm, `matcherVersion` tells us which clips were matched with the old algorithm. We can selectively re-match only those clips.

---

### Layer 6: Metadata Extraction State Machine

**Purpose:** Track the metadata extraction and matching pipeline per clip, same way the import tracks per-file copy/verify state. Enables crash recovery and completeness verification.

#### States

```
pending -> extracting -> extracted -> matching -> matched -> verified
                |                        |
                v                        v
        extraction_failed          extraction_failed
```

| State | Meaning |
|-------|---------|
| `pending` | Clip record created in DB. No metadata extraction attempted. |
| `extracting` | exiftool and/or ffprobe are running. If app crashes here, we retry on next launch. |
| `extracted` | Raw dumps captured and stored in sidecar. Parsed fields populated in DB. May be partial (one tool failed). |
| `matching` | Camera matcher is evaluating this clip against profiles. |
| `matched` | Profile assigned. matchAudit written to sidecar. |
| `verified` | All metadata fields cross-checked. Sidecar written atomically. Terminal state. |
| `extraction_failed` | Both tools failed. Clip still imported with filename/size only. Can retry later. |

#### DB Columns (Migration 11)

**`metadata_status` on `clips` table:**
```sql
metadata_status TEXT CHECK (metadata_status IN
  ('pending','extracting','extracted','matching','matched','verified','extraction_failed'))
DEFAULT 'pending'
```
Backfill existing clips as `'verified'`.

**`media_type` on `clips` table:**

Already exists from migration 1 with `CHECK (media_type IN ('video','audio','image'))`. Default `'video'`. No migration needed. The value `'unknown'` is NOT stored in the DB -- use `'video'` as conservative default for unclassifiable files. The sidecar `mediaType` field can store `'unknown'` since it's just JSON.

**`metadata_complete_at` on `ingest_sessions` table:**
Set when all clips from that session reach terminal state (`verified` or `extraction_failed`). NULL until then.

#### Crash Recovery

**On app launch:** Query clips `WHERE metadata_status IN ('extracting', 'matching')`. These crashed mid-pipeline. Reset to `pending` and re-queue.

**On ingest complete:** Query clips `WHERE metadata_status != 'verified' AND metadata_status != 'extraction_failed'` for this ingest session. If any exist, session is NOT fully processed.

#### Completeness Gate

`metadata_complete_at` is NOT a prerequisite for SAFE TO WIPE. SAFE TO WIPE is gated solely by the import pipeline (all manifest entries verified + rescan diff empty). Metadata extraction can still be in progress or failed and SAFE TO WIPE can still be true -- the bytes are safely copied regardless of whether we parsed them yet.

`metadata_complete_at` is a separate signal for:
- The UI (e.g., "Metadata processing: 847/850 clips done")
- The re-matching job (don't re-match a session that hasn't finished initial matching)

#### Reference Mode Behavior

In reference-mode imports (`ingest_mode='reference'`), files are NOT copied. The metadata extraction pipeline still runs:

- Layer 0: Raw dump capture runs against the original file path. File must be accessible (drive mounted). If disconnected, extraction fails gracefully -- `metadata_status='extraction_failed'`. Re-extraction can run later when drive is reconnected.
- Layers 1-5: Parsing, matching, audit trail, state machine all work identically.
- Sidecars: Written to the library's `.dadcam/sidecars/` directory as normal. These are app-side artifacts, not source-device files, so reference mode does not affect them.
- SAFE TO WIPE: NOT offered for reference-mode sessions (nothing was copied). The import pipeline enforces this gate, not the metadata plan.
- Re-extraction (layer 8): Requires original file. If source drive not mounted, skip and log -- same behavior as copy-mode when originals have been deleted.

---

### Layer 7: Sidecar Atomic Writes

**Purpose:** Sidecars now contain critical data (raw dumps, audit trail). Apply the same temp-verify-rename pattern used for file copies.

#### Current Code Problem

`ingest/sidecar.rs` uses `std::fs::write()` directly -- NOT atomic. No temp file, no fsync. If the app crashes mid-write, the sidecar could be half-written.

#### Atomic Write Pattern

1. Serialize sidecar to JSON string in memory.
2. Validate: parse the JSON back to confirm it's valid. If parse fails, bug in serializer -- don't write corrupt data.
3. Write to temp file: `.dadcam/sidecars/.tmp_<clip_id>.json` (same directory = same filesystem = atomic rename possible).
4. `fsync` temp file.
5. Atomic rename: `.tmp_<clip_id>.json` -> `<clip_id>.json`
6. `fsync` parent directory.

After this, the sidecar is either the old complete version or the new complete version. Never a partial mess.

#### Read Pattern

- **Missing sidecar:** All sidecar-sourced data is null. This is fine -- the DB has the core fields. Sidecar is enrichment, not source of truth for core data.
- **Corrupt sidecar:** If sidecar exists but fails JSON parse, log error and treat as missing. Do NOT delete -- it might be recoverable. Mark clip for re-extraction.

#### Import Pipeline Linkage

Sidecar `.json` files written to `.dadcam/sidecars/` MUST be tracked in the import manifest as `entry_type='sidecar'`. They are first-class eligible entries subject to full-hash verification and rescan. If a sidecar fails verification, SAFE TO WIPE is blocked.

Do not confuse source-device sidecars (camera-generated XML/THM files -- import concern) with app-generated sidecars (`.dadcam/sidecars/*.json` -- metadata concern).

---

### Layer 8: Re-extraction and Re-matching

**Purpose:** Background jobs that can re-run extraction or matching on existing clips. Needed when: exiftool tags are expanded, `bundled_profiles.json` is updated, matching algorithm is improved.

#### Re-extraction Job

**Trigger:** Manual (user clicks "Rescan Library Metadata" in settings) or automatic (app detects `pipeline_version` bump).

**Scope:** All clips in the library, or filtered by `metadata_status`.

**Process:**
1. For each clip, check if `rawExifDump` exists in sidecar. If yes AND `pipeline_version` matches, skip (already extracted with current version).
2. If missing or outdated: re-run exiftool and ffprobe on the original file.
3. Update sidecar with new raw dumps. Re-parse fields. Atomic write.
4. Reset `metadata_status` to `extracted` (needs re-matching).

**Requires original file:** YES. If file is missing (reference mode, drive disconnected), skip and log.

#### Re-matching Job

**Trigger:** Automatic when `bundled_profiles.json` version changes, or manual.

**Scope:** All clips with `winner='generic-fallback'`, or all clips if `matcherVersion < current`.

**Process:**
1. For each clip, read `inputSignature` from sidecar `matchAudit`.
2. Re-run matching algorithm against current profile database.
3. If new winner != old winner AND new confidence > old confidence: update clip's profile assignment.
4. Write new `matchAudit` to sidecar. Atomic write.
5. If profile changed and clip has a proxy, mark proxy for regeneration (new transforms may apply).

**Does NOT require original file.** Re-matching uses the stored `inputSignature`. This is the key benefit of the audit trail.

---

## 5. Implementation Order

These 11 steps are ordered by dependency. Each step builds on the previous ones. Do them in order.

### Step 1: Generic Fallback Profile + Atomic Sidecar Writes

**What to do:**
- Add `generic-fallback` entry to `bundled_profiles.json`. It syncs to App DB via existing `sync_bundled_profiles()`.
- Change the fallback return in `resolve_profile_from_app_db()`.
- Replace `std::fs::write()` in sidecar writer with atomic temp-fsync-rename.

**Files to touch:**
- `resources/cameras/bundled_profiles.json` -- add generic-fallback entry (first in array)
- `src-tauri/src/ingest/mod.rs` -- line 1271: change `('none', '')` to `('bundled', 'generic-fallback')`
- `src-tauri/src/ingest/sidecar.rs` -- replace `std::fs::write()` with atomic temp-fsync-rename

**Migration:** None

**Risk:** Low

**Why first:** Generic fallback is the safety net. Everything else builds on "every clip always has a profile." Atomic writes protect the sidecars that all subsequent layers depend on.

---

### Step 2: Raw Dump Capture (exiftool + ffprobe)

**What to do:**
- Change exiftool to `-j -G -n` (full dump).
- Change ffprobe to capture extended fields.
- Store raw outputs in sidecar.
- Parse structured fields from dumps instead of from tool-specific code.

**Files to touch:**
- `src-tauri/src/metadata/exiftool.rs` -- switch from 9-tag request to full `-j -G -n` dump
- `src-tauri/src/metadata/ffprobe.rs` -- add extended stream fields to `FFprobeStream` struct
- `src-tauri/src/metadata/mod.rs` -- expand `MediaMetadata` with extended fields
- `src-tauri/src/ingest/sidecar.rs` -- add `rawExifDump` and `rawFfprobe` sections

**Migration:** None

**Risk:** Low -- existing fields still parsed, just from a richer source

---

### Step 3: Metadata Extraction State Machine

**What to do:**
- Add `metadata_status` column to clips.
- Add `media_type` column to clips.
- Add `metadata_complete_at` to `ingest_sessions`.
- Implement state transitions in ingest pipeline.
- Add crash recovery on app launch.

**Files to touch:**
- `src-tauri/src/db/migrations.rs` -- migration 11
- `src-tauri/src/ingest/mod.rs` -- state transitions during ingest

**Migration:** Migration 11:
- Add `metadata_status` to clips (default `'pending'`, backfill existing as `'verified'`)
- Add `metadata_complete_at` to `ingest_sessions`
- Note: `media_type` already exists on clips (migration 1). No change needed.

**Risk:** Low

---

### Step 4: Match Audit Trail + Reject Rules

**What to do:**
- Add reject_codec/reject_container/reject_model checking BEFORE existing `score_match_rules()`.
- Add minimum score threshold (3.0).
- Return all candidates from matcher (not just winner).
- Write `matchAudit` section to sidecar with `matchSource` field.

**Files to touch:**
- `src-tauri/src/ingest/mod.rs` -- add reject phase before `score_match_rules()`, add threshold check, return candidates
- `src-tauri/src/ingest/sidecar.rs` -- add `matchAudit` section to `SidecarData`

**Migration:** None

**Risk:** Medium -- matching behavior changes, need to verify no regressions

**Important:** Weighted scoring is NOT changed -- it already exists and works correctly.

---

### Step 5: Populate and Verify bundled_profiles.json

**What to do:**
- Build tier 1 profiles (7 profiles).
- Verify each against real samples using the verification workflow.
- Run cross-validation.
- Make `bundled_profiles.json` the single source. Update or deprecate legacy `canonical.json`/`bundled.rs`.

**Files to touch:**
- `resources/cameras/bundled_profiles.json` -- add verified profiles
- `src-tauri/src/camera/bundled.rs` -- update to read from `bundled_profiles.json` or deprecate
- `resources/cameras/canonical.json` -- deprecate or symlink to `bundled_profiles.json`

**Migration:** None

**Risk:** Medium -- need real sample files

---

### Step 6: Re-matching Background Job

**What to do:**
- Implement re-matching job that scans generic-fallback clips using stored `inputSignature`.
- Triggered when `bundled_profiles.json` version changes.

**Files to touch:**
- `src-tauri/src/jobs/` (new or existing job module)
- `src-tauri/src/ingest/mod.rs`

**Migration:** None

**Risk:** Low

---

### Step 7: Re-extraction Background Job

**What to do:**
- Implement re-extraction job for existing libraries.
- Triggered manually or by `pipeline_version` bump.

**Files to touch:**
- `src-tauri/src/jobs/` (new or existing job module)
- `src-tauri/src/ingest/sidecar.rs`

**Migration:** None

**Risk:** Low

---

### Step 8: Device Registration EXIF Dump + Auto-Profile

**What to do:**
- During device registration (R3-R4): run full exiftool+ffprobe on sample files from the camera.
- Auto-match to a profile using `match_bundled_profile_rules()` + `match_app_profile_rules()`.
- Store dumps in `~/.dadcam/device_dumps/<uuid>.json`.
- Pre-fill registration form.

**Files to touch:**
- `src-tauri/src/camera/devices.rs` -- add registration flow with EXIF dump + auto-match
- `src-tauri/src/metadata/exiftool.rs` -- reuse full dump mode
- `src-tauri/src/metadata/ffprobe.rs` -- reuse extended extraction

**Migration:** None needed. App DB `camera_devices` already has `profile_type` + `profile_ref`. Store dump at conventional path instead of adding column.

**Risk:** Low -- reuses layer 0 extraction code

---

### Step 9: Forward Flow -- Device Matching During Ingest

**What to do:**
- Verify `resolve_stable_camera_refs()` correctly propagates `device.profile_type` + `device.profile_ref` to clip columns.
- Add `matchSource` to sidecar `cameraMatch` section.

**Files to touch:**
- `src-tauri/src/ingest/mod.rs` -- verify device -> clip profile propagation
- `src-tauri/src/ingest/sidecar.rs` -- add `matchSource` field

**Migration:** None -- uses existing migration 7 columns

**Risk:** Medium -- touches ingest pipeline

---

### Step 10: Backflow -- Re-match Existing Clips on Device Registration

**What to do:**
- After `insert_device()`, scan all clips with `camera_device_uuid=NULL`.
- Match by serial, then by ingest session USB fingerprint, then by make+model.
- Update clips in transaction.
- Re-write sidecars.
- Invalidate proxies if profile changed.
- Report results to UI.

**Files to touch:**
- `src-tauri/src/camera/devices.rs` -- add backflow scan after registration
- `src-tauri/src/ingest/mod.rs` -- matching functions reuse
- `src-tauri/src/ingest/sidecar.rs` -- sidecar updates

**Migration:** None

**Risk:** Medium -- batch updates to existing clips, need transaction safety

---

### Step 11: Backflow -- Re-match on Profile Add/Update

**What to do:**
- When `bundled_profiles.json` version changes or user creates/edits a profile:
  - Check registered devices with `profile_type='none'` -- offer to assign
  - Re-match generic-fallback clips using stored `inputSignature`
  - Re-check devices' stored EXIF dumps against new profiles

**Files to touch:**
- `src-tauri/src/db/app_schema.rs` -- detect profile version changes
- `src-tauri/src/ingest/mod.rs` -- re-matching logic
- `src-tauri/src/jobs/` -- background job definition

**Migration:** None

**Risk:** Low -- uses stored data, no file access needed

---

## Gap Corrections (v3.2)

These 14 items were identified as gaps in the v3.1 plan. Each is now resolved with a specific decision and implementation detail.

### G1: `pipeline_version` Definition

**Gap:** Layer 8 references `pipeline_version` but never defines where it lives.

**Resolution:** `pipeline_version` ALREADY EXISTS on the `assets` table (migration 1, column `pipeline_version INTEGER`). It is per-asset, not per-clip. For metadata re-extraction:

- Define a constant `METADATA_PIPELINE_VERSION: u32 = 1` in `ingest/mod.rs`. Bump this when extraction logic changes (e.g., switching to full exiftool dump).
- During re-extraction (layer 8), compare the clip's original asset's `assets.pipeline_version` against `METADATA_PIPELINE_VERSION`. If asset version < constant, re-extract.
- After successful re-extraction, update `assets.pipeline_version = METADATA_PIPELINE_VERSION` on the original asset.
- The sidecar also stores `pipelineVersion` in the `rawExifDump` / `rawFfprobe` sections so the version is captured alongside the data.

No migration needed. The column already exists.

---

### G2: `extraction_partial` vs `extraction_failed` State Conflict

**Gap:** Layer 0 text says "mark as `extraction_partial`" when ffprobe fails but exiftool succeeds. Layer 6 state machine has no `extraction_partial` state.

**Resolution:** There is NO `extraction_partial` state. The correct behavior:

- If **both** tools fail: `metadata_status = 'extraction_failed'`
- If **one tool** fails and one succeeds: `metadata_status = 'extracted'` (partial data is still extracted data)
- The sidecar records which tool failed in the `extractionStatus` section (see G7).

Layer 0 text corrected: where it says "mark as `extraction_partial`", it should say "mark as `extracted` with partial data noted in sidecar." The state machine in Layer 6 is correct as-is.

---

### G3: Score-to-Confidence Mapping Formula

**Gap:** The confidence scale (0.1-1.0) and scoring weights (+5, +3, etc.) are both specified, but no formula converts raw score to confidence.

**Resolution:** `score_match_rules()` returns a raw specificity score. Maximum possible: +5 (make+model) + 3 (codec+container) + 3 (folder) + 2 (resolution) + 1 (fps) = 14.0.

**Confidence formula:**

```
For device matches (fixed values, not computed from score):
  USB fingerprint match:     confidence = 1.0
  Serial number match:       confidence = 0.95
  Make+model device match:   confidence = 0.6

For profile matches (computed from score):
  confidence = min(score / 14.0, 0.95)

  Examples:
    score 11.0 (make+model+codec+container+folder) -> confidence 0.786 -> rounds to ~0.8
    score 14.0 (all rules match)                    -> confidence 0.95 (capped)
    score  5.0 (make+model only)                    -> confidence 0.357
    score  3.0 (minimum threshold)                  -> confidence 0.214
    score  0.0 or below threshold                   -> generic-fallback, confidence 0.1
```

Cap at 0.95 because profile-only matches (no device) can never reach 1.0 certainty. Only registered device matches get >= 0.95.

The minimum threshold of score >= 3.0 means a profile must match at least make+model (5.0) or codec+container (3.0) or folder (3.0) to beat generic-fallback. A single weak signal (fps alone = 1.0) is not enough.

**Implementation:** Add `fn score_to_confidence(score: f64) -> f64` in `ingest/mod.rs`:
```rust
fn score_to_confidence(score: f64) -> f64 {
    const MAX_SCORE: f64 = 14.0;
    (score / MAX_SCORE).min(0.95)
}
```

---

### G4: `rotation_fix` Not in Rust TransformRules Struct

**Gap:** Layer 3 profile schema shows `rotation_fix: true|false` but the Rust `TransformRules` struct only has `deinterlace`, `deinterlace_mode`, `color_space`, `lut`.

**Resolution:** Add `rotation_fix: Option<bool>` to the `TransformRules` struct in `camera/mod.rs`:

```rust
pub struct TransformRules {
    pub deinterlace: Option<bool>,
    pub deinterlace_mode: Option<String>,
    pub color_space: Option<String>,
    pub lut: Option<String>,
    pub rotation_fix: Option<bool>,     // NEW: true=apply rotation correction, false=skip, None=auto-detect
    pub field_order: Option<String>,     // NEW: "tff", "bff", "auto", null (see G5)
}
```

Semantics:
- `None` (null in JSON): auto-detect rotation from exiftool `Rotation` tag or ffprobe displaymatrix. This is what generic-fallback uses.
- `Some(true)`: always apply rotation correction (useful when metadata is known correct).
- `Some(false)`: never apply rotation (useful when camera writes wrong rotation metadata).

**When to add:** Step 2 (raw dump capture) since it changes the struct. The struct change is backward-compatible because serde `Option<T>` deserializes missing fields as `None`.

---

### G5: `field_order` Not in Rust TransformRules Struct

**Gap:** Layer 3 profile schema shows `field_order: "tff|bff|auto|null"` in `transform_rules` but the Rust struct doesn't have it.

**Resolution:** Add `field_order: Option<String>` to the `TransformRules` struct (see G4 above for full struct).

Semantics:
- `None` (null in JSON): auto-detect from ffprobe `field_order` value. This is what generic-fallback uses.
- `Some("tff")`: force top-field-first (override ffprobe if it's wrong or missing).
- `Some("bff")`: force bottom-field-first.
- `Some("auto")`: same as None, explicit auto-detect.

**When to add:** Same as G4, Step 2.

---

### G6: `is_system` / `deletable` / `category` Fields Not in DB

**Gap:** Generic-fallback profile has `is_system: true`, `deletable: false`, `category: "system"` in its definition, but these aren't columns in the `bundled_profiles` DB table.

**Resolution:** These are **JSON-file-only fields**. They do NOT sync to the DB.

How the app uses them:
- `is_system`: checked by `sync_bundled_profiles()` during startup. System profiles are never deleted from the DB even if removed from the JSON file (defensive). Also checked by the UI to gray out the delete button.
- `deletable`: redundant with `is_system` for bundled profiles. Exists for user profiles where `is_system=false` but we might still want to prevent deletion of a user-created profile that's assigned to devices. The UI checks this before allowing delete.
- `category`: display-only field used for grouping in the profile list UI. Stored in the JSON file, read at runtime from the in-memory profile list.

**Implementation:** `sync_bundled_profiles()` already ignores unknown fields when syncing to DB (it only reads `slug`, `name`, `version`, `match_rules`, `transform_rules`). No migration needed. The JSON file stores these fields, the Rust code reads them from the parsed JSON when needed for UI display.

Add to the `AppBundledProfile` Rust struct (or a separate display struct):
```rust
pub is_system: Option<bool>,
pub deletable: Option<bool>,
pub category: Option<String>,
```

These are `Option` so they deserialize gracefully from profiles that don't have them (all existing profiles).

---

### G7: Extraction Error Detail Storage

**Gap:** Layer 0 says "mark clip as `extraction_failed` with error detail" but doesn't specify where error detail is stored.

**Resolution:** Error detail is stored in the **sidecar** in a new `extractionStatus` section. NOT in a DB column -- the DB only stores the enum state (`metadata_status`), not the error text.

```json
{
  "extractionStatus": {
    "status": "extracted",
    "exiftool": {
      "success": true,
      "exitCode": 0,
      "error": null,
      "pipelineVersion": 1
    },
    "ffprobe": {
      "success": false,
      "exitCode": 1,
      "error": "ffprobe: Invalid data found when processing input",
      "pipelineVersion": 1
    },
    "extractedAt": "2024-01-15T10:30:00Z"
  }
}
```

If the clip is `extraction_failed` (both tools failed), the sidecar may not exist or may be minimal. In that case, the error detail is logged to the app log and to `jobs.last_error` if running as a job. The sidecar is best-effort for failed extractions.

If the clip is `extracted` with partial data (one tool failed), the sidecar has the successful tool's raw dump and the `extractionStatus` section records which tool failed and why.

---

### G8: `matcherVersion` Initial Value

**Gap:** `matcherVersion` is used in audit trail and re-matching but never defined.

**Resolution:** Define as a constant in `ingest/mod.rs`:

```rust
/// Increment when matching algorithm changes (reject rules, scoring weights, threshold).
/// Used in matchAudit to track which algorithm version produced a result.
const MATCHER_VERSION: u32 = 1;
```

Version history:
- **1**: Current algorithm. Weighted scoring, no reject rules, no minimum threshold.
- **2**: After Step 4. Adds reject rules, minimum threshold (3.0), audit trail output.

When `MATCHER_VERSION` is bumped, the re-matching job (Step 6) queries clips where `matchAudit.matcherVersion < MATCHER_VERSION` and re-runs matching using stored `inputSignature`.

---

### G9: Concurrent Extraction

**Gap:** Neither document discusses parallelism for exiftool/ffprobe extraction.

**Resolution:**

**Concurrency model:** exiftool and ffprobe are spawned as child processes. Multiple clips CAN be extracted concurrently (each clip spawns its own processes). The ingest pipeline already processes clips sequentially within a single ingest job, but background re-extraction jobs (layer 8) can process clips in parallel.

**Thread safety:**
- **Sidecar writes:** Atomic writes use `clip_id` in the temp filename (`.tmp_<clip_id>.json`). Different clips write to different temp files. Concurrent writes for different clips are safe. The same clip cannot be extracted concurrently because the state machine requires `metadata_status = 'pending'` to start, and the transition to `extracting` is an atomic DB update.
- **DB updates:** `metadata_status` transitions use `UPDATE clips SET metadata_status = 'extracting' WHERE id = ? AND metadata_status = 'pending'`. The `AND metadata_status = 'pending'` clause prevents double-extraction. If the update affects 0 rows, another process already claimed the clip.

**Parallelism limit:** For re-extraction jobs, limit to 4 concurrent extractions (configurable). Each extraction spawns 2 processes (exiftool + ffprobe), so 4 clips = 8 child processes. This avoids overwhelming the disk on mechanical drives.

---

### G10: `metadata_complete_at` Trigger Mechanism

**Gap:** Both docs say "set when all clips reach terminal state" but don't specify the trigger mechanism.

**Resolution:** Event-driven, checked after each clip reaches terminal state.

After updating a clip's `metadata_status` to `'verified'` or `'extraction_failed'`:

```sql
-- Check if all clips in this session are in terminal state
SELECT COUNT(*) FROM clips c
JOIN ingest_files f ON c.id = f.clip_id
JOIN ingest_manifest_entries me ON f.source_path = me.relative_path
WHERE me.session_id = ?
  AND c.metadata_status NOT IN ('verified', 'extraction_failed');
```

If count = 0, set:
```sql
UPDATE ingest_sessions
SET metadata_complete_at = datetime('now')
WHERE id = ? AND metadata_complete_at IS NULL;
```

The `AND metadata_complete_at IS NULL` prevents re-setting if already complete.

**Who calls it:** The metadata extraction pipeline calls this check as the final step after transitioning any clip to a terminal state. It's a cheap query (indexed on session_id and metadata_status).

---

### G11: `discover_sample_files()` Not in Implementation Steps

**Gap:** R2 identifies `discover_sample_files(mount_point)` as a gap but Step 8 doesn't list it in scope.

**Resolution:** Step 8 scope updated to include: "Implement `discover_sample_files(mount_point)` in `camera/devices.rs` that returns 1-3 representative video files from a mounted volume."

Algorithm:
1. Check known camera directory structures first: `DCIM/`, `AVCHD/BDMV/STREAM/`, `PRIVATE/AVCHD/`.
2. If found, return the first (or largest) video file from each structure.
3. If no known structure, walk root directory (max depth 3), collect video files by extension, return the 3 largest.
4. Video extensions: `.mts`, `.m2ts`, `.mp4`, `.mov`, `.avi`, `.dv`, `.mpg`, `.mxf`.

Files touched in Step 8 already lists `camera/devices.rs` -- this function goes there.

---

### G12: Soft Match Confirmation Flow for Device Backflow

**Gap:** B3/B6 mention model-matched clips "need confirmation" but no mechanism is defined.

**Resolution:** Soft matches (make+model, confidence < 0.9) are NOT auto-applied. They are returned as **suggestions** only.

The backflow function returns a structured result:

```rust
pub struct BackflowResult {
    pub device_uuid: String,
    pub auto_assigned: Vec<i64>,       // clip IDs matched by serial/USB (applied)
    pub suggested: Vec<SuggestedMatch>, // clip IDs matched by model (NOT applied)
}

pub struct SuggestedMatch {
    pub clip_id: i64,
    pub confidence: f64,
    pub match_method: String,  // "make_model"
}
```

**Auto-assigned (applied immediately):**
- USB fingerprint match (confidence 1.0)
- Serial number match (confidence 0.95)

**Suggested (NOT applied, presented to user):**
- Make+model match (confidence 0.6)

The UI shows: "12 clips auto-assigned by serial number. 35 clips may be from this camera (same model). [Assign All] [Review]"

If user clicks "Assign All", the app updates those clips in a transaction. If user clicks "Review", they see the list and can accept/reject per-clip.

No new DB column needed. The `camera_device_uuid` column is only written when confirmed (auto or manual). Unconfirmed suggestions live only in the UI response.

---

### G13: Proxy Invalidation Mechanism

**Gap:** Both docs say "invalidate proxy" but don't specify the exact mechanism.

**Resolution:** Use the existing `assets.pipeline_version` column.

When a clip's profile changes (backflow, re-match, or manual reassignment):
1. Find the clip's proxy asset: `SELECT id, pipeline_version FROM assets WHERE id IN (SELECT asset_id FROM clip_assets WHERE clip_id = ? AND role = 'proxy')`.
2. Set `pipeline_version = 0`: `UPDATE assets SET pipeline_version = 0 WHERE id = ?`.
3. The proxy generator checks `WHERE pipeline_version < ?` (current version) to find stale proxies.
4. On next access (or background job), regenerate the proxy with the new profile's transform_rules.

Do NOT delete the proxy file immediately. Setting `pipeline_version = 0` marks it stale. The old proxy remains playable until the new one is generated. This avoids a gap where the clip has no proxy at all.

If a bulk re-match changes 500 clips' profiles, this creates 500 stale proxy markers. The proxy regeneration job processes them in priority order (most recently viewed first).

---

### G14: `sensor` Object Storage

**Gap:** Layer 3 profile schema has `sensor: {type, size, megapixels}` but the `bundled_profiles` DB table has no sensor columns.

**Resolution:** `sensor` is a **JSON-file-only field**, same as `is_system`/`deletable`/`category` (see G6).

- Stored in `bundled_profiles.json` per-profile.
- NOT synced to the `bundled_profiles` DB table. The DB table only stores what the matcher needs: `slug`, `name`, `version`, `match_rules`, `transform_rules`.
- Read at runtime from the in-memory profile list for UI display ("Sony Handycam -- 1/5.8\" CCD, 2.0MP").
- Also stored in sidecar `extendedMetadata` section when available from exiftool (fields like `EXIF:ImageSensorType`, `Composite:Megapixels`).

For user profiles (not bundled), sensor info comes from the camera's exiftool dump at registration time (Step 8, R3). The device dump at `~/.dadcam/device_dumps/<uuid>.json` contains the full exiftool output which includes sensor data.

No migration needed.

---

### Migration 11 Correction (from G2 audit)

The original plan says migration 11 adds `media_type` to clips. In reality, `media_type TEXT CHECK (media_type IN ('video','audio','image'))` ALREADY EXISTS on the clips table (migration 1). Migration 11 only needs to:

1. **ALTER the CHECK constraint** to add `'unknown'`: This requires creating a new table with the updated constraint and migrating data (SQLite doesn't support ALTER CHECK). Alternatively, skip `'unknown'` as a DB value and use `'video'` as default for unclassifiable files (they still play through the video pipeline). Decision: **skip the 'unknown' value in the DB**. Use `'video'` as the conservative default. The sidecar `mediaType` field can store `'unknown'` since it's just JSON. This avoids a complex table rebuild migration.

2. **Add `metadata_status`** to clips (genuinely new).
3. **Add `metadata_complete_at`** to ingest_sessions (genuinely new).

Updated migration 11:
```sql
ALTER TABLE clips ADD COLUMN metadata_status TEXT
  CHECK (metadata_status IN ('pending','extracting','extracted','matching','matched','verified','extraction_failed'))
  DEFAULT 'verified';
-- Backfill: all existing clips are already verified (they have metadata from the old pipeline)

ALTER TABLE ingest_sessions ADD COLUMN metadata_complete_at TEXT;
```

Note: `media_type` does NOT need to be added -- it already exists from migration 1.

---

## 6. Version History

### v3.2 Changes (from v3.1)
- Corrected 14 implementation gaps (G1-G14) identified during audit
- G1: Defined `pipeline_version` lifecycle (already exists on `assets` table, use constant for metadata version)
- G2: Resolved `extraction_partial` vs `extraction_failed` conflict (no `extraction_partial` state; partial data = `extracted`)
- G3: Added score-to-confidence formula: `confidence = min(score / 14.0, 0.95)`
- G4: Added `rotation_fix: Option<bool>` to TransformRules struct (Step 2)
- G5: Added `field_order: Option<String>` to TransformRules struct (Step 2)
- G6: Clarified `is_system`/`deletable`/`category` are JSON-file-only, not DB columns
- G7: Defined error detail storage in sidecar `extractionStatus` section
- G8: Defined `MATCHER_VERSION` constant starting at 1, bumped in Step 4
- G9: Specified concurrent extraction model (state machine prevents double-extraction, 4-clip parallel limit)
- G10: Defined `metadata_complete_at` trigger (event-driven check after each clip terminal state)
- G11: Added `discover_sample_files()` to Step 8 scope
- G12: Clarified soft match (make+model) is suggestion-only, not auto-applied
- G13: Defined proxy invalidation via `assets.pipeline_version = 0` (existing column)
- G14: Clarified `sensor` is JSON-file-only display metadata, not synced to DB
- Corrected migration 11: `media_type` already exists (migration 1), removed from migration 11. Dropped `'unknown'` from DB CHECK (use `'video'` default, sidecar stores `'unknown'`).

### v3.1 Changes (from v3.0)
- Added layer 0b: outlier media handling (contract #4)
- Added `media_type` column to clips table (migration 11)
- Specified parsed field expectations per media type
- Specified generic-fallback behavior for outlier types
- Specified that profile matching still runs on outlier types

### v3.0 Changes (from v2.1)
- Added `existing_code_reality` section documenting the actual codebase
- Added `v3_corrections` section with 8 fixes (C1-C8)
- Fixed step 1: targets `ingest/mod.rs:1271` (not `camera/matcher.rs`)
- Fixed generic-fallback seeding: `bundled_profiles.json` (not `canonical.json`)
- Removed migration 12: already exists
- Fixed deinterlace type: uses null (`None`)
- Noted weighted scoring already exists
- Fixed all `canonical.json` references to `bundled_profiles.json`
- Added `matchSource` field to audit trail
- Layer 4 schema gaps rewritten

### v2.0 Changes (from v1.0)
- Added layer 0: raw dump capture
- Added layer 6: state machine
- Added layer 7: atomic sidecar writes
- Added layer 8: re-extraction and re-matching
- Expanded matching with reject rules and multi-phase scoring
- Changed exiftool from 9 tags to full dump
- Added confidence scale
- Added cross-validation to verification workflow
- Added `metadata_status` column (migration 11)
- Added `metadata_complete_at` to ingest_sessions
- Expanded layer 4 from stub to complete flow

---

## The Sharp Idea

Dad Cam is a memory machine. It works on any video because memories come from everywhere -- dad's camcorder, mom's phone, the GoPro from vacation, that weird DVD camcorder from 2006, a VHS tape digitized at Costco, a screen recording of a FaceTime call. The metadata and profile system makes each of those look and play its best, but NONE of them are blocked or degraded if we don't recognize the source.

**Simple user:** Import folder. Everything plays. Interlaced stuff gets deinterlaced. Rotated stuff gets rotated. No settings. No camera selection. No questions. It just works.

**Pro user:** Register your cameras. Get fleet tracking. Get camera-specific color and deinterlace. Export audit trails. But none of this is required.

**Technical guarantee:** Every clip always has: a profile (at minimum generic-fallback), metadata (at minimum filename + size), a sidecar (at minimum raw dumps), and an audit trail (at minimum "no match, using generic"). Zero clips in limbo. Zero clips with incomplete state.
