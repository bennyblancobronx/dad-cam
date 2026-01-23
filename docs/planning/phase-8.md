Dad Cam - Phase 8 Implementation Guide

Version: 1.0
Target Audience: Developers new to ML integration in Rust/Tauri apps

---

Overview

Phase 8 adds machine learning capabilities to improve clip scoring beyond simple heuristics. While Phase 4 introduced FFmpeg-based scoring (scene changes, audio loudness, sharpness, motion), Phase 8 adds intelligent analysis that understands the content of clips: faces, emotions, speech, and personalized preferences.

When complete, you can:
- Detect faces in clips and know which clips have people
- Identify happy moments (smiles, positive emotions)
- Find clips with speech vs silent footage
- Get improved motion salience scores using optical flow
- See "Best Clips" that improve over time based on your preferences

Prerequisites:
- Phases 1-7 complete and working
- Understanding of Phase 4's scoring engine and clip_scores table
- Understanding of Phase 1's job system
- Basic familiarity with ONNX models and inference concepts
- FFmpeg available via `tools.rs` resolver

Done when: Best Clips is noticeably better than heuristics alone and improves over time per user.

---

What We're Building

Phase 8 runs ML analysis as background jobs (like proxy generation) and stores results alongside heuristic scores:

```
Clip Ingested (Phase 1)
    |
    v  Job Queue (Background)
ML Analysis Jobs
    |-- Face Detection (find faces)
    |-- Emotion Analysis (classify expressions)
    |-- Voice Activity Detection (speech segments)
    |-- Motion Flow Analysis (optical flow)
    |
    v  Store Results
ml_analyses table (per-clip ML data)
    |
    v  Scoring Engine (Updated)
Combined Score
    |-- Heuristic score (Phase 4): 40% weight
    |-- ML score (Phase 8): 40% weight
    |-- Personalized boost (Phase 8): 20% weight
    |
    v  Best Clips View
Improved recommendations
```

Core concepts:

1. **ML Analyses**: Per-clip storage for face count, emotion scores, speech percentage, motion quality

2. **ONNX Runtime**: All ML models run via `ort` (ONNX Runtime for Rust) - no Python required

3. **Background Jobs**: ML analysis runs after proxy generation, never blocks the UI

4. **Personalized Learning**: System learns from user favorites/bad tags to boost similar clips

5. **Offline-Only**: All models bundled with app, no network calls (per contracts.md)

---

Technology Choices

After research, these are the recommended libraries for Phase 8:

**ONNX Runtime (ort crate)**
- Foundation for all ML inference
- Cross-platform: macOS, Windows, Linux
- CPU inference (GPU optional via features)
- Crate: `ort = "2.0"`

**Face Detection**
- Model: BlazeFace or SCRFD (via rust-faces or custom ONNX loading)
- Input: Video frames extracted via FFmpeg
- Output: Face bounding boxes per frame
- Crate: `rust-faces = "0.4"` or manual ONNX loading

**Emotion Detection**
- Model: emotion-ferplus-8.onnx
- Input: 64x64 grayscale face crops
- Output: 8 emotion probabilities (neutral, happiness, surprise, sadness, anger, disgust, fear, contempt)
- Source: ONNX Model Zoo (bundle with app)

**Voice Activity Detection**
- Model: Silero VAD v5
- Input: Audio extracted from clip (16kHz mono)
- Output: Speech timestamps and percentage
- Crate: `voice_activity_detector = "0.2"` or `silero-vad-rs`

**Motion Analysis**
- Method: FFmpeg optical flow estimation
- Filter: `mpdecimate` for duplicate frames, custom dense flow
- Output: Motion quality score (stable vs shaky, activity level)

**Personalized Scoring**
- Method: Feature similarity with user-preferred clips
- Input: User favorites/bad tags + clip features
- Output: Personalized boost multiplier

---

Part 1: Database Extensions

1.1 Understanding the New Tables

Phase 8 adds two new concepts:

- **ml_analyses**: Stores ML analysis results per clip (faces, emotions, speech)
- **user_interactions**: Tracks implicit user behavior for personalized scoring

1.2 Add Migration 6

Add this to `src-tauri/src/db/migrations.rs` in the MIGRATIONS array:

```rust
// Migration 6: ML & Intelligence (Phase 8)
r#"
-- ML analysis results per clip
-- Stores face detection, emotion analysis, and speech detection results
CREATE TABLE ml_analyses (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
    analysis_version INTEGER NOT NULL DEFAULT 1,

    -- Face detection results
    face_count_avg REAL,
    face_count_max INTEGER,
    face_frames_percent REAL,
    face_timestamps TEXT,

    -- Emotion detection results (aggregated across detected faces)
    emotion_happiness_avg REAL,
    emotion_surprise_avg REAL,
    emotion_neutral_avg REAL,
    emotion_sadness_avg REAL,
    emotion_dominant TEXT,
    smile_frames_percent REAL,

    -- Voice activity detection results
    speech_percent REAL,
    speech_segments TEXT,
    speech_duration_ms INTEGER,
    silence_duration_ms INTEGER,

    -- Motion analysis results
    motion_flow_score REAL,
    motion_stability_score REAL,
    motion_activity_level TEXT,

    -- Combined ML score (weighted average of components)
    ml_score REAL,
    ml_score_reasons TEXT,

    -- Metadata
    analysis_duration_ms INTEGER,
    models_used TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(clip_id, analysis_version)
);

-- User interaction tracking for personalized scoring
-- Tracks implicit feedback (views, watch time, skips)
CREATE TABLE user_interactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,

    -- View tracking
    view_count INTEGER NOT NULL DEFAULT 0,
    total_watch_time_ms INTEGER NOT NULL DEFAULT 0,
    completion_rate_avg REAL,
    last_viewed_at TEXT,

    -- Skip tracking
    skip_count INTEGER NOT NULL DEFAULT 0,
    skip_position_avg REAL,

    -- Export tracking (clips included in exports)
    export_count INTEGER NOT NULL DEFAULT 0,
    last_exported_at TEXT,

    -- Rewatch tracking
    rewatch_count INTEGER NOT NULL DEFAULT 0,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(clip_id)
);

-- Personalized scoring model
-- Stores learned feature weights from user feedback
CREATE TABLE scoring_models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL REFERENCES libraries(id),
    model_version INTEGER NOT NULL DEFAULT 1,

    -- Feature weights (JSON: {"face_present": 0.3, "has_speech": 0.2, ...})
    feature_weights TEXT NOT NULL DEFAULT '{}',

    -- Training metadata
    training_samples INTEGER NOT NULL DEFAULT 0,
    positive_samples INTEGER NOT NULL DEFAULT 0,
    negative_samples INTEGER NOT NULL DEFAULT 0,

    -- Model performance
    accuracy_estimate REAL,
    last_trained_at TEXT,

    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    UNIQUE(library_id, model_version)
);

-- ML job queue tracking
-- Extends jobs table with ML-specific fields
ALTER TABLE jobs ADD COLUMN frames_analyzed INTEGER DEFAULT 0;
ALTER TABLE jobs ADD COLUMN frames_total INTEGER DEFAULT 0;

-- Add ML score to clip_scores for combined scoring
ALTER TABLE clip_scores ADD COLUMN ml_score REAL;
ALTER TABLE clip_scores ADD COLUMN personalized_boost REAL DEFAULT 0;
ALTER TABLE clip_scores ADD COLUMN combined_score REAL;

-- Indexes
CREATE INDEX idx_ml_analyses_clip ON ml_analyses(clip_id);
CREATE INDEX idx_ml_analyses_version ON ml_analyses(analysis_version);
CREATE INDEX idx_user_interactions_clip ON user_interactions(clip_id);
CREATE INDEX idx_user_interactions_views ON user_interactions(view_count DESC);
CREATE INDEX idx_scoring_models_library ON scoring_models(library_id);
"#,
```

1.3 Schema Constants

Add to `src-tauri/src/constants.rs`:

```rust
// ----- Phase 8: ML & Intelligence -----

/// Current ML analysis pipeline version
/// Bump this to invalidate all ML analyses and regenerate
pub const ML_ANALYSIS_VERSION: i32 = 1;

/// Scoring weights for combined score calculation
pub const WEIGHT_HEURISTIC: f64 = 0.40;
pub const WEIGHT_ML: f64 = 0.40;
pub const WEIGHT_PERSONALIZED: f64 = 0.20;

/// Frame sampling for ML analysis
/// Analyze 1 frame per second (balance accuracy vs performance)
pub const ML_FRAME_SAMPLE_RATE: f64 = 1.0;

/// Minimum faces detected to consider "has faces"
pub const MIN_FACE_FRAMES_PERCENT: f64 = 10.0;

/// Minimum speech percentage to consider "has speech"
pub const MIN_SPEECH_PERCENT: f64 = 20.0;

/// Smile detection threshold (happiness probability)
pub const SMILE_THRESHOLD: f64 = 0.6;

/// Emotion categories from FER+ model
pub const EMOTIONS: &[&str] = &[
    "neutral", "happiness", "surprise", "sadness",
    "anger", "disgust", "fear", "contempt"
];

/// Models bundled with the app
pub const MODEL_FACE_DETECTION: &str = "models/blazeface.onnx";
pub const MODEL_EMOTION: &str = "models/emotion-ferplus-8.onnx";
pub const MODEL_VAD: &str = "models/silero_vad.onnx";
```

1.4 Schema Query Helpers

Add to `src-tauri/src/db/schema.rs`:

```rust
// ----- ML Analyses -----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MlAnalysis {
    pub id: i64,
    pub clip_id: i64,
    pub analysis_version: i32,

    // Face detection
    pub face_count_avg: Option<f64>,
    pub face_count_max: Option<i32>,
    pub face_frames_percent: Option<f64>,
    pub face_timestamps: Option<String>,

    // Emotion detection
    pub emotion_happiness_avg: Option<f64>,
    pub emotion_surprise_avg: Option<f64>,
    pub emotion_neutral_avg: Option<f64>,
    pub emotion_sadness_avg: Option<f64>,
    pub emotion_dominant: Option<String>,
    pub smile_frames_percent: Option<f64>,

    // Voice activity
    pub speech_percent: Option<f64>,
    pub speech_segments: Option<String>,
    pub speech_duration_ms: Option<i64>,
    pub silence_duration_ms: Option<i64>,

    // Motion
    pub motion_flow_score: Option<f64>,
    pub motion_stability_score: Option<f64>,
    pub motion_activity_level: Option<String>,

    // Combined
    pub ml_score: Option<f64>,
    pub ml_score_reasons: Option<String>,

    pub analysis_duration_ms: Option<i64>,
    pub models_used: Option<String>,
    pub created_at: String,
}

/// Get ML analysis for a clip
pub fn get_ml_analysis(
    conn: &Connection,
    clip_id: i64,
    version: i32,
) -> Result<Option<MlAnalysis>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, clip_id, analysis_version,
                  face_count_avg, face_count_max, face_frames_percent, face_timestamps,
                  emotion_happiness_avg, emotion_surprise_avg, emotion_neutral_avg,
                  emotion_sadness_avg, emotion_dominant, smile_frames_percent,
                  speech_percent, speech_segments, speech_duration_ms, silence_duration_ms,
                  motion_flow_score, motion_stability_score, motion_activity_level,
                  ml_score, ml_score_reasons, analysis_duration_ms, models_used, created_at
           FROM ml_analyses
           WHERE clip_id = ?1 AND analysis_version = ?2"#
    )?;

    let result = stmt.query_row(params![clip_id, version], |row| {
        Ok(MlAnalysis {
            id: row.get(0)?,
            clip_id: row.get(1)?,
            analysis_version: row.get(2)?,
            face_count_avg: row.get(3)?,
            face_count_max: row.get(4)?,
            face_frames_percent: row.get(5)?,
            face_timestamps: row.get(6)?,
            emotion_happiness_avg: row.get(7)?,
            emotion_surprise_avg: row.get(8)?,
            emotion_neutral_avg: row.get(9)?,
            emotion_sadness_avg: row.get(10)?,
            emotion_dominant: row.get(11)?,
            smile_frames_percent: row.get(12)?,
            speech_percent: row.get(13)?,
            speech_segments: row.get(14)?,
            speech_duration_ms: row.get(15)?,
            silence_duration_ms: row.get(16)?,
            motion_flow_score: row.get(17)?,
            motion_stability_score: row.get(18)?,
            motion_activity_level: row.get(19)?,
            ml_score: row.get(20)?,
            ml_score_reasons: row.get(21)?,
            analysis_duration_ms: row.get(22)?,
            models_used: row.get(23)?,
            created_at: row.get(24)?,
        })
    });

    match result {
        Ok(analysis) => Ok(Some(analysis)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Insert or update ML analysis
pub fn upsert_ml_analysis(
    conn: &Connection,
    clip_id: i64,
    analysis: &MlAnalysisInput,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO ml_analyses (
               clip_id, analysis_version,
               face_count_avg, face_count_max, face_frames_percent, face_timestamps,
               emotion_happiness_avg, emotion_surprise_avg, emotion_neutral_avg,
               emotion_sadness_avg, emotion_dominant, smile_frames_percent,
               speech_percent, speech_segments, speech_duration_ms, silence_duration_ms,
               motion_flow_score, motion_stability_score, motion_activity_level,
               ml_score, ml_score_reasons, analysis_duration_ms, models_used
           ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
               ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
           )
           ON CONFLICT(clip_id, analysis_version) DO UPDATE SET
               face_count_avg = excluded.face_count_avg,
               face_count_max = excluded.face_count_max,
               face_frames_percent = excluded.face_frames_percent,
               face_timestamps = excluded.face_timestamps,
               emotion_happiness_avg = excluded.emotion_happiness_avg,
               emotion_surprise_avg = excluded.emotion_surprise_avg,
               emotion_neutral_avg = excluded.emotion_neutral_avg,
               emotion_sadness_avg = excluded.emotion_sadness_avg,
               emotion_dominant = excluded.emotion_dominant,
               smile_frames_percent = excluded.smile_frames_percent,
               speech_percent = excluded.speech_percent,
               speech_segments = excluded.speech_segments,
               speech_duration_ms = excluded.speech_duration_ms,
               silence_duration_ms = excluded.silence_duration_ms,
               motion_flow_score = excluded.motion_flow_score,
               motion_stability_score = excluded.motion_stability_score,
               motion_activity_level = excluded.motion_activity_level,
               ml_score = excluded.ml_score,
               ml_score_reasons = excluded.ml_score_reasons,
               analysis_duration_ms = excluded.analysis_duration_ms,
               models_used = excluded.models_used,
               updated_at = datetime('now')"#,
        params![
            clip_id,
            analysis.analysis_version,
            analysis.face_count_avg,
            analysis.face_count_max,
            analysis.face_frames_percent,
            analysis.face_timestamps,
            analysis.emotion_happiness_avg,
            analysis.emotion_surprise_avg,
            analysis.emotion_neutral_avg,
            analysis.emotion_sadness_avg,
            analysis.emotion_dominant,
            analysis.smile_frames_percent,
            analysis.speech_percent,
            analysis.speech_segments,
            analysis.speech_duration_ms,
            analysis.silence_duration_ms,
            analysis.motion_flow_score,
            analysis.motion_stability_score,
            analysis.motion_activity_level,
            analysis.ml_score,
            analysis.ml_score_reasons,
            analysis.analysis_duration_ms,
            analysis.models_used,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug, Clone, Default)]
pub struct MlAnalysisInput {
    pub analysis_version: i32,
    pub face_count_avg: Option<f64>,
    pub face_count_max: Option<i32>,
    pub face_frames_percent: Option<f64>,
    pub face_timestamps: Option<String>,
    pub emotion_happiness_avg: Option<f64>,
    pub emotion_surprise_avg: Option<f64>,
    pub emotion_neutral_avg: Option<f64>,
    pub emotion_sadness_avg: Option<f64>,
    pub emotion_dominant: Option<String>,
    pub smile_frames_percent: Option<f64>,
    pub speech_percent: Option<f64>,
    pub speech_segments: Option<String>,
    pub speech_duration_ms: Option<i64>,
    pub silence_duration_ms: Option<i64>,
    pub motion_flow_score: Option<f64>,
    pub motion_stability_score: Option<f64>,
    pub motion_activity_level: Option<String>,
    pub ml_score: Option<f64>,
    pub ml_score_reasons: Option<String>,
    pub analysis_duration_ms: Option<i64>,
    pub models_used: Option<String>,
}

// ----- User Interactions -----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInteraction {
    pub id: i64,
    pub clip_id: i64,
    pub view_count: i64,
    pub total_watch_time_ms: i64,
    pub completion_rate_avg: Option<f64>,
    pub last_viewed_at: Option<String>,
    pub skip_count: i64,
    pub skip_position_avg: Option<f64>,
    pub export_count: i64,
    pub last_exported_at: Option<String>,
    pub rewatch_count: i64,
    pub created_at: String,
}

/// Record a clip view
pub fn record_clip_view(
    conn: &Connection,
    clip_id: i64,
    watch_time_ms: i64,
    completion_rate: f64,
    was_skipped: bool,
    skip_position: Option<f64>,
) -> Result<()> {
    // First ensure the record exists
    conn.execute(
        "INSERT OR IGNORE INTO user_interactions (clip_id) VALUES (?1)",
        params![clip_id],
    )?;

    // Then update with the view data
    if was_skipped {
        conn.execute(
            r#"UPDATE user_interactions SET
                   view_count = view_count + 1,
                   total_watch_time_ms = total_watch_time_ms + ?2,
                   skip_count = skip_count + 1,
                   skip_position_avg = COALESCE(
                       (skip_position_avg * skip_count + ?3) / (skip_count + 1),
                       ?3
                   ),
                   last_viewed_at = datetime('now'),
                   updated_at = datetime('now')
               WHERE clip_id = ?1"#,
            params![clip_id, watch_time_ms, skip_position],
        )?;
    } else {
        conn.execute(
            r#"UPDATE user_interactions SET
                   view_count = view_count + 1,
                   total_watch_time_ms = total_watch_time_ms + ?2,
                   completion_rate_avg = COALESCE(
                       (completion_rate_avg * (view_count - 1) + ?3) / view_count,
                       ?3
                   ),
                   last_viewed_at = datetime('now'),
                   updated_at = datetime('now')
               WHERE clip_id = ?1"#,
            params![clip_id, watch_time_ms, completion_rate],
        )?;
    }

    // Check for rewatch (viewed before, now viewing again)
    let existing: i64 = conn.query_row(
        "SELECT view_count FROM user_interactions WHERE clip_id = ?1",
        params![clip_id],
        |row| row.get(0),
    )?;

    if existing > 1 {
        conn.execute(
            "UPDATE user_interactions SET rewatch_count = rewatch_count + 1 WHERE clip_id = ?1",
            params![clip_id],
        )?;
    }

    Ok(())
}

/// Record clip included in export
pub fn record_clip_export(conn: &Connection, clip_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO user_interactions (clip_id) VALUES (?1)",
        params![clip_id],
    )?;

    conn.execute(
        r#"UPDATE user_interactions SET
               export_count = export_count + 1,
               last_exported_at = datetime('now'),
               updated_at = datetime('now')
           WHERE clip_id = ?1"#,
        params![clip_id],
    )?;

    Ok(())
}

/// Get clips needing ML analysis
pub fn get_clips_needing_ml_analysis(
    conn: &Connection,
    library_id: i64,
    version: i32,
    limit: i64,
) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        r#"SELECT c.id FROM clips c
           LEFT JOIN ml_analyses ma ON c.id = ma.clip_id AND ma.analysis_version = ?2
           WHERE c.library_id = ?1 AND ma.id IS NULL
           ORDER BY c.created_at DESC
           LIMIT ?3"#
    )?;

    let clip_ids = stmt.query_map(params![library_id, version, limit], |row| {
        row.get(0)
    })?;

    clip_ids.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

---

Part 2: Model Management

2.1 Understanding Model Bundling

ML models must be bundled with the app (no network downloads per contracts.md). Models are stored in the app's resources directory and loaded at runtime.

2.2 Directory Structure

```
src-tauri/
  resources/
    models/
      blazeface.onnx          # Face detection (~400KB)
      emotion-ferplus-8.onnx  # Emotion classification (~8MB)
      silero_vad.onnx         # Voice activity detection (~3MB)
