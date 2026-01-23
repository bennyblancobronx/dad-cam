// File copy operations for ingest

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use chrono::{Datelike, Utc};
use crate::error::{DadCamError, Result};

/// Copy a file to the library originals folder with date-based organization
pub fn copy_file_to_library(source: &Path, originals_dir: &Path) -> Result<PathBuf> {
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

    // Copy file with verification
    copy_with_verify(source, &dest_path)?;

    // Return relative path from library root
    let relative = dest_path
        .strip_prefix(originals_dir.parent().unwrap_or(originals_dir))
        .unwrap_or(&dest_path);

    Ok(relative.to_path_buf())
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

/// Copy file with read-back verification
fn copy_with_verify(source: &Path, dest: &Path) -> Result<()> {
    // Read source file
    let mut source_file = fs::File::open(source)?;
    let mut buffer = Vec::new();
    source_file.read_to_end(&mut buffer)?;

    // Write to destination
    let mut dest_file = fs::File::create(dest)?;
    dest_file.write_all(&buffer)?;
    dest_file.sync_all()?;

    // Verify by checking size
    let source_size = fs::metadata(source)?.len();
    let dest_size = fs::metadata(dest)?.len();

    if source_size != dest_size {
        // Remove failed copy
        let _ = fs::remove_file(dest);
        return Err(DadCamError::Ingest(format!(
            "Verification failed: size mismatch ({} vs {})",
            source_size, dest_size
        )));
    }

    // Preserve modification time
    if let Ok(source_meta) = fs::metadata(source) {
        if let Ok(modified) = source_meta.modified() {
            let _ = filetime::set_file_mtime(dest, filetime::FileTime::from_system_time(modified));
        }
    }

    Ok(())
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
            copy_with_verify(&source_path, &dest_path)?;
        }
    }

    Ok(())
}
