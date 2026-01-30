// File copy operations for ingest
// Gold-standard: streaming BLAKE3 hash + temp file + atomic rename + read-back verification

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use chrono::{Datelike, Utc};
use crate::constants::{COPY_CHUNK_SIZE, TEMP_FILE_PREFIX};
use crate::error::{DadCamError, Result};

/// Copy a file to the library originals folder with date-based organization.
/// Returns (relative_path, source_hash) where source_hash is the full BLAKE3 hash.
pub fn copy_file_to_library(source: &Path, originals_dir: &Path) -> Result<(PathBuf, String)> {
    // Get file modified date for organization
    let modified = fs::metadata(source)
        .and_then(|m| m.modified())
        .ok();

    let date_folder = if let Some(modified) = modified {
        let datetime: chrono::DateTime<Utc> = modified.into();
        format!("{}/{:02}", datetime.year(), datetime.month())
    } else {
        "unknown".to_string()
    };

    // Create destination directory
    let dest_dir = originals_dir.join(&date_folder);
    fs::create_dir_all(&dest_dir)?;

    // Generate unique filename
    let filename = source
        .file_name()
        .ok_or_else(|| DadCamError::InvalidPath("No filename".to_string()))?;

    let mut dest_path = dest_dir.join(filename);

    // Handle filename conflicts
    if dest_path.exists() {
        dest_path = generate_unique_path(&dest_path)?;
    }

    // Copy file with streaming verification
    let source_hash = copy_with_verify(source, &dest_path)?;

    // Return relative path from library root
    let relative = dest_path
        .strip_prefix(originals_dir.parent().unwrap_or(originals_dir))
        .unwrap_or(&dest_path);

    Ok((relative.to_path_buf(), source_hash))
}

/// Generate a unique path by appending a number
fn generate_unique_path(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    for i in 1..1000 {
        let new_name = if ext.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, ext)
        };
        let new_path = parent.join(new_name);
        if !new_path.exists() {
            return Ok(new_path);
        }
    }

    Err(DadCamError::Other("Could not generate unique filename".to_string()))
}

/// Streaming copy with concurrent BLAKE3 hashing + temp file + atomic rename + read-back.
///
/// Algorithm (per importplan section 4.2):
/// 1. Open source file
/// 2. Create temp dest on SAME filesystem as final path
/// 3. Stream loop: read chunk -> update BLAKE3 hasher -> write chunk to temp
/// 4. fsync(temp file), close
/// 5. Source hash = finalize hasher -> "blake3:full:<hex>"
/// 6. Read temp back from disk streaming -> compute dest hash
/// 7. Compare source_hash == dest_hash
///    - Mismatch: delete temp, return error
///    - Match: atomic rename(temp, dest), fsync(dest parent dir)
/// 8. Preserve mtime
/// 9. Return source_hash
///
/// Memory: never more than 1 chunk (1MB) in RAM.
fn copy_with_verify(source: &Path, dest: &Path) -> Result<String> {
    // Step 1: Open source
    let mut source_file = fs::File::open(source)?;

    // Step 2: Create temp file on same filesystem
    let dest_parent = dest.parent().unwrap_or(Path::new("."));
    let temp_name = format!("{}{}", TEMP_FILE_PREFIX, uuid::Uuid::new_v4());
    let temp_path = dest_parent.join(&temp_name);

    // Step 3: Streaming copy with hash
    let source_hash = {
        let mut temp_file = fs::File::create(&temp_path)
            .map_err(|e| {
                DadCamError::Ingest(format!("Failed to create temp file {}: {}", temp_path.display(), e))
            })?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; COPY_CHUNK_SIZE];

        loop {
            let bytes_read = source_file.read(&mut buffer)
                .map_err(|e| {
                    let _ = fs::remove_file(&temp_path);
                    DadCamError::Ingest(format!("Failed to read source: {}", e))
                })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            temp_file.write_all(&buffer[..bytes_read])
                .map_err(|e| {
                    let _ = fs::remove_file(&temp_path);
                    DadCamError::Ingest(format!("Failed to write temp file: {}", e))
                })?;
        }

        // Step 4: fsync temp file
        temp_file.sync_all()
            .map_err(|e| {
                let _ = fs::remove_file(&temp_path);
                DadCamError::Ingest(format!("Failed to fsync temp file: {}", e))
            })?;

        // Step 5: Finalize source hash
        let hash = hasher.finalize();
        format!("blake3:full:{}", hash.to_hex())
    };

    // Step 6: Read-back verification -- stream temp file and compute dest hash
    let dest_hash = {
        let mut readback = fs::File::open(&temp_path)
            .map_err(|e| {
                let _ = fs::remove_file(&temp_path);
                DadCamError::Ingest(format!("Failed to open temp for readback: {}", e))
            })?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; COPY_CHUNK_SIZE];

        loop {
            let bytes_read = readback.read(&mut buffer)
                .map_err(|e| {
                    let _ = fs::remove_file(&temp_path);
                    DadCamError::Ingest(format!("Failed to read temp for verification: {}", e))
                })?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        format!("blake3:full:{}", hash.to_hex())
    };

    // Step 7: Compare
    if source_hash != dest_hash {
        let _ = fs::remove_file(&temp_path);
        return Err(DadCamError::Ingest(format!(
            "Read-back verification failed: source={} dest={}",
            source_hash, dest_hash
        )));
    }

    // Step 7b: Atomic rename temp -> final
    fs::rename(&temp_path, dest)
        .map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            DadCamError::Ingest(format!("Failed to rename temp to final: {}", e))
        })?;

    // fsync parent directory to ensure directory entry is durable
    #[cfg(unix)]
    {
        if let Ok(dir) = fs::File::open(dest_parent) {
            let _ = dir.sync_all();
        }
    }

    // Step 8: Preserve mtime
    if let Ok(source_meta) = fs::metadata(source) {
        if let Ok(modified) = source_meta.modified() {
            let _ = filetime::set_file_mtime(dest, filetime::FileTime::from_system_time(modified));
        }
    }

    // Step 9: Return source hash
    Ok(source_hash)
}

/// Copy an entire directory structure (for AVCHD preservation)
pub fn copy_directory_structure(source_dir: &Path, dest_dir: &Path) -> Result<()> {
    if !source_dir.is_dir() {
        return Err(DadCamError::InvalidPath("Source is not a directory".to_string()));
    }

    fs::create_dir_all(dest_dir)?;

    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest_dir.join(entry.file_name());

        if source_path.is_dir() {
            copy_directory_structure(&source_path, &dest_path)?;
        } else {
            // Directory structure copy does not need hash -- just verified copy
            copy_with_verify(&source_path, &dest_path)?;
        }
    }

    Ok(())
}