```

2.3 Model Resolver

Create `src-tauri/src/ml/models.rs`:

```rust
use std::path::{Path, PathBuf};
use anyhow::{Result, Context};
use ort::{Session, SessionBuilder, GraphOptimizationLevel};

/// Paths to bundled ML models
pub struct ModelPaths {
    pub face_detection: PathBuf,
    pub emotion: PathBuf,
    pub vad: PathBuf,
}

impl ModelPaths {
    /// Resolve model paths from app resources
    pub fn from_app_handle(app: &tauri::AppHandle) -> Result<Self> {
        let resource_dir = app
            .path()
            .resource_dir()
            .context("Failed to get resource directory")?;

        let models_dir = resource_dir.join("models");

        Ok(Self {
            face_detection: models_dir.join("blazeface.onnx"),
            emotion: models_dir.join("emotion-ferplus-8.onnx"),
            vad: models_dir.join("silero_vad.onnx"),
        })
    }

    /// Verify all models exist
    pub fn verify(&self) -> Result<()> {
        if !self.face_detection.exists() {
            anyhow::bail!("Face detection model not found: {:?}", self.face_detection);
        }
        if !self.emotion.exists() {
            anyhow::bail!("Emotion model not found: {:?}", self.emotion);
        }
        if !self.vad.exists() {
            anyhow::bail!("VAD model not found: {:?}", self.vad);
        }
        Ok(())
    }
}

/// Load an ONNX model session
pub fn load_model(model_path: &Path) -> Result<Session> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(4)?
        .commit_from_file(model_path)
        .context(format!("Failed to load model: {:?}", model_path))?;

    Ok(session)
}

/// Model manager for lazy loading and caching
pub struct ModelManager {
    paths: ModelPaths,
    face_session: Option<Session>,
    emotion_session: Option<Session>,
    vad_session: Option<Session>,
}

impl ModelManager {
    pub fn new(paths: ModelPaths) -> Self {
        Self {
            paths,
            face_session: None,
            emotion_session: None,
            vad_session: None,
        }
    }

    /// Get or load face detection model
    pub fn face_model(&mut self) -> Result<&Session> {
        if self.face_session.is_none() {
            self.face_session = Some(load_model(&self.paths.face_detection)?);
        }
        Ok(self.face_session.as_ref().unwrap())
    }

    /// Get or load emotion model
    pub fn emotion_model(&mut self) -> Result<&Session> {
        if self.emotion_session.is_none() {
            self.emotion_session = Some(load_model(&self.paths.emotion)?);
        }
        Ok(self.emotion_session.as_ref().unwrap())
    }

    /// Get or load VAD model
    pub fn vad_model(&mut self) -> Result<&Session> {
        if self.vad_session.is_none() {
            self.vad_session = Some(load_model(&self.paths.vad)?);
        }
        Ok(self.vad_session.as_ref().unwrap())
    }
}
```

2.4 Tauri Configuration for Models

Add to `tauri.conf.json` in the `bundle` section:

```json
{
  "bundle": {
    "resources": [
      "resources/models/*"
    ]
  }
}
```

---

Part 3: Face Detection Module

3.1 Understanding Face Detection

Face detection finds rectangular regions in video frames that contain human faces. We use BlazeFace, a lightweight model designed for mobile that runs well on CPU.

Pipeline:
1. Extract frames from video at 1 FPS using FFmpeg
2. Preprocess each frame (resize to 128x128, normalize)
3. Run face detection model
4. Apply non-maximum suppression
5. Store face count and timestamps

3.2 Create Face Detection Module

Create `src-tauri/src/ml/face.rs`:

```rust
use std::path::Path;
use anyhow::{Result, Context};
use ort::{Session, Value, TensorElementType};
use ndarray::{Array, Array4, Axis};
use image::{DynamicImage, GenericImageView, imageops::FilterType};

/// Face detection result for a single frame
#[derive(Debug, Clone)]
pub struct FaceDetectionResult {
    pub frame_index: usize,
    pub timestamp_ms: i64,
    pub face_count: usize,
    pub faces: Vec<FaceBoundingBox>,
}

/// Bounding box for a detected face
#[derive(Debug, Clone, serde::Serialize)]
pub struct FaceBoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub confidence: f32,
}

/// Face detector using BlazeFace ONNX model
pub struct FaceDetector<'a> {
    session: &'a Session,
    input_size: (u32, u32),
    confidence_threshold: f32,
    nms_threshold: f32,
}

impl<'a> FaceDetector<'a> {
    pub fn new(session: &'a Session) -> Self {
        Self {
            session,
            input_size: (128, 128), // BlazeFace input size
            confidence_threshold: 0.7,
            nms_threshold: 0.3,
        }
    }

    /// Detect faces in an image
    pub fn detect(&self, image: &DynamicImage) -> Result<Vec<FaceBoundingBox>> {
        // Preprocess: resize and normalize to [0, 1]
        let resized = image.resize_exact(
            self.input_size.0,
            self.input_size.1,
            FilterType::Triangle,
        );

        let rgb = resized.to_rgb8();
        let (width, height) = rgb.dimensions();

        // Create input tensor: [1, 3, 128, 128] in CHW format
        let mut input = Array4::<f32>::zeros((1, 3, height as usize, width as usize));

        for (x, y, pixel) in rgb.enumerate_pixels() {
            let [r, g, b] = pixel.0;
            input[[0, 0, y as usize, x as usize]] = r as f32 / 255.0;
            input[[0, 1, y as usize, x as usize]] = g as f32 / 255.0;
            input[[0, 2, y as usize, x as usize]] = b as f32 / 255.0;
        }

        // Run inference
        let input_tensor = Value::from_array(input.view())?;
        let outputs = self.session.run(ort::inputs![input_tensor]?)?;

        // Parse outputs (BlazeFace outputs boxes and scores)
        // Output format depends on specific model variant
        let boxes = self.parse_outputs(&outputs, image.width(), image.height())?;

        // Apply NMS
        let filtered = self.non_maximum_suppression(boxes);

        Ok(filtered)
    }

    fn parse_outputs(
        &self,
        outputs: &ort::SessionOutputs,
        orig_width: u32,
        orig_height: u32,
    ) -> Result<Vec<FaceBoundingBox>> {
        // Note: Exact parsing depends on the model variant
        // This is a simplified example - adjust based on actual model outputs
        let boxes_output = outputs.get("boxes")
            .or_else(|| outputs.get("output0"))
            .context("No boxes output found")?;

        let scores_output = outputs.get("scores")
            .or_else(|| outputs.get("output1"))
            .context("No scores output found")?;

        let boxes_array: ndarray::ArrayView2<f32> = boxes_output.try_extract_tensor()?;
        let scores_array: ndarray::ArrayView1<f32> = scores_output.try_extract_tensor()?;

        let scale_x = orig_width as f32 / self.input_size.0 as f32;
        let scale_y = orig_height as f32 / self.input_size.1 as f32;

        let mut faces = Vec::new();

        for (i, confidence) in scores_array.iter().enumerate() {
            if *confidence >= self.confidence_threshold {
                let row = boxes_array.row(i);
                faces.push(FaceBoundingBox {
                    x: row[0] * scale_x,
                    y: row[1] * scale_y,
                    width: (row[2] - row[0]) * scale_x,
                    height: (row[3] - row[1]) * scale_y,
                    confidence: *confidence,
                });
            }
        }

        Ok(faces)
    }

    fn non_maximum_suppression(&self, mut boxes: Vec<FaceBoundingBox>) -> Vec<FaceBoundingBox> {
        // Sort by confidence descending
        boxes.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        let mut keep = Vec::new();
        let mut suppressed = vec![false; boxes.len()];

        for i in 0..boxes.len() {
            if suppressed[i] {
                continue;
            }
            keep.push(boxes[i].clone());

            for j in (i + 1)..boxes.len() {
                if suppressed[j] {
                    continue;
                }

                let iou = self.compute_iou(&boxes[i], &boxes[j]);
                if iou > self.nms_threshold {
                    suppressed[j] = true;
                }
            }
        }

        keep
    }

    fn compute_iou(&self, a: &FaceBoundingBox, b: &FaceBoundingBox) -> f32 {
        let x1 = a.x.max(b.x);
        let y1 = a.y.max(b.y);
        let x2 = (a.x + a.width).min(b.x + b.width);
        let y2 = (a.y + a.height).min(b.y + b.height);

        let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let area_a = a.width * a.height;
        let area_b = b.width * b.height;
        let union = area_a + area_b - intersection;

        if union > 0.0 {
            intersection / union
        } else {
            0.0
        }
    }
}

/// Extract frames from video using FFmpeg
pub fn extract_frames(
    video_path: &Path,
    output_dir: &Path,
    fps: f64,
) -> Result<Vec<PathBuf>> {
    use std::process::Command;

    std::fs::create_dir_all(output_dir)?;

    let output_pattern = output_dir.join("frame_%04d.jpg");

    let status = Command::new("ffmpeg")
        .args([
            "-i", video_path.to_str().unwrap(),
            "-vf", &format!("fps={}", fps),
            "-q:v", "2",
            output_pattern.to_str().unwrap(),
        ])
        .output()
        .context("Failed to run ffmpeg")?;

    if !status.status.success() {
        anyhow::bail!("FFmpeg failed: {}", String::from_utf8_lossy(&status.stderr));
    }

    // Collect extracted frames
    let mut frames: Vec<PathBuf> = std::fs::read_dir(output_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "jpg").unwrap_or(false))
        .collect();

    frames.sort();
    Ok(frames)
}

/// Aggregate face detection results across frames
#[derive(Debug, Clone)]
pub struct FaceAnalysisResult {
    pub face_count_avg: f64,
    pub face_count_max: i32,
    pub face_frames_percent: f64,
    pub face_timestamps: Vec<i64>, // Timestamps where faces detected
}

