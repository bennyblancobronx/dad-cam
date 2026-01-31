// Ingest job creation and manifest building

use std::path::Path;
use rusqlite::Connection;

use crate::db::schema::{
    NewJob, NewIngestSession, NewManifestEntry,
    insert_job, insert_ingest_file, insert_ingest_session, insert_manifest_entry,
    update_ingest_session_manifest_hash,
};
use crate::hash::compute_full_hash_from_bytes;
use crate::error::Result;
use super::IngestPayload;

/// Create an ingest job for a source path.
/// Now also creates an IngestSession + ManifestEntries for gold-standard verification.
pub fn create_ingest_job(conn: &Connection, library_id: i64, source_path: &str, ingest_mode: &str) -> Result<i64> {
    // Discover files
    let files = super::discover::discover_media_files(Path::new(source_path))?;

    // Create job first (session references job)
    let payload_initial = IngestPayload {
        source_path: source_path.to_string(),
        ingest_mode: ingest_mode.to_string(),
        session_id: None,
    };
    let job = NewJob {
        job_type: "ingest".to_string(),
        library_id: Some(library_id),
        clip_id: None,
        asset_id: None,
        priority: 10,
        payload: serde_json::to_string(&payload_initial)?,
    };
    let job_id = insert_job(conn, &job)?;

    // Create ingest_files records (legacy table, kept for backward compat)
    for file_path in &files {
        insert_ingest_file(conn, job_id, &file_path.to_string_lossy())?;
    }

    // Create IngestSession
    let volume_info = if !files.is_empty() {
        super::discover::get_volume_info(Path::new(source_path))
    } else {
        super::discover::VolumeInfo { serial: None, label: None, mount_point: None }
    };

    let session = NewIngestSession {
        job_id,
        source_root: source_path.to_string(),
        device_serial: volume_info.serial.clone(),
        device_label: volume_info.label.clone(),
        device_mount_point: volume_info.mount_point.clone(),
        device_capacity_bytes: None, // Could be populated if we stat the mount point
    };
    let session_id = insert_ingest_session(conn, &session)?;

    // Create manifest entries for each discovered file (media + sidecars)
    let source_root = Path::new(source_path);
    let mut manifest_data = Vec::new(); // for computing manifest_hash

    // Helper: stat a file and return (size_bytes, mtime_string)
    fn stat_for_manifest(file_path: &Path) -> (i64, Option<String>) {
        let meta = std::fs::metadata(file_path).ok();
        let size_bytes = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
        let mtime = meta.as_ref()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
            });
        (size_bytes, mtime)
    }

    // Phase 1: Insert media entries (these get lower IDs, processed first)
    // Track media_path -> manifest_entry_id for sidecar parent linking
    let mut media_entry_ids: std::collections::HashMap<std::path::PathBuf, i64> = std::collections::HashMap::new();

    for file_path in &files {
        let relative_path = file_path
            .strip_prefix(source_root)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string();

        let (size_bytes, mtime) = stat_for_manifest(file_path);

        let entry_id = insert_manifest_entry(conn, &NewManifestEntry {
            session_id,
            relative_path: relative_path.clone(),
            size_bytes,
            mtime: mtime.clone(),
            entry_type: "media".to_string(),
            parent_entry_id: None,
        })?;

        media_entry_ids.insert(file_path.clone(), entry_id);

        manifest_data.push(format!("{}|{}|{}", relative_path, size_bytes, mtime.as_deref().unwrap_or("")));
    }

    // Phase 2: Discover sidecars and insert as manifest entries
    let (paired_sidecars, orphan_sidecars) = super::discover::discover_all_sidecars(source_root, &files);

    // Insert paired sidecars (linked to parent media entry)
    for (media_path, sidecar_paths) in &paired_sidecars {
        let parent_entry_id = media_entry_ids.get(media_path).copied();
        for sidecar_path in sidecar_paths {
            let relative_path = sidecar_path
                .strip_prefix(source_root)
                .unwrap_or(sidecar_path)
                .to_string_lossy()
                .to_string();

            let (size_bytes, mtime) = stat_for_manifest(sidecar_path);

            insert_manifest_entry(conn, &NewManifestEntry {
                session_id,
                relative_path: relative_path.clone(),
                size_bytes,
                mtime: mtime.clone(),
                entry_type: "sidecar".to_string(),
                parent_entry_id,
            })?;

            manifest_data.push(format!("{}|{}|{}", relative_path, size_bytes, mtime.as_deref().unwrap_or("")));
        }
    }

    // Insert orphan sidecars (no parent media)
    for sidecar_path in &orphan_sidecars {
        let relative_path = sidecar_path
            .strip_prefix(source_root)
            .unwrap_or(sidecar_path)
            .to_string_lossy()
            .to_string();

        let (size_bytes, mtime) = stat_for_manifest(sidecar_path);

        insert_manifest_entry(conn, &NewManifestEntry {
            session_id,
            relative_path: relative_path.clone(),
            size_bytes,
            mtime: mtime.clone(),
            entry_type: "sidecar".to_string(),
            parent_entry_id: None,
        })?;

        manifest_data.push(format!("{}|{}|{}", relative_path, size_bytes, mtime.as_deref().unwrap_or("")));
    }

    // Compute manifest_hash = BLAKE3 of sorted manifest entries (media + sidecars)
    manifest_data.sort();
    let manifest_blob = manifest_data.join("\n");
    let manifest_hash = compute_full_hash_from_bytes(manifest_blob.as_bytes());
    update_ingest_session_manifest_hash(conn, session_id, &manifest_hash)?;

    // Update job payload with session_id
    let payload_final = IngestPayload {
        source_path: source_path.to_string(),
        ingest_mode: ingest_mode.to_string(),
        session_id: Some(session_id),
    };
    conn.execute(
        "UPDATE jobs SET payload = ?1 WHERE id = ?2",
        rusqlite::params![serde_json::to_string(&payload_final)?, job_id],
    )?;

    Ok(job_id)
}
