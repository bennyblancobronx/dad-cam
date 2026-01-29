// Job progress event payload and helpers

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Progress payload emitted during long-running operations.
/// All job types (ingest, export, scoring) use this same shape.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub job_id: String,
    pub phase: String,
    pub current: u64,
    pub total: u64,
    pub percent: f64,
    pub message: String,
    pub is_cancelled: bool,
    pub is_error: bool,
    pub error_message: Option<String>,
}

impl JobProgress {
    pub fn new(job_id: impl Into<String>, phase: impl Into<String>, current: u64, total: u64) -> Self {
        let total_safe = total.max(1);
        let percent = (current as f64 / total_safe as f64) * 100.0;
        Self {
            job_id: job_id.into(),
            phase: phase.into(),
            current,
            total,
            percent: percent.min(100.0),
            message: String::new(),
            is_cancelled: false,
            is_error: false,
            error_message: None,
        }
    }

    pub fn with_message(mut self, msg: impl Into<String>) -> Self {
        self.message = msg.into();
        self
    }

    pub fn cancelled(mut self) -> Self {
        self.is_cancelled = true;
        self
    }

    pub fn error(mut self, msg: impl Into<String>) -> Self {
        self.is_error = true;
        self.error_message = Some(msg.into());
        self
    }
}

/// Emit a job-progress event to the frontend.
pub fn emit_progress(app: &AppHandle, progress: &JobProgress) {
    let _ = app.emit("job-progress", progress);
}

/// Emit a job-progress event when an AppHandle is available.
/// No-op when app is None (e.g. CLI context).
pub fn emit_progress_opt(app: Option<&AppHandle>, progress: &JobProgress) {
    if let Some(app) = app {
        let _ = app.emit("job-progress", progress);
    }
}