pub fn aggregate_face_results(results: &[FaceDetectionResult]) -> FaceAnalysisResult {
    if results.is_empty() {
        return FaceAnalysisResult {
            face_count_avg: 0.0,
            face_count_max: 0,
            face_frames_percent: 0.0,
            face_timestamps: Vec::new(),
        };
    }

    let total_faces: usize = results.iter().map(|r| r.face_count).sum();
    let max_faces = results.iter().map(|r| r.face_count).max().unwrap_or(0);
    let frames_with_faces = results.iter().filter(|r| r.face_count > 0).count();

    let face_timestamps: Vec<i64> = results
        .iter()
        .filter(|r| r.face_count > 0)
        .map(|r| r.timestamp_ms)
        .collect();

    FaceAnalysisResult {
        face_count_avg: total_faces as f64 / results.len() as f64,
        face_count_max: max_faces as i32,
        face_frames_percent: (frames_with_faces as f64 / results.len() as f64) * 100.0,
        face_timestamps,
    }
}
```

---

Part 4: Emotion Detection Module

4.1 Understanding Emotion Detection

After detecting faces, we crop each face region and run it through an emotion classifier. The emotion-ferplus model outputs probabilities for 8 emotions.

4.2 Create Emotion Module

Create `src-tauri/src/ml/emotion.rs`:

```rust
use anyhow::{Result, Context};
use ort::{Session, Value};
use ndarray::Array4;
use image::{DynamicImage, GrayImage, imageops::FilterType};

use super::face::FaceBoundingBox;

/// Emotion categories from FER+ model
pub const EMOTIONS: [&str; 8] = [
    "neutral", "happiness", "surprise", "sadness",
    "anger", "disgust", "fear", "contempt"
];

/// Emotion classification result
#[derive(Debug, Clone, serde::Serialize)]
pub struct EmotionResult {
    pub probabilities: [f32; 8],
    pub dominant: String,
    pub dominant_confidence: f32,
    pub happiness: f32,
    pub is_smiling: bool,
}

/// Emotion classifier using emotion-ferplus ONNX model
pub struct EmotionClassifier<'a> {
    session: &'a Session,
    smile_threshold: f32,
}

impl<'a> EmotionClassifier<'a> {
    pub fn new(session: &'a Session, smile_threshold: f32) -> Self {
        Self {
            session,
            smile_threshold,
        }
    }

    /// Classify emotion from a face crop
    pub fn classify(&self, face_image: &GrayImage) -> Result<EmotionResult> {
        // Preprocess: resize to 64x64 and normalize
        let resized = image::imageops::resize(
            face_image,
            64,
            64,
            FilterType::Triangle,
        );

        // Create input tensor: [1, 1, 64, 64]
        let mut input = Array4::<f32>::zeros((1, 1, 64, 64));

        for (x, y, pixel) in resized.enumerate_pixels() {
            input[[0, 0, y as usize, x as usize]] = pixel.0[0] as f32 / 255.0;
        }

        // Run inference
        let input_tensor = Value::from_array(input.view())?;
        let outputs = self.session.run(ort::inputs![input_tensor]?)?;

        // Parse output
        let output = outputs.get("output")
            .or_else(|| outputs.iter().next().map(|(_, v)| v))
            .context("No output found")?;

        let probs: ndarray::ArrayView1<f32> = output.try_extract_tensor()?;

        // Apply softmax if not already applied
        let probs_vec: Vec<f32> = probs.iter().cloned().collect();
        let probs_softmax = softmax(&probs_vec);

        // Find dominant emotion
        let (max_idx, max_prob) = probs_softmax
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap();

        let happiness = probs_softmax[1]; // Index 1 is happiness

        let mut probabilities = [0.0f32; 8];
        for (i, p) in probs_softmax.iter().enumerate().take(8) {
            probabilities[i] = *p;
        }

        Ok(EmotionResult {
            probabilities,
            dominant: EMOTIONS[max_idx].to_string(),
            dominant_confidence: *max_prob,
            happiness,
            is_smiling: happiness >= self.smile_threshold,
        })
    }

    /// Classify emotion from a full image with a face bounding box
    pub fn classify_face_region(
        &self,
        image: &DynamicImage,
        face: &FaceBoundingBox,
    ) -> Result<EmotionResult> {
        // Crop face region with some padding
        let padding = 0.2;
        let x = (face.x - face.width * padding).max(0.0) as u32;
        let y = (face.y - face.height * padding).max(0.0) as u32;
        let width = (face.width * (1.0 + 2.0 * padding)).min(image.width() as f32 - x as f32) as u32;
        let height = (face.height * (1.0 + 2.0 * padding)).min(image.height() as f32 - y as f32) as u32;

        let cropped = image.crop_imm(x, y, width.max(1), height.max(1));
        let gray = cropped.to_luma8();

        self.classify(&gray)
    }
}

fn softmax(x: &[f32]) -> Vec<f32> {
    let max = x.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp: Vec<f32> = x.iter().map(|v| (v - max).exp()).collect();
    let sum: f32 = exp.iter().sum();
    exp.iter().map(|v| v / sum).collect()
}

/// Aggregate emotion results across frames
#[derive(Debug, Clone)]
pub struct EmotionAnalysisResult {
    pub happiness_avg: f64,
    pub surprise_avg: f64,
    pub neutral_avg: f64,
    pub sadness_avg: f64,
    pub dominant_emotion: String,
    pub smile_frames_percent: f64,
}

pub fn aggregate_emotion_results(results: &[EmotionResult]) -> EmotionAnalysisResult {
    if results.is_empty() {
        return EmotionAnalysisResult {
            happiness_avg: 0.0,
            surprise_avg: 0.0,
            neutral_avg: 0.0,
            sadness_avg: 0.0,
            dominant_emotion: "unknown".to_string(),
            smile_frames_percent: 0.0,
        };
    }

    let n = results.len() as f64;
    let happiness_sum: f64 = results.iter().map(|r| r.happiness as f64).sum();
    let surprise_sum: f64 = results.iter().map(|r| r.probabilities[2] as f64).sum();
    let neutral_sum: f64 = results.iter().map(|r| r.probabilities[0] as f64).sum();
    let sadness_sum: f64 = results.iter().map(|r| r.probabilities[3] as f64).sum();
    let smile_count = results.iter().filter(|r| r.is_smiling).count();

    // Find most common dominant emotion
    let mut emotion_counts = std::collections::HashMap::new();
    for r in results {
        *emotion_counts.entry(r.dominant.clone()).or_insert(0) += 1;
    }
    let dominant = emotion_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(emotion, _)| emotion)
        .unwrap_or_else(|| "unknown".to_string());

    EmotionAnalysisResult {
        happiness_avg: happiness_sum / n,
        surprise_avg: surprise_sum / n,
        neutral_avg: neutral_sum / n,
        sadness_avg: sadness_sum / n,
        dominant_emotion: dominant,
        smile_frames_percent: (smile_count as f64 / n) * 100.0,
    }
}
```

---

Part 5: Voice Activity Detection Module

5.1 Understanding VAD

Voice Activity Detection identifies segments of audio that contain speech. We use the Silero VAD model which outputs speech probabilities for audio chunks.

5.2 Create VAD Module

Create `src-tauri/src/ml/vad.rs`:

```rust
use std::path::Path;
use anyhow::{Result, Context};
use ort::{Session, Value};
use ndarray::{Array1, Array2};

/// Speech segment with start/end timestamps
#[derive(Debug, Clone, serde::Serialize)]
pub struct SpeechSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f32,
}

/// Voice activity detection result
#[derive(Debug, Clone)]
pub struct VadResult {
    pub speech_segments: Vec<SpeechSegment>,
    pub speech_percent: f64,
    pub speech_duration_ms: i64,
    pub silence_duration_ms: i64,
    pub total_duration_ms: i64,
}

/// Voice activity detector using Silero VAD
pub struct VoiceActivityDetector<'a> {
    session: &'a Session,
    sample_rate: i32,
    threshold: f32,
    min_speech_duration_ms: i64,
    min_silence_duration_ms: i64,
}

impl<'a> VoiceActivityDetector<'a> {
    pub fn new(session: &'a Session) -> Self {
        Self {
            session,
            sample_rate: 16000,
            threshold: 0.5,
            min_speech_duration_ms: 250,
            min_silence_duration_ms: 100,
        }
    }

    /// Process audio samples and detect speech segments
    pub fn detect(&self, audio_path: &Path) -> Result<VadResult> {
        // Extract audio using FFmpeg
        let samples = self.extract_audio(audio_path)?;

        if samples.is_empty() {
            return Ok(VadResult {
                speech_segments: Vec::new(),
                speech_percent: 0.0,
                speech_duration_ms: 0,
                silence_duration_ms: 0,
                total_duration_ms: 0,
            });
        }

        let total_duration_ms = (samples.len() as i64 * 1000) / self.sample_rate as i64;

        // Process in chunks (512 samples at 16kHz = 32ms)
        let chunk_size = 512;
        let chunk_duration_ms = (chunk_size as i64 * 1000) / self.sample_rate as i64;

        let mut speech_probs: Vec<(i64, f32)> = Vec::new();
        let mut h = Array2::<f32>::zeros((2, 1, 64)); // Hidden state
        let mut c = Array2::<f32>::zeros((2, 1, 64)); // Cell state

        for (i, chunk) in samples.chunks(chunk_size).enumerate() {
            if chunk.len() < chunk_size {
                break; // Skip incomplete final chunk
            }

            let timestamp_ms = i as i64 * chunk_duration_ms;

            // Create input tensor
            let input = Array1::from_iter(chunk.iter().cloned());
            let input_2d = input.insert_axis(ndarray::Axis(0));

            let input_tensor = Value::from_array(input_2d.view())?;
            let sr_tensor = Value::from_array(Array1::from_vec(vec![self.sample_rate as i64]).view())?;
            let h_tensor = Value::from_array(h.view())?;
            let c_tensor = Value::from_array(c.view())?;

            // Run inference
            let outputs = self.session.run(ort::inputs![
                "input" => input_tensor,
                "sr" => sr_tensor,
                "h" => h_tensor,
                "c" => c_tensor
            ]?)?;

            // Parse outputs
            let prob_output = outputs.get("output").context("No output")?;
            let prob: f32 = prob_output.try_extract_scalar()?;

            // Update hidden states for next iteration
            if let Some(hn) = outputs.get("hn") {
                h = hn.try_extract_tensor::<f32>()?.to_owned();
            }
            if let Some(cn) = outputs.get("cn") {
                c = cn.try_extract_tensor::<f32>()?.to_owned();
            }

            speech_probs.push((timestamp_ms, prob));
        }

        // Convert probabilities to segments
        let segments = self.probs_to_segments(&speech_probs);

        // Calculate statistics
        let speech_duration_ms: i64 = segments.iter()
            .map(|s| s.end_ms - s.start_ms)
            .sum();
        let silence_duration_ms = total_duration_ms - speech_duration_ms;
        let speech_percent = if total_duration_ms > 0 {
            (speech_duration_ms as f64 / total_duration_ms as f64) * 100.0
        } else {
            0.0
        };

        Ok(VadResult {
            speech_segments: segments,
            speech_percent,
            speech_duration_ms,
            silence_duration_ms,
            total_duration_ms,
        })
    }

    fn extract_audio(&self, video_path: &Path) -> Result<Vec<f32>> {
        use std::process::Command;

        // Extract audio to raw PCM using FFmpeg
        let output = Command::new("ffmpeg")
            .args([
                "-i", video_path.to_str().unwrap(),
                "-vn",
                "-acodec", "pcm_f32le",
                "-ar", &self.sample_rate.to_string(),
                "-ac", "1",
                "-f", "f32le",
                "-",
            ])
            .output()
            .context("Failed to extract audio with ffmpeg")?;

        if !output.status.success() {
            // No audio track or error - return empty
            return Ok(Vec::new());
        }

        // Parse raw bytes to f32 samples
        let samples: Vec<f32> = output.stdout
            .chunks_exact(4)
            .map(|chunk| {
                let bytes: [u8; 4] = chunk.try_into().unwrap();
                f32::from_le_bytes(bytes)
            })
            .collect();

        Ok(samples)
    }

    fn probs_to_segments(&self, probs: &[(i64, f32)]) -> Vec<SpeechSegment> {
        let mut segments = Vec::new();
        let mut current_segment: Option<(i64, f32)> = None;

        for (timestamp, prob) in probs {
            if *prob >= self.threshold {
                // Speech detected
                if current_segment.is_none() {
                    current_segment = Some((*timestamp, *prob));
                }
            } else {
                // Silence detected
                if let Some((start, conf)) = current_segment.take() {
                    let duration = timestamp - start;
                    if duration >= self.min_speech_duration_ms {
                        segments.push(SpeechSegment {
                            start_ms: start,
                            end_ms: *timestamp,
                            confidence: conf,
                        });
                    }
                }
            }
        }

        // Handle final segment
        if let Some((start, conf)) = current_segment {
            if let Some((last_ts, _)) = probs.last() {
                let duration = last_ts - start;
                if duration >= self.min_speech_duration_ms {
                    segments.push(SpeechSegment {
                        start_ms: start,
                        end_ms: *last_ts,
                        confidence: conf,
                    });
                }
            }
        }

        // Merge close segments
        self.merge_close_segments(segments)
    }

    fn merge_close_segments(&self, mut segments: Vec<SpeechSegment>) -> Vec<SpeechSegment> {
        if segments.len() < 2 {
            return segments;
        }

        let mut merged = Vec::new();
        let mut current = segments.remove(0);

        for next in segments {
            if next.start_ms - current.end_ms < self.min_silence_duration_ms {
                // Merge segments
                current.end_ms = next.end_ms;
                current.confidence = current.confidence.max(next.confidence);
            } else {
                merged.push(current);
                current = next;
            }
        }
        merged.push(current);

        merged
    }
}
```

---

Part 6: Motion Analysis Module

6.1 Understanding Motion Analysis

Beyond Phase 4's basic motion detection, Phase 8 adds optical flow analysis for better motion quality assessment. This helps identify:
- Stable vs shaky footage
- High-activity vs static scenes
- Camera motion vs subject motion

6.2 Create Motion Module

Create `src-tauri/src/ml/motion.rs`:

```rust
use std::path::Path;
use std::process::Command;
use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};

/// Motion analysis result
#[derive(Debug, Clone, Serialize)]
pub struct MotionAnalysisResult {
    pub flow_score: f64,
    pub stability_score: f64,
    pub activity_level: String,
    pub duplicate_frame_percent: f64,
}

/// Analyze motion in a video using FFmpeg filters
pub fn analyze_motion(video_path: &Path) -> Result<MotionAnalysisResult> {
    // Use mpdecimate to detect duplicate/static frames
    let duplicate_info = analyze_duplicates(video_path)?;

    // Use blend filter for frame differencing (motion estimation)
    let motion_info = analyze_frame_diff(video_path)?;

    // Combine results
    let activity_level = if motion_info.avg_diff > 20.0 {
        "high"
    } else if motion_info.avg_diff > 5.0 {
        "medium"
    } else {
        "low"
    };

    // Stability = inverse of motion variance
    let stability_score = 1.0 / (1.0 + motion_info.variance * 0.01);

    Ok(MotionAnalysisResult {
        flow_score: motion_info.avg_diff / 50.0, // Normalize to 0-1
        stability_score,
        activity_level: activity_level.to_string(),
        duplicate_frame_percent: duplicate_info.duplicate_percent,
    })
}

