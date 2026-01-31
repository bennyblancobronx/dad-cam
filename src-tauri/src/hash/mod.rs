// Hashing module using BLAKE3

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use crate::constants::{HASH_CHUNK_SIZE, HASH_FAST_SCHEME};
use crate::error::{DadCamError, Result};

/// Compute fast hash for dedup: first 1MB + last 1MB + file size
/// Format: "blake3:first_last_size_v1:<hash>"
pub fn compute_fast_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .map_err(|e| DadCamError::Hash(format!("Failed to open file: {}", e)))?;

    let file_size = file.metadata()
        .map_err(|e| DadCamError::Hash(format!("Failed to get metadata: {}", e)))?
        .len();

    let mut hasher = blake3::Hasher::new();

    // Hash first chunk
    let mut first_chunk = vec![0u8; HASH_CHUNK_SIZE.min(file_size as usize)];
    file.read_exact(&mut first_chunk)
        .map_err(|e| DadCamError::Hash(format!("Failed to read first chunk: {}", e)))?;
    hasher.update(&first_chunk);

    // Hash last chunk if file is larger than one chunk
    if file_size > HASH_CHUNK_SIZE as u64 {
        let last_offset = file_size.saturating_sub(HASH_CHUNK_SIZE as u64);
        file.seek(SeekFrom::Start(last_offset))
            .map_err(|e| DadCamError::Hash(format!("Failed to seek: {}", e)))?;

        let mut last_chunk = vec![0u8; HASH_CHUNK_SIZE];
        file.read_exact(&mut last_chunk)
            .map_err(|e| DadCamError::Hash(format!("Failed to read last chunk: {}", e)))?;
        hasher.update(&last_chunk);
    }

    // Include file size in hash
    hasher.update(&file_size.to_le_bytes());

    let hash = hasher.finalize();
    Ok(format!("blake3:{}:{}", HASH_FAST_SCHEME, hash.to_hex()))
}

/// Compute full BLAKE3 hash of entire file
pub fn compute_full_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .map_err(|e| DadCamError::Hash(format!("Failed to open file: {}", e)))?;

    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0u8; HASH_CHUNK_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer)
            .map_err(|e| DadCamError::Hash(format!("Failed to read: {}", e)))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(format!("blake3:full:{}", hash.to_hex()))
}

/// Compute full BLAKE3 hash from an in-memory byte slice.
/// Used for manifest hash computation (hashing serialized manifest data).
pub fn compute_full_hash_from_bytes(data: &[u8]) -> String {
    let hash = blake3::hash(data);
    format!("blake3:full:{}", hash.to_hex())
}

/// Verify a file matches its stored hash
pub fn verify_hash(path: &Path, expected_hash: &str) -> Result<bool> {
    let actual_hash = if expected_hash.contains(HASH_FAST_SCHEME) {
        compute_fast_hash(path)?
    } else {
        compute_full_hash(path)?
    };

    Ok(actual_hash == expected_hash)
}

/// Generate a size_duration fingerprint for relink matching
pub fn compute_size_duration_fingerprint(size_bytes: i64, duration_ms: Option<i64>) -> String {
    match duration_ms {
        Some(d) => format!("size_duration:{}:{}", size_bytes, d),
        None => format!("size_duration:{}:0", size_bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_fast_hash_small_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Hello, World!").unwrap();

        let hash = compute_fast_hash(file.path()).unwrap();
        assert!(hash.starts_with("blake3:first_last_size_v1:"));
    }

    #[test]
    fn test_full_hash() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"Hello, World!").unwrap();

        let hash = compute_full_hash(file.path()).unwrap();
        assert!(hash.starts_with("blake3:full:"));
    }
}
