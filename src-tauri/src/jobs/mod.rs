// Job system module

pub mod runner;
pub mod progress;
pub mod rematch;
pub mod reextract;
pub mod profile_update;
pub mod worker;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use uuid::Uuid;
use chrono::Utc;
use crate::db::schema::{self, Job, NewJob};
use crate::constants::{JOB_LEASE_DURATION_SECONDS, JOB_MAX_RETRIES, JOB_BASE_BACKOFF_SECONDS};
use crate::error::{DadCamError, Result};

/// Global registry of cancel flags keyed by job_id string.
/// When a cancel is requested, the corresponding AtomicBool is set to true.
/// Job runners check this flag between phases.
static CANCEL_FLAGS: std::sync::LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a cancel flag for a job. Returns the flag for the runner to check.
pub fn register_cancel_flag(job_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut flags = CANCEL_FLAGS.lock().unwrap();
    flags.insert(job_id.to_string(), Arc::clone(&flag));
    flag
}

/// Request cancellation of a job.
pub fn request_cancel(job_id: &str) -> bool {
    let flags = CANCEL_FLAGS.lock().unwrap();
    if let Some(flag) = flags.get(job_id) {
        flag.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// Remove a cancel flag after a job finishes.
pub fn remove_cancel_flag(job_id: &str) {
    let mut flags = CANCEL_FLAGS.lock().unwrap();
    flags.remove(job_id);
}

/// Check if a job has been cancelled.
pub fn is_cancelled(flag: &AtomicBool) -> bool {
    flag.load(Ordering::Relaxed)
}

/// Claim a pending job with lease
pub fn claim_job(conn: &Connection, job_type: Option<&str>) -> Result<Option<Job>> {
    let worker_id = get_worker_id();
    let run_token = Uuid::new_v4().to_string();
    let lease_expires = Utc::now()
        .checked_add_signed(chrono::Duration::seconds(JOB_LEASE_DURATION_SECONDS))
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    // Find and claim a job atomically
    let sql = match job_type {
        Some(_) => r#"
            UPDATE jobs
            SET status = 'running',
                claimed_by = ?1,
                run_token = ?2,
                lease_expires_at = ?3,
                heartbeat_at = datetime('now'),
                started_at = datetime('now')
            WHERE id = (
                SELECT id FROM jobs
                WHERE type = ?4
                  AND status = 'pending'
                  AND (attempts < ?5 OR attempts IS NULL)
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
            )
            RETURNING id, type, status, library_id, clip_id, asset_id, priority, attempts,
                      last_error, progress, payload, claimed_by, run_token, lease_expires_at,
                      heartbeat_at, created_at, started_at, completed_at
        "#,
        None => r#"
            UPDATE jobs
            SET status = 'running',
                claimed_by = ?1,
                run_token = ?2,
                lease_expires_at = ?3,
                heartbeat_at = datetime('now'),
                started_at = datetime('now')
            WHERE id = (
                SELECT id FROM jobs
                WHERE status = 'pending'
                  AND (attempts < ?4 OR attempts IS NULL)
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
            )
            RETURNING id, type, status, library_id, clip_id, asset_id, priority, attempts,
                      last_error, progress, payload, claimed_by, run_token, lease_expires_at,
                      heartbeat_at, created_at, started_at, completed_at
        "#,
    };

    let result = match job_type {
        Some(jt) => conn.query_row(
            sql,
            rusqlite::params![worker_id, run_token, lease_expires, jt, JOB_MAX_RETRIES],
            |row| {
                Ok(Job {
                    id: row.get(0)?,
                    job_type: row.get(1)?,
                    status: row.get(2)?,
                    library_id: row.get(3)?,
                    clip_id: row.get(4)?,
                    asset_id: row.get(5)?,
                    priority: row.get(6)?,
                    attempts: row.get(7)?,
                    last_error: row.get(8)?,
                    progress: row.get(9)?,
                    payload: row.get(10)?,
                    claimed_by: row.get(11)?,
                    run_token: row.get(12)?,
                    lease_expires_at: row.get(13)?,
                    heartbeat_at: row.get(14)?,
                    created_at: row.get(15)?,
                    started_at: row.get(16)?,
                    completed_at: row.get(17)?,
                })
            },
        ),
        None => conn.query_row(
            sql,
            rusqlite::params![worker_id, run_token, lease_expires, JOB_MAX_RETRIES],
            |row| {
                Ok(Job {
                    id: row.get(0)?,
                    job_type: row.get(1)?,
                    status: row.get(2)?,
                    library_id: row.get(3)?,
                    clip_id: row.get(4)?,
                    asset_id: row.get(5)?,
                    priority: row.get(6)?,
                    attempts: row.get(7)?,
                    last_error: row.get(8)?,
                    progress: row.get(9)?,
                    payload: row.get(10)?,
                    claimed_by: row.get(11)?,
                    run_token: row.get(12)?,
                    lease_expires_at: row.get(13)?,
                    heartbeat_at: row.get(14)?,
                    created_at: row.get(15)?,
                    started_at: row.get(16)?,
                    completed_at: row.get(17)?,
                })
            },
        ),
    };

    match result {
        Ok(job) => Ok(Some(job)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DadCamError::Database(e)),
    }
}

/// Complete a job successfully
pub fn complete_job(conn: &Connection, job_id: i64, run_token: &str) -> Result<bool> {
    let rows = conn.execute(
        "UPDATE jobs SET status = 'completed', completed_at = datetime('now'), progress = 100
         WHERE id = ?1 AND run_token = ?2 AND status = 'running'",
        rusqlite::params![job_id, run_token],
    )?;

    Ok(rows > 0)
}

/// Fail a job with error message
pub fn fail_job(conn: &Connection, job_id: i64, run_token: &str, error: &str) -> Result<bool> {
    let job = schema::get_job(conn, job_id)?
        .ok_or_else(|| DadCamError::JobNotFound(job_id))?;

    let new_attempts = job.attempts + 1;

    if new_attempts >= JOB_MAX_RETRIES {
        // Max retries exceeded, mark as failed
        let rows = conn.execute(
            "UPDATE jobs SET status = 'failed', last_error = ?1, attempts = ?2, completed_at = datetime('now')
             WHERE id = ?3 AND run_token = ?4",
            rusqlite::params![error, new_attempts, job_id, run_token],
        )?;
        Ok(rows > 0)
    } else {
        // Schedule for retry with exponential backoff
        let backoff = JOB_BASE_BACKOFF_SECONDS * (2_i64.pow(new_attempts as u32 - 1));
        let retry_after = Utc::now()
            .checked_add_signed(chrono::Duration::seconds(backoff))
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let rows = conn.execute(
            "UPDATE jobs SET status = 'pending', last_error = ?1, attempts = ?2,
             claimed_by = NULL, run_token = NULL, lease_expires_at = ?3
             WHERE id = ?4 AND run_token = ?5",
            rusqlite::params![error, new_attempts, retry_after, job_id, run_token],
        )?;
        Ok(rows > 0)
    }
}

/// Reclaim expired/abandoned jobs
pub fn reclaim_expired_jobs(conn: &Connection) -> Result<usize> {
    let rows = conn.execute(
        "UPDATE jobs SET status = 'pending', claimed_by = NULL, run_token = NULL
         WHERE status = 'running'
           AND lease_expires_at < datetime('now')
           AND attempts < ?1",
        rusqlite::params![JOB_MAX_RETRIES],
    )?;

    Ok(rows)
}

/// Get worker identifier
fn get_worker_id() -> String {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let pid = std::process::id();
    format!("{}:{}", hostname, pid)
}

/// Create a new job
pub fn create_job(conn: &Connection, job: &NewJob) -> Result<i64> {
    schema::insert_job(conn, job)
}