#[derive(Debug)]
struct DuplicateInfo {
    duplicate_percent: f64,
    total_frames: i64,
    dropped_frames: i64,
}

fn analyze_duplicates(video_path: &Path) -> Result<DuplicateInfo> {
    // Use mpdecimate filter to detect duplicate frames
    let output = Command::new("ffmpeg")
        .args([
            "-i", video_path.to_str().unwrap(),
            "-vf", "mpdecimate=hi=64*12:lo=64*5:frac=0.1",
            "-f", "null",
            "-",
        ])
        .output()
        .context("Failed to run ffmpeg mpdecimate")?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse output for dropped frame count
    // Example: "mpdecimate ... drop=42 dup=0"
    let dropped = parse_mpdecimate_output(&stderr);

    // Get total frame count
    let total = get_frame_count(video_path)?;

    let duplicate_percent = if total > 0 {
        (dropped as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    Ok(DuplicateInfo {
        duplicate_percent,
        total_frames: total,
        dropped_frames: dropped,
    })
}

fn parse_mpdecimate_output(stderr: &str) -> i64 {
    // Look for "drop=" in mpdecimate output
    for line in stderr.lines() {
        if line.contains("mpdecimate") {
            if let Some(drop_pos) = line.find("drop=") {
                let start = drop_pos + 5;
                let end = line[start..]
                    .find(|c: char| !c.is_ascii_digit())
                    .map(|i| start + i)
                    .unwrap_or(line.len());
                if let Ok(count) = line[start..end].parse::<i64>() {
                    return count;
                }
            }
        }
    }
    0
}

fn get_frame_count(video_path: &Path) -> Result<i64> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-count_packets",
            "-show_entries", "stream=nb_read_packets",
            "-of", "csv=p=0",
            video_path.to_str().unwrap(),
        ])
        .output()
        .context("Failed to get frame count")?;

    let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    count_str.parse::<i64>().unwrap_or(0)
}

#[derive(Debug)]
struct MotionInfo {
    avg_diff: f64,
    variance: f64,
}

fn analyze_frame_diff(video_path: &Path) -> Result<MotionInfo> {
    // Use blend filter with difference mode to measure frame-to-frame changes
    let output = Command::new("ffmpeg")
        .args([
            "-i", video_path.to_str().unwrap(),
            "-vf", "tblend=all_mode=difference,blackframe=threshold=5",
            "-f", "null",
            "-",
        ])
        .output()
        .context("Failed to run ffmpeg tblend")?;

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse blackframe output for motion estimation
    // More black frames = less motion
    let black_percent = parse_blackframe_output(&stderr);

    // Invert: high black percent = low motion
    let avg_diff = 100.0 - black_percent;

    Ok(MotionInfo {
        avg_diff,
        variance: 10.0, // Simplified - would need more complex analysis
    })
}

fn parse_blackframe_output(stderr: &str) -> f64 {
    let mut total_frames = 0;
    let mut black_frames = 0;

    for line in stderr.lines() {
        if line.contains("blackframe") && line.contains("pblack") {
            black_frames += 1;
        }
        if line.contains("frame=") {
            total_frames += 1;
        }
    }

    if total_frames > 0 {
        (black_frames as f64 / total_frames as f64) * 100.0
    } else {
        0.0
    }
}
```

---

Part 7: Personalized Scoring Module

7.1 Understanding Personalized Scoring

Personalized scoring learns from user behavior to boost clips that match user preferences. It uses:

- **Explicit feedback**: Favorites and "Bad" tags from Phase 4
- **Implicit feedback**: View counts, watch time, export inclusion
- **Feature similarity**: Compare new clips to preferred clips

7.2 Create Personalized Scoring Module

Create `src-tauri/src/ml/personalized.rs`:

```rust
use std::collections::HashMap;
use anyhow::Result;
use rusqlite::Connection;
use crate::db::schema;

/// Feature vector for a clip (used for similarity matching)
#[derive(Debug, Clone)]
pub struct ClipFeatures {
    pub has_faces: f64,
    pub face_count_norm: f64,
    pub has_smiles: f64,
    pub happiness_avg: f64,
    pub has_speech: f64,
    pub speech_percent_norm: f64,
    pub motion_score: f64,
    pub stability_score: f64,
    pub duration_norm: f64,
}

impl ClipFeatures {
    /// Extract features from ML analysis and clip metadata
    pub fn from_analysis(
        ml: &schema::MlAnalysis,
        duration_ms: i64,
        max_duration_ms: i64,
    ) -> Self {
        Self {
            has_faces: if ml.face_frames_percent.unwrap_or(0.0) > 10.0 { 1.0 } else { 0.0 },
            face_count_norm: ml.face_count_avg.unwrap_or(0.0).min(5.0) / 5.0,
            has_smiles: if ml.smile_frames_percent.unwrap_or(0.0) > 10.0 { 1.0 } else { 0.0 },
            happiness_avg: ml.emotion_happiness_avg.unwrap_or(0.0),
            has_speech: if ml.speech_percent.unwrap_or(0.0) > 20.0 { 1.0 } else { 0.0 },
            speech_percent_norm: ml.speech_percent.unwrap_or(0.0) / 100.0,
            motion_score: ml.motion_flow_score.unwrap_or(0.5),
            stability_score: ml.motion_stability_score.unwrap_or(0.5),
            duration_norm: (duration_ms as f64 / max_duration_ms as f64).min(1.0),
        }
    }

    /// Convert to vector for similarity calculation
    pub fn to_vec(&self) -> Vec<f64> {
        vec![
            self.has_faces,
            self.face_count_norm,
            self.has_smiles,
            self.happiness_avg,
            self.has_speech,
            self.speech_percent_norm,
            self.motion_score,
            self.stability_score,
            self.duration_norm,
        ]
    }
}

/// Learned feature weights from user preferences
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeatureWeights {
    pub has_faces: f64,
    pub face_count: f64,
    pub has_smiles: f64,
    pub happiness: f64,
    pub has_speech: f64,
    pub speech_percent: f64,
    pub motion: f64,
    pub stability: f64,
    pub duration: f64,
}

impl Default for FeatureWeights {
    fn default() -> Self {
        Self {
            has_faces: 0.2,
            face_count: 0.1,
            has_smiles: 0.3,
            happiness: 0.2,
            has_speech: 0.2,
            speech_percent: 0.1,
            motion: 0.1,
            stability: 0.1,
            duration: 0.0,
        }
    }
}

impl FeatureWeights {
    pub fn to_vec(&self) -> Vec<f64> {
        vec![
            self.has_faces,
            self.face_count,
            self.has_smiles,
            self.happiness,
            self.has_speech,
            self.speech_percent,
            self.motion,
            self.stability,
            self.duration,
        ]
    }
}

/// Personalized scoring model
pub struct PersonalizedScorer {
    weights: FeatureWeights,
}

impl PersonalizedScorer {
    pub fn new(weights: FeatureWeights) -> Self {
        Self { weights }
    }

    /// Calculate personalized boost for a clip
    pub fn calculate_boost(&self, features: &ClipFeatures) -> f64 {
        let feature_vec = features.to_vec();
        let weight_vec = self.weights.to_vec();

        // Weighted sum of features
        let weighted_sum: f64 = feature_vec.iter()
            .zip(weight_vec.iter())
            .map(|(f, w)| f * w)
            .sum();

        // Normalize to [-0.2, +0.2] range (20% max boost/penalty)
        let max_positive: f64 = weight_vec.iter().filter(|w| **w > 0.0).sum();
        let max_negative: f64 = weight_vec.iter().filter(|w| **w < 0.0).map(|w| w.abs()).sum();
        let max_range = max_positive.max(max_negative);

        if max_range > 0.0 {
            (weighted_sum / max_range) * 0.2
        } else {
            0.0
        }
    }

    /// Train weights from user feedback
    pub fn train(&mut self, conn: &Connection, library_id: i64) -> Result<()> {
        // Get positive examples (favorites)
        let favorites = get_favorite_features(conn, library_id)?;

        // Get negative examples (bad-tagged)
        let bad_clips = get_bad_clip_features(conn, library_id)?;

        // Get implicit positive examples (high engagement)
        let engaged = get_engaged_clip_features(conn, library_id)?;

        if favorites.is_empty() && bad_clips.is_empty() && engaged.is_empty() {
            // No training data - keep default weights
            return Ok(());
        }

        // Simple weight learning: increase weights for features in favorites,
        // decrease for features in bad clips
        let mut new_weights = FeatureWeights::default();
        let learning_rate = 0.1;

        for features in &favorites {
            self.update_weights_positive(&mut new_weights, features, learning_rate);
        }

        for features in &engaged {
            // Implicit positive feedback gets half weight
            self.update_weights_positive(&mut new_weights, features, learning_rate * 0.5);
        }

        for features in &bad_clips {
            self.update_weights_negative(&mut new_weights, features, learning_rate);
        }

        // Normalize weights
        self.normalize_weights(&mut new_weights);

        self.weights = new_weights;

        // Save to database
        save_scoring_model(conn, library_id, &self.weights)?;

        Ok(())
    }

    fn update_weights_positive(&self, weights: &mut FeatureWeights, features: &ClipFeatures, lr: f64) {
        weights.has_faces += features.has_faces * lr;
        weights.face_count += features.face_count_norm * lr;
        weights.has_smiles += features.has_smiles * lr;
        weights.happiness += features.happiness_avg * lr;
        weights.has_speech += features.has_speech * lr;
        weights.speech_percent += features.speech_percent_norm * lr;
        weights.motion += features.motion_score * lr;
        weights.stability += features.stability_score * lr;
    }

    fn update_weights_negative(&self, weights: &mut FeatureWeights, features: &ClipFeatures, lr: f64) {
        weights.has_faces -= features.has_faces * lr;
        weights.face_count -= features.face_count_norm * lr;
        weights.has_smiles -= features.has_smiles * lr;
        weights.happiness -= features.happiness_avg * lr;
        weights.has_speech -= features.has_speech * lr;
        weights.speech_percent -= features.speech_percent_norm * lr;
        weights.motion -= features.motion_score * lr;
        weights.stability -= features.stability_score * lr;
    }

    fn normalize_weights(&self, weights: &mut FeatureWeights) {
        let vec = weights.to_vec();
        let sum: f64 = vec.iter().map(|v| v.abs()).sum();
        if sum > 0.0 {
            let scale = 1.0 / sum;
            weights.has_faces *= scale;
            weights.face_count *= scale;
            weights.has_smiles *= scale;
            weights.happiness *= scale;
            weights.has_speech *= scale;
            weights.speech_percent *= scale;
            weights.motion *= scale;
            weights.stability *= scale;
            weights.duration *= scale;
        }
    }
}

