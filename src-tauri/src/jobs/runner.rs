// Job runner - executes jobs from the queue

use std::path::Path;
use rusqlite::Connection;
use crate::db::schema;
use crate::jobs::{claim_job, complete_job, fail_job, heartbeat_job, reclaim_expired_jobs};
use crate::ingest;
use crate::error::{DadCamError, Result};

/// Run a single job from the queue
pub fn run_next_job(conn: &Connection, library_root: &Path) -> Result<bool> {
    // First reclaim any expired jobs
    let reclaimed = reclaim_expired_jobs(conn)?;
    if reclaimed > 0 {
        eprintln!("Reclaimed {} expired jobs", reclaimed);
    }

    // Try to claim a job
    let job = match claim_job(conn, None)? {
        Some(j) => j,
        None => return Ok(false), // No jobs available
    };

    let run_token = job.run_token.clone().unwrap_or_default();

    eprintln!("Running job {} (type: {})", job.id, job.job_type);

    // Execute based on job type
    let result = match job.job_type.as_str() {
        "ingest" => run_ingest_job(conn, &job, library_root),
        "hash_full" => run_hash_full_job(conn, &job),
        "proxy" | "thumb" | "sprite" | "score" | "export" | "ml" => {
            // Phase 2+ jobs - not implemented yet
            Err(DadCamError::Other(format!("Job type '{}' not yet implemented", job.job_type)))
        }
        _ => Err(DadCamError::Other(format!("Unknown job type: {}", job.job_type))),
    };

    // Update job status
    match result {
        Ok(_) => {
            complete_job(conn, job.id, &run_token)?;
            eprintln!("Job {} completed successfully", job.id);
        }
        Err(e) => {
            fail_job(conn, job.id, &run_token, &e.to_string())?;
            eprintln!("Job {} failed: {}", job.id, e);
        }
    }

    Ok(true)
}

/// Run all pending jobs
pub fn run_all_jobs(conn: &Connection, library_root: &Path) -> Result<usize> {
    let mut count = 0;
    while run_next_job(conn, library_root)? {
        count += 1;
    }
    Ok(count)
}

/// Run an ingest job
fn run_ingest_job(conn: &Connection, job: &schema::Job, library_root: &Path) -> Result<()> {
    let result = ingest::run_ingest_job(conn, job.id, library_root)?;

    eprintln!(
        "Ingest complete: {} processed, {} skipped, {} failed",
        result.processed, result.skipped, result.failed
    );

    if result.failed > 0 && result.processed == 0 {
        return Err(DadCamError::Ingest("All files failed to ingest".to_string()));
    }

    Ok(())
}

/// Run a full hash job
fn run_hash_full_job(conn: &Connection, job: &schema::Job) -> Result<()> {
    let asset_id = job.asset_id
        .ok_or_else(|| DadCamError::Other("Hash job has no asset_id".to_string()))?;

    let asset = schema::get_asset(conn, asset_id)?
        .ok_or_else(|| DadCamError::AssetNotFound(asset_id))?;

    // Get full path
    let library = schema::get_library(conn, asset.library_id)?
        .ok_or_else(|| DadCamError::LibraryNotFound(asset.library_id.to_string()))?;

    let full_path = Path::new(&library.root_path).join(&asset.path);

    if !full_path.exists() {
        return Err(DadCamError::FileNotFound(full_path.to_string_lossy().to_string()));
    }

    // Compute full hash
    let hash_full = crate::hash::compute_full_hash(&full_path)?;

    // Update asset
    schema::update_asset_hash_full(conn, asset_id, &hash_full)?;
    schema::update_asset_verified(conn, asset_id)?;

    Ok(())
}

/// Count pending jobs by type
pub fn count_pending_jobs(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT type, COUNT(*) FROM jobs WHERE status = 'pending' GROUP BY type ORDER BY type"
    )?;

    let counts = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?.collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(counts)
}
