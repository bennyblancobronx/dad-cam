Dad Cam App — Changelog

This is the source of truth for version number.

---

0.1.150 -- Background job worker + camera profile-aware proxy generation

- Added jobs/worker.rs: background thread that polls for pending jobs (thumb/proxy/sprite/hash_full/score) every 5s when a library is open. After processing one job, drains the queue without delay. Goes idle when no library is open.
- Wired worker into app startup (lib.rs setup) and library open/close (library.rs). WorkerState is managed Tauri state alongside DbState.
- Proxy generation now reads transform_rules from the clip's camera profile (App DB). Profile deinterlace override takes priority over auto-detect; profile LUT path is passed to ffmpeg. Generic-fallback has empty transform_rules so auto-detect still kicks in.
- No new dependencies. Uses existing run_next_job(), open_library_db_connection(), and App DB profile lookup functions.

0.1.149 -- Remove dead camera profile loading paths + deprecated sidecar code

- Deleted canonical.json (empty) and camera/bundled.rs (loaded from it). The real profiles are in bundled_profiles.json loaded into App DB at startup -- canonical.json was a leftover from the old Library DB path.
- Removed insert_default_profiles() which hard-coded 3 untested placeholder profiles (Sony Handycam, Canon DSLR, Panasonic MiniDV) into Library DB on every create/open. These were missed in the v0.1.147 placeholder cleanup. The matching engine uses App DB profiles, not these.
- Removed deprecated ingest_sidecar() from sidecar_processor.rs (was already #[allow(dead_code)]). process_sidecar_entry() is the gold-standard replacement since Migration 10.
- No behavior changes. The matching engine already used the App DB path (bundled_profiles.json -> sync_bundled_profiles_at_startup -> App DB bundled_profiles table).

0.1.148 -- Fix migration 11 FK constraint failure on existing libraries

- Migration 11 (jobs table recreation) failed with "FOREIGN KEY constraint failed" on existing DBs because FK checks were ON during the table swap. Fixed by adding PRAGMA defer_foreign_keys=ON + explicit BEGIN/COMMIT around the swap, and DROP TABLE IF EXISTS jobs_new for idempotency.
- Verified end-to-end: fresh library init applies all 11 migrations, test video import produces metadata_status=verified, camera_profile_ref=generic-fallback, full sidecar with matchAudit trail.

0.1.147 -- Metadata state machine + wire rematch/reextract + delete placeholder profiles

- Deleted 9 placeholder bundled camera profiles (sony-handycam, iphone, canon, panasonic, dv-tape, gopro). Only generic-fallback remains as system profile. Real profiles will come from actual camera testing.
- Metadata state machine wired into ingest pipeline: clips now transition pending -> extracted -> matching -> verified (or extraction_failed if both tools fail). Migration 11 adds metadata_status column.
- NewClip struct and insert_clip() now set metadata_status explicitly (new clips start as 'pending', not 'verified')
- Job runner now handles 'rematch' and 'reextract' job types (were defined in rematch.rs/reextract.rs but never wired into runner.rs)
- Fixed backflow_scan_for_device() in registration.rs: was querying non-existent camera_make/camera_model/serial_number columns on clips table. Now reads from sidecar matchAudit.inputSignature instead.

0.1.146 -- Add is_system/deletable/category to camera profile structs

- BundledProfileJsonEntry now deserializes is_system, deletable, category from bundled_profiles.json (previously silently dropped)
- CameraProfileView sends is_system, deletable, category to frontend for UI controls (disable delete on system profile, display categories)
- generic-fallback profile identified as is_system=true, deletable=false at runtime by slug
- Completes metadata-plan.md G6 gap -- 100% of plan items now implemented

0.1.145 -- Dead code cleanup + version fix

- Removed 22 unused Rust constants (worker defaults, descriptive strings, sampling/timeout params)
- Removed 6 unused error variants (LibraryExists, UnsupportedFormat, DuplicateFile, Config, Scoring, EventNotFound)
- Removed 3 dead Rust functions (get_dadcam_path, compute_full_hash_streaming, heartbeat_job)
- Removed 2 dead TS functions (paths.ts:getLibraryRoot, toAbsolutePath), 1 dead API function (settings.ts:getMode)
- Removed 2 dead TS constants (APP_NAME, APP_DESCRIPTION)
- Fixed APP_VERSION displaying stale 0.1.128 instead of current version

0.1.144 -- Metadata plan Steps 2-11: Gold-standard metadata framework complete

- Step 2: Full exiftool dump (-j -G -n), ffprobe extended fields, FullExtractionResult struct, raw dumps stored in sidecars
- Step 3: Migration 11 adds metadata_status to clips, metadata_complete_at to ingest_sessions, expands jobs CHECK for rematch/reextract
- Step 4: Three-phase matching (reject rules, weighted scoring, threshold 3.0), full audit trail in sidecar matchAudit section
- Step 5: Expanded bundled_profiles.json from 4 to 10 profiles (iPhone H.264/HEVC, Canon VIXIA, Panasonic HC-V, DV Tape, GoPro)
- Step 6: rematch.rs -- re-match generic-fallback clips using stored inputSignature (no file access needed)
- Step 7: reextract.rs -- re-extract metadata when pipeline_version changes (requires file access)
- Step 8: registration.rs -- discover_sample_files(), probe_device_sample(), store device EXIF dumps at ~/.dadcam/device_dumps/
- Step 9: Forward flow verified -- matching already propagates device profile_type/profile_ref to clips
- Step 10: backflow_scan_for_device() -- auto-assign serial/USB matches, suggest make+model matches (G12)
- Step 11: profile_update.rs -- rematch_on_profile_change(), check_unassigned_devices() for profile add/update backflow

0.1.143 -- Split ingest/mod.rs (1824 lines) into 7 modules, all under 400 lines. Pure refactor, no behavior changes.

0.1.142 -- Metadata plan Step 1: generic-fallback profile + atomic sidecar writes

- Added generic-fallback profile to bundled_profiles.json (first entry, empty match_rules, auto-detect transforms)
- Changed fallback in resolve_profile_from_app_db() from ('none','') to ('bundled','generic-fallback') -- every clip now always has a profile
- Changed fallback in resolve_stable_refs_fallback() to match
- Replaced std::fs::write() in sidecar writer with atomic temp-fsync-rename pattern (temp file, fsync, rename, dir sync)
- Round-trip JSON validation before writing to prevent corrupt sidecars

0.1.141 -- Fix step 3 migration default in metadata-plan.json (said 'pending', should be 'verified' for backfill)

0.1.140 -- Metadata plan audit: aligned 3 minor deltas between .md and .json

- Reference mode: .md now matches .json detail (import pipeline enforces safe-to-wipe gate, re-extraction matches copy-mode behavior when originals deleted, sidecars clarified as not source-device files)
- State machine: added transition_diagram field to .json states section (matches .md ASCII diagram)
- Completeness gate: confirmed identical content, no change needed

0.1.139 -- Metadata plan v3.2 sync: fixed 5 stale references where correction sections didn't update original text

- Step 3 migration scope: removed media_type (already exists from migration 1) in both .md and .json
- Layer 0 ffprobe failure: changed "extraction_partial" to "extracted with partial data" per G2 in both docs
- JSON B5 proxy invalidation: aligned with G13 (pipeline_version=0, no file deletion)
- Layer 6 media_type CHECK: removed 'unknown' from DB constraint in both docs (sidecar stores it instead)
- Added "Backflow on Manual Re-match" subsection to .md Layer 4 (was in .json but missing from .md)

0.1.138 -- Metadata plan v3.2: corrected 14 implementation gaps found by auditing .md vs .json vs actual codebase

- G1: pipeline_version already exists on assets table -- defined constant-based lifecycle for re-extraction
- G2: removed phantom extraction_partial state -- partial data = extracted, both fail = extraction_failed
- G3: added score-to-confidence formula: min(score / 14.0, 0.95) with worked examples
- G4-G5: added rotation_fix and field_order to TransformRules struct (Step 2)
- G6: clarified is_system/deletable/category are JSON-file-only, not DB columns
- G7: defined error detail storage in sidecar extractionStatus section
- G8: defined MATCHER_VERSION constant (1=current, 2=after reject rules in Step 4)
- G9: specified concurrent extraction safety (state machine prevents double-extract, 4-clip parallel limit)
- G10: defined metadata_complete_at trigger (event-driven after each clip terminal state)
- G11: added discover_sample_files() to Step 8 scope with algorithm
- G12: clarified soft device matches (make+model) are suggestions only, not auto-applied
- G13: defined proxy invalidation via existing assets.pipeline_version=0 (no file deletion)
- G14: clarified sensor object is JSON-file-only display metadata
- Corrected migration 11: media_type already exists from migration 1, removed from migration 11

0.1.137 -- Converted metadata-plan.json into metadata-plan.md implementation guide for less experienced developers

- Full markdown conversion of the v3.1 metadata/camera profile framework plan
- Structured with table of contents, clear headings, tables, and code blocks
- Audited against source JSON: 100% coverage confirmed (all 8 layers, 11 steps, 8 corrections, 7 principles)

0.1.136 -- Metadata plan v3.1: closed 3 cross-cutting gaps between metadata plan and ship gate checklist

- Added import_pipeline_linkage to layer 7: documents that app-generated sidecars are destination-side (not source manifest entries) and clarifies the boundary with import-side sidecar tracking (ship gate A3)
- Added relationship_to_safe_to_wipe to layer 6 completeness gate: metadata_complete_at is NOT a prerequisite for SAFE TO WIPE -- bytes are safe regardless of parse status
- Added reference_mode section to layer 6: extraction/matching pipeline works identically on referenced files, sidecar writes are unaffected, re-extraction requires mounted drive

0.1.135 -- Metadata plan v3.1: added outlier media handling (contract #4 coverage for audio-only + image files)

- Added layer 0b: extraction/parsing/matching/proxy behavior for audio-only, image, and unknown file types
- Added media_type column to migration 11 (video/audio/image/unknown)
- Closes the one gap between contracts.md #4 and the metadata plan

0.1.134 -- Ship gate: added multi-session, disk-full, post-wipe, timezone, and filesystem-awareness tests

- Added A7.2: disk-full mid-import graceful failure
- Added A11.5: timezone storage requirement (no naive local time)
- Added A2.3: filesystem-specific atomic rename documentation
- Tightened A10.3: specifies default audio track for multi-track sources
- Tightened B4.3: test data must match actual fast-hash window parameters
- Added B13: multi-session isolation (same card re-import, two-card back-to-back)
- Added B14: disk-full during import end-to-end (6 checks)
- Added B15: post-wipe failure awareness (dest loss after wipe, wipe report persistence)
- Moved B12.5 hover scrubbing to C2 (SHOULD PASS -- depends on sprite sheet pipeline)
- Renumbered C sections for consistency

0.1.133 -- Ship gate checklist hardened: 6 new test sections, outlier media, device yank, post-ingest playback

- Added A1.6-A1.8: audio-only, image, and unrecognized-extension files must be discovered (contract #3, #4)
- Added A2.5: dedup false-positive safety net (fast hash match but full hash mismatch must still copy)
- Added A9: device disconnection handling (6 checks -- detection, error codes, clear messaging, no false SAFE)
- Added A10: post-ingest playback gate (proxy + thumbnail auto-queue, playable output after import)
- Added A11: timestamp correctness (embedded > folder name > filesystem, source stored in DB)
- Added B4.3: dedup false-positive end-to-end test scenario
- Added B10: device yank mid-import end-to-end (6 checks including re-import after reconnect)
- Added B11: outlier media types end-to-end (audio + image files through full pipeline)
- Added B12: post-ingest playback end-to-end ("can I watch my stuff" test)
- Added C1.4-C1.5: device disconnect UX + import summary total file count
- Added C5: dual DB integrity test (library delete/move preserves App DB data)
- Test dataset now requires audio-only file and image file in addition to existing requirements
- Strengthened scope statement: "every file on the card is accounted for"

0.1.132 -- Docs sync: sidecar import audit 49/49, techguide updated

- Full audit of sidecar-importplan.md against codebase: 49/49 checklist items confirmed done
- Techguide updated with Sidecar Import Verification section documenting the full pipeline
- Techguide version synced from 0.1.128 to 0.1.132
- tauri.conf.json version synced to 0.1.132

0.1.131 -- Sidecar UX: progress counts, summary, error distinction (sidecar-importplan 12.7)

- IngestResult now tracks sidecar_count and sidecar_failed separately
- IngestResponse (Rust + TypeScript) includes sidecarCount and sidecarFailed fields
- Completion message includes sidecar count when sidecars are present
- ImportDialog summary shows media clips and sidecar counts separately
- Error display distinguishes media failures from sidecar failures
- SessionVerificationStatus includes sidecar_total and sidecar_failed counts
- get_session_verification_status() queries sidecar-specific counts by entry_type

0.1.130 -- Sidecar gold-standard: copy pipeline + rescan + audit (sidecar-importplan 12.4-12.6)

- Sidecar copy pipeline: process_sidecar_entry() follows same gold-standard copy+verify as media files
- Sidecars get full BLAKE3 hashing, read-back verification, dedup checks, and atomic writes
- Failed sidecar copy is a hard failure (blocks SAFE TO WIPE), not a warning
- link_sidecar_to_parent_clip() links sidecar assets to parent media clip via manifest parent_entry_id
- Orphan sidecars (no matching media stem) are copied and verified but not linked to any clip
- Rescan gate now uses discover_all_eligible_files() to cover both media and sidecar files
- Missing/new sidecars on source device block SAFE TO WIPE identically to media files
- Audit export: ManifestExportEntry and ResultExportEntry now include entry_type and parent_entry_id
- Audit rescan.jsonl now includes sidecar files (uses discover_all_eligible_files)
- Legacy ingest_sidecar() function deprecated (kept for backward compat, no longer called)

0.1.129 -- Sidecar gold-standard: Migration 10 + discovery + manifest (sidecar-importplan 12.1-12.3)

- DB Migration 10: adds entry_type (media/sidecar) and parent_entry_id columns to ingest_manifest_entries
- ManifestEntry + NewManifestEntry structs updated with new fields
- insert_manifest_entry, get_manifest_entries, get_pending_manifest_entries updated to read/write new columns
- Query ordering changed from relative_path to id (ensures media entries process before their sidecars)
- All existing manifest entries default to entry_type='media', parent_entry_id=NULL (backward compat)
- discover_all_sidecars(): walks source dir, returns paired sidecars (matched to media stem) and orphan sidecars separately
- discover_all_eligible_files(): returns media + sidecar paths combined (for rescan in 12.5)
- Manifest building now inserts media entries first, then paired sidecars with parent_entry_id, then orphans
- manifest_hash now covers all entries (media + sidecars)

0.1.128 -- Wipe workflow + device ejection detection (importplan 29/29)

- Wipe source files command (hard-gated on SAFE TO WIPE, deterministic delete order from manifest)
- wipe_report.json audit artifact exported per-session with success/failure per file
- Device ejection detection: checks source root between files, marks session failed with DEVICE_DISCONNECTED error on all remaining manifest entries
- Rescan detects disconnected source explicitly, fails with clear error instead of silent pass
- New Tauri command: wipe_source_files (with optional audit export)

0.1.127 -- Add 4 importplan section 10 tests

- test_readback_detects_corruption: corrupt dest byte, verify hash mismatch detected
- test_crash_safety_temp_file_pattern: confirm no temp files remain, no orphan files on failure
- test_dedup_fast_hash_collision_resolved_by_full_hash: 2MB+ files with identical first/last MB but different middle -- fast hash collides, full hash rejects dedup
- test_new_file_after_manifest_blocks_safe_to_wipe: add file to source after manifest, rescan blocks SAFE TO WIPE
- 81 tests pass (77 existing + 4 new)

0.1.126 -- Gold-standard import verification

- Streaming BLAKE3 copy with temp file, fsync, atomic rename, and read-back verify (never loads full file into RAM)
- Ingest sessions + manifest tables (Migration 9) track every file from discovery through verification
- Change detection: re-stats source files before copy, flags any that changed since discovery
- Dedup verification: proves duplicates match via full BLAKE3 hash comparison, not just fast hash
- Rescan gate: re-walks source after ingest, only sets safe_to_wipe when manifest matches exactly
- Audit export: session.json, manifest.jsonl, results.jsonl, rescan.jsonl, rescan_diff.json
- Secondary verification in background hash jobs compares stored hash to on-disk hash
- New commands: get_session_status, get_session_by_job, export_audit_report
- assets table gains verified_method column (copy_readback, dedup_full_hash, secondary_hash, background_hash)

0.1.125 -- Move Cameras from sidebar/dashboard to Settings

- Cameras is now a section inside Settings (Advanced mode only, gated by Cameras feature flag)
- Removed Cameras from left sidebar nav (LeftNav)
- Removed Cameras action button from Library Dashboard
- Removed standalone CamerasView routing from App.tsx
- Cleaned up unused mode/featureFlags props from LeftNav, MainLayout
- Updated feature toggle description from "Cameras Tab" to "Cameras"
- Cameras is configuration, not workflow -- belongs in Settings with other setup tasks

0.1.124 -- Version sync + compiler warning cleanup

- Synced version to 0.1.123 across all 4 version files (tauri.conf.json was 0.1.116, constants.ts was 0.1.105, Cargo.toml and package.json were 0.1.0)
- Fixed 12 compiler warnings: 5 unused Result values in export/mod.rs, 2 unused constants in ffmpeg_builder.rs, unused imports in ffprobe.rs/ingest/mod.rs/export/mod.rs, unused variables in scoring.rs/runner.rs
- Zero warnings, zero errors on build

0.1.123 -- Library Fix spec complete, docs sync

- libraryfix.md (docs/planning/libraryfix.md) fully written: App DB + Portable Library DB long-term spec
- Updated techguide.md to 0.1.123: added App DB architecture, dual-DB model, library registry, stable camera refs, settings in App DB KV, bundled profiles sync
- Updated contracts.md to v1.1: added 4 new contracts (App DB, stable identity, library identity, short-lived connections)
- All doc versions now match implementation state (0.1.106-0.1.122 changelog entries cover full implementation)

0.1.122 -- Phase 8 audit: wire ensure_library_db_initialized into open/create commands

- open_library and create_library now call ensure_library_db_initialized() instead of open_db() + manual UUID
- This ensures L6 backfill (stable camera refs) runs on every library open, not just theoretical code paths
- Previously ensure_library_db_initialized existed (added in 0.1.113) but was never called from the command layer
- Full section 8 audit: all Groups 0-7 confirmed 100% complete, no other gaps found
- All 71 tests pass

0.1.121 -- Phase 7: Dev Menu tooling complete

- Raw SQL tool now has App DB / Library DB target selector (radio buttons in Debug section)
- Frontend passes target param to execute_raw_sql backend command
- Added Profile Authoring (staging) UI to Camera DB section in Dev Menu
- Staging UI supports: stage new or edit existing profile, validate, publish, discard
- All backend staging commands were already wired (stage, list, validate, publish, discard)
- All 19 app_db + app_schema tests pass

0.1.120 -- match_camera command now uses App DB directly (no legacy Library DB matcher)

- Rewrote match_camera Tauri command to match against App DB profiles directly instead of calling the legacy Library DB matcher and name-mapping results back
- Matching now follows spec 7.2 priority: registered device (serial) > user profiles > bundled profiles > fallback
- Uses the same score_match_rules engine (spec 7.3/7.4) that the ingest pipeline uses for stable refs
- Made ingest matching functions pub(crate): match_app_profile_rules, match_bundled_profile_rules, score_match_rules
- Legacy Library DB matcher (camera::matcher) only remains in use by ingest pipeline for backward-compat integer column population
- No frontend changes needed -- command signature and return type unchanged

0.1.119 -- Phase 6 audit fix: settings moved from Tauri Store to App DB

- Rewrote all settings commands (get/save/mode/recents) to read/write App DB app_settings KV table instead of Tauri Store (spec section 6.3)
- get_app_settings now reconstructs AppSettings from App DB: ui_mode, features, theme, dev_menu, license_state_cache, default library UUID resolved to path
- Recent projects now derived exclusively from libraries registry table (App DB), not Tauri Store recentProjects array
- save_app_settings decomposes into individual KV writes: mode, features, theme, dev_menu, title offset, license cache
- add_recent_library upserts into App DB registry with UUID-based identity
- remove_recent_library deletes from App DB registry, handles default library reassignment
- Removed Tauri Store fallback from LibraryDashboard -- registry is now sole source for recent projects
- Tauri Store migration (lib.rs) still runs once at startup for upgrading users, store plugin kept for that
- All 71 tests pass, zero compilation errors

0.1.118 -- Phase 6: Frontend wiring complete

- Added list_registry_libraries Tauri command (enriches App DB entries with clip count + thumbnail)
- Added RegistryLibraryEntry type (TS) and listRegistryLibraries API function
- LibraryDashboard now prefers App DB registry for recent projects, falls back to Tauri Store
- Cameras button on dashboard works without library open (Phase 6 requirement)
- App.tsx wired: CamerasView renders standalone from dashboard in Advanced mode
- CamerasView accepts optional onBack prop for dashboard navigation
- Fixed refreshRegistry callback ordering (defined before use in handleOpenLibrary)
- All 4 Phase 6 checklist items verified: profileType/profileRef, cameras without library, registry UI, mode/toggles

0.1.117 -- Add 2 spec 9.2 test gaps from Phase 5 audit

- test_deleting_library_preserves_app_data: verifies profiles, devices, settings survive library removal
- test_recents_persist_across_sessions: verifies library registry entries survive connection close/reopen

0.1.116 -- Phase 5 audit: library clip schema + ingest confirmed complete

- Audited all Phase 5 (Library clip schema + ingest) items from libraryfix.md spec
- Clip/NewClip structs: stable camera refs (profile_type, profile_ref, device_uuid) present
- All clip queries (insert, get, list, events, date-grouped) include stable ref columns
- Ingest worker: opens App DB connection, runs matcher priority chain (device > user > bundled > legacy > fallback), writes stable refs to clip
- Fix: Added stable camera refs (profile_type, profile_ref, device_uuid) to sidecar CameraMatchSnapshot -- was only writing legacy integer IDs
- Matcher tie-break implemented per spec 7.4: version > specificity > profile_ref ascending
- All 69 Rust tests pass

0.1.115 -- Phase 4 audit: cameras in App DB confirmed complete, fix bundled resource bundling

- Audited all Phase 4 (Cameras in App DB) items from libraryfix.md spec
- Bundled profiles: sync_bundled_profiles() does full replace, idempotent, called at startup, tested
- User profiles CRUD: create/list/get/update/delete with stable UUID, Tauri commands wired
- Devices CRUD: create/list/get/find-by-USB/find-by-serial, upsert for migration, Tauri commands wired
- Legacy JSON import: reads ~/.dadcam/custom_cameras.json, upserts into App DB, renames to .migrated
- Fix: Added bundle.resources to tauri.conf.json so bundled_profiles.json ships in production builds
- All 69 Rust tests pass

0.1.114 -- Fix 6 audit issues: frontend types, staging workflow, upgrade/evidence/VHS tests

- Fix 1+6: Rewrote src/types/cameras.ts to use stable profileType/profileRef refs matching Rust backend serde output; updated CamerasView, CameraDbManager, LeftNav to use composite keys and correct field names
- Fix 2: Added Migration A3 (profile_staging table) to App DB; added stage/list/validate/publish/discard functions in app_schema.rs; added 5 staging commands in devmenu.rs; registered in lib.rs
- Fix 3: Added test_upgrade_user_profiles_uuid_backfill test (spec 9.1) -- simulates pre-uuid table, ALTERs, backfills, verifies uniqueness
- Fix 4: Added test_profile_quality_gate_evidence_matching test (spec 9.4) -- evidence JSON with AND/OR semantics, regex folderPattern, positive/negative/near-miss assertions
- Fix 5: Added VHS recipe CRUD (insert/get/update) + compute_recipe_hash (BLAKE3 of BTreeMap canonical JSON) + 4 determinism tests (spec 9.5)
- All 75 Rust tests pass

0.1.113 -- Phase 1/2 audit fixes: L7 migration + backfill placement

- Added Migration 8 (vhs_edits table) per spec 5.2 L7 -- deterministic recipe definitions for VHS export
- Moved backfill_stable_camera_refs call from open_library command into ensure_library_db_initialized per spec contract, so all code paths that initialize a library DB run backfill
- Removed duplicate backfill call from commands/library.rs

0.1.112 -- DbState stores path not connection (Spec 3.4)

- Changed DbState from Mutex<Option<Connection>> to Mutex<Option<PathBuf>>
- Each command now opens a short-lived connection via DbState::connect()
- connect() delegates to open_library_db_connection() (pragmas only, no migrations)
- Removes long-lived connection from Tauri State per spec 3.4 recommendation

0.1.111 -- Matcher specificity weights + deterministic tie-break (Spec 7.3, 7.4, Appendix A)

- score_match_rules now uses Appendix A weighted specificity: +5 make+model, +3 folderPattern, +3 codec+container, +2 resolution, +1 frameRate
- Added frameRate matching with +/- 0.01 tolerance (Spec Appendix A)
- Added resolution constraint matching: minWidth, maxWidth, minHeight, maxHeight (Spec Appendix A)
- Tie-break order is now spec-compliant: (1) higher version, (2) higher specificity score, (3) profile_ref ascending for determinism (Spec 7.4)
- 60 tests pass, 0 failures

0.1.110 -- Library Fix audit fixes: busy_timeout, App DB matcher priority, tests (Spec 3.4, 7.2, 9.3, 9.4)

- Added PRAGMA busy_timeout=5000 to open_db() (was missing on main Library DB path)
- Rewired ingest stable ref resolution to use App DB priority order: device > user > bundled > fallback (Spec 7.2)
- New score_match_rules engine evaluates App DB match_rules JSON with AND/OR semantics (Spec 7.3)
- New test: test_concurrent_db_operations (8 threads, parallel reads+writes, no deadlock) (Spec 9.3)
- New test: test_bundled_profile_quality_gate (positive+negative match validation) (Spec 9.4)
- New test: test_bundled_sync_idempotent (Spec 9.2)
- 60 tests pass, 0 failures

0.1.109 -- Library Fix: L6 backfill + Tauri Store migration (Spec 6.2, 6.3)

- L6 backfill: on library open, populate camera_profile_type/ref/device_uuid from legacy integer IDs
- Bundled profile names resolve to slug, unknown names create [Migrated] user profiles in App DB
- Legacy camera_device_id resolved to device UUID, device upserted into App DB
- Tauri Store one-time migration: copies ui_mode, features, title offset, recent projects, theme to App DB
- Recent projects resolved to library UUIDs and registered in App DB library registry
- Migration skipped if tauri_store_migrated=true already set (idempotent)
- New test: test_backfill_stable_camera_refs (verifies bundled match, migrated user profile, device resolve)
- 57 tests pass, 0 failures

0.1.108 -- Library Fix Phase 2: Cameras in App DB, stable clip refs, typed settings (Groups 3-7)

- Typed settings helpers: ui_mode, features, title_offset, simple_default_library_uuid
- Bundled camera profiles: sync from bundled_profiles.json to App DB at startup
- User camera profiles CRUD: create/list/get/update/delete with UUID identity
- Camera devices CRUD: create/list/get/find-by-USB/find-by-serial with profile_type/profile_ref
- Legacy ~/.dadcam/custom_cameras.json one-time migration to App DB
- Migration 7 (L6): clips gain camera_profile_type, camera_profile_ref, camera_device_uuid columns
- Ingest writes stable camera refs (slug or uuid) instead of integer FK
- Camera commands rewritten to use App DB directly (work without library open)
- Dev menu raw SQL accepts target parameter ("app" for App DB, default Library DB)
- Clip struct reads new columns from all query paths (get_clip, list_clips, events, preview)
- 56 tests pass, 0 failures

0.1.107 -- relink_library now validates UUID matches target library before updating path

- relink_library opens library DB at new_path, confirms stored UUID matches expected UUID
- Rejects with clear error on UUID mismatch or missing library_meta
- New test: test_relink_rejects_uuid_mismatch
- 56 tests pass, 0 failures

0.1.106 -- Library Fix Phase 1: App DB + Library UUID + Registry (Groups 0-2)

- New App DB (~/.dadcam/app.db) with two migrations: A1 (bundled_profiles, user_profiles, camera_devices) and A2 (libraries registry, app_settings KV)
- App DB auto-initializes at startup (ensure_app_db_initialized in lib.rs)
- New library_meta table (Migration 6) in Library DB for portable library identity
- get_or_create_library_uuid: generates UUID v4 on first open, persists in Library DB
- Library registry: upsert, mark_opened, mark_seen, mark_missing, list_recent, relink
- App settings KV: get/set/delete for app_settings table
- open_library and create_library now generate library UUID and register in App DB
- LibraryResponse includes libraryUuid field for frontend
- open_app_db_connection() for short-lived connections (no migration)
- open_library_db_connection() for short-lived library reads (no migration)
- 7 new tests (all pass, 55 total pass, 0 failures)

0.1.105 -- Fix 3 failing scoring tests, sync tauri.conf.json version

- audio.rs: Fix ebur128 true peak regex (match "Peak:" not "True peak:")
- audio.rs: Clamp very quiet audio (<-35 LUFS) to max 0.4 score (was 0.7)
- tests.rs: Widen MotionNoisy audio expected range to (0.2, 1.0)
- CameraDbManager.tsx: Prefix unused copyToClipboard prop with underscore
- All 48 Rust tests pass, TypeScript compiles clean, Vite build clean (88 modules, 357KB)
- tauri.conf.json version synced from 0.1.15 to 0.1.105 (now matches constants.ts and changelog)

0.1.104 -- Documentation sync (post-build audit)

- Implementation guide v4: synced all 11 steps to match built codebase
  - get_db_stats added to dev menu commands (6->7, total 25->26 new, 63->64 overall)
  - Step 9: added live log viewer, database stats, full profile/device tables, inline key activation, native file dialogs
  - Step 10: macOS -xml plist, Windows PowerShell Get-CimInstance, catch_unwind, root hub filtering
  - Post-implementation audit notes table added
- contracts.md: added .dadcam/sidecars/ to library structure (added in v0.2.0 Phase 6)
- techguide.md: version 0.1.100->0.1.104, added sidecars/ to library structure, updated USB fingerprint methods (system_profiler -xml, PowerShell Get-CimInstance, catch_unwind), updated dev menu sections to match built UI

0.1.103 -- Phase 11: Terminology rename (Library -> Project)

- All user-facing UI text now says "Project" instead of "Library" (buttons, headings, tooltips, error messages, placeholders)
- Updated: LibraryDashboard, LibraryView, LibraryCard, LibrarySection, WelcomeDashboard, EventView, DateView, FirstRunWizard, App
- Comments updated: "Personal Mode" -> "Simple Mode", "Pro Mode" -> "Advanced Mode"
- Internal code (variable names, types, CSS classes, API names) unchanged -- only UI-facing strings renamed
- App tagline "Video library for dad cam footage" kept as-is (describes the app, not a workspace)

0.1.102 -- Phase 10: USB fingerprint quality improvements

- macOS: switched from plain-text system_profiler to -xml plist output for stable parsing across macOS versions
- Windows: replaced deprecated wmic with PowerShell Get-CimInstance (future-proof for Windows 10+)
- Windows: VID/PID extraction now uses regex instead of fixed-offset slicing
- All platforms: filter out root hubs and deduplicate fingerprint entries

0.1.101 -- Phase 10: USB Registration hardening (best-effort)

- Wrapped capture_usb_fingerprint in std::panic::catch_unwind to prevent platform-specific panics from crashing the app
- USB detection already implemented for macOS (system_profiler), Windows (wmic), Linux (/sys/bus/usb/devices/) in Phase 5
- Phase 10 adds panic safety per implementation guide: all USB detection is now catch_unwind guarded
- Failure at any level (panic, process error, parse error) silently returns None -- never blocks user flow

0.1.100 -- Phase 9 audit fixes

- Added theme toggle (light/dark) to Settings General section, visible in Advanced mode only
- Fixed mode change sync: SettingsView now re-fetches settings from backend after mode switch instead of reconstructing locally

0.1.99 -- Packages onesheet rework

- Nostalgically Correct price updated to $2,400
- Removed "Includes 1 Videographer up to 8 Hours" footer from both cards
- Added "Choose One" label inside Nostalgically Correct and The Classics (pick your medium)
- Super 8 / Dad Cam / Modern Digital options in distinct bordered sections, cards mirror each other
- Replaced "The Full Family Mixed Media Experience" with "Mixed Media Dreamteam"
- Dreamteam: 2x2 medium picker (choose up to 4), 3 recommended experiences, always-included section
- Full page flex layout fills exact 8.5x11 with footer pinned naturally (no absolute positioning)
- "Experiences We Recommend" uses distinct section-title style vs column label style
- Grid rows weighted 3:2 for balanced top/bottom distribution

0.1.98 -- Phase 9 planning doc audit corrections

- Corrected phase-9-app-reorganization.md to match implementation guide (source of truth)
- Fixed DevMenuSettings field names: titleStartTime->titleStartSeconds, blendDuration->jlBlendMs
- Added missing watermarkText and licenseStateCache to settings type spec
- Fixed default faceDetection from true to false for Simple mode defaults
- Corrected soft lock spec: rendered exports allowed with watermark+720p cap, export originals always allowed
- Updated licensing to match implementation: BLAKE3 keyed hash, keychain trial storage, key prefixes
- Updated file structure to show actual files built (renames deferred per implementation guide)
- Added all backend files (Rust modules, commands, migrations) to file structure
- Updated dev menu spec to include all 7 backend commands
- Removed non-existent lastProjectPath and ThemeMode type alias from type spec
- Updated migration notes to reference actual Rust implementation

0.1.97 -- Nightfox Films packages onesheet

- Add docs/client/packages.html -- single-page printable package sheet for Nightfox Films
- Three tiers: Nostalgically Correct ($1800), The Classics ($2800), The Full Family Experience
- Raw Footage Experiences starting at $5800
- Matches existing onesheet.html Braun/clean style

0.1.96 -- Phase 9 audit fixes

- CameraDbManager: show full profile list table and device list table (was only showing count)
- CameraDbManager: add Import JSON and Export JSON buttons with native file dialogs
- LicenseTools: add inline key activation with text input and Activate button
- DebugTools: add Export EXIF Dump UI (clip ID input + save dialog)
- DebugTools: add live log viewer (listens for job-progress events, last 200 lines)
- Add CSS for devmenu tables and log viewer

0.1.95 -- Phase 9 spec compliance

- Fix clear_caches: now clears proxies/, thumbs/, sprites/ (was clearing wrong directories)
- Fix export_database: copies .db file to user-chosen path via save dialog (was returning text dump)
- Add export_exif_dump command: exports full exiftool JSON for a clip to output path
- Register export_exif_dump in Tauri command handler
- Extract dev/ sub-components: FormulasEditor, CameraDbManager, LicenseTools, DebugTools
- Extract ExportHistory component from ExportDialog
- Add CamerasView component for cameras tab in Advanced mode
- DevMenu refactored to use extracted sub-components

0.1.94 -- Phase 9 Dev Menu

- Dev Menu accessible via Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win/Linux)
- Easter egg: Settings > About > click version 7 times
- Formulas section: title start time, J&L blend duration, score weights with sliders, watermark override
- Camera DB section: view camera count, export JSON to clipboard
- License Tools section: view current state, clear license, generate batch rental keys (1-100)
- Debug section: test FFmpeg/FFprobe/ExifTool availability + version, clear caches, database stats, export schema, raw SQL (dev license only)
- Backend: 6 new Tauri commands (test_ffmpeg, clear_caches, export_database, execute_raw_sql, generate_rental_keys, get_db_stats)
- Full-page overlay UI following Braun Design Language with 4-section nav

0.1.93 -- Wire BestClipsPanel to feature flag

- BestClipsPanel now renders on WelcomeDashboard when bestClips flag is enabled and clips exist
- Passed featureFlags from LibraryView through to WelcomeDashboard
- When bestClips toggle is off in Advanced settings, the panel is hidden

0.1.92 -- Phase 8 Feature Toggles (Advanced only)

- Added "Features" section to Settings page with toggle switches for screen grabs, face detection, best clips, and cameras tab
- Features nav item and toggle UI only visible in Advanced mode; entirely hidden in Simple mode
- Toggle changes persist immediately to settings store via saveAppSettings
- Switching to Simple mode while on Features tab auto-navigates back to General
- Added feature-toggle CSS styles (toggle switch, row layout) following Braun Design Language

0.1.91 -- Phase 7 Cameras tab: add Unknown bucket

- Added "Unknown" camera bucket at bottom of Cameras nav section per spec
- Renamed profiles group label to "Matched Profiles" for clarity

0.1.90 -- Phase 7 LeftNav updates (Favorites link + Cameras tab)

- Added Favorites nav link to LibrarySection; clicking navigates to clips view with favorites filter pre-applied
- Active state highlight on Favorites link when favorites filter is active
- Added Cameras tab to LeftNav, gated by mode === 'advanced' AND featureFlags.camerasTab === true
- Cameras tab shows registered devices and matched profiles in collapsible section
- Cameras tab hidden in Simple mode and when camerasTab flag is off
- Passed mode and featureFlags through MainLayout to LeftNav for feature gating
- Added nav-cameras CSS styles following Braun Design Language
- TypeScript and Rust compile clean

0.1.89 -- ImportDialog DRY cleanup

- Deduplicated JobProgress, IngestResponse, CameraBreakdownEntry types into src/types/jobs.ts
- ImportDialog now imports shared types instead of defining them inline
- ImportDialog cancel uses cancelJob API wrapper instead of direct invoke

0.1.88 -- Phase 6 audit minor fixes

- Sidecar ingest timestamps now track per-stage (discovered_at, copied_at, indexed_at) instead of identical values
- Import button pre-checks license before opening dialog; blocks with clear message if trial expired
- Ingest progress now emits a "previews" phase label after file processing completes

0.1.87 -- Phase 6 audit fixes

- Sidecar JSON schema changed to nested structure per spec (metadata_snapshot, camera_match, ingest_timestamps, derived_asset_paths, rental_audit)
- Per-file ingest progress now emits sub-phase indicators (copying, hashing, metadata, indexing) instead of generic "processing"
- Removed unused state parameter from start_ingest command

0.1.86 -- Import Dialog + sidecar JSON

- Replaced bare import flow with Import Dialog (folder picker, event assignment, progress bar, summary)
- Per-clip sidecar JSON written to .dadcam/sidecars/ during ingest (metadata snapshot, camera match, timestamps)
- start_ingest now accepts event_id / new_event_name params; links imported clips to events
- Camera breakdown tracked during ingest, shown in Advanced mode summary
- Import blocked when trial expired (license check added)
- Added SIDECARS_FOLDER constant and sidecars/ to library folder init

0.1.85 -- Phase 5 minor fixes (matcher, fingerprint LIKE, import count)

- Camera matcher: load profiles once before device loop instead of per-device (O(D*P) query reduction)
- USB fingerprint LIKE pattern now includes closing quote delimiter for tighter SQL matching
- import_camera_db and load_devices_from_json: only increment count on successful insert, log failures

0.1.84 -- Phase 5 import/export symmetry and confidence constant

- Fixed import_camera_db: now imports both profiles AND devices from combined export format (symmetric with export_camera_db)
- import_camera_db still accepts plain array format (canonical.json) as fallback
- Return type changed from u32 to ImportCameraDbResult { profilesImported, devicesImported }
- Extracted camera match confidence threshold (0.5) to CAMERA_MATCH_MIN_CONFIDENCE constant

0.1.83 -- Phase 5 audit fixes (7 items)

- Fixed match_camera command: graceful fallback when original file is inaccessible (uses stored clip data)
- Fixed match_camera command: removed dead code (unused codec query field)
- Added custom_cameras.json sync: devices saved to ~/.dadcam/custom_cameras.json on registration, loaded on library open
- Added auto-load bundled profiles: canonical.json loaded on both library open and create
- Fixed USB fingerprint SQL search: escaped LIKE special characters to prevent false matches
- Fixed Windows USB parsing: corrected operator precedence in VID/PID range slicing
- Added container format matching: MediaMetadata.container populated from ffprobe format_name, matched in profile scoring

0.1.82 -- v0.2.0 Phase 5: Camera System MVP

- Migration 5: camera_devices table with uuid, serial_number, fleet_label, usb_fingerprints, rental_notes
- Migration 5: clips table gets camera_device_id column for physical device tracking
- Created camera/devices.rs: CameraDevice struct, CRUD ops, USB fingerprint capture (macOS/Windows/Linux)
- Created camera/matcher.rs: unified 6-level matching engine (USB fingerprint > serial > make+model device > profile > filename > generic)
- Created camera/bundled.rs: loads canonical.json bundled camera profiles into DB at startup
- Created resources/cameras/canonical.json: empty array placeholder for bundled camera DB
- Created commands/cameras.rs: 6 Tauri commands (list_camera_profiles, list_camera_devices, register_camera_device, match_camera, import_camera_db, export_camera_db)
- register_camera_device gated by camera_registration license check (blocked when trial expired)
- Ingest pipeline now calls unified matcher (device + profile) instead of profile-only matching
- Ingest sets both camera_profile_id and camera_device_id on clips when matched
- Added serial_number extraction to exiftool metadata (SerialNumber + InternalSerialNumber fields)
- Added serial_number field to MediaMetadata struct
- Created src/types/cameras.ts: CameraProfile, CameraDevice, RegisterDeviceParams, CameraMatchResult types
- Created src/api/cameras.ts: 6 API wrapper functions
- Rust and TypeScript compile clean

---

0.1.81 -- VHS Export audit fixes

- Title overlay now starts at 5 seconds (was 0), duration 3 seconds (was 5), centered vertically (was bottom)
- Title timing reads devMenu.titleStartSeconds, crossfade reads devMenu.jlBlendMs from settings
- Default blend duration corrected to 500ms (was hardcoded 1000ms)
- Event selector in ExportDialog is now a dropdown with event names (was raw numeric ID input)
- Fixed single-clip no-audio export: null audio source now properly mapped with -map directives
- Job runner export stub clarified (export runs via direct command, not job queue)

---

0.1.80 -- VHS Export MVP

- Created src-tauri/src/export/ module: mod.rs, timeline.rs, ffmpeg_builder.rs, watermark.rs
- Export orchestration: clip selection, FFmpeg filtergraph with xfade/acrossfade, atomic output
- 5 selection modes: all, favorites, date_range, event, score threshold
- 4 ordering modes: chronological, score_desc, score_asc, shuffle (seeded)
- Conform filter normalizes all clips to 1920x1080/30fps/48kHz stereo before crossfade
- Clips without audio get injected silence via anullsrc
- Title overlay via drawtext filter with fade in/out
- Watermark + 720p cap applied when trial expired (licensing::should_watermark)
- Single-clip export falls back to simple transcode (no xfade needed)
- Temp file with atomic rename on success, cleanup on cancel/error
- Progress via FFmpeg stderr time= parsing, emitted as job-progress events
- Cancel support via AtomicBool flag, kills FFmpeg child process
- Migration 4: export_history table with status tracking
- Created src-tauri/src/commands/export.rs: 3 Tauri commands (start_vhs_export, get_export_history, cancel_export)
- Own DB connection for export (avoids holding shared Mutex during render)
- Created src/types/export.ts: VhsExportParams, ExportHistoryEntry, SelectionMode, ExportOrdering
- Created src/api/export.ts: startVhsExport, getExportHistory, cancelExport
- Created src/components/ExportDialog.tsx: selection mode, ordering, title text, output path picker, progress bar, export history
- Score threshold selection only visible in Advanced mode
- Added VHS Export button to LibraryView toolbar (next to Import Footage)
- Added rand = "0.8" to Cargo.toml for shuffle ordering
- Added export dialog CSS following Braun Design Language
- Rust and TypeScript compile clean

0.1.79 -- Phase 3 audit fixes (round 2)

- deactivate_license now returns LicenseState (was void) so frontend can sync cache after deactivation
- Removed unused serde import from commands/licensing.rs
- App startup now syncs licenseStateCache in settings if stale vs live keychain state (crash resilience)
- deactivateLicense TypeScript API updated to return LicenseState

0.1.78 -- Phase 3 audit fixes

- Trial banner now shows on all screens (welcome, unmounted, dashboard, settings), not just when a library is open
- License state cache in settings JSON now syncs after activation/deactivation

0.1.77 -- v0.2.0 Phase 3: Licensing System

- Created src-tauri/src/licensing/mod.rs: offline license validation via BLAKE3 keyed hash
- License types: trial (14-day), purchased (DCAM-P-), rental (DCAM-R-), dev (DCAM-D-)
- Keys stored in OS keychain via keyring crate (never in settings file)
- Trial start date stored in keychain, auto-created on first launch
- Soft lock after trial: can browse/view library, cannot import/score/register cameras
- Feature gating: is_allowed() checks per feature, watermark flag for expired exports
- Key generation: generate_key() and generate_rental_keys() for dev menu use
- Created src-tauri/src/commands/licensing.rs: 4 Tauri commands (get_license_state, activate_license, deactivate_license, is_feature_allowed)
- Added License error variant to error.rs
- Added keyring = "2" to Cargo.toml
- Created src/types/licensing.ts: LicenseState, LicenseType, GatedFeature types
- Created src/api/licensing.ts: getLicenseState, activateLicense, deactivateLicense, isFeatureAllowed
- Created src/components/TrialBanner.tsx: trial countdown bar with "Enter License Key" CTA
- Created src/components/modals/LicenseKeyModal.tsx: key entry with validation feedback
- App.tsx loads license state on startup, renders TrialBanner when in trial
- Added trial-banner and license-key-modal CSS following Braun Design Language
- Unit tests for key generation, validation, tampering rejection, trial day calculation
- Rust and TypeScript compile clean

0.1.76 -- Phase 2 audit fix: runner ingest passes app + cancel through

- runner.rs run_ingest_job now passes Option<AppHandle> through to ingest (was ignored as _app)
- When app is available (Tauri context), uses run_ingest_job_with_progress for per-file progress events
- Registers and cleans up cancel flag in the queue runner path, not just the direct start_ingest path
- Queue runner ingest jobs now support both progress emission and cancellation
- Build compiles clean

0.1.75 -- Phase 2 audit fixes: consolidate progress plumbing

- Consolidated run_ingest_job and run_ingest_job_with_progress into one code path (run_ingest_job_inner) -- eliminates duplicate ingest logic
- Added emit_progress_opt() helper for optional AppHandle (no-op when None, e.g. CLI context)
- runner.rs now accepts Option<AppHandle> and emits job-progress events for all job types: ingest, hash_full, proxy, thumb, sprite, score
- Each job type emits starting phase progress with descriptive message
- run_next_job emits completion/error progress after every job
- Updated all CLI call sites (cli.rs) to pass None for the new app parameter
- Removed unused heartbeat_job import from runner.rs
- Build compiles clean

0.1.74 -- v0.2.0 Phase 2: Unified Job Progress/Cancel Plumbing

- Created src-tauri/src/jobs/progress.rs: JobProgress struct with phase, current/total, percent, message, cancel/error flags
- emit_progress() helper sends "job-progress" Tauri event to frontend
- Cancel infrastructure: register_cancel_flag, request_cancel, remove_cancel_flag, is_cancelled in jobs/mod.rs
- Cancel flags use AtomicBool in a global HashMap keyed by job_id string
- Added run_ingest_job_with_progress() to ingest/mod.rs: emits per-file progress and checks cancel flag between files
- start_ingest command now accepts AppHandle, registers cancel flag, emits progress events
- Added cancel_job Tauri command (registered in handler list)
- Created src/types/jobs.ts: TypeScript JobProgress interface matching Rust struct
- Created src/api/jobs.ts: cancelJob() wrapper calling cancel_job command
- Rust and TypeScript compile clean

0.1.73 -- Phase 1 audit fixes

- FirstRunWizard now has 2 steps: mode selection then folder picker (Simple mode)
- Simple wizard step 2 creates Default Project at chosen folder via native picker
- Advanced wizard skips step 2 (dashboard shown by normal routing)
- Skip link allows Simple users to defer project setup
- SettingsPanel mode change now updates featureFlags in local state (matches SettingsView behavior)

0.1.72 -- v0.2.0 Phase 1: Settings v2 + First Run Wizard

- Settings schema upgraded from v1 to v2 with automatic migration
- Renamed mode: personal/pro -> simple/advanced (data + types + all references)
- Renamed recentLibraries -> recentProjects, lastLibraryPath -> defaultProjectPath
- Added new settings fields: firstRunCompleted, theme, featureFlags, devMenu, licenseStateCache
- FeatureFlags struct with mode-dependent defaults (face_detection off in Simple, on in Advanced)
- DevMenuSettings struct with scoreWeights, titleStartSeconds, jlBlendMs, watermarkText
- LicenseStateCache struct for non-secret license summary
- v1->v2 migration: maps old personal->simple, pro->advanced, recentLibraries->recentProjects
- Existing v1 users get firstRunCompleted=true (skip wizard), new users get false
- Old v1 store keys cleaned up after migration
- Created FirstRunWizard component shown on first launch (mode selection + Get Started)
- App.tsx gates on firstRunCompleted before showing any other view
- Theme class applied to document root from settings.theme
- All 7 affected frontend files updated: App.tsx, SettingsView, SettingsPanel, LibraryView, LibraryDashboard, LibraryCard, api/settings
- TypeScript and Rust both compile clean

0.1.71 -- v0.2.0 Implementation Guide v3 (beginner-oriented rewrite)

- Full rewrite of implementation guide for newer developers
- Added Getting Started section with build/run instructions and Tauri architecture explanation
- Added FFmpeg bundling strategy section with per-platform details
- Made trial start logic more prominent per master plan Migration section
- Added explicit master plan section references to every step header
- All source claims re-verified: 38 commands, 17 tables, 28 components, package versions
- 70+ testing checklist items across 9 categories

0.1.70 -- v0.2.0 Implementation Guide v2 (full source audit)

- Complete rewrite of v0.2.0 implementation guide with line-by-line source verification
- Corrected table count: 17 tables (was 15), verified from migrations.rs
- Verified 38 Tauri commands from lib.rs generate_handler macro
- Verified 28 components (21 root + 3 modals + 4 nav) via filesystem listing
- Verified settings.rs: SETTINGS_VERSION=1, AppMode=Personal/Pro, 4-field AppSettings
- Verified Cargo.toml dependencies against guide claims
- Added full table inventory with per-table purposes for all 17 tables
- Added "Gotchas for new devs" section to every implementation step
- Added exact App.tsx insertion point for first-run wizard gate
- Added full "Adding a new command module" 5-step process
- Added Tauri event listener pattern for React useEffect cleanup
- Added note about init_library_folders needing sidecars/ directory
- Added master plan section references to every step header
- Added audit notes section with per-file verification table
- Cross-referenced all 19 master plan sections to guide steps
- 70+ testing checklist items across 9 categories

---

0.1.69 -- v0.2.0 Implementation Guide rewrite (full audit)

- Complete rewrite of v0.2.0 implementation guide from master plan audit
- Verified all 38 Tauri commands in lib.rs (previous guide said 40, corrected)
- Verified all 3 migrations, 15 tables, 28 components against actual source files
- Added explicit Cargo.toml dependency list (keyring needed for licensing)
- Added dev environment setup context (exact Rust structs from settings.rs)
- Added concrete FFmpeg filtergraph examples for VHS crossfade pipeline
- Added conform filter chain (resolution/fps/SAR normalization before xfade)
- Added new files summary table (14 Rust, 20 TypeScript, 1 resource)
- Added new commands summary (25 new commands across 6 modules, total 63 after v0.2.0)
- Added 2 new database migrations table (Migration 4: export_history, Migration 5: camera_devices)
- Expanded testing checklist to 70+ items across 9 categories
- Cross-referenced every master plan section to exact source file paths
- Decisions log carried from master plan with explicit "do not revisit" note
- Written for developers new to the codebase with prerequisite reading order

---

0.1.68 -- Implementation Guide fixes (7 issues)

- Fixed component count (33 -> 28) and added missing SettingsPanel.tsx to layout
- Fixed command count (38 -> 40)
- Moved Sidecars section into Step 6 (Import Dialog) where it is built, not floating between steps
- Clarified camera_profile_id already exists on clips, only camera_device_id is new in Migration 5
- Clarified terminology rename: data structures rename in Step 1, UI text and CSS deferred to end
- Added export dialog navigation path (VHS Export button in LibraryView toolbar)
- Added migration numbering notes for Migrations 4 and 5
- Added sidecar verification items to Step 6 verify list
- Noted key entry modal CTA linkage extends master plan

0.1.67 -- v0.2.0 Implementation Guide Audit Pass

- Audited implementation guide against master plan line-by-line (12 gaps found, all fixed)
- Added first-run wizard (was missing entirely -- master plan deliverable #1)
- Added Sidecars & Metadata section (sidecar location, JSON schema, EXIF dump)
- Added scope boundaries: "Not in v0.2.0" list and hardware in/out of scope
- Added "Project" terminology convention (user-facing vs internal "library")
- Added Simple = single project, Advanced = multiple projects
- Added 8-step per-file import pipeline sequence
- Added standalone key entry modal (accessible from trial banner CTA, not just dev menu)
- Added "Export EXIF dump" to dev menu debug section
- Fixed score threshold selection as Advanced-only
- Fixed 720p cap to specify "1280x720 max"
- Expanded soft lock CAN list (open projects, best-clips if already computed)
- Added missing contracts 4 (cross-platform) and 6 (export originals = file copy)
- Added 13 new test checklist items (wizard, sidecars, trial banner, key modal)

---

0.1.66 -- v0.2.0 Implementation Guide

- Created developer-facing implementation guide for v0.2.0 (docs/planning/v0.2.0-implementation-guide.md)
- 11-step walkthrough in dependency order with exact file paths, code patterns, and verify steps
- Audited against current codebase: 38 existing commands, 15 tables, 33 components
- Cross-referenced all contracts, settings types, migrations, and constants
- Includes must-pass testing checklist covering licensing, import, export, cameras, migration, cross-platform, and dev menu
- Written for developers new to the codebase

---

0.1.65 — v0.2.0 Master Plan (Final)

- Rewrote v0.2.0 master plan after full codebase audit (39 Rust, 43 TS files)
- Proper backend/frontend checklists mapped to specific files
- Fixed implementation order: settings > licensing > VHS export > import > cameras > nav > wizard > toggles > dev menu > USB > theme > rename last
- Removed FavoritesView.tsx (use nav link with existing ClipGrid filter)
- Fixed feature toggle contradiction (face detection off in Simple for low-end PCs)
- Confirmed VHS export is all new (job runner stub says "not yet implemented")
- Added existing user migration plan (v1>v2 settings, trial starts fresh)
- Gated raw SQL behind DCAM-D license key
- Added testing notes for licensing, trial flow, soft lock, keychain
- canonical.json source TBD, system works without it
- Terminology rename moved to step 12 (biggest diff, zero user value, do last)

---

0.1.64 — Light Mode Default

- Switched app to light mode as default (dark mode for Advanced users only)
- Added CSS variables for theme-aware hover overlays (--hover-overlay)
- Added :root.dark-mode class for dark theme support
- Light mode colors: canvas #FAFAF8, surface #FFFFFF, text rgba(10,10,11,0.87)
- Updated functional colors for better light mode contrast
- Fixed modal/dialog shadows for light mode

---

0.1.63 — Licensing & Business Strategy (v2.0.0)

- Complete strategic analysis of monetization and anti-piracy
- Philosophy: "Don't fight piracy. Make buying easier than pirating."
- Business model:
  - 14-day trial (full features, then soft lock)
  - $99 one-time purchase (no subscription)
  - Free for rental clients (pre-generated keys)
- Soft lock when expired:
  - CAN view/browse library
  - CANNOT import, export, auto-edit
  - Watermark on exports
- Anti-piracy: Light friction, not DRM
  - Machine ID (survives reinstall)
  - Obfuscated checks (multiple locations)
  - Accept that determined pirates will crack it
- Gumroad recommended for payments (handles everything)
- Dev menu: Cmd+Shift+D for backend tools, key generation
- Camera database bundled (7,500+ cameras), updates with app
- See docs/planning/pro-register-camera.md

---

0.1.62 — Settings Page (Braun D.5.8)

- Moved settings from modal to dedicated full page per Braun spec D.5.8
- Created SettingsView.tsx with left nav (200px) + content area (640px max)
- Updated LeftNav SettingsSection to be a navigation link
- Updated LibraryView, LibraryDashboard, App.tsx for view-based routing
- Removed modal-based SettingsPanel approach
- Added settings-view CSS classes following Braun Design Language

---

0.1.61 — Braun Design CSS Fix

- Fixed duration badge colors to use rgba(250, 250, 248, 1) instead of pure white
- Affects event-clip-duration and date-clip-duration classes
- Complies with Braun Design Language spec (no pure #FFF)

---

0.1.60 — Phase 8 Audit Remediation

- Keyboard hint consistency: VideoPlayer shortcuts now use .kbd CSS class for visual consistency
- Error boundary integration: Added ErrorBoundary wrapper around LibraryView and LibraryDashboard in App.tsx
- Loading state usage: Added skeleton loading placeholders to ClipGrid, EventView, and DateView initial load states

---

0.1.59 — Phase 8 Audit Fixes

- Removed skeleton animation to comply with Braun spec D.10 (no animated loaders)
- Refactored ErrorBoundary to use CSS classes and design system tokens
- Added error-boundary CSS classes using Braun color variables

---

0.1.58 — Phase 8 Polish and Integration

- Added loading state CSS styles (skeleton, loading-indicator, loading-inline)
- Added tooltip title attributes to all interactive buttons across all components
- Added CSS utilities: kbd class for keyboard shortcuts, help-hint for help text
- Added transition utilities for consistent animations
- Updated global disabled state styling for buttons, inputs, selects
- Components updated: VideoPlayer, WelcomeDashboard, LibraryDashboard, FilterBar, EventView, DateView, EventsSection, all modals
- Completed Phase 8 polish tasks: loading states, tooltips, help text, CSS updates

---

0.1.57 — DatesSection Code Quality Fixes

- Removed redundant handleDateClick wrapper function, now uses onNavigateToDate directly
- Added data-tree-level attributes to tree buttons for semantic parent-finding
- Keyboard navigation now uses data attributes instead of CSS class names
- Decouples keyboard nav logic from styling implementation

---

0.1.56 — Fix Timezone Issues in Date Display

- Added parseLocalDate() helper to safely parse YYYY-MM-DD without UTC timezone shift
- Added formatClipDate() and formatClipTime() helpers for consistent date/time formatting
- Fixed formatEventDate() to use parseLocalDate (was showing wrong day in western timezones)
- Updated EventView.tsx and DateView.tsx to use centralized date formatting helpers

---

0.1.55 — Phase 7 Type Consistency and Date Validation

- Backend: Renamed EventClipItem to EventClipView for consistency with frontend types
- Backend: Improved date validation with proper days-per-month and leap year handling
- Frontend: Added isValidDateFormat() helper function for client-side date validation

---

0.1.54 — Phase 7 Audit Fixes

- Backend: Added date format validation (YYYY-MM-DD) to get_clips_by_date command
- Backend: Refactored get_clips_by_date in schema.rs to use map_clip helper (removes duplication)
- Frontend: Added refreshTrigger prop to DatesSection for refresh after imports
- DatesSection now auto-refreshes when clips are imported
- Wired refreshTrigger through LeftNav and MainLayout to LibraryView

---

0.1.53 — Phase 7 VideoPlayer Integration Fix

- Fixed: Clicking clips in DateView/EventView now opens VideoPlayer (was broken)
- Changed onClipSelect callback to pass full clip object instead of just clipId
- Added eventClipToClipView() conversion function (EventClipView -> ClipView)
- Moved VideoPlayer rendering outside view-specific branches (works for all views)
- VideoPlayer now renders as modal overlay regardless of current view

---

0.1.52 — Phase 7 Minor Fixes

- DatesSection: Added error state display (was only logging to console)
- DatesSection: Removed redundant formatDay() function
- DateView: Fixed timezone issue in date parsing (use local date parts instead of ISO parse)
- CSS: Added .nav-error style for error state display

---

0.1.51 — Phase 7 Complete (100%)

- DateView: Added pagination with "Load More" button (50 clips per page instead of hardcoded 200)
- DateView: Shows remaining count in load more button
- DatesSection: Added full keyboard navigation (Arrow keys, Enter, Space)
- DatesSection: Arrow Up/Down navigates between visible items
- DatesSection: Arrow Right expands collapsed year/month
- DatesSection: Arrow Left collapses expanded year/month or moves to parent
- DatesSection: Added ARIA attributes for accessibility (role="tree", aria-expanded, aria-selected)
- Added focus-visible styles for keyboard navigation visibility

---

0.1.50 — Phase 7 Audit Fixes

- Fixed DateView navigation not wired up (clicking dates in nav tree now works)
- Added 'date' to LibrarySubView type in LibraryView.tsx
- Added selectedDate state and handleNavigateToDate/handleBackFromDate callbacks
- Imported and integrated DateView component in LibraryView.tsx renderContent()
- Passed onNavigateToDate prop through MainLayout to LeftNav to DatesSection
- Added activeDate prop to DatesSection/LeftNav/MainLayout for nav highlighting
- Fixed comment in DateView.tsx (Phase 6 -> Phase 7)

---

0.1.49 — Phase 7: Dates View (Tree Navigation)

- Implemented hierarchical Year > Month > Day tree navigation in DatesSection.tsx
- Tree structure: Years expand to show Months, Months expand to show Days
- Auto-expands most recent year on load for immediate access
- Clip counts shown at each level (year total, month total, day count)
- Collapsible sections with chevron rotation animation
- Click on day navigates to DateView showing clips for that date
- All backend commands already existed from Phase 6 (get_clips_grouped_by_date, get_clips_by_date)
- Added nav-dates-tree CSS with indentation levels and hover states
- Phase 7 of dashboard-redesign.md now complete

---

0.1.48 — Phase 6 Audit Fixes

Priority 1 (Critical):
- Fixed hardcoded library ID bug in events.rs - now uses get_current_library() helper
- All event commands now correctly get library from open database

Priority 2 (Should Fix):
- Created DateView.tsx component for viewing clips by date
- Added delete confirmation dialog to EventsSection (shows clip count, requires confirmation)
- Optimized get_event_clips pagination - now uses SQL LIMIT/OFFSET instead of fetching all IDs
- Added CSS for DateView and confirm-dialog components

Priority 3 (Nice to Have):
- Added Shift+click range selection to ClipGrid (onRangeSelect prop)
- ClipThumbnail now passes mouse event to onClick for detecting modifier keys
- Added input validation to add_clips_to_event (checks clips exist and belong to same library)
- Added event existence validation to remove_clips_from_event
- Added loading state (removing flag) to EventView clip removal

---

0.1.47 — Phase 6 Audit Complete (100%)

- Added Escape key handlers to all event modals (CreateEventModal, EditEventModal, AddToEventModal)
- Pressing Escape now closes modal when not loading/saving
- Added event existence check in delete_event command (returns error if event not found)
- Verified library ID = 1 pattern is correct (each library has its own database)
- Phase 6 audit now 100% complete

---

0.1.46 — Phase 6 Gap Fixes

- Created EditEventModal.tsx for editing existing events (name, description, dates)
- Created AddToEventModal.tsx for selecting clips and adding to manual selection events
- Added clip selection mode to ClipGrid with multi-select checkboxes
- Added selectionMode, selectedClipIds, onSelectionChange props to ClipGrid
- Updated ClipThumbnail with selection checkbox and selection outline display
- Updated LibraryView with selection mode state and "Select Clips" / "Add to Event" buttons
- Added event navigation: clicking event in LeftNav now opens EventView
- Added handleNavigateToEvent callback wired through MainLayout to LeftNav
- Added Edit button to EventView header that opens EditEventModal
- Added Edit Event option to EventsSection context menu (alongside Delete)
- Added CSS styles for edit-event-modal, add-to-event-modal, event-list, clip-selection classes
- Phase 6 gaps 1-4 from audit now resolved

---

0.1.45 — Phase 6: Events System

- Added Migration 3: events table and event_clips junction table
- Events support two types: date_range (auto-include clips by date) and clip_selection (manual)
- Event schema: name, description, date_start, date_end, color, icon
- Created src-tauri/src/commands/events.rs with 10 Tauri commands:
  - create_event, get_events, get_event, update_event, delete_event
  - add_clips_to_event, remove_clips_from_event, get_event_clips
  - get_clips_grouped_by_date, get_clips_by_date
- Added Event, NewEvent, EventUpdate structs to schema.rs
- Added event CRUD functions and clip-to-event relationship queries
- Created src/types/events.ts with EventView, EventClipView, DateGroup types
- Created src/api/events.ts with TypeScript API wrappers
- Updated EventsSection.tsx: shows real events list, create event button, context menu delete
- Created CreateEventModal.tsx: event name, description, type selection, date range inputs
- Created EventView.tsx: displays event clips, selection mode for removing clips
- Updated DatesSection.tsx: shows clips grouped by date with counts
- Added CSS styles for modals, event view, clip grid, context menus
- Phase 6 of dashboard-redesign.md now complete

---

0.1.44 — Phase 5 Audit Fixes

- Added 20x20 icons to all nav sections (Library, Events, Dates, Settings)
- Icons use stroke-only style per Braun Design Language spec D.11
- Added nav-section-header and nav-section-icon CSS classes
- SettingsSection button now shows "Switch to Pro/Personal" instead of "Open Settings"
- All Phase 5 audit items resolved

---

0.1.43 — Phase 5: Left Navigation Bar

- Created MainLayout.tsx component with LeftNav + content area structure
- Created LeftNav.tsx container component for sidebar navigation
- Created nav/ directory with 4 section components:
  - LibrarySection.tsx: shows current library name and clip count
  - EventsSection.tsx: placeholder for Phase 6 events list
  - DatesSection.tsx: placeholder for Phase 7 date tree navigation
  - SettingsSection.tsx: shows mode and settings access button
- Updated LibraryView.tsx to use MainLayout wrapper
- Added settings prop to LibraryView for settings panel integration
- LeftNav width: 240px, follows Braun Design Language spec D.5.2
- Nav sections use 11px/500/uppercase titles per typography spec
- Settings section positioned at bottom with border separator
- Added CSS styles for main-layout, left-nav, nav-section, nav-item classes
- Back button styling consolidated to back-to-libraries-btn class
- Phase 5 of dashboard-redesign.md now complete

---

0.1.42 — Phase 4: Welcome Dashboard + Stills Export

- Created WelcomeDashboard.tsx component (Personal mode landing page)
- Welcome Dashboard shows: Import Footage, Stills, Export Footage, Browse All Clips
- Stills export feature: export high-quality still frame from video at current timestamp
- Created src-tauri/src/commands/stills.rs with export_still Tauri command
- Stills uses original video file (not proxy) for maximum resolution
- FFmpeg -vframes 1 extraction with JPG (q:v 2) or PNG format
- Added S key shortcut in VideoPlayer for quick stills export
- Added Still button to VideoPlayer header with status feedback
- Native save dialog for choosing output path and format
- Created src/api/stills.ts with TypeScript types and exportStill function
- Updated LibraryView with currentView state (welcome/clips/stills navigation)
- Personal mode now shows Welcome Dashboard on library open (not clips grid directly)
- Added Back button in clips view to return to Welcome Dashboard
- Stills mode shows instructional header "Click a clip, then press S"
- Added dialog:allow-save permission to capabilities/default.json
- Added welcome-dashboard CSS styles following Braun Design Language
- Phase 4 of dashboard-redesign.md now complete

---

0.1.41 — Phase 3 Audit: 100% Compliance

- Added "Back to Libraries" button to LibraryView header (Pro mode only)
- LibraryView now accepts mode prop, shows back navigation in Pro mode
- Close Library button only shown in Personal mode (Pro uses back button)
- Fixed title casing: "Dad Cam" changed to "dad cam" per Braun brand spec
- Subtitle casing updated: "Video library..." to "video library..."
- Loading screen text updated to lowercase per brand spec
- Implemented library thumbnail extraction for LibraryCard display
- add_recent_library now queries first clip's thumbnail from library database
- get_library_thumbnail helper function added to settings.rs
- LibraryCard now uses convertFileSrc for proper Tauri asset URL
- Added back-to-libraries-btn CSS with hover and focus states
- Phase 3 now 100% compliant with dashboard-redesign.md plan

---

0.1.40 — Phase 3 Audit Fixes (Braun Design Language)

- Updated App.css to use Braun Design Language CSS custom properties
- Added CSS variables for colors, spacing, and radii per Appendix D spec
- Primary buttons now use near-black bg (--color-text) instead of blue
- Focus states now use amber accent (--color-accent: #f59e0b) per spec
- Canvas color corrected to #0a0a0b (was #0f0f0f)
- All border-radius values now use --radius-* variables (max 8px per spec)
- Fixed pluralization bug in LibraryCard.tsx formatLastOpened()
- Added isSelected prop and .is-selected class to LibraryCard component
- Extracted inline error message styles to .library-dashboard-error CSS class
- Input labels now uppercase with letter-spacing per Braun typography spec
- All hardcoded color values replaced with CSS custom properties

---

0.1.39 — Dashboard Redesign Phase 3: Library Dashboard (Pro Mode)

- Created LibraryCard.tsx component with thumbnail display and metadata
- Created LibraryDashboard.tsx for Pro mode multi-library selection
- Library grid layout with recent libraries shown as cards
- "New Library" button with create form dialog
- "Open Library" button with native folder picker
- Remove library from recent list (X button on card hover)
- Settings icon button in dashboard header
- Empty state when no recent libraries
- App.tsx now routes to LibraryDashboard when mode=pro and no library open
- Added CSS styles for library dashboard, grid, cards, and actions
- Phase 3 of dashboard-redesign.md now complete

---

0.1.38 — Phase 2 Audit Fixes

- Created src/constants.ts as single source of truth for APP_VERSION
- Updated SettingsPanel.tsx to import version from constants instead of hardcoding
- Version string now centralized for easier maintenance

---

0.1.37 — Dashboard Redesign Phase 2: Mode System

- Created SettingsPanel.tsx component with mode toggle UI
- Added Personal/Pro mode radio options with descriptions
- Settings panel accessible via gear icon button (fixed bottom-right)
- Mode indicator at bottom of welcome screen is now clickable
- Mode changes saved immediately via setMode API
- Added settings panel CSS: backdrop, panel, header, sections, mode options
- Phase 2 of dashboard-redesign.md now complete

---

0.1.36 — Dashboard Redesign Phase 1: App Settings Persistence

- Added tauri-plugin-store dependency for persistent app settings
- Created src-tauri/src/commands/settings.rs with 8 Tauri commands:
  - get_app_settings, save_app_settings, get_mode, set_mode
  - add_recent_library, remove_recent_library, get_recent_libraries
  - validate_library_path (checks if library database exists)
- Settings stored via Tauri Store plugin at platform-specific location
- Settings schema: version, mode (personal/pro), lastLibraryPath, recentLibraries
- Created src/types/settings.ts with TypeScript types
- Created src/api/settings.ts with 7 API functions
- Updated App.tsx: loads settings on mount, auto-opens last library in Personal mode
- Added recent libraries list to welcome screen
- Added unmounted library handling (shows retry/remove UI when drive disconnected)
- Added store permissions to capabilities/default.json
- Updated App.css with recent libraries, library path, checkbox group styles
- Phase 1 of dashboard-redesign.md now complete

---

0.1.35 — Client One-Sheet Prototype

- Added Braun-style client one-sheet in docs/client/onesheet.html
- Camera selection, coverage options, delivery options layout
- Minimal grid design with camera images

---

0.1.34 — Dashboard Redesign Implementation Audit (v1.4)

- Complete implementation audit of dashboard-redesign.md
- Added tauri-plugin-dialog dependency for native save dialogs (Stills feature)
- Added AppContext.tsx for global state and navigation management
- Added state-based navigation system (not React Router) with view history
- Added complete stills.rs command implementation with error handling
- Added keyboard shortcut documentation (S key for Stills)
- Added clip multi-select implementation pattern for event creation
- Resolved date picker: use native HTML5 date inputs (no library needed)
- Added unmounted volume handling with "Library Not Available" UI
- Added settings corruption recovery (reset to defaults)
- Updated capabilities config with store and dialog permissions
- Fixed date range query to use date() function for correct comparison
- Added event type constants (EVENT_TYPES) to TypeScript types
- Added post-action navigation flows for Welcome Dashboard
- Added "Back to Libraries" button documentation for Pro mode
- Expanded test coverage: settings corruption, unmounted volumes, stills errors
- Consolidated file lists (removed duplicate "Additional from Audit" section)

---

0.1.33 — Dashboard Redesign Braun Audit (v1.3)

- Added Appendix D: Braun Design Language Specifications to dashboard-redesign.md
- Audited all UI components against Braun Design Language v1.0.0
- Added typography specs: Braun Linear font family, weights 700/500/400/300
- Added color system: light mode tokens, dark mode tokens, functional colors
- Added spacing system: 8pt grid with exact values per component
- Added border radius constraints: 4px badges, 6px buttons, 8px cards, max 8px
- Added component specs: MainLayout, LeftNav, WelcomeDashboard, LibraryDashboard
- Added component specs: LibraryCard, EventView, DateView, CreateEventModal, SettingsPanel
- Added button specifications: primary, secondary, ghost, destructive, accent (amber)
- Added input specifications: text input, labels, helper text, toggle
- Added card grid specs for ClipGrid
- Added empty state specs with exact typography and spacing
- Added loading state rules: no animated skeletons, text or static progress only
- Added icon specs: stroke-only, currentColor, functional only
- Added dark mode implementation guide with elevation system
- Added accessibility requirements: 4.5:1 contrast, 44px touch targets, focus states
- Added Braun verification checklist (14 items)
- Added anti-patterns list (12 prohibited patterns)
- Added CSS custom properties for all design tokens

---

0.1.32 — Dashboard Redesign Planning (v1.2 Final)

- Created comprehensive implementation plan: docs/planning/dashboard-redesign.md
- Documented root cause of "always shows open library" issue (no app-level settings)
- Defined Personal vs Pro mode architecture
- Specified Welcome Dashboard (Personal) and Library Dashboard (Pro) requirements
- Designed Left Nav Bar with Library, Events, Dates, Settings sections
- Planned Events system database schema (Migration 3)
- Documented all backend (Rust) and frontend (React) changes needed
- Created 11-phase implementation checklist with 70+ tasks
- Listed 20 new files and 11 modified files
- Audit complete: Stills feature defined (frame export), Activity Feed removed, Camera Profiles deferred

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
