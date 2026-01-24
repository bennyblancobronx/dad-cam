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
pub const SPRITE_MAX_FRAMES: u32 = 600;      // Overall cap (10 minutes @ 1fps)
pub const SPRITE_PAGE_COLS: u32 = 60;        // Frames per sprite sheet page

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

// ----- Phase 4: Scoring Constants -----

// Scoring pipeline version (bump when algorithm changes to trigger recalculation)
pub const SCORING_VERSION: u32 = 1;

// Component weights (must sum to 1.0)
pub const SCORE_WEIGHT_SCENE: f64 = 0.25;
pub const SCORE_WEIGHT_AUDIO: f64 = 0.25;
pub const SCORE_WEIGHT_SHARPNESS: f64 = 0.25;
pub const SCORE_WEIGHT_MOTION: f64 = 0.25;

// Default threshold for "best clips" filter
pub const BEST_CLIPS_THRESHOLD: f64 = 0.6;

// Override adjustment values (default)
pub const SCORE_PROMOTE_DEFAULT: f64 = 0.2;
pub const SCORE_DEMOTE_DEFAULT: f64 = 0.2;

// Scene detection thresholds
pub const SCENE_MIN_CHANGES: i32 = 2;      // Minimum scene changes for interesting
pub const SCENE_MAX_CHANGES: i32 = 20;     // Maximum before penalizing (too chaotic)
pub const SCENE_THRESHOLD: f64 = 0.3;      // FFmpeg scene detection threshold

// Audio analysis thresholds
pub const AUDIO_SILENCE_THRESHOLD: f64 = -40.0;  // dB below which is "silent"
pub const AUDIO_LOUD_THRESHOLD: f64 = -10.0;     // dB above which is "loud"
pub const AUDIO_MIN_DURATION_FOR_SPEECH: i64 = 3000;  // ms minimum to analyze

// Sharpness thresholds (laplacian variance)
pub const SHARPNESS_BLUR_THRESHOLD: f64 = 100.0;   // Below this is blurry
pub const SHARPNESS_SHARP_THRESHOLD: f64 = 500.0;  // Above this is sharp

// Motion detection thresholds
pub const MOTION_LOW_THRESHOLD: f64 = 0.01;   // Below this is static
pub const MOTION_HIGH_THRESHOLD: f64 = 0.3;   // Above this is high motion

// Sampling parameters (for efficient analysis)
pub const SCORE_SAMPLE_FRAMES: u32 = 10;      // Frames to sample per clip
pub const SCORE_SAMPLE_DURATION_MS: i64 = 500; // Sample every N ms for audio

// Scoring job concurrency and timeouts
pub const SCORE_JOB_TIMEOUT_SECS: u64 = 300;      // 5 minutes max per job
pub const SCORE_ANALYZE_TIMEOUT_SECS: u64 = 60;   // 1 minute max per analyzer
pub const SCORE_MAX_CONCURRENT_JOBS: usize = 2;   // Max parallel scoring jobs

// ----- Stable Reason Tokens -----
// Use these for machine-parseable reasons. Format: R_<CATEGORY>_<DETAIL>

// Scene reasons
pub const R_SCENE_STATIC: &str = "R_SCENE_STATIC";
pub const R_SCENE_GOOD: &str = "R_SCENE_GOOD";
pub const R_SCENE_CHAOTIC: &str = "R_SCENE_CHAOTIC";
pub const R_SCENE_SHORT: &str = "R_SCENE_SHORT";

// Audio reasons
pub const R_AUDIO_NONE: &str = "R_AUDIO_NONE";
pub const R_AUDIO_SILENT: &str = "R_AUDIO_SILENT";
pub const R_AUDIO_LOUD: &str = "R_AUDIO_LOUD";
pub const R_AUDIO_GOOD: &str = "R_AUDIO_GOOD";
pub const R_AUDIO_MODERATE: &str = "R_AUDIO_MODERATE";
pub const R_AUDIO_QUIET: &str = "R_AUDIO_QUIET";
pub const R_AUDIO_SHORT: &str = "R_AUDIO_SHORT";

// Sharpness reasons
pub const R_SHARP_BLURRY: &str = "R_SHARP_BLURRY";
pub const R_SHARP_OK: &str = "R_SHARP_OK";
pub const R_SHARP_GOOD: &str = "R_SHARP_GOOD";
pub const R_SHARP_SHORT: &str = "R_SHARP_SHORT";
pub const R_SHARP_UNAVAIL: &str = "R_SHARP_UNAVAIL";

// Motion reasons
pub const R_MOTION_STATIC: &str = "R_MOTION_STATIC";
pub const R_MOTION_CALM: &str = "R_MOTION_CALM";
pub const R_MOTION_GOOD: &str = "R_MOTION_GOOD";
pub const R_MOTION_HIGH: &str = "R_MOTION_HIGH";
pub const R_MOTION_CHAOTIC: &str = "R_MOTION_CHAOTIC";
pub const R_MOTION_SHORT: &str = "R_MOTION_SHORT";
pub const R_MOTION_UNAVAIL: &str = "R_MOTION_UNAVAIL";

// General reasons
pub const R_NON_VIDEO: &str = "R_NON_VIDEO";
pub const R_UNAVAILABLE: &str = "R_UNAVAILABLE";

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