fn get_favorite_features(conn: &Connection, library_id: i64) -> Result<Vec<ClipFeatures>> {
    // Get clips tagged as favorites with ML analysis
    let mut stmt = conn.prepare(
        r#"SELECT ma.face_frames_percent, ma.face_count_avg, ma.smile_frames_percent,
                  ma.emotion_happiness_avg, ma.speech_percent, ma.motion_flow_score,
                  ma.motion_stability_score, c.duration_ms,
                  (SELECT MAX(duration_ms) FROM clips WHERE library_id = ?1) as max_duration
           FROM clips c
           JOIN clip_tags ct ON c.id = ct.clip_id
           JOIN tags t ON ct.tag_id = t.id
           JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1 AND t.name = 'favorite'
           AND ma.analysis_version = (SELECT MAX(analysis_version) FROM ml_analyses)"#
    )?;

    let features = stmt.query_map(rusqlite::params![library_id], |row| {
        let face_frames: f64 = row.get::<_, Option<f64>>(0)?.unwrap_or(0.0);
        let face_count: f64 = row.get::<_, Option<f64>>(1)?.unwrap_or(0.0);
        let smile_frames: f64 = row.get::<_, Option<f64>>(2)?.unwrap_or(0.0);
        let happiness: f64 = row.get::<_, Option<f64>>(3)?.unwrap_or(0.0);
        let speech: f64 = row.get::<_, Option<f64>>(4)?.unwrap_or(0.0);
        let motion: f64 = row.get::<_, Option<f64>>(5)?.unwrap_or(0.5);
        let stability: f64 = row.get::<_, Option<f64>>(6)?.unwrap_or(0.5);
        let duration: i64 = row.get(7)?;
        let max_duration: i64 = row.get::<_, Option<i64>>(8)?.unwrap_or(1);

        Ok(ClipFeatures {
            has_faces: if face_frames > 10.0 { 1.0 } else { 0.0 },
            face_count_norm: face_count.min(5.0) / 5.0,
            has_smiles: if smile_frames > 10.0 { 1.0 } else { 0.0 },
            happiness_avg: happiness,
            has_speech: if speech > 20.0 { 1.0 } else { 0.0 },
            speech_percent_norm: speech / 100.0,
            motion_score: motion,
            stability_score: stability,
            duration_norm: (duration as f64 / max_duration as f64).min(1.0),
        })
    })?;

    features.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn get_bad_clip_features(conn: &Connection, library_id: i64) -> Result<Vec<ClipFeatures>> {
    // Get clips tagged as 'bad' with ML analysis
    let mut stmt = conn.prepare(
        r#"SELECT ma.face_frames_percent, ma.face_count_avg, ma.smile_frames_percent,
                  ma.emotion_happiness_avg, ma.speech_percent, ma.motion_flow_score,
                  ma.motion_stability_score, c.duration_ms,
                  (SELECT MAX(duration_ms) FROM clips WHERE library_id = ?1) as max_duration
           FROM clips c
           JOIN clip_tags ct ON c.id = ct.clip_id
           JOIN tags t ON ct.tag_id = t.id
           JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1 AND t.name = 'bad'
           AND ma.analysis_version = (SELECT MAX(analysis_version) FROM ml_analyses)"#
    )?;

    let features = stmt.query_map(rusqlite::params![library_id], |row| {
        let face_frames: f64 = row.get::<_, Option<f64>>(0)?.unwrap_or(0.0);
        let face_count: f64 = row.get::<_, Option<f64>>(1)?.unwrap_or(0.0);
        let smile_frames: f64 = row.get::<_, Option<f64>>(2)?.unwrap_or(0.0);
        let happiness: f64 = row.get::<_, Option<f64>>(3)?.unwrap_or(0.0);
        let speech: f64 = row.get::<_, Option<f64>>(4)?.unwrap_or(0.0);
        let motion: f64 = row.get::<_, Option<f64>>(5)?.unwrap_or(0.5);
        let stability: f64 = row.get::<_, Option<f64>>(6)?.unwrap_or(0.5);
        let duration: i64 = row.get(7)?;
        let max_duration: i64 = row.get::<_, Option<i64>>(8)?.unwrap_or(1);

        Ok(ClipFeatures {
            has_faces: if face_frames > 10.0 { 1.0 } else { 0.0 },
            face_count_norm: face_count.min(5.0) / 5.0,
            has_smiles: if smile_frames > 10.0 { 1.0 } else { 0.0 },
            happiness_avg: happiness,
            has_speech: if speech > 20.0 { 1.0 } else { 0.0 },
            speech_percent_norm: speech / 100.0,
            motion_score: motion,
            stability_score: stability,
            duration_norm: (duration as f64 / max_duration as f64).min(1.0),
        })
    })?;

    features.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn get_engaged_clip_features(conn: &Connection, library_id: i64) -> Result<Vec<ClipFeatures>> {
    // Get clips with high implicit engagement (viewed multiple times, high completion, exported)
    let mut stmt = conn.prepare(
        r#"SELECT ma.face_frames_percent, ma.face_count_avg, ma.smile_frames_percent,
                  ma.emotion_happiness_avg, ma.speech_percent, ma.motion_flow_score,
                  ma.motion_stability_score, c.duration_ms,
                  (SELECT MAX(duration_ms) FROM clips WHERE library_id = ?1) as max_duration
           FROM clips c
           JOIN user_interactions ui ON c.id = ui.clip_id
           JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1
           AND ma.analysis_version = (SELECT MAX(analysis_version) FROM ml_analyses)
           AND (
               ui.view_count >= 3
               OR ui.completion_rate_avg >= 0.8
               OR ui.export_count >= 1
               OR ui.rewatch_count >= 1
           )"#
    )?;

    let features = stmt.query_map(rusqlite::params![library_id], |row| {
        let face_frames: f64 = row.get::<_, Option<f64>>(0)?.unwrap_or(0.0);
        let face_count: f64 = row.get::<_, Option<f64>>(1)?.unwrap_or(0.0);
        let smile_frames: f64 = row.get::<_, Option<f64>>(2)?.unwrap_or(0.0);
        let happiness: f64 = row.get::<_, Option<f64>>(3)?.unwrap_or(0.0);
        let speech: f64 = row.get::<_, Option<f64>>(4)?.unwrap_or(0.0);
        let motion: f64 = row.get::<_, Option<f64>>(5)?.unwrap_or(0.5);
        let stability: f64 = row.get::<_, Option<f64>>(6)?.unwrap_or(0.5);
        let duration: i64 = row.get(7)?;
        let max_duration: i64 = row.get::<_, Option<i64>>(8)?.unwrap_or(1);

        Ok(ClipFeatures {
            has_faces: if face_frames > 10.0 { 1.0 } else { 0.0 },
            face_count_norm: face_count.min(5.0) / 5.0,
            has_smiles: if smile_frames > 10.0 { 1.0 } else { 0.0 },
            happiness_avg: happiness,
            has_speech: if speech > 20.0 { 1.0 } else { 0.0 },
            speech_percent_norm: speech / 100.0,
            motion_score: motion,
            stability_score: stability,
            duration_norm: (duration as f64 / max_duration as f64).min(1.0),
        })
    })?;

    features.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn save_scoring_model(
    conn: &Connection,
    library_id: i64,
    weights: &FeatureWeights,
) -> Result<()> {
    let weights_json = serde_json::to_string(weights)?;

    conn.execute(
        r#"INSERT INTO scoring_models (library_id, feature_weights, last_trained_at)
           VALUES (?1, ?2, datetime('now'))
           ON CONFLICT(library_id, model_version) DO UPDATE SET
               feature_weights = excluded.feature_weights,
               last_trained_at = datetime('now')"#,
        rusqlite::params![library_id, weights_json],
    )?;

    Ok(())
}

/// Load or create personalized scorer for a library
pub fn load_scorer(conn: &Connection, library_id: i64) -> Result<PersonalizedScorer> {
    let result: Result<String, _> = conn.query_row(
        "SELECT feature_weights FROM scoring_models WHERE library_id = ?1 ORDER BY model_version DESC LIMIT 1",
        rusqlite::params![library_id],
        |row| row.get(0),
    );

    match result {
        Ok(json) => {
            let weights: FeatureWeights = serde_json::from_str(&json)?;
            Ok(PersonalizedScorer::new(weights))
        }
        Err(_) => {
            // No saved model - use defaults
            Ok(PersonalizedScorer::new(FeatureWeights::default()))
        }
    }
}
```

---

Part 8: ML Job Integration

8.1 Create ML Analysis Job

Create `src-tauri/src/ml/jobs.rs`:

```rust
use std::path::Path;
use std::time::Instant;
use anyhow::Result;
use rusqlite::Connection;

use crate::constants::*;
use crate::db::schema::{self, MlAnalysisInput};
use super::{
    face::{self, FaceDetector, aggregate_face_results},
    emotion::{EmotionClassifier, aggregate_emotion_results},
    vad::VoiceActivityDetector,
    motion,
    models::ModelManager,
    personalized,
};

/// Run ML analysis on a single clip
pub fn run_ml_analysis(
    conn: &Connection,
    clip_id: i64,
    video_path: &Path,
    models: &mut ModelManager,
) -> Result<schema::MlAnalysis> {
    let start = Instant::now();
    let mut models_used = Vec::new();

    // Create temp directory for frames
    let temp_dir = tempfile::tempdir()?;
    let frames_dir = temp_dir.path().join("frames");

    // --- Face Detection ---
    let face_result = {
        let frames = face::extract_frames(video_path, &frames_dir, ML_FRAME_SAMPLE_RATE)?;
        let detector = FaceDetector::new(models.face_model()?);

        let mut results = Vec::new();
        for (i, frame_path) in frames.iter().enumerate() {
            let image = image::open(frame_path)?;
            let faces = detector.detect(&image)?;

            results.push(face::FaceDetectionResult {
                frame_index: i,
                timestamp_ms: (i as f64 / ML_FRAME_SAMPLE_RATE * 1000.0) as i64,
                face_count: faces.len(),
                faces,
            });
        }

        models_used.push("blazeface");
        aggregate_face_results(&results)
    };

    // --- Emotion Detection (on frames with faces) ---
    let emotion_result = {
        let classifier = EmotionClassifier::new(
            models.emotion_model()?,
            SMILE_THRESHOLD as f32,
        );

        let frames = face::extract_frames(video_path, &frames_dir, ML_FRAME_SAMPLE_RATE)?;
        let detector = FaceDetector::new(models.face_model()?);

        let mut emotion_results = Vec::new();
        for frame_path in &frames {
            let image = image::open(frame_path)?;
            let faces = detector.detect(&image)?;

            for face in &faces {
                if let Ok(emotion) = classifier.classify_face_region(&image, face) {
                    emotion_results.push(emotion);
                }
            }
        }

        models_used.push("emotion-ferplus");
        emotion::aggregate_emotion_results(&emotion_results)
    };

    // --- Voice Activity Detection ---
    let vad_result = {
        let vad = VoiceActivityDetector::new(models.vad_model()?);
        models_used.push("silero-vad");
        vad.detect(video_path)?
    };

    // --- Motion Analysis ---
    let motion_result = motion::analyze_motion(video_path)?;

    // --- Calculate Combined ML Score ---
    let ml_score = calculate_ml_score(
        &face_result,
        &emotion_result,
        &vad_result,
        &motion_result,
    );

    let ml_score_reasons = build_ml_reasons(
        &face_result,
        &emotion_result,
        &vad_result,
        &motion_result,
    );

    let analysis_duration_ms = start.elapsed().as_millis() as i64;

    // Build input struct
    let input = MlAnalysisInput {
        analysis_version: ML_ANALYSIS_VERSION,
        face_count_avg: Some(face_result.face_count_avg),
        face_count_max: Some(face_result.face_count_max),
        face_frames_percent: Some(face_result.face_frames_percent),
        face_timestamps: Some(serde_json::to_string(&face_result.face_timestamps)?),
        emotion_happiness_avg: Some(emotion_result.happiness_avg),
        emotion_surprise_avg: Some(emotion_result.surprise_avg),
        emotion_neutral_avg: Some(emotion_result.neutral_avg),
        emotion_sadness_avg: Some(emotion_result.sadness_avg),
        emotion_dominant: Some(emotion_result.dominant_emotion),
        smile_frames_percent: Some(emotion_result.smile_frames_percent),
        speech_percent: Some(vad_result.speech_percent),
        speech_segments: Some(serde_json::to_string(&vad_result.speech_segments)?),
        speech_duration_ms: Some(vad_result.speech_duration_ms),
        silence_duration_ms: Some(vad_result.silence_duration_ms),
        motion_flow_score: Some(motion_result.flow_score),
        motion_stability_score: Some(motion_result.stability_score),
        motion_activity_level: Some(motion_result.activity_level),
        ml_score: Some(ml_score),
        ml_score_reasons: Some(serde_json::to_string(&ml_score_reasons)?),
        analysis_duration_ms: Some(analysis_duration_ms),
        models_used: Some(models_used.join(",")),
    };

    // Save to database
    schema::upsert_ml_analysis(conn, clip_id, &input)?;

    // Return the saved analysis
    schema::get_ml_analysis(conn, clip_id, ML_ANALYSIS_VERSION)?
        .ok_or_else(|| anyhow::anyhow!("Failed to retrieve saved analysis"))
}

fn calculate_ml_score(
    face: &face::FaceAnalysisResult,
    emotion: &emotion::EmotionAnalysisResult,
    vad: &vad::VadResult,
    motion: &motion::MotionAnalysisResult,
) -> f64 {
    // Component weights
    let face_weight = 0.25;
    let emotion_weight = 0.35;
    let speech_weight = 0.25;
    let motion_weight = 0.15;

    // Face score: clips with faces score higher
    let face_score = (face.face_frames_percent / 100.0).min(1.0);

    // Emotion score: happiness and smiles score higher
    let emotion_score = emotion.happiness_avg * 0.5 + (emotion.smile_frames_percent / 100.0) * 0.5;

    // Speech score: clips with speech score higher (but not too much)
    let speech_score = (vad.speech_percent / 100.0).min(0.8);

    // Motion score: moderate motion is good, too much or too little is bad
    let motion_score = motion.stability_score * 0.5 + (1.0 - motion.duplicate_frame_percent / 100.0) * 0.5;

    // Weighted average
    let score = face_score * face_weight
        + emotion_score * emotion_weight
        + speech_score * speech_weight
        + motion_score * motion_weight;

    score.clamp(0.0, 1.0)
}

fn build_ml_reasons(
    face: &face::FaceAnalysisResult,
    emotion: &emotion::EmotionAnalysisResult,
    vad: &vad::VadResult,
    motion: &motion::MotionAnalysisResult,
) -> Vec<String> {
    let mut reasons = Vec::new();

    if face.face_frames_percent > 50.0 {
        reasons.push(format!("Faces detected in {:.0}% of frames", face.face_frames_percent));
    }

    if emotion.smile_frames_percent > 20.0 {
        reasons.push(format!("Smiles detected in {:.0}% of face frames", emotion.smile_frames_percent));
    }

    if vad.speech_percent > 30.0 {
        reasons.push(format!("Speech detected ({:.0}% of audio)", vad.speech_percent));
    }

    if motion.stability_score > 0.7 {
        reasons.push("Stable footage".to_string());
    } else if motion.stability_score < 0.3 {
        reasons.push("Shaky footage".to_string());
    }

    reasons
}

/// Update combined score in clip_scores table
pub fn update_combined_score(
    conn: &Connection,
    clip_id: i64,
    library_id: i64,
) -> Result<f64> {
    // Get heuristic score
    let heuristic_score: Option<f64> = conn.query_row(
        "SELECT overall_score FROM clip_scores WHERE clip_id = ?1",
        rusqlite::params![clip_id],
        |row| row.get(0),
    ).ok();

    // Get ML score
    let ml_score: Option<f64> = conn.query_row(
        "SELECT ml_score FROM ml_analyses WHERE clip_id = ?1 ORDER BY analysis_version DESC LIMIT 1",
        rusqlite::params![clip_id],
        |row| row.get(0),
    ).ok();

    // Get personalized boost
    let scorer = personalized::load_scorer(conn, library_id)?;
    let personalized_boost = if let Some(analysis) = schema::get_ml_analysis(conn, clip_id, ML_ANALYSIS_VERSION)? {
        let clip_duration: i64 = conn.query_row(
            "SELECT duration_ms FROM clips WHERE id = ?1",
            rusqlite::params![clip_id],
            |row| row.get(0),
        )?;
        let features = personalized::ClipFeatures::from_analysis(&analysis, clip_duration, 600000);
        scorer.calculate_boost(&features)
    } else {
        0.0
    };

    // Calculate combined score
    let combined = match (heuristic_score, ml_score) {
        (Some(h), Some(m)) => {
            h * WEIGHT_HEURISTIC + m * WEIGHT_ML + personalized_boost
        }
        (Some(h), None) => h + personalized_boost,
        (None, Some(m)) => m + personalized_boost,
        (None, None) => 0.5 + personalized_boost,
    };

    let combined_clamped = combined.clamp(0.0, 1.0);

    // Update clip_scores
    conn.execute(
        r#"UPDATE clip_scores SET
               ml_score = ?2,
               personalized_boost = ?3,
               combined_score = ?4
           WHERE clip_id = ?1"#,
        rusqlite::params![clip_id, ml_score, personalized_boost, combined_clamped],
    )?;

    Ok(combined_clamped)
}
```

8.2 Register ML Job Type

Add to `src-tauri/src/jobs/runner.rs`:

```rust
// In the job type match statement:
"ml_analysis" => {
    let clip_id = payload["clip_id"].as_i64().unwrap();
    let video_path = PathBuf::from(payload["video_path"].as_str().unwrap());

    // Get or create model manager
    let mut models = ml::models::ModelManager::new(
        ml::models::ModelPaths::from_app_handle(&app_handle)?
    );

    ml::jobs::run_ml_analysis(conn, clip_id, &video_path, &mut models)?;
}
```

---

Part 9: CLI Commands

9.1 ML Analysis Commands

Add to `src-tauri/src/cli.rs`:

```rust
// ----- Phase 8: ML Commands -----

/// Run ML analysis on clips
/// dadcam ml-analyze [--clip <id>] [--force] [--verbose]
pub fn handle_ml_analyze(
    conn: &Connection,
    library_root: &Path,
    clip_id: Option<i64>,
    force: bool,
    verbose: bool,
) -> Result<()> {
    let model_paths = ml::models::ModelPaths::from_library_root(library_root)?;
    model_paths.verify()?;

    let mut models = ml::models::ModelManager::new(model_paths);

    let clip_ids = if let Some(id) = clip_id {
        vec![id]
    } else {
        let library_id = get_current_library_id(conn)?;
        if force {
            // Get all clips
            schema::get_all_clip_ids(conn, library_id)?
        } else {
            // Get clips needing analysis
            schema::get_clips_needing_ml_analysis(
                conn, library_id, ML_ANALYSIS_VERSION, 100
            )?
        }
    };

    println!("Analyzing {} clips...", clip_ids.len());

    for (i, id) in clip_ids.iter().enumerate() {
        let clip = schema::get_clip(conn, *id)?
            .ok_or_else(|| anyhow::anyhow!("Clip not found: {}", id))?;

        let video_path = resolve_clip_path(library_root, &clip)?;

        if verbose {
            println!("[{}/{}] Analyzing clip {}: {}", i + 1, clip_ids.len(), id, clip.filename);
        }

        match ml::jobs::run_ml_analysis(conn, *id, &video_path, &mut models) {
            Ok(analysis) => {
                if verbose {
                    println!("  Faces: {:.1}% of frames", analysis.face_frames_percent.unwrap_or(0.0));
                    println!("  Smiles: {:.1}%", analysis.smile_frames_percent.unwrap_or(0.0));
                    println!("  Speech: {:.1}%", analysis.speech_percent.unwrap_or(0.0));
                    println!("  ML Score: {:.2}", analysis.ml_score.unwrap_or(0.0));
                }
            }
            Err(e) => {
                eprintln!("  Error: {}", e);
            }
        }
    }

    println!("Done.");
    Ok(())
}

/// Show ML analysis status
/// dadcam ml-status [--missing-only]
pub fn handle_ml_status(
    conn: &Connection,
    missing_only: bool,
) -> Result<()> {
    let library_id = get_current_library_id(conn)?;

    let total_clips: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clips WHERE library_id = ?1",
        params![library_id],
        |row| row.get(0),
    )?;

    let analyzed_clips: i64 = conn.query_row(
        r#"SELECT COUNT(DISTINCT c.id) FROM clips c
           JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1 AND ma.analysis_version = ?2"#,
        params![library_id, ML_ANALYSIS_VERSION],
        |row| row.get(0),
    )?;

    let missing = total_clips - analyzed_clips;

    println!("ML Analysis Status (version {}):", ML_ANALYSIS_VERSION);
    println!("  Total clips: {}", total_clips);
    println!("  Analyzed: {} ({:.1}%)", analyzed_clips, (analyzed_clips as f64 / total_clips as f64) * 100.0);
    println!("  Missing: {}", missing);

    if missing_only && missing > 0 {
        println!("\nClips needing analysis:");
        let ids = schema::get_clips_needing_ml_analysis(conn, library_id, ML_ANALYSIS_VERSION, 20)?;
        for id in ids {
            let clip = schema::get_clip(conn, id)?;
            if let Some(c) = clip {
                println!("  {} - {}", id, c.filename);
            }
        }
    }

    Ok(())
}

/// Train personalized scoring model
/// dadcam train-scoring [--verbose]
pub fn handle_train_scoring(
    conn: &Connection,
    verbose: bool,
) -> Result<()> {
    let library_id = get_current_library_id(conn)?;

    println!("Training personalized scoring model...");

    let mut scorer = ml::personalized::load_scorer(conn, library_id)?;
    scorer.train(conn, library_id)?;

    if verbose {
        println!("Learned weights:");
        let weights = &scorer.weights;
        println!("  has_faces: {:.3}", weights.has_faces);
        println!("  has_smiles: {:.3}", weights.has_smiles);
        println!("  has_speech: {:.3}", weights.has_speech);
        println!("  happiness: {:.3}", weights.happiness);
        println!("  motion: {:.3}", weights.motion);
        println!("  stability: {:.3}", weights.stability);
    }

    println!("Done. Personalized scoring model updated.");
    Ok(())
}

/// Show clips with ML insights
/// dadcam best-clips-ml [--threshold 0.6] [--limit 20]
pub fn handle_best_clips_ml(
    conn: &Connection,
    threshold: f64,
    limit: i64,
) -> Result<()> {
    let library_id = get_current_library_id(conn)?;

    let clips = conn.prepare(
        r#"SELECT c.id, c.filename, cs.combined_score, ma.ml_score,
                  ma.face_frames_percent, ma.smile_frames_percent, ma.speech_percent
           FROM clips c
           JOIN clip_scores cs ON c.id = cs.clip_id
           LEFT JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1 AND COALESCE(cs.combined_score, cs.overall_score) >= ?2
           ORDER BY COALESCE(cs.combined_score, cs.overall_score) DESC
           LIMIT ?3"#
    )?
    .query_map(params![library_id, threshold, limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<f64>>(2)?,
            row.get::<_, Option<f64>>(3)?,
            row.get::<_, Option<f64>>(4)?,
            row.get::<_, Option<f64>>(5)?,
            row.get::<_, Option<f64>>(6)?,
        ))
    })?;

    println!("Best Clips (ML-enhanced, threshold {}):", threshold);
    println!("{:<6} {:<30} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "ID", "Filename", "Score", "ML", "Faces%", "Smiles%", "Speech%");
    println!("{}", "-".repeat(90));

    for clip in clips {
        let (id, filename, combined, ml, faces, smiles, speech) = clip?;
        println!("{:<6} {:<30} {:>8.2} {:>8.2} {:>8.1} {:>8.1} {:>8.1}",
            id,
            truncate(&filename, 30),
            combined.unwrap_or(0.0),
            ml.unwrap_or(0.0),
            faces.unwrap_or(0.0),
            smiles.unwrap_or(0.0),
            speech.unwrap_or(0.0),
        );
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max-3])
    } else {
        s.to_string()
    }
}
```

9.2 Sample CLI Output

```bash
$ dadcam ml-analyze --verbose
Analyzing 47 clips...
[1/47] Analyzing clip 1: MVI_0001.MTS
  Faces: 85.2% of frames
  Smiles: 42.1%
  Speech: 67.3%
  ML Score: 0.78
