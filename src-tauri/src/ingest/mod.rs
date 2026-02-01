// Ingest pipeline module

pub mod discover;
pub mod copy;
pub mod sidecar;
pub mod audit;
mod matching;
mod verification;
mod sidecar_processor;
mod file_processor;
mod job_setup;
mod pipeline;

use serde::{Deserialize, Serialize};

// Re-export public API (preserves external interface)
pub use job_setup::create_ingest_job;
pub use pipeline::{run_ingest_job, run_ingest_job_with_progress};
#[allow(unused_imports)]
pub use verification::{WipeReport, WipeReportEntry, wipe_source_files, run_rescan};
pub(crate) use matching::{
    match_app_profile_rules, match_bundled_profile_rules,
    resolve_stable_camera_refs_with_audit,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestPayload {
    pub source_path: String,
    pub ingest_mode: String,
    #[serde(default)]
    pub session_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraBreakdown {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    pub total_files: usize,
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub clips_created: Vec<i64>,
    pub camera_breakdown: Vec<CameraBreakdown>,
    pub session_id: Option<i64>,
    /// Number of sidecar files discovered and processed (sidecar-importplan 12.7)
    pub sidecar_count: usize,
    /// Number of sidecar files that failed verification (sidecar-importplan 12.7)
    pub sidecar_failed: usize,
}
