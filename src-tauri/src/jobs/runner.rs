// Job runner - executes jobs from the queue

use std::path::Path;
use rusqlite::Connection;
use tauri::AppHandle;
use crate::db::schema;
use crate::jobs::{claim_job, complete_job, fail_job, reclaim_expired_jobs};
use crate::jobs::progress::{JobProgress, emit_progress_opt};
use crate::ingest;
use crate::preview;
use crate::scoring;
use crate::error::{DadCamError, Result};
use crate::constants::PIPELINE_VERSION;

/// Run a single job from the queue.
/// When app is Some, emits job-progress events to the frontend.
pub fn run_next_job(conn: &Connection, library_root: &Path, app: Option<&AppHandle>) -> Result<bool> {
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
    let job_id_str = job.id.to_string();

    eprintln!("Running job {} (type: {})", job.id, job.job_type);

    // Emit starting progress
    emit_progress_opt(app, &JobProgress::new(&job_id_str, &job.job_type, 0, 1)
        .with_message(format!("Starting {} job", job.job_type)));

    // Execute based on job type
    let result = match job.job_type.as_str() {
        "ingest" => run_ingest_job(conn, &job, library_root, app),
        "hash_full" => run_hash_full_job(conn, &job, app),
        "proxy" => run_proxy_job(conn, &job, library_root, app),
        "thumb" => run_thumb_job(conn, &job, library_root, app),
        "sprite" => run_sprite_job(conn, &job, library_root, app),
        "score" => run_score_job(conn, &job, library_root, app),
        "export" => {
            // VHS export is invoked directly via start_vhs_export command, not through the job queue.
            Err(DadCamError::Other("Export jobs run via start_vhs_export command, not the job queue".to_string()))
        }
        "ml" => {
            Err(DadCamError::Other(format!("Job type '{}' not yet implemented", job.job_type)))
        }
        _ => Err(DadCamError::Other(format!("Unknown job type: {}", job.job_type))),
    };

    // Update job status and emit completion/error
    match result {
        Ok(_) => {
            complete_job(conn, job.id, &run_token)?;
            emit_progress_opt(app, &JobProgress::new(&job_id_str, "complete", 1, 1)
                .with_message(format!("{} job completed", job.job_type)));
            eprintln!("Job {} completed successfully", job.id);
        }
        Err(ref e) => {
            fail_job(conn, job.id, &run_token, &e.to_string())?;
            emit_progress_opt(app, &JobProgress::new(&job_id_str, "failed", 0, 1)
                .error(e.to_string()));
            eprintln!("Job {} failed: {}", job.id, e);
        }
    }

    // Propagate error after recording it
    result.map(|_| true)
}

/// Run all pending jobs.
/// When app is Some, emits job-progress events to the frontend.
pub fn run_all_jobs(conn: &Connection, library_root: &Path, app: Option<&AppHandle>) -> Result<usize> {
    let mut count = 0;
    while run_next_job(conn, library_root, app)? {
        count += 1;
    }
    Ok(count)
}