[2/47] Analyzing clip 2: MVI_0002.MTS
  Faces: 0.0% of frames
  Smiles: 0.0%
  Speech: 12.4%
  ML Score: 0.31
...
Done.

$ dadcam ml-status
ML Analysis Status (version 1):
  Total clips: 152
  Analyzed: 47 (30.9%)
  Missing: 105

$ dadcam ml-status --missing-only
ML Analysis Status (version 1):
  Total clips: 152
  Analyzed: 47 (30.9%)
  Missing: 105

Clips needing analysis:
  48 - MVI_0048.MTS
  49 - MVI_0049.MTS
  50 - MVI_0050.MTS
  ...

$ dadcam train-scoring --verbose
Training personalized scoring model...
Learned weights:
  has_faces: 0.285
  has_smiles: 0.342
  has_speech: 0.187
  happiness: 0.098
  motion: 0.045
  stability: 0.043
Done. Personalized scoring model updated.

$ dadcam best-clips-ml --threshold 0.6 --limit 10
Best Clips (ML-enhanced, threshold 0.6):
ID     Filename                          Score       ML   Faces%  Smiles%  Speech%
------------------------------------------------------------------------------------------
12     birthday_party_2019.MTS            0.89     0.82     92.3     58.4     45.2
7      kids_playing_backyard.MTS          0.85     0.78     88.1     62.1     23.8
23     christmas_morning.MTS              0.81     0.75     95.0     71.2     38.9
45     beach_vacation_day2.MTS            0.78     0.71     67.4     45.3     52.1
3      first_steps_emma.MTS               0.76     0.80     78.9     55.8     12.4
...
```

---

Part 10: Tauri Commands

10.1 ML Analysis Commands

Add to `src-tauri/src/commands.rs`:

```rust
// ----- Phase 8: ML Commands -----

/// Get ML analysis for a clip
#[tauri::command]
pub async fn get_ml_analysis(
    state: State<'_, DbState>,
    clip_id: i64,
) -> Result<Option<schema::MlAnalysis>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    schema::get_ml_analysis(&conn, clip_id, constants::ML_ANALYSIS_VERSION)
        .map_err(|e| e.to_string())
}

/// Run ML analysis on a clip (queues as background job)
#[tauri::command]
pub async fn analyze_clip_ml(
    state: State<'_, DbState>,
    clip_id: i64,
) -> Result<i64, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    let clip = schema::get_clip(&conn, clip_id)
        .map_err(|e| e.to_string())?
        .ok_or("Clip not found")?;

    let video_path = resolve_clip_path_from_db(&conn, &clip)
        .map_err(|e| e.to_string())?;

    let payload = serde_json::json!({
        "clip_id": clip_id,
        "video_path": video_path.to_string_lossy(),
    });

    let job_id = schema::create_job(&conn, &schema::NewJob {
        job_type: "ml_analysis".to_string(),
        library_id: Some(clip.library_id),
        clip_id: Some(clip_id),
        asset_id: None,
        priority: 5,
        payload: payload.to_string(),
    }).map_err(|e| e.to_string())?;

    Ok(job_id)
}

