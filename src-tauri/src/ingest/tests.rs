// --- Section 10 tests (importplan.md) ---

use super::super::*;
use crate::db::schema;
use std::io::Write as IoWrite;
use std::path::Path;
use tempfile::TempDir;

/// Set up an in-memory DB with all migrations applied and a library record.
/// Returns (conn, library_id).
fn setup_test_db() -> (rusqlite::Connection, i64) {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    crate::db::migrations::run_migrations(&conn).unwrap();
    let lib_id = schema::insert_library(&conn, "/test/lib", "TestLib", "copy").unwrap();
    (conn, lib_id)
}

/// Create a source directory with N video files of known content.
/// Returns (source_dir, Vec<(filename, content_bytes)>).
fn create_source_files(dir: &Path, files: &[(&str, &[u8])]) {
    std::fs::create_dir_all(dir).unwrap();
    for (name, content) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
    }
}

// ---------------------------------------------------------------
// Test 1: Integrity -- corrupt dest after copy, read-back must fail
// ---------------------------------------------------------------
#[test]
fn test_readback_detects_corruption() {
    // copy_with_verify writes to temp, reads it back, compares hashes.
    // We can't corrupt *during* copy_with_verify (it's atomic inside the fn),
    // so instead we test copy_file_to_library with a valid file to confirm
    // the happy path, then directly call the internal verify mechanism:
    // write a temp file, corrupt it, and confirm hash mismatch is detected.

    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("source");
    let originals_dir = tmp.path().join("originals");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&originals_dir).unwrap();

    // Create a source file with known content
    let content = b"This is test video content for integrity check. Repeating data to exceed trivial size.";
    let source_file = source_dir.join("test_clip.mp4");
    {
        let mut f = std::fs::File::create(&source_file).unwrap();
        f.write_all(content).unwrap();
    }

    // Happy path: copy succeeds and returns a valid hash
    let (rel_path, source_hash) = copy::copy_file_to_library(&source_file, &originals_dir).unwrap();
    assert!(source_hash.starts_with("blake3:full:"), "Hash should be blake3:full: prefixed");

    // Verify the dest file matches
    let dest_full = originals_dir.parent().unwrap_or(&originals_dir).join(&rel_path);
    assert!(dest_full.exists(), "Dest file should exist after copy");
    let dest_hash = crate::hash::compute_full_hash(&dest_full).unwrap();
    assert_eq!(source_hash, dest_hash, "Source and dest hash should match after copy");

    // Now corrupt the dest file (flip a byte) and verify hash no longer matches
    {
        let mut bytes = std::fs::read(&dest_full).unwrap();
        assert!(!bytes.is_empty());
        bytes[0] ^= 0xFF; // flip first byte
        std::fs::write(&dest_full, &bytes).unwrap();
    }
    let corrupted_hash = crate::hash::compute_full_hash(&dest_full).unwrap();
    assert_ne!(source_hash, corrupted_hash, "Corrupted file must produce different hash");
    assert_ne!(dest_hash, corrupted_hash, "Corrupted file must not match original dest hash");

    // Verify via verify_hash reports mismatch
    let matches = crate::hash::verify_hash(&dest_full, &source_hash).unwrap();
    assert!(!matches, "verify_hash must report mismatch on corrupted file");
}