/// Run an ingest job
fn run_ingest_job(conn: &Connection, job: &schema::Job, library_root: &Path, app: Option<&AppHandle>) -> Result<()> {
    let job_id_str = job.id.to_string();

    // Register cancel flag so this job can be cancelled
    let cancel_flag = crate::jobs::register_cancel_flag(&job_id_str);

    let result = match app {
        Some(app_handle) => ingest::run_ingest_job_with_progress(conn, job.id, library_root, app_handle, &cancel_flag),
        None => ingest::run_ingest_job(conn, job.id, library_root),
    };

    crate::jobs::remove_cancel_flag(&job_id_str);

    let result = result?;

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
fn run_hash_full_job(conn: &Connection, job: &schema::Job, app: Option<&AppHandle>) -> Result<()> {
    let asset_id = job.asset_id
        .ok_or_else(|| DadCamError::Other("Hash job has no asset_id".to_string()))?;
    let job_id_str = job.id.to_string();

    let asset = schema::get_asset(conn, asset_id)?
        .ok_or_else(|| DadCamError::AssetNotFound(asset_id))?;

    // Get full path
    let library = schema::get_library(conn, asset.library_id)?
        .ok_or_else(|| DadCamError::LibraryNotFound(asset.library_id.to_string()))?;

    let full_path = Path::new(&library.root_path).join(&asset.path);

    if !full_path.exists() {
        return Err(DadCamError::FileNotFound(full_path.to_string_lossy().to_string()));
    }

    emit_progress_opt(app, &JobProgress::new(&job_id_str, "hashing", 0, 1)
        .with_message("Computing full file hash"));

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

/// Run a proxy generation job
fn run_proxy_job(conn: &Connection, job: &schema::Job, library_root: &Path, app: Option<&AppHandle>) -> Result<()> {
    let clip_id = job.clip_id
        .ok_or_else(|| DadCamError::Other("Proxy job has no clip_id".to_string()))?;
    let job_id_str = job.id.to_string();

    // Get clip and its original asset
    let clip = schema::get_clip(conn, clip_id)?
        .ok_or_else(|| DadCamError::ClipNotFound(clip_id))?;

    let original = preview::get_clip_original_asset(conn, clip_id)?
        .ok_or_else(|| DadCamError::AssetNotFound(clip.original_asset_id))?;

    // Build source path
    let source_path = library_root.join(&original.path);
    if !source_path.exists() {
        return Err(DadCamError::FileNotFound(source_path.to_string_lossy().to_string()));
    }

    emit_progress_opt(app, &JobProgress::new(&job_id_str, "proxy", 0, 1)
        .with_message(format!("Generating proxy for clip {}", clip_id)));

    // Determine parameters based on media type
    let deinterlace = if clip.media_type == "video" {
        let meta = crate::metadata::ffprobe::probe(&source_path)?;
        preview::proxy::needs_deinterlace(&meta)
    } else {
        false
    };

    // Create derived params (include camera_profile_id and source hash for invalidation)
    let params = preview::DerivedParams::for_proxy(
        deinterlace,
        None, // No LUT for now
        clip.camera_profile_id,
        original.hash_fast.clone(),
    );

    // Check for existing asset and if it's stale
    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "proxy")? {
        if !preview::is_asset_stale(&existing, &params, original.hash_fast.as_deref()) {
            // Already up to date
            return Ok(());
        }
        // Remove old file
        let old_path = library_root.join(&existing.path);
        let _ = std::fs::remove_file(&old_path);
    }

    // Determine extension and generate
    let extension = if clip.media_type == "audio" { "m4a" } else { "mp4" };
    let output_path = preview::get_derived_path(library_root, clip_id, "proxy", &params, extension);

    // Generate the proxy
    if clip.media_type == "audio" {
        preview::proxy::generate_audio_proxy(&source_path, &output_path)
            .map_err(|e| DadCamError::FFmpeg(e.to_string()))?;
    } else {
        let options = preview::proxy::ProxyOptions {
            deinterlace,
            target_fps: 30,
            lut_path: None,
        };
        preview::proxy::generate_proxy(&source_path, &output_path, &options)
            .map_err(|e| DadCamError::FFmpeg(e.to_string()))?;
    }

    // Get file size
    let size = std::fs::metadata(&output_path)?.len() as i64;

    // Store relative path
    let rel_path = preview::to_relative_path(library_root, &output_path);

    // Create or update asset record
    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "proxy")? {
        preview::update_derived_asset(
            conn,
            existing.id,
            &rel_path,
            size,
            PIPELINE_VERSION,
            &params.to_json(),
        )?;
    } else {
        let asset_id = preview::create_derived_asset(
            conn,
            clip.library_id,
            "proxy",
            &rel_path,
            size,
            PIPELINE_VERSION,
            &params.to_json(),
        )?;
        schema::link_clip_asset(conn, clip_id, asset_id, "proxy")?;
    }

    Ok(())
}