/// Get ML analysis status for library
#[tauri::command]
pub async fn get_ml_analysis_status(
    state: State<'_, DbState>,
    library_id: i64,
) -> Result<MlAnalysisStatus, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clips WHERE library_id = ?1",
        params![library_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    let analyzed: i64 = conn.query_row(
        r#"SELECT COUNT(DISTINCT c.id) FROM clips c
           JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1 AND ma.analysis_version = ?2"#,
        params![library_id, constants::ML_ANALYSIS_VERSION],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    Ok(MlAnalysisStatus {
        total_clips: total,
        analyzed_clips: analyzed,
        missing_clips: total - analyzed,
        analysis_version: constants::ML_ANALYSIS_VERSION,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MlAnalysisStatus {
    pub total_clips: i64,
    pub analyzed_clips: i64,
    pub missing_clips: i64,
    pub analysis_version: i32,
}

/// Record clip view for personalized scoring
#[tauri::command]
pub async fn record_clip_view(
    state: State<'_, DbState>,
    clip_id: i64,
    watch_time_ms: i64,
    completion_rate: f64,
    was_skipped: bool,
    skip_position: Option<f64>,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    schema::record_clip_view(&conn, clip_id, watch_time_ms, completion_rate, was_skipped, skip_position)
        .map_err(|e| e.to_string())
}

/// Train personalized scoring model
#[tauri::command]
pub async fn train_personalized_scoring(
    state: State<'_, DbState>,
    library_id: i64,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    let mut scorer = ml::personalized::load_scorer(&conn, library_id)
        .map_err(|e| e.to_string())?;

    scorer.train(&conn, library_id)
        .map_err(|e| e.to_string())
}

/// Get best clips using combined ML + heuristic scoring
#[tauri::command]
pub async fn get_best_clips_ml(
    state: State<'_, DbState>,
    library_id: i64,
    threshold: f64,
    limit: i64,
) -> Result<Vec<ClipWithMlScore>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn.prepare(
        r#"SELECT c.id, c.filename, c.duration_ms,
                  cs.combined_score, cs.overall_score, cs.ml_score, cs.personalized_boost,
                  ma.face_frames_percent, ma.smile_frames_percent, ma.speech_percent
           FROM clips c
           LEFT JOIN clip_scores cs ON c.id = cs.clip_id
           LEFT JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1 AND COALESCE(cs.combined_score, cs.overall_score, 0) >= ?2
           ORDER BY COALESCE(cs.combined_score, cs.overall_score, 0) DESC
           LIMIT ?3"#
    ).map_err(|e| e.to_string())?;

    let clips = stmt.query_map(params![library_id, threshold, limit], |row| {
        Ok(ClipWithMlScore {
            id: row.get(0)?,
            filename: row.get(1)?,
            duration_ms: row.get(2)?,
            combined_score: row.get(3)?,
            heuristic_score: row.get(4)?,
            ml_score: row.get(5)?,
            personalized_boost: row.get(6)?,
            face_frames_percent: row.get(7)?,
            smile_frames_percent: row.get(8)?,
            speech_percent: row.get(9)?,
        })
    }).map_err(|e| e.to_string())?;

    clips.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipWithMlScore {
    pub id: i64,
    pub filename: String,
    pub duration_ms: i64,
    pub combined_score: Option<f64>,
    pub heuristic_score: Option<f64>,
    pub ml_score: Option<f64>,
    pub personalized_boost: Option<f64>,
    pub face_frames_percent: Option<f64>,
    pub smile_frames_percent: Option<f64>,
    pub speech_percent: Option<f64>,
}
```

10.2 Register Commands

Add to the command handler registration in `main.rs`:

```rust
// Phase 8: ML commands
get_ml_analysis,
analyze_clip_ml,
get_ml_analysis_status,
record_clip_view,
train_personalized_scoring,
get_best_clips_ml,
```

---

Part 11: TypeScript Types and API

11.1 Types

Add to `src/types/ml.ts`:

```typescript
// ----- Phase 8: ML Types -----

export interface MlAnalysis {
  id: number;
  clipId: number;
  analysisVersion: number;

  // Face detection
  faceCountAvg: number | null;
  faceCountMax: number | null;
  faceFramesPercent: number | null;
  faceTimestamps: string | null;

  // Emotion
  emotionHappinessAvg: number | null;
  emotionSurpriseAvg: number | null;
  emotionNeutralAvg: number | null;
  emotionSadnessAvg: number | null;
  emotionDominant: string | null;
  smileFramesPercent: number | null;

  // Speech
  speechPercent: number | null;
  speechSegments: string | null;
  speechDurationMs: number | null;
  silenceDurationMs: number | null;

  // Motion
  motionFlowScore: number | null;
  motionStabilityScore: number | null;
  motionActivityLevel: string | null;

  // Combined
  mlScore: number | null;
  mlScoreReasons: string | null;

  analysisDurationMs: number | null;
  modelsUsed: string | null;
  createdAt: string;
}

export interface MlAnalysisStatus {
  totalClips: number;
  analyzedClips: number;
  missingClips: number;
  analysisVersion: number;
}

export interface ClipWithMlScore {
  id: number;
  filename: string;
  durationMs: number;
  combinedScore: number | null;
  heuristicScore: number | null;
  mlScore: number | null;
  personalizedBoost: number | null;
  faceFramesPercent: number | null;
  smileFramesPercent: number | null;
  speechPercent: number | null;
}

export interface SpeechSegment {
  startMs: number;
  endMs: number;
  confidence: number;
}
```

11.2 API Functions

Add to `src/api/ml.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';
import type { MlAnalysis, MlAnalysisStatus, ClipWithMlScore } from '../types/ml';

// ----- Phase 8: ML API -----

export async function getMlAnalysis(clipId: number): Promise<MlAnalysis | null> {
  return invoke('get_ml_analysis', { clipId });
}

export async function analyzeClipMl(clipId: number): Promise<number> {
  return invoke('analyze_clip_ml', { clipId });
}

export async function getMlAnalysisStatus(libraryId: number): Promise<MlAnalysisStatus> {
  return invoke('get_ml_analysis_status', { libraryId });
}

export async function recordClipView(
  clipId: number,
  watchTimeMs: number,
  completionRate: number,
  wasSkipped: boolean,
  skipPosition?: number
): Promise<void> {
  return invoke('record_clip_view', {
    clipId,
    watchTimeMs,
    completionRate,
    wasSkipped,
    skipPosition,
  });
}

export async function trainPersonalizedScoring(libraryId: number): Promise<void> {
  return invoke('train_personalized_scoring', { libraryId });
}

export async function getBestClipsMl(
  libraryId: number,
  threshold: number = 0.6,
  limit: number = 20
): Promise<ClipWithMlScore[]> {
  return invoke('get_best_clips_ml', { libraryId, threshold, limit });
}
```

---

Part 12: UI Components

12.1 ML Score Badge

Create `src/components/MlScoreBadge.tsx`:

```tsx
import React from 'react';
import type { MlAnalysis } from '../types/ml';

interface MlScoreBadgeProps {
  analysis: MlAnalysis | null;
  showDetails?: boolean;
}

export function MlScoreBadge({ analysis, showDetails = false }: MlScoreBadgeProps) {
  if (!analysis || analysis.mlScore === null) {
    return (
      <span className="ml-badge ml-badge-pending" title="ML analysis pending">
        --
      </span>
    );
  }

  const score = analysis.mlScore;
  const scoreClass = score >= 0.7 ? 'ml-badge-high' :
                     score >= 0.4 ? 'ml-badge-medium' : 'ml-badge-low';

  const indicators = [];
  if (analysis.faceFramesPercent && analysis.faceFramesPercent > 10) {
    indicators.push('face');
  }
  if (analysis.smileFramesPercent && analysis.smileFramesPercent > 10) {
    indicators.push('smile');
  }
  if (analysis.speechPercent && analysis.speechPercent > 20) {
    indicators.push('speech');
  }

  return (
    <div className={`ml-badge ${scoreClass}`}>
      <span className="ml-score">{(score * 100).toFixed(0)}</span>
      {showDetails && indicators.length > 0 && (
        <span className="ml-indicators">
          {indicators.map(i => (
            <span key={i} className={`ml-indicator ml-indicator-${i}`} title={i} />
          ))}
        </span>
      )}
    </div>
  );
}
```

12.2 ML Insights Panel

Create `src/components/MlInsightsPanel.tsx`:

```tsx
import React from 'react';
import type { MlAnalysis } from '../types/ml';

interface MlInsightsPanelProps {
  analysis: MlAnalysis;
}

export function MlInsightsPanel({ analysis }: MlInsightsPanelProps) {
  const reasons = analysis.mlScoreReasons
    ? JSON.parse(analysis.mlScoreReasons) as string[]
    : [];

  return (
    <div className="ml-insights-panel">
      <h4>ML Insights</h4>

      <div className="ml-insights-grid">
        <div className="ml-insight">
          <span className="ml-insight-label">Faces</span>
          <span className="ml-insight-value">
            {analysis.faceFramesPercent !== null
              ? `${analysis.faceFramesPercent.toFixed(0)}% of frames`
              : 'N/A'}
          </span>
        </div>

        <div className="ml-insight">
          <span className="ml-insight-label">Smiles</span>
          <span className="ml-insight-value">
            {analysis.smileFramesPercent !== null
              ? `${analysis.smileFramesPercent.toFixed(0)}%`
              : 'N/A'}
          </span>
        </div>

        <div className="ml-insight">
          <span className="ml-insight-label">Speech</span>
          <span className="ml-insight-value">
            {analysis.speechPercent !== null
              ? `${analysis.speechPercent.toFixed(0)}%`
              : 'N/A'}
          </span>
        </div>

        <div className="ml-insight">
          <span className="ml-insight-label">Motion</span>
          <span className="ml-insight-value">
            {analysis.motionActivityLevel || 'N/A'}
          </span>
        </div>

        <div className="ml-insight">
          <span className="ml-insight-label">Dominant Emotion</span>
          <span className="ml-insight-value">
            {analysis.emotionDominant || 'N/A'}
          </span>
        </div>

        <div className="ml-insight">
          <span className="ml-insight-label">ML Score</span>
          <span className="ml-insight-value ml-score-large">
            {analysis.mlScore !== null
              ? (analysis.mlScore * 100).toFixed(0)
              : 'N/A'}
          </span>
        </div>
      </div>

      {reasons.length > 0 && (
        <div className="ml-reasons">
          <h5>Why this score?</h5>
          <ul>
            {reasons.map((reason, i) => (
              <li key={i}>{reason}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
```

12.3 ML Analysis Progress

Create `src/components/MlAnalysisProgress.tsx`:

```tsx
import React from 'react';
import type { MlAnalysisStatus } from '../types/ml';

interface MlAnalysisProgressProps {
  status: MlAnalysisStatus;
  onAnalyzeAll?: () => void;
}

export function MlAnalysisProgress({ status, onAnalyzeAll }: MlAnalysisProgressProps) {
  const percent = status.totalClips > 0
    ? (status.analyzedClips / status.totalClips) * 100
    : 0;

  return (
    <div className="ml-analysis-progress">
      <div className="ml-progress-header">
        <span>ML Analysis</span>
        <span className="ml-progress-stats">
          {status.analyzedClips} / {status.totalClips} clips
        </span>
      </div>

      <div className="ml-progress-bar">
        <div
          className="ml-progress-fill"
          style={{ width: `${percent}%` }}
        />
      </div>

      {status.missingClips > 0 && onAnalyzeAll && (
        <button
          className="ml-analyze-btn"
          onClick={onAnalyzeAll}
        >
          Analyze {status.missingClips} remaining clips
        </button>
      )}
    </div>
  );
}
```

12.4 View Tracking Hook

Create `src/hooks/useViewTracking.ts`:

```typescript
import { useEffect, useRef, useCallback } from 'react';
import { recordClipView } from '../api/ml';

interface UseViewTrackingOptions {
  clipId: number;
  durationMs: number;
  enabled?: boolean;
}

export function useViewTracking({ clipId, durationMs, enabled = true }: UseViewTrackingOptions) {
  const startTimeRef = useRef<number>(0);
  const lastPositionRef = useRef<number>(0);

  const startTracking = useCallback(() => {
    if (!enabled) return;
    startTimeRef.current = Date.now();
    lastPositionRef.current = 0;
  }, [enabled]);

  const updatePosition = useCallback((positionMs: number) => {
    lastPositionRef.current = positionMs;
  }, []);

  const endTracking = useCallback(async (wasSkipped: boolean = false) => {
    if (!enabled || startTimeRef.current === 0) return;

    const watchTimeMs = Date.now() - startTimeRef.current;
    const completionRate = durationMs > 0
      ? lastPositionRef.current / durationMs
      : 0;

    const skipPosition = wasSkipped && durationMs > 0
      ? lastPositionRef.current / durationMs
      : undefined;

    try {
      await recordClipView(
        clipId,
        watchTimeMs,
        completionRate,
        wasSkipped,
        skipPosition
      );
    } catch (e) {
      console.error('Failed to record view:', e);
    }

    startTimeRef.current = 0;
  }, [clipId, durationMs, enabled]);

  return {
    startTracking,
    updatePosition,
    endTracking,
  };
}
```

---

Part 12.5: Error Handling

12.5.1 Model Loading Failures

When ML models fail to load, the app should degrade gracefully:

```rust
/// Safe model loading with fallback
pub fn load_models_safely(paths: &ModelPaths) -> ModelLoadResult {
    let mut result = ModelLoadResult {
        face_available: false,
        emotion_available: false,
        vad_available: false,
        errors: Vec::new(),
    };

    // Try loading each model independently
    match load_model(&paths.face_detection) {
        Ok(_) => result.face_available = true,
        Err(e) => result.errors.push(format!("Face model: {}", e)),
    }

    match load_model(&paths.emotion) {
        Ok(_) => result.emotion_available = true,
        Err(e) => result.errors.push(format!("Emotion model: {}", e)),
    }

    match load_model(&paths.vad) {
        Ok(_) => result.vad_available = true,
        Err(e) => result.errors.push(format!("VAD model: {}", e)),
    }

    result
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelLoadResult {
    pub face_available: bool,
    pub emotion_available: bool,
    pub vad_available: bool,
    pub errors: Vec<String>,
}
```

UI handling:
- Show warning banner if any models failed to load
- Disable ML analysis button if all models unavailable
- Show partial results when some models work

12.5.2 Corrupted Video Handling

```rust
/// Analyze with corruption resilience
pub fn run_ml_analysis_safe(
    conn: &Connection,
    clip_id: i64,
    video_path: &Path,
    models: &mut ModelManager,
) -> Result<MlAnalysisInput> {
    let mut analysis = MlAnalysisInput::default();
    analysis.analysis_version = ML_ANALYSIS_VERSION;
    let mut models_used = Vec::new();
    let mut errors = Vec::new();

    // Try face detection (may fail on corrupt frames)
    match analyze_faces(video_path, models) {
        Ok(face_result) => {
            analysis.face_count_avg = Some(face_result.face_count_avg);
            analysis.face_count_max = Some(face_result.face_count_max);
            analysis.face_frames_percent = Some(face_result.face_frames_percent);
            models_used.push("blazeface");
        }
        Err(e) => {
            errors.push(format!("Face detection failed: {}", e));
            // Continue with other analyses
        }
    }

    // Try VAD (may fail on corrupt audio)
    match analyze_speech(video_path, models) {
        Ok(vad_result) => {
            analysis.speech_percent = Some(vad_result.speech_percent);
            analysis.speech_duration_ms = Some(vad_result.speech_duration_ms);
            models_used.push("silero_vad");
        }
        Err(e) => {
            errors.push(format!("VAD failed: {}", e));
        }
    }

    // Try motion analysis (may fail on short clips)
    match motion::analyze_motion(video_path) {
        Ok(motion_result) => {
            analysis.motion_flow_score = Some(motion_result.flow_score);
            analysis.motion_stability_score = Some(motion_result.stability_score);
            analysis.motion_activity_level = Some(motion_result.activity_level);
        }
        Err(e) => {
            errors.push(format!("Motion analysis failed: {}", e));
        }
    }

    // Calculate partial ML score from available data
    analysis.ml_score = Some(calculate_partial_ml_score(&analysis));
    analysis.models_used = Some(models_used.join(","));

    // Store errors in reasons if any occurred
    if !errors.is_empty() {
        analysis.ml_score_reasons = Some(serde_json::json!({
            "partial": true,
            "errors": errors,
        }).to_string());
    }

    Ok(analysis)
}
```

12.5.3 Partial Analysis Resume

If analysis is interrupted (crash, user cancel), resume from where it left off:

```rust
/// Get partially analyzed clips for resume
pub fn get_incomplete_analyses(
    conn: &Connection,
    library_id: i64,
) -> Result<Vec<IncompleteAnalysis>> {
    let mut stmt = conn.prepare(
        r#"SELECT c.id, c.filename, ma.models_used, ma.ml_score_reasons
           FROM clips c
           JOIN ml_analyses ma ON c.id = ma.clip_id
           WHERE c.library_id = ?1
           AND ma.analysis_version = ?2
           AND (ma.ml_score_reasons LIKE '%"partial": true%'
                OR ma.models_used NOT LIKE '%blazeface%silero_vad%')"#
    )?;

    // Return clips that need completion
    // ...
}

/// Resume analysis on incomplete clips
pub fn resume_ml_analysis(
    conn: &Connection,
    clip_id: i64,
    video_path: &Path,
    models: &mut ModelManager,
) -> Result<()> {
    let existing = schema::get_ml_analysis(conn, clip_id, ML_ANALYSIS_VERSION)?;

    if let Some(analysis) = existing {
        let models_used: Vec<&str> = analysis.models_used
            .as_deref()
            .unwrap_or("")
            .split(',')
            .collect();

        // Only run missing analyses
        if !models_used.contains(&"blazeface") {
            // Run face detection and update
        }
        if !models_used.contains(&"silero_vad") {
            // Run VAD and update
        }
    }

    Ok(())
}
```

12.5.4 Error User Messages

Map technical errors to user-friendly messages:

```typescript
// src/utils/mlErrors.ts
export function getMlErrorMessage(error: string): string {
  if (error.includes('model not found')) {
    return 'ML models not installed. Please reinstall the app.';
  }
  if (error.includes('ONNX')) {
    return 'ML engine error. Try restarting the app.';
  }
  if (error.includes('ffmpeg')) {
    return 'Could not read video file. The file may be corrupted.';
  }
  if (error.includes('no audio')) {
    return 'No audio track found. Speech detection skipped.';
  }
  if (error.includes('timeout')) {
    return 'Analysis took too long. Try a shorter clip.';
  }
  return 'ML analysis failed. Check the logs for details.';
}
```

---

Part 13: Testing Workflow

13.1 Manual Test Checklist

Model Loading:
- [ ] Verify models exist in `resources/models/`
- [ ] App launches without model loading errors
- [ ] `dadcam ml-status` shows correct analysis version

Face Detection:
- [ ] Run `dadcam ml-analyze --clip <id> --verbose` on clip with faces
- [ ] Verify face_frames_percent > 0 for clips with people
- [ ] Verify face_frames_percent = 0 for clips without people

Emotion Detection:
- [ ] Verify smile_frames_percent > 0 when people are smiling
- [ ] Verify emotion_dominant shows reasonable values
- [ ] Verify happiness_avg correlates with visible happiness

Voice Activity Detection:
- [ ] Verify speech_percent > 0 for clips with talking
- [ ] Verify speech_percent = 0 for silent clips
- [ ] Verify speech_segments JSON is parseable

Motion Analysis:
- [ ] Verify motion_activity_level shows "high" for active scenes
- [ ] Verify motion_stability_score is lower for shaky footage
- [ ] Verify static clips have low motion scores

Combined Scoring:
- [ ] Verify ml_score is calculated and stored
- [ ] Verify combined_score uses heuristic + ML + personalized
- [ ] Verify best-clips-ml shows ML-enhanced rankings

Personalized Scoring:
- [ ] Tag some clips as favorites
- [ ] Tag some clips as bad
- [ ] Run `dadcam train-scoring --verbose`
- [ ] Verify weights changed based on feedback

UI:
- [ ] ML badge appears on clips with analysis
- [ ] ML insights panel shows when clicking clip
- [ ] View tracking records playback data
- [ ] Analysis progress shows in library view

13.2 Integration Tests

Add to `src-tauri/tests/phase8_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_ml_analysis_schema() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let conn = crate::db::open_db(&db_path).unwrap();

        // Run migrations
        crate::db::run_migrations(&conn).unwrap();

        // Verify ml_analyses table exists
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='ml_analyses'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_personalized_scorer_defaults() {
        let scorer = ml::personalized::PersonalizedScorer::new(
            ml::personalized::FeatureWeights::default()
        );

        // Test neutral features produce near-zero boost
        let neutral = ml::personalized::ClipFeatures {
            has_faces: 0.0,
            face_count_norm: 0.0,
            has_smiles: 0.0,
            happiness_avg: 0.0,
            has_speech: 0.0,
            speech_percent_norm: 0.0,
            motion_score: 0.5,
            stability_score: 0.5,
            duration_norm: 0.5,
        };

        let boost = scorer.calculate_boost(&neutral);
        assert!(boost.abs() < 0.1);
    }

    #[test]
    fn test_combined_score_weights() {
        // Verify weights sum appropriately
        let total = crate::constants::WEIGHT_HEURISTIC
            + crate::constants::WEIGHT_ML
            + crate::constants::WEIGHT_PERSONALIZED;
        assert!((total - 1.0).abs() < 0.01);
    }
}
```

---

Part 14: Verification Checklist

Before considering Phase 8 complete, verify:

Database:
- [ ] Migration 6 applies cleanly
- [ ] `ml_analyses` table exists with all columns
- [ ] `user_interactions` table exists
- [ ] `scoring_models` table exists
- [ ] `clip_scores` has new columns (ml_score, personalized_boost, combined_score)

Models:
- [ ] blazeface.onnx bundled and loads
- [ ] emotion-ferplus-8.onnx bundled and loads
- [ ] silero_vad.onnx bundled and loads
- [ ] Models work on macOS, Windows, Linux

Face Detection:
- [ ] Detects faces in test clips
- [ ] Returns bounding boxes
- [ ] NMS filters duplicates
- [ ] Performance < 500ms per frame

Emotion Detection:
- [ ] Classifies emotions correctly
- [ ] Smile detection threshold works
- [ ] Aggregation produces sensible averages

Voice Activity:
- [ ] Detects speech segments
- [ ] Calculates speech percentage
- [ ] Handles clips without audio

Motion Analysis:
- [ ] Optical flow scoring works
- [ ] Stability detection works
- [ ] Activity level classification works

Scoring:
- [ ] ML score calculated correctly
- [ ] Combined score uses all three components
- [ ] Personalized boost learned from feedback

CLI:
- [ ] `dadcam ml-analyze` runs analysis
- [ ] `dadcam ml-status` shows progress
- [ ] `dadcam train-scoring` updates model
- [ ] `dadcam best-clips-ml` shows enhanced results

UI:
- [ ] ML badge renders correctly
- [ ] Insights panel shows all data
- [ ] View tracking works
- [ ] Progress indicator accurate

---

Deferred to Later Phases

- **GPU Acceleration**: CUDA/Metal support for faster inference
- **Face Recognition**: Identify specific people across clips
- **Clustering**: Group similar faces together
- **Advanced Speech Analysis**: Transcription, speaker diarization
- **Scene Understanding**: Object detection, scene classification
- **Auto-Tagging**: Automatic tag suggestions based on content
- **Batch ML Processing**: Analyze entire library in parallel
- **Model Updates**: Download improved models (requires cloud)
- **Custom Model Training**: Fine-tune models on user data
- **Real-time Analysis**: Analyze during playback
- **Memory Optimization**: Model quantization, shared inference sessions

---

Cargo Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
# Phase 8: ML & Intelligence
ort = { version = "2.0", features = ["load-dynamic"] }
ndarray = "0.15"
image = { version = "0.24", features = ["jpeg", "png"] }
tempfile = "3.10"
```

Note: The `load-dynamic` feature for ort allows dynamic loading of ONNX Runtime, which must be bundled with the app or installed system-wide. For static linking, remove this feature and configure ort to bundle the runtime.

---

Model Acquisition

The following models must be obtained and placed in `src-tauri/resources/models/`:

1. **blazeface.onnx** - Face detection
   - Source: MediaPipe or convert from TensorFlow
   - License: Apache 2.0

2. **emotion-ferplus-8.onnx** - Emotion classification
   - Source: ONNX Model Zoo (archived at Hugging Face)
   - License: MIT

3. **silero_vad.onnx** - Voice activity detection
   - Source: github.com/snakers4/silero-vad
   - License: MIT

All models must be verified to work offline without network calls.

---

End of Phase 8 Implementation Guide


---

# Addendum: Phase 8 to 100% (v1.1 “Offline ML + Trustworthy Scores”)

You requested two things:
1) Use **BLAKE3** (not SHA-256) for model/input manifests.
2) Make Phase 8 **100% complete** as an implementation guide (no hand-waves).

This addendum closes all Phase 8 audit gaps:
- fixes combined-score weighting bug
- fixes unsafe fallback behavior
- adds deterministic invalidation inputs to `ml_analyses`
- locks ORT + model bundling strategy
- defines the personalization training contract (bounded, explainable, repeatable)
- adds the missing tests (idempotency, invalidation, no-audio, weighting correctness)

---

## 0) Canonical Hashing Policy (BLAKE3)

Phase 8 uses **BLAKE3** everywhere for integrity / manifest hashing.

### 0.1 Short hashes for filenames vs full hashes for integrity
- Store **full** BLAKE3 hex (64 chars) in DB for integrity.
- Use short prefixes (first 16 chars) only for filenames/log labels.

Rust helper:

```rust
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub fn blake3_file_hex(path: &Path) -> anyhow::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut f = std::fs::File::open(path)?;
    std::io::copy(&mut f, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}
```

---

## 1) Database: Make ML analysis invalidation deterministic (Required)

Phase 8 currently tracks `analysis_version` and `models_used`, but that’s not sufficient to guarantee that stored results match:
- the input content (proxy/source changed)
- the sampling policy
- the model bundle version

### 1.1 Migration: Extend `ml_analyses`

Add the following columns (new migration after Phase 8’s existing migration):

```rust
// Migration X: Phase 8.1 ML analysis determinism (BLAKE3)
r#"
ALTER TABLE ml_analyses ADD COLUMN pipeline_version INTEGER NOT NULL DEFAULT 0;

-- Identity of analyzed input (choose one primary identity; proxy is recommended)
ALTER TABLE ml_analyses ADD COLUMN analyzed_asset_id INTEGER REFERENCES assets(id) ON DELETE SET NULL;
ALTER TABLE ml_analyses ADD COLUMN analyzed_hash_fast TEXT;
ALTER TABLE ml_analyses ADD COLUMN analyzed_size_bytes INTEGER;

-- Sampling policy
ALTER TABLE ml_analyses ADD COLUMN frame_sample_fps REAL NOT NULL DEFAULT 1.0;
ALTER TABLE ml_analyses ADD COLUMN audio_sample_rate_hz INTEGER NOT NULL DEFAULT 16000;

-- Model bundle identity (BLAKE3 hash of manifest JSON bytes)
ALTER TABLE ml_analyses ADD COLUMN model_manifest_b3 TEXT;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_ml_analyses_clip_version ON ml_analyses(clip_id, analysis_version, pipeline_version);
CREATE INDEX IF NOT EXISTS idx_ml_analyses_manifest ON ml_analyses(model_manifest_b3);
"#,
```

### 1.2 Freshness rule (the contract)
A clip’s ML analysis is “fresh” if ALL match:
- `analysis_version == ML_ANALYSIS_VERSION`
- `pipeline_version == PIPELINE_VERSION` (from your derived assets pipeline)
- `analyzed_hash_fast` and `analyzed_size_bytes` match the current chosen analyzed asset (proxy or original)
- `frame_sample_fps` and `audio_sample_rate_hz` match constants
- `model_manifest_b3` matches current bundle

If any mismatch → recompute.

This makes “re-run analysis” deterministic and prevents stale ML from poisoning combined scores.

---

## 2) Model Bundle Manifest (BLAKE3) + Licensing (Required)

### 2.1 Add a manifest file
Create `resources/models/manifest.json`:

```json
{
  "bundleVersion": 1,
  "files": [
    {
      "name": "blazeface.onnx",
      "license": "Apache-2.0",
      "source": "https://...",
      "blake3": "<full b3 hex>",
      "bytes": 1234567,
      "input": "128x128 RGB"
    },
    {
      "name": "emotion-ferplus-8.onnx",
      "license": "MIT/Other",
      "source": "https://...",
      "blake3": "<full b3 hex>",
      "bytes": 2345678,
      "input": "64x64 RGB"
    },
    {
      "name": "silero_vad.onnx",
      "license": "MIT",
      "source": "https://...",
      "blake3": "<full b3 hex>",
      "bytes": 3456789,
      "input": "16kHz mono"
    }
  ]
}
```

### 2.2 Compute manifest hash (BLAKE3)
At startup, read manifest bytes and compute:

```rust
let manifest_bytes = std::fs::read(manifest_path)?;
let model_manifest_b3 = blake3::hash(&manifest_bytes).to_hex().to_string();
```

Validate each model file:
- exists
- size matches `bytes`
- file BLAKE3 matches manifest `blake3`

If mismatch → show a hard error:
> “Model bundle corrupted or incomplete. Please reinstall.”

Store `model_manifest_b3` into every `ml_analyses` row.

---

## 3) ONNX Runtime Bundling Strategy (Required)

Phase 8 must not hand-wave “works on macOS/Windows/Linux”.

### 3.1 Adopt the Phase 1 tool-resolver pattern
Create `src-tauri/src/onnx_runtime.rs` that resolves the ORT dynamic library path:

Resolution order:
1) env override `DADCAM_ORT_PATH` (dev/pro)
2) sidecar next to executable (bundled)
3) macOS app `Contents/Resources/`
4) dev fallback (PATH/system)

### 3.2 Packaging contract
- Bundle ORT dynamic libraries per platform into app resources:
  - macOS: `libonnxruntime.dylib`
  - Windows: `onnxruntime.dll`
  - Linux: `libonnxruntime.so`
- `ort` crate uses `load-dynamic` and you pass the resolved library path.

If you later switch to static, keep the same resolver API but no-op.

---

## 4) Job System Integration (Required)

### 4.1 Enqueue rule (dependency)
ML analyze jobs MUST run only after:
- clip exists in DB
- proxy exists (preferred) OR you explicitly choose originals

Recommended policy:
- Analyze the **proxy** if present (stable decode, consistent fps)
- Fallback to original only if proxy missing

### 4.2 Job stages + progress
Use a single job `ml_analyze_clip` with stages:
1) decode/sample frames (progress frames_analyzed / frames_total)
2) face detect
3) emotion (only where faces found)
4) VAD (audio)
5) motion (proxy frames)
6) aggregate metrics + ml_score + reasons + persist