// ---------------------------------------------------------------
// Test 2: Crash safety -- only temp file exists mid-copy;
//         no final file until verification passes
// ---------------------------------------------------------------
#[test]
fn test_crash_safety_temp_file_pattern() {
    // Verify that the copy function uses a temp file prefix and that
    // on failure, the final path does not exist.

    let tmp = TempDir::new().unwrap();
    let originals_dir = tmp.path().join("originals");
    std::fs::create_dir_all(&originals_dir).unwrap();

    // Create a source file
    let source_file = tmp.path().join("source_clip.mp4");
    {
        let mut f = std::fs::File::create(&source_file).unwrap();
        f.write_all(b"crash safety test content padding data").unwrap();
    }

    // Copy should succeed -- verify no temp files remain
    let (rel_path, _hash) = copy::copy_file_to_library(&source_file, &originals_dir).unwrap();
    let dest_full = originals_dir.parent().unwrap_or(&originals_dir).join(&rel_path);
    assert!(dest_full.exists(), "Final file should exist after successful copy");

    // Verify no temp files left behind in the destination directory
    let dest_parent = dest_full.parent().unwrap();
    for entry in std::fs::read_dir(dest_parent).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !name.starts_with(crate::constants::TEMP_FILE_PREFIX),
            "No temp files should remain after successful copy, found: {}",
            name
        );
    }

    // Now test that copying a nonexistent source fails and leaves no final file
    let missing_source = tmp.path().join("does_not_exist.mp4");
    let result = copy::copy_file_to_library(&missing_source, &originals_dir);
    assert!(result.is_err(), "Copying nonexistent file should fail");

    // The dest directory should have exactly 1 file (the successful copy)
    let file_count = std::fs::read_dir(dest_parent)
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().is_file())
        .count();
    assert_eq!(file_count, 1, "Only the one successful copy should exist -- no orphan files");
}

// ---------------------------------------------------------------
// Test 3: Dedup correctness -- two files that share first/last MB
//         + size cause fast-hash collision; full hash resolves
// ---------------------------------------------------------------
#[test]
fn test_dedup_fast_hash_collision_resolved_by_full_hash() {
    let (conn, library_id) = setup_test_db();
    let tmp = TempDir::new().unwrap();

    // Create two files with identical first 1MB, last 1MB, and size,
    // but different middle content.
    // For fast_hash (first_last_size_v1): first 1MB + last 1MB + size.
    // If file <= 2MB, entire content is hashed. So make files > 2MB.
    let size = 2 * 1024 * 1024 + 1024; // 2MB + 1KB
    let mut content_a = vec![0xAAu8; size];
    let mut content_b = vec![0xAAu8; size];
    // Differ only in the middle (outside first/last 1MB windows)
    let mid = size / 2;
    content_a[mid] = 0x01;
    content_b[mid] = 0x02;

    let file_a = tmp.path().join("file_a.mp4");
    let file_b = tmp.path().join("file_b.mp4");
    std::fs::write(&file_a, &content_a).unwrap();
    std::fs::write(&file_b, &content_b).unwrap();

    // Verify fast hashes collide
    let hash_fast_a = crate::hash::compute_fast_hash(&file_a).unwrap();
    let hash_fast_b = crate::hash::compute_fast_hash(&file_b).unwrap();
    assert_eq!(hash_fast_a, hash_fast_b, "Fast hashes should collide for files differing only in middle");

    // Verify full hashes differ
    let hash_full_a = crate::hash::compute_full_hash(&file_a).unwrap();
    let hash_full_b = crate::hash::compute_full_hash(&file_b).unwrap();
    assert_ne!(hash_full_a, hash_full_b, "Full hashes must differ for different file content");

    // Insert file_a as an existing asset with hash_fast and hash_full
    let asset_a_id = schema::insert_asset(&conn, &schema::NewAsset {
        library_id,
        asset_type: "original".to_string(),
        path: "originals/file_a.mp4".to_string(),
        source_uri: None,
        size_bytes: size as i64,
        hash_fast: Some(hash_fast_a.clone()),
        hash_fast_scheme: Some("first_last_size_v1".to_string()),
    }).unwrap();
    schema::update_asset_hash_full(&conn, asset_a_id, &hash_full_a).unwrap();

    // Now simulate dedup check for file_b:
    // find_asset_by_hash should find asset_a (fast hash match)
    let candidate = schema::find_asset_by_hash(&conn, library_id, &hash_fast_b).unwrap();
    assert!(candidate.is_some(), "Should find candidate via fast hash");
    let candidate = candidate.unwrap();
    assert_eq!(candidate.id, asset_a_id);

    // But full hash comparison must reject the dedup
    let source_full = crate::hash::compute_full_hash(&file_b).unwrap();
    assert_ne!(
        source_full,
        candidate.hash_full.unwrap(),
        "Full hash mismatch must prevent dedup -- these are different files"
    );
}

