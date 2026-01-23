// Dad Cam Constants
// These values come from Phase 0 contracts. Do not change without updating contracts.md.

pub const PIPELINE_VERSION: u32 = 1;
pub const DEFAULT_INGEST_MODE: &str = "copy";

// Hashing
pub const HASH_ALGORITHM: &str = "blake3";
pub const HASH_CHUNK_SIZE: usize = 1_048_576; // 1MB
pub const HASH_FAST_SCHEME: &str = "first_last_size_v1";

// Concurrency defaults
pub const DEFAULT_INGEST_WORKERS: usize = 1;
pub const DEFAULT_PREVIEW_WORKERS: usize = 1;
pub const DEFAULT_SCORE_WORKERS: usize = 1;
pub const DEFAULT_EXPORT_WORKERS: usize = 1;
pub const DEFAULT_ML_WORKERS: usize = 1;
pub const MAX_CONCURRENT_FFMPEG: usize = 2;

// Paths
pub const PATH_DB_SEPARATOR: char = '/';
pub const DADCAM_FOLDER: &str = ".dadcam";
pub const DB_FILENAME: &str = "dadcam.db";
pub const ORIGINALS_FOLDER: &str = "originals";
pub const PROXIES_FOLDER: &str = "proxies";
pub const THUMBS_FOLDER: &str = "thumbs";
pub const SPRITES_FOLDER: &str = "sprites";
pub const EXPORTS_FOLDER: &str = "exports";

// Time
pub const EVENT_TIME_GAP_HOURS: i64 = 4;
pub const TIMESTAMP_PRECEDENCE: [&str; 3] = ["metadata", "folder", "filesystem"];

// Proxy settings (used in Phase 2, defined here for schema)
pub const PROXY_CODEC: &str = "h264";
pub const PROXY_RESOLUTION: u32 = 720;
pub const PROXY_CRF: u32 = 23;

// Thumbnail settings
pub const THUMB_FORMAT: &str = "jpg";
pub const THUMB_QUALITY: u32 = 85;

// Sprite settings
pub const SPRITE_FPS: u32 = 1;
pub const SPRITE_TILE_WIDTH: u32 = 160;
pub const SPRITE_MAX_FRAMES: u32 = 120;

// Camera profiles
pub const CAMERA_PROFILE_FORMAT: &str = "json";

// Storage semantics
pub const RECORDED_AT_STORAGE: &str = "utc";
pub const DERIVED_PARAMS_HASH_ALGO: &str = "blake3";

// Format handling
pub const SUPPORTED_FORMATS: &str = "ffmpeg-native";
pub const OUTLIER_TYPES: [&str; 2] = ["audio", "image"];

// Job settings
pub const JOB_MAX_RETRIES: i32 = 3;
pub const JOB_BASE_BACKOFF_SECONDS: i64 = 60;
pub const JOB_LEASE_DURATION_SECONDS: i64 = 300; // 5 minutes
pub const JOB_HEARTBEAT_INTERVAL_SECONDS: i64 = 30;

// Sidecar extensions (files to copy alongside videos)
pub const SIDECAR_EXTENSIONS: [&str; 6] = ["thm", "xml", "xmp", "srt", "lrf", "idx"];

// Video extensions (primary supported formats)
pub const VIDEO_EXTENSIONS: [&str; 20] = [
    "mp4", "mov", "avi", "mkv", "mts", "m2ts", "mxf", "mpg", "mpeg",
    "wmv", "flv", "webm", "3gp", "m4v", "ts", "vob", "mod", "tod",
    "dv", "ogv"
];

// Audio extensions
pub const AUDIO_EXTENSIONS: [&str; 8] = [
    "mp3", "wav", "aac", "flac", "m4a", "ogg", "wma", "aiff"
];

// Image extensions
pub const IMAGE_EXTENSIONS: [&str; 6] = [
    "jpg", "jpeg", "png", "gif", "bmp", "tiff"
];
