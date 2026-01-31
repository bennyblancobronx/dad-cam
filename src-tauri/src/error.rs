// Dad Cam Error Types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DadCamError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Library not found: {0}")]
    LibraryNotFound(String),

    #[error("Clip not found: {0}")]
    ClipNotFound(i64),

    #[error("Asset not found: {0}")]
    AssetNotFound(i64),

    #[error("Job not found: {0}")]
    JobNotFound(i64),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("FFprobe error: {0}")]
    FFprobe(String),

    #[error("ExifTool error: {0}")]
    ExifTool(String),

    #[error("FFmpeg error: {0}")]
    FFmpeg(String),

    #[error("Hash error: {0}")]
    Hash(String),

    #[error("Ingest error: {0}")]
    Ingest(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("{0}")]
    Other(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("License error: {0}")]
    License(String),
}

impl From<anyhow::Error> for DadCamError {
    fn from(err: anyhow::Error) -> Self {
        DadCamError::Other(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, DadCamError>;