/// Run a thumbnail generation job
fn run_thumb_job(conn: &Connection, job: &schema::Job, library_root: &Path, app: Option<&AppHandle>) -> Result<()> {
    let clip_id = job.clip_id
        .ok_or_else(|| DadCamError::Other("Thumb job has no clip_id".to_string()))?;
    let job_id_str = job.id.to_string();

    let clip = schema::get_clip(conn, clip_id)?
        .ok_or_else(|| DadCamError::ClipNotFound(clip_id))?;

    let original = preview::get_clip_original_asset(conn, clip_id)?
        .ok_or_else(|| DadCamError::AssetNotFound(clip.original_asset_id))?;

    let source_path = library_root.join(&original.path);
    if !source_path.exists() {
        return Err(DadCamError::FileNotFound(source_path.to_string_lossy().to_string()));
    }

    emit_progress_opt(app, &JobProgress::new(&job_id_str, "thumb", 0, 1)
        .with_message(format!("Generating thumbnail for clip {}", clip_id)));

    // Create params (include camera_profile_id and source hash)
    let params = preview::DerivedParams::for_thumb(
        clip.camera_profile_id,
        original.hash_fast.clone(),
    );

    // Check for existing
    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "thumb")? {
        if !preview::is_asset_stale(&existing, &params, original.hash_fast.as_deref()) {
            return Ok(());
        }
        let old_path = library_root.join(&existing.path);
        let _ = std::fs::remove_file(&old_path);
    }

    let output_path = preview::get_derived_path(library_root, clip_id, "thumb", &params, "jpg");
    let options = preview::thumb::ThumbOptions::default();

    // Generate based on media type
    match clip.media_type.as_str() {
        "video" => {
            preview::thumb::generate_thumbnail(&source_path, &output_path, clip.duration_ms, &options)
                .map_err(|e| DadCamError::FFmpeg(e.to_string()))?;
        }
        "audio" => {
            preview::thumb::generate_audio_thumbnail(&source_path, &output_path, &options)
                .map_err(|e| DadCamError::FFmpeg(e.to_string()))?;
        }
        "image" => {
            preview::thumb::generate_image_thumbnail(&source_path, &output_path, &options)
                .map_err(|e| DadCamError::FFmpeg(e.to_string()))?;
        }
        _ => {
            return Err(DadCamError::Other(format!(
                "Unsupported media type for thumbnail: {}",
                clip.media_type
            )));
        }
    }

    let size = std::fs::metadata(&output_path)?.len() as i64;
    let rel_path = preview::to_relative_path(library_root, &output_path);

    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "thumb")? {
        preview::update_derived_asset(
            conn,
            existing.id,
            &rel_path,
            size,
            PIPELINE_VERSION,
            &params.to_json(),
        )?;
    } else {
        let asset_id = preview::create_derived_asset(
            conn,
            clip.library_id,
            "thumb",
            &rel_path,
            size,
            PIPELINE_VERSION,
            &params.to_json(),
        )?;
        schema::link_clip_asset(conn, clip_id, asset_id, "thumb")?;
    }

    Ok(())
}

