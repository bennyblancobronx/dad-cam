// Sidecar manifest entry processing for gold-standard pipeline

use std::path::Path;
use rusqlite::Connection;

use crate::db::schema::{
    self, NewAsset, insert_asset, link_clip_asset,
    find_asset_by_hash, update_manifest_entry_result, update_manifest_entry_hash_fast,
    update_asset_hash_full, update_asset_verified_with_method,
};
use crate::hash::{compute_fast_hash, compute_full_hash};
use crate::constants::HASH_FAST_SCHEME;
use crate::error::{DadCamError, Result};

/// Process a single sidecar manifest entry through the gold-standard pipeline.
/// Same copy+verify algorithm as media files (sidecar-importplan section 4.2).
/// Does NOT create clips -- creates sidecar asset and links to parent clip.
pub(crate) fn process_sidecar_entry(
    conn: &Connection,
    library_id: i64,
    entry: &schema::ManifestEntry,
    source_root: &Path,
    originals_dir: &Path,
    ingest_mode: &str,
) -> Result<()> {
    let source_path = source_root.join(&entry.relative_path);

    // Step 1: Re-stat and compare to manifest baseline (change detection)
    let current_meta = std::fs::metadata(&source_path).map_err(|e| {
        DadCamError::Ingest(format!(
            "Sidecar file disappeared: {} ({})", entry.relative_path, e
        ))
    })?;
    let current_size = current_meta.len() as i64;
    let current_mtime = current_meta.modified().ok().map(|t| {
        let dt: chrono::DateTime<chrono::Utc> = t.into();
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    });

    if current_size != entry.size_bytes
        || (entry.mtime.is_some() && current_mtime != entry.mtime)
    {
        update_manifest_entry_result(
            conn, entry.id, "changed", None, None,
            Some("CHANGED_SINCE_MANIFEST"),
            Some(&format!("Sidecar changed: manifest={}/{:?} current={}/{:?}",
                entry.size_bytes, entry.mtime, current_size, current_mtime)),
        )?;
        return Err(DadCamError::Ingest(format!(
            "Sidecar file changed since manifest: {}", entry.relative_path
        )));
    }

    // Step 2: Compute fast hash for dedup candidate lookup
    let hash_fast = compute_fast_hash(&source_path)?;
    let _ = update_manifest_entry_hash_fast(conn, entry.id, &hash_fast);

    // Step 2b: Check for dedup against existing assets
    if let Some(existing) = find_asset_by_hash(conn, library_id, &hash_fast)? {
        if let Some(ref existing_full_hash) = existing.hash_full {
            let source_full_hash = compute_full_hash(&source_path)?;
            if source_full_hash == *existing_full_hash {
                // Dedup verified
                update_manifest_entry_result(
                    conn, entry.id, "dedup_verified",
                    Some(&source_full_hash), Some(existing.id),
                    None, None,
                )?;
                // Still link to parent clip if available
                link_sidecar_to_parent_clip(conn, entry, existing.id)?;
                return Ok(());
            }
        }
    }

    // Step 3: Copy with verification (same algorithm as media files)
    let (dest_path, source_uri, source_hash) = if ingest_mode == "copy" {
        let (dest, hash) = super::copy::copy_file_to_library(&source_path, originals_dir)?;
        (dest.to_string_lossy().to_string(), None, Some(hash))
    } else {
        let relative_path = format!("ref:{}", source_path.display());
        (relative_path, Some(source_path.to_string_lossy().to_string()), None)
    };

    // Step 4: Create sidecar asset record
    let asset = NewAsset {
        library_id,
        asset_type: "sidecar".to_string(),
        path: dest_path,
        source_uri,
        size_bytes: current_size,
        hash_fast: Some(hash_fast),
        hash_fast_scheme: Some(HASH_FAST_SCHEME.to_string()),
    };
    let asset_id = insert_asset(conn, &asset)?;

    // Step 5: Store full hash and mark verified
    if let Some(ref hash) = source_hash {
        update_asset_hash_full(conn, asset_id, hash)?;
        update_asset_verified_with_method(conn, asset_id, "copy_readback")?;
    }

    // Step 6: Update manifest entry
    update_manifest_entry_result(
        conn, entry.id, "copied_verified",
        source_hash.as_deref(), Some(asset_id),
        None, None,
    )?;

    // Step 7: Link sidecar asset to parent clip
    link_sidecar_to_parent_clip(conn, entry, asset_id)?;

    Ok(())
}

/// Link a sidecar asset to its parent media file's clip.
/// Uses parent_entry_id -> parent manifest entry -> asset_id -> clip lookup.
/// Orphan sidecars (parent_entry_id = NULL) are not linked to any clip.
fn link_sidecar_to_parent_clip(
    conn: &Connection,
    sidecar_entry: &schema::ManifestEntry,
    sidecar_asset_id: i64,
) -> Result<()> {
    let parent_id = match sidecar_entry.parent_entry_id {
        Some(id) => id,
        None => return Ok(()), // Orphan sidecar, no clip to link
    };

    // Get parent manifest entry's asset_id
    let parent_asset_id: Option<i64> = conn.query_row(
        "SELECT asset_id FROM ingest_manifest_entries WHERE id = ?1",
        rusqlite::params![parent_id],
        |row| row.get(0),
    ).unwrap_or(None);

    if let Some(asset_id) = parent_asset_id {
        if let Some(clip) = schema::get_clip_by_asset(conn, asset_id)? {
            link_clip_asset(conn, clip.id, sidecar_asset_id, "sidecar")?;
        }
    }

    Ok(())
}

/// DEPRECATED: Legacy sidecar ingest (pre-sidecar-importplan).
/// Kept for backward compatibility with pre-Migration-10 sessions.
/// New sessions use process_sidecar_entry() which follows the gold-standard pipeline.
#[allow(dead_code)]
pub(crate) fn ingest_sidecar(
    conn: &Connection,
    library_id: i64,
    clip_id: i64,
    sidecar_path: &Path,
    originals_dir: &Path,
    ingest_mode: &str,
) -> Result<i64> {
    let file_size = std::fs::metadata(sidecar_path)
        .map_err(|e| DadCamError::Io(e))?
        .len() as i64;

    // Copy or reference the sidecar
    let (dest_path, source_uri) = if ingest_mode == "copy" {
        let (dest, _hash) = super::copy::copy_file_to_library(sidecar_path, originals_dir)?;
        (dest.to_string_lossy().to_string(), None)
    } else {
        let relative_path = format!("ref:{}", sidecar_path.display());
        (relative_path, Some(sidecar_path.to_string_lossy().to_string()))
    };

    // Create sidecar asset record
    let asset = NewAsset {
        library_id,
        asset_type: "sidecar".to_string(),
        path: dest_path,
        source_uri,
        size_bytes: file_size,
        hash_fast: None, // Sidecars don't need dedup hashing
        hash_fast_scheme: None,
    };
    let asset_id = insert_asset(conn, &asset)?;

    // Link sidecar to clip with role="sidecar"
    link_clip_asset(conn, clip_id, asset_id, "sidecar")?;

    Ok(asset_id)
}