Store stage label in job logs and in a `current_operation` field if your job schema supports it.

### 4.3 Timeouts and failure modes
- Guard each stage with a timeout (clip-level) to prevent permanent hangs.
- If audio missing → VAD stage returns zero speech segments; job succeeds.
- If video decode fails → job fails with clear error and does not write partial metrics.

---

## 5) Combined Score: Fix weighting + defensible fallback (Required)

### 5.1 Correct formula (weights always respected)
Phase 8 defines weights:
- heuristic 0.40
- ml 0.40
- personalized 0.20

You must apply the personalized weight the same as others.

Define:
- `personalized_raw` = unweighted boost in [-1.0, +1.0] (bounded)
- `personalized_component = WEIGHT_PERSONALIZED * personalized_raw`

Then:

```
combined =
  WEIGHT_HEURISTIC * heuristic +
  WEIGHT_ML * ml +
  WEIGHT_PERSONALIZED * personalized_raw
```

### 5.2 Fix `update_combined_score()` implementation

```rust
pub fn update_combined_score(
    conn: &Connection,
    clip_id: i64,
    heuristic: Option<f64>,
    ml: Option<f64>,
    personalized_raw: f64,
) -> Result<()> {
    let h = heuristic.unwrap_or(0.0);
    let m = ml.unwrap_or(0.0);

    // If we have neither heuristic nor ML, do NOT invent a score.
    // Keep combined_score NULL and rely on UI to show "Not scored".
    if heuristic.is_none() && ml.is_none() {
        conn.execute(
            "UPDATE clip_scores SET personalized_boost = ?1, combined_score = NULL WHERE clip_id = ?2",
            params![personalized_raw, clip_id],
        )?;
        return Ok(());
    }

    let combined = (h * WEIGHT_HEURISTIC) + (m * WEIGHT_ML) + (personalized_raw * WEIGHT_PERSONALIZED);
    let combined = combined.clamp(0.0, 1.0);

    conn.execute(
        "UPDATE clip_scores SET ml_score = ?1, personalized_boost = ?2, combined_score = ?3 WHERE clip_id = ?4",
        params![ml, personalized_raw, combined, clip_id],
    )?;
    Ok(())
}
```

### 5.3 UI contract for NULL combined_score
- If `combined_score` is NULL, show badge “Not scored” and exclude from “Best Clips” by default.
- Provide a “Show unscored” toggle.

This prevents “0.5 baseline inflation”.

---

## 6) Personalization Trainer: Define a deterministic, bounded algorithm (Required)

Phase 8 needs a trainer that:
- works with few interactions
- is explainable
- can’t explode scores

### 6.1 Feature vector definition
For each clip, define a fixed feature vector in [-1, +1]:
- `f_face_presence` = map face_percent [0..1] → [-1..+1]
- `f_happiness` = map happiness_avg [0..1] → [-1..+1]
- `f_speech` = map speech_percent [0..1] → [-1..+1]
- `f_motion` = map motion_score [0..1] → [-1..+1]
- `f_audio_quality` = map heuristic audio_score [0..1] → [-1..+1]
- `f_scene_density` = map heuristic scene_score [0..1] → [-1..+1]

### 6.2 Training objective (simple + stable)
Compute means for favorite vs bad, then weights are proportional to the difference:

```
w_i = clamp( (mean_fav_i - mean_bad_i) * LEARNING_RATE, -W_MAX, +W_MAX )
```

Where:
- `LEARNING_RATE` = 1.0 (or 0.5)
- `W_MAX` = 0.8 (prevents any single feature dominating)

If you don’t have enough data:
- require at least `min_favorites=10` and `min_bads=10`
- otherwise keep weights at defaults (zeros) and show “Need more feedback”.

### 6.3 Personalized boost computation
For a clip:

```
personalized_raw = clamp( dot(w, f), -1.0, +1.0 )
```

Store `personalized_raw` in `clip_scores.personalized_boost`.

This is intentionally raw/unweighted; the combined score applies WEIGHT_PERSONALIZED.

### 6.4 Store model metadata
In `scoring_models` store:
- weights JSON
- training counts
- `model_manifest_b3` + `analysis_version` used for training
- timestamp

---

## 7) ML Feature Aggregation: Make reasons consistent + explainable

### 7.1 Reason builder
Store concise reasons like:
- “faces in 65% of frames”
- “speech in 40% of duration”
- “high happiness avg”
- “steady motion”

Keep reasons bounded (max 6) and stable ordering.

### 7.2 Clamp and normalize all ML components
Every aggregated metric must be clamped to [0..1] before being used in scoring.

---

## 8) Tests: Add the missing invariants (Required)

Add tests that actually catch the real bugs:

### 8.1 Combined score weights test (catches the Phase 8 bug)
- heuristic=1.0, ml=1.0, personalized_raw=1.0
- expected combined = 0.4 + 0.4 + 0.2 = 1.0

### 8.2 NULL fallback test
- heuristic=None, ml=None, personalized_raw=1.0
- expected combined_score = NULL (not 0.7, not 0.5 baseline)

### 8.3 Idempotency test
- run ML analysis twice with same inputs/version/manifest
- ensure second run is a no-op or produces identical stored values

### 8.4 Invalidation test
- change `ML_ANALYSIS_VERSION` or `model_manifest_b3`
- ensure analysis is considered stale and recomputed

### 8.5 No-audio test
- clip with no audio stream
- VAD stage must not crash; speech_percent=0, segments empty; analysis succeeds

---

# Phase 8 Done-When Checklist (100% Definition)

**Determinism**
- [ ] `ml_analyses` stores: analysis_version, pipeline_version, analyzed hash/size, sampling params, model_manifest_b3
- [ ] freshness check recomputes on any mismatch

**Packaging**
- [ ] ORT resolver exists (env override, sidecar/resources, dev fallback)
- [ ] model manifest validates all files with BLAKE3

**Scoring correctness**
- [ ] combined score uses all weights (including personalized weight)
- [ ] combined_score is NULL when no heuristic and no ML

**Personalization**
- [ ] trainer is deterministic, bounded, explainable
- [ ] personalized_raw in [-1, +1]; combined clamps to [0, 1]

**Jobs**
- [ ] ML jobs run after proxy; progress tracked; stage labels emitted
- [ ] timeouts prevent permanent hangs
- [ ] no-audio and decode edge cases handled

**Tests**
- [ ] weighting test
- [ ] NULL fallback test
- [ ] idempotency test
- [ ] invalidation test
- [ ] no-audio test

---

End of Addendum