// ---------------------------------------------------------------
// Test 4: Completeness -- new file added after manifest must block
//         SAFE TO WIPE via rescan diff
// ---------------------------------------------------------------
#[test]
fn test_new_file_after_manifest_blocks_safe_to_wipe() {
    let (conn, _library_id) = setup_test_db();
    let tmp = TempDir::new().unwrap();
    let source_dir = tmp.path().join("sd_card");
    std::fs::create_dir_all(&source_dir).unwrap();

    // Create initial files that form the manifest baseline
    create_source_files(&source_dir, &[
        ("DCIM/clip001.mp4", b"video content 001"),
        ("DCIM/clip002.mp4", b"video content 002"),
    ]);

    // Create a fake job to hang the session off of (library_id from setup_test_db)
    let job_id = schema::insert_job(&conn, &schema::NewJob {
        job_type: "ingest".to_string(),
        library_id: Some(_library_id),
        clip_id: None,
        asset_id: None,
        priority: 10,
        payload: "{}".to_string(),
    }).unwrap();

    // Create session
    let session_id = schema::insert_ingest_session(&conn, &schema::NewIngestSession {
        job_id,
        source_root: source_dir.to_string_lossy().to_string(),
        device_serial: None,
        device_label: None,
        device_mount_point: None,
        device_capacity_bytes: None,
    }).unwrap();

    // Build manifest from the 2 files
    let files = discover::discover_media_files(&source_dir).unwrap();
    assert_eq!(files.len(), 2, "Should discover exactly 2 files");

    for file_path in &files {
        let relative = file_path.strip_prefix(&source_dir).unwrap().to_string_lossy().to_string();
        let meta = std::fs::metadata(file_path).unwrap();
        schema::insert_manifest_entry(&conn, &schema::NewManifestEntry {
            session_id,
            relative_path: relative,
            size_bytes: meta.len() as i64,
            mtime: None,
            entry_type: "media".to_string(),
            parent_entry_id: None,
        }).unwrap();
    }

    // Create a dummy asset so manifest entries can reference it
    let dummy_asset_id = schema::insert_asset(&conn, &schema::NewAsset {
        library_id: _library_id,
        asset_type: "original".to_string(),
        path: "originals/dummy.mp4".to_string(),
        source_uri: None,
        size_bytes: 100,
        hash_fast: None,
        hash_fast_scheme: None,
    }).unwrap();

    // Mark all manifest entries as copied_verified (simulate successful ingest)
    let entries = schema::get_manifest_entries(&conn, session_id).unwrap();
    assert_eq!(entries.len(), 2);
    for entry in &entries {
        schema::update_manifest_entry_result(
            &conn, entry.id, "copied_verified", Some("blake3:full:abc123"), Some(dummy_asset_id),
            None, None,
        ).unwrap();
    }

    // Run rescan BEFORE adding new file -- should be safe
    verification::run_rescan(&conn, session_id, &source_dir).unwrap();
    let session = schema::get_ingest_session(&conn, session_id).unwrap().unwrap();
    assert!(
        session.safe_to_wipe_at.is_some(),
        "Should be SAFE TO WIPE when manifest matches rescan exactly"
    );

    // Now add a NEW file to the source (simulating camera still writing)
    create_source_files(&source_dir, &[
        ("DCIM/clip003.mp4", b"video content 003 -- new file added after manifest"),
    ]);

    // Clear safe_to_wipe by resetting session (simulate re-check)
    conn.execute(
        "UPDATE ingest_sessions SET safe_to_wipe_at = NULL, rescan_hash = NULL WHERE id = ?1",
        rusqlite::params![session_id],
    ).unwrap();

    // Run rescan AFTER adding new file -- must NOT be safe
    verification::run_rescan(&conn, session_id, &source_dir).unwrap();
    let session = schema::get_ingest_session(&conn, session_id).unwrap().unwrap();
    assert!(
        session.safe_to_wipe_at.is_none(),
        "Must NOT be SAFE TO WIPE when new file appears on source after manifest"
    );
}
