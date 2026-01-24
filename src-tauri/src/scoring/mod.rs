// Phase 4: Scoring Engine
// Analyzes clips and computes quality scores for intelligent sorting

pub mod analyzer;
pub mod scene;
pub mod audio;
pub mod sharpness;
pub mod motion;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

/// Result of scoring a single clip
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringResult {
    pub clip_id: i64,
    pub overall_score: f64,
    pub scene_score: f64,
    pub audio_score: f64,
    pub sharpness_score: f64,
    pub motion_score: f64,
    pub reasons: Vec<String>,
    pub pipeline_version: u32,
    pub scoring_version: u32,
}

impl ScoringResult {
    pub fn new(clip_id: i64) -> Self {
        Self {
            clip_id,
            overall_score: 0.0,
            scene_score: 0.0,
            audio_score: 0.0,
            sharpness_score: 0.0,
            motion_score: 0.0,
            reasons: Vec::new(),
            pipeline_version: crate::constants::PIPELINE_VERSION,
            scoring_version: crate::constants::SCORING_VERSION,
        }
    }

    /// Compute overall score from components using configured weights
    pub fn compute_overall(&mut self) {
        use crate::constants::*;

        self.overall_score = (self.scene_score * SCORE_WEIGHT_SCENE)
            + (self.audio_score * SCORE_WEIGHT_AUDIO)
            + (self.sharpness_score * SCORE_WEIGHT_SHARPNESS)
            + (self.motion_score * SCORE_WEIGHT_MOTION);

        // Clamp to 0-1 range
        self.overall_score = self.overall_score.clamp(0.0, 1.0);
    }

    /// Add a reason for the score
    pub fn add_reason(&mut self, reason: &str) {
        self.reasons.push(reason.to_string());
    }
}

/// Score override info from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreOverride {
    pub id: i64,
    pub clip_id: i64,
    pub override_type: String,
    pub override_value: f64,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Stored clip score from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipScore {
    pub id: i64,
    pub clip_id: i64,
    pub overall_score: f64,
    pub scene_score: f64,
    pub audio_score: f64,
    pub sharpness_score: f64,
    pub motion_score: f64,
    pub reasons: Vec<String>,
    pub pipeline_version: u32,
    pub scoring_version: u32,
    pub created_at: String,
    pub updated_at: String,
}

/// Apply any override to a base score
pub fn apply_override(base_score: f64, override_info: Option<&ScoreOverride>) -> f64 {
    let Some(ov) = override_info else {
        return base_score;
    };

    let adjusted = match ov.override_type.as_str() {
        "promote" => base_score + ov.override_value,
        "demote" => base_score - ov.override_value,
        "pin" => ov.override_value,
        _ => base_score,
    };

    adjusted.clamp(0.0, 1.0)
}