/// Run a sprite sheet generation job
fn run_sprite_job(conn: &Connection, job: &schema::Job, library_root: &Path, app: Option<&AppHandle>) -> Result<()> {
    let clip_id = job.clip_id
        .ok_or_else(|| DadCamError::Other("Sprite job has no clip_id".to_string()))?;
    let job_id_str = job.id.to_string();

    let clip = schema::get_clip(conn, clip_id)?
        .ok_or_else(|| DadCamError::ClipNotFound(clip_id))?;

    // Sprites only make sense for video
    if clip.media_type != "video" {
        return Ok(());
    }

    let duration_ms = clip.duration_ms.ok_or_else(|| {
        DadCamError::Other("Clip has no duration for sprite generation".to_string())
    })?;

    let original = preview::get_clip_original_asset(conn, clip_id)?
        .ok_or_else(|| DadCamError::AssetNotFound(clip.original_asset_id))?;

    let source_path = library_root.join(&original.path);
    if !source_path.exists() {
        return Err(DadCamError::FileNotFound(source_path.to_string_lossy().to_string()));
    }

    emit_progress_opt(app, &JobProgress::new(&job_id_str, "sprite", 0, 1)
        .with_message(format!("Generating sprite sheet for clip {}", clip_id)));

    // Create params (include camera_profile_id and source hash)
    let params = preview::DerivedParams::for_sprite(
        duration_ms,
        clip.camera_profile_id,
        original.hash_fast.clone(),
    );

    // Check for existing
    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "sprite")? {
        if !preview::is_asset_stale(&existing, &params, original.hash_fast.as_deref()) {
            return Ok(());
        }
        let old_path = library_root.join(&existing.path);
        let _ = std::fs::remove_file(&old_path);
        // Also remove VTT and JSON metadata if exist
        let old_vtt = old_path.with_extension("vtt");
        let old_json = old_path.with_extension("json");
        let _ = std::fs::remove_file(&old_vtt);
        let _ = std::fs::remove_file(&old_json);
    }

    let output_path = preview::get_derived_path(library_root, clip_id, "sprite", &params, "jpg");
    let options = preview::sprite::SpriteOptions::default();

    let info = preview::sprite::generate_sprite_sheet(&source_path, &output_path, duration_ms, &options)
        .map_err(|e| DadCamError::FFmpeg(e.to_string()))?;

    // Generate VTT file
    let sprite_filename = output_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("{}.jpg", clip_id));
    let vtt_content = preview::sprite::generate_vtt(&sprite_filename, duration_ms, &info);
    let vtt_path = output_path.with_extension("vtt");
    std::fs::write(&vtt_path, vtt_content)?;

    // Save sprite metadata JSON (per phase-2.md spec)
    let metadata = preview::sprite::SpriteMetadata::from(&info);
    preview::sprite::save_sprite_metadata(&output_path, &metadata)
        .map_err(|e| DadCamError::Other(format!("Failed to save sprite metadata: {}", e)))?;

    let size = std::fs::metadata(&output_path)?.len() as i64;
    let rel_path = preview::to_relative_path(library_root, &output_path);

    if let Some(existing) = preview::find_derived_asset(conn, clip_id, "sprite")? {
        preview::update_derived_asset(
            conn,
            existing.id,
            &rel_path,
            size,
            PIPELINE_VERSION,
            &params.to_json(),
        )?;
    } else {
        let asset_id = preview::create_derived_asset(
            conn,
            clip.library_id,
            "sprite",
            &rel_path,
            size,
            PIPELINE_VERSION,
            &params.to_json(),
        )?;
        schema::link_clip_asset(conn, clip_id, asset_id, "sprite")?;
    }

    Ok(())
}

/// Run a scoring job
fn run_score_job(conn: &Connection, job: &schema::Job, library_root: &Path, app: Option<&AppHandle>) -> Result<()> {
    let clip_id = job.clip_id
        .ok_or_else(|| DadCamError::Other("Score job has no clip_id".to_string()))?;
    let job_id_str = job.id.to_string();

    let _clip = schema::get_clip(conn, clip_id)?
        .ok_or_else(|| DadCamError::ClipNotFound(clip_id))?;

    // Check if already scored and up to date
    if !scoring::analyzer::needs_scoring(conn, clip_id)? {
        eprintln!("Clip {} already has up-to-date score", clip_id);
        return Ok(());
    }

    emit_progress_opt(app, &JobProgress::new(&job_id_str, "scoring", 0, 1)
        .with_message(format!("Scoring clip {}", clip_id)));

    // Run analysis
    let result = scoring::analyzer::analyze_clip(conn, clip_id, library_root, false)?;

    // Save the score
    scoring::analyzer::save_clip_score(conn, &result)?;

    eprintln!("Scored clip {}: {:.2} (scene={:.2}, audio={:.2}, sharp={:.2}, motion={:.2})",
        clip_id,
        result.overall_score,
        result.scene_score,
        result.audio_score,
        result.sharpness_score,
        result.motion_score
    );

    Ok(())
}
