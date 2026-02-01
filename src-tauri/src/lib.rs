// Dad Cam - Tauri Library Entry Point

pub mod constants;
pub mod error;
pub mod tools;
pub mod db;
pub mod hash;
pub mod metadata;
pub mod ingest;
pub mod jobs;
pub mod camera;
pub mod preview;
pub mod scoring;
pub mod licensing;
pub mod export;
pub mod commands;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use serde::{Deserialize, Serialize};

use db::{open_db, get_db_path};
use db::schema::{self, Job};
use jobs::progress::{JobProgress, emit_progress};

// Re-export DbState from commands module for state management
pub use commands::DbState;

// Ingest-specific response type (not part of Phase 3 commands)
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestResponse {
    pub job_id: i64,
    pub total_files: usize,
    pub processed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub clips_created: Vec<i64>,
    pub camera_breakdown: Vec<ingest::CameraBreakdown>,
    pub session_id: Option<i64>,
    pub sidecar_count: usize,
    pub sidecar_failed: usize,
}

// Ingest Commands (separate from Phase 3 clip/library/tag commands)

#[tauri::command]
fn start_ingest(
    app: AppHandle,
    source_path: String,
    library_path: String,
    event_id: Option<i64>,
    new_event_name: Option<String>,
) -> Result<IngestResponse, String> {
    // License check
    if !licensing::is_allowed("import") {
        return Err("Trial expired. Please activate a license to import footage.".to_string());
    }

    let library_root = PathBuf::from(&library_path);
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path).map_err(|e| e.to_string())?;

    let lib = schema::get_library_by_path(&conn, &library_path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Library not found".to_string())?;

    // Create and run ingest job
    let job_id = ingest::create_ingest_job(&conn, lib.id, &source_path, &lib.ingest_mode)
        .map_err(|e| e.to_string())?;

    let job_id_str = job_id.to_string();

    // Register cancel flag so frontend can cancel this job
    let cancel_flag = jobs::register_cancel_flag(&job_id_str);

    // Emit initial progress
    emit_progress(&app, &JobProgress::new(&job_id_str, "discover", 0, 1)
        .with_message("Discovering files..."));

    let result = ingest::run_ingest_job_with_progress(&conn, job_id, &library_root, &app, &cancel_flag)
        .map_err(|e| {
            jobs::remove_cancel_flag(&job_id_str);
            e.to_string()
        })?;

    // Clean up cancel flag
    jobs::remove_cancel_flag(&job_id_str);

    // Link clips to event if requested
    if !result.clips_created.is_empty() {
        let target_event_id = if let Some(name) = &new_event_name {
            // Create a new event, then use its ID
            let new_event = schema::NewEvent {
                library_id: lib.id,
                name: name.clone(),
                description: None,
                event_type: "clip_selection".to_string(),
                date_start: None,
                date_end: None,
                color: None,
                icon: None,
            };
            Some(schema::insert_event(&conn, &new_event).map_err(|e| e.to_string())?)
        } else {
            event_id
        };

        if let Some(eid) = target_event_id {
            schema::add_clips_to_event(&conn, eid, &result.clips_created)
                .map_err(|e| e.to_string())?;
        }
    }

    // Emit previews phase (preview jobs are queued as background work)
    let total = result.total_files as u64;
    if result.processed > 0 {
        emit_progress(&app, &JobProgress::new(&job_id_str, "previews", total, total)
            .with_message(format!("Queued preview generation for {} clips", result.processed)));
    }

    // Emit completion
    let completion_msg = if result.sidecar_count > 0 {
        format!(
            "Import complete: {} processed ({} sidecars), {} skipped, {} failed",
            result.processed, result.sidecar_count, result.skipped, result.failed
        )
    } else {
        format!(
            "Import complete: {} processed, {} skipped, {} failed",
            result.processed, result.skipped, result.failed
        )
    };
    emit_progress(&app, &JobProgress::new(&job_id_str, "complete", total, total)
        .with_message(completion_msg));

    Ok(IngestResponse {
        job_id,
        total_files: result.total_files,
        processed: result.processed,
        skipped: result.skipped,
        failed: result.failed,
        clips_created: result.clips_created,
        camera_breakdown: result.camera_breakdown,
        session_id: result.session_id,
        sidecar_count: result.sidecar_count,
        sidecar_failed: result.sidecar_failed,
    })
}

#[tauri::command]
fn cancel_job(job_id: String) -> Result<bool, String> {
    Ok(jobs::request_cancel(&job_id))
}

#[tauri::command]
fn get_jobs(state: State<DbState>, status: Option<String>, limit: i64) -> Result<Vec<Job>, String> {
    let conn = state.connect()?;

    let jobs = schema::list_jobs(&conn, None, status.as_deref(), limit)
        .map_err(|e| e.to_string())?;

    Ok(jobs)
}

/// Sync bundled camera profiles from resources/cameras/bundled_profiles.json into App DB.
/// Best-effort: failures are logged but never block startup.
fn sync_bundled_profiles_at_startup() {
    let conn = match db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Cannot sync bundled profiles (App DB unavailable): {}", e);
            return;
        }
    };

    // Try known locations for bundled_profiles.json
    let candidates = [
        std::path::PathBuf::from("resources/cameras/bundled_profiles.json"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("../Resources/resources/cameras/bundled_profiles.json")))
            .unwrap_or_default(),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("resources/cameras/bundled_profiles.json")))
            .unwrap_or_default(),
    ];

    for path in &candidates {
        if path.exists() {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("Failed to read bundled_profiles.json: {}", e);
                    return;
                }
            };

            let entries: Vec<db::app_schema::BundledProfileJsonEntry> = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("Failed to parse bundled_profiles.json: {}", e);
                    return;
                }
            };

            match db::app_schema::sync_bundled_profiles(&conn, &entries) {
                Ok(count) => {
                    if count > 0 {
                        log::info!("Synced {} bundled camera profiles to App DB", count);
                    }
                }
                Err(e) => log::warn!("Failed to sync bundled profiles: {}", e),
            }
            return;
        }
    }
}

/// One-time migration: copy Tauri Store (settings.json) into App DB (Spec 6.3).
/// Runs inside .setup() so the store plugin is available.
/// Best-effort: failures logged, never block startup.
fn migrate_tauri_store_to_app_db(app: &mut tauri::App) {
    use tauri_plugin_store::StoreExt;

    let app_conn = match db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(_) => return,
    };

    // Skip if already migrated
    if let Ok(Some(val)) = db::app_schema::get_setting(&app_conn, "tauri_store_migrated") {
        if val == "true" {
            return;
        }
    }

    // Open Tauri Store
    let store = match app.store("settings.json") {
        Ok(s) => s,
        Err(_) => {
            // No store file = nothing to migrate. Mark done.
            let _ = db::app_schema::set_setting(&app_conn, "tauri_store_migrated", "true");
            return;
        }
    };

    // Check if store has any data (version=0 means empty/nonexistent)
    let version = store.get("version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if version == 0 {
        let _ = db::app_schema::set_setting(&app_conn, "tauri_store_migrated", "true");
        return;
    }

    // ui_mode
    let mode_str = store.get("mode")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "simple".to_string());
    let _ = db::app_schema::set_ui_mode(&app_conn, &mode_str);

    // features (JSON)
    if let Some(ff) = store.get("featureFlags") {
        let _ = db::app_schema::set_features(&app_conn, &ff.to_string());
    }

    // title_card_offset_seconds from devMenu.titleStartSeconds
    if let Some(dm) = store.get("devMenu") {
        if let Some(tss) = dm.get("titleStartSeconds").and_then(|v| v.as_f64()) {
            let _ = db::app_schema::set_title_offset(&app_conn, tss);
        }
        // Also persist full devMenu blob
        let _ = db::app_schema::set_setting(&app_conn, "dev_menu", &dm.to_string());
    }

    // simple_default_library_uuid: resolve from defaultProjectPath
    if let Some(default_path) = store.get("defaultProjectPath")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
    {
        if !default_path.is_empty() {
            let lib_db_path = std::path::Path::new(&default_path)
                .join(constants::DADCAM_FOLDER)
                .join(constants::DB_FILENAME);
            if lib_db_path.exists() {
                if let Ok(lib_conn) = db::open_db(&lib_db_path) {
                    if let Ok(uuid) = db::app_schema::get_or_create_library_uuid(&lib_conn) {
                        let _ = db::app_schema::set_simple_default_library_uuid(&app_conn, &uuid);
                    }
                }
            }
        }
    }

    // recentProjects -> library registry
    if let Some(rp) = store.get("recentProjects") {
        if let Ok(projects) = serde_json::from_value::<Vec<serde_json::Value>>(rp.clone()) {
            for project in &projects {
                if let Some(path) = project.get("path").and_then(|v| v.as_str()) {
                    let label = project.get("name").and_then(|v| v.as_str());
                    let lib_db_path = std::path::Path::new(path)
                        .join(constants::DADCAM_FOLDER)
                        .join(constants::DB_FILENAME);
                    if lib_db_path.exists() {
                        if let Ok(lib_conn) = db::open_db(&lib_db_path) {
                            if let Ok(uuid) = db::app_schema::get_or_create_library_uuid(&lib_conn) {
                                let _ = db::app_schema::upsert_library(&app_conn, &uuid, path, label);
                            }
                        }
                    }
                }
            }
        }
    }

    // firstRunCompleted
    if let Some(frc) = store.get("firstRunCompleted") {
        let _ = db::app_schema::set_setting(&app_conn, "first_run_completed", &frc.to_string());
    }

    // theme
    if let Some(theme) = store.get("theme").and_then(|v| v.as_str().map(|s| s.to_string())) {
        let _ = db::app_schema::set_setting(&app_conn, "theme", &theme);
    }

    // licenseStateCache
    if let Some(lsc) = store.get("licenseStateCache") {
        let _ = db::app_schema::set_setting(&app_conn, "license_state_cache", &lsc.to_string());
    }

    // Mark as migrated (do not delete old store file, per spec 6.3)
    let _ = db::app_schema::set_setting(&app_conn, "tauri_store_migrated", "true");

    log::info!("Migrated Tauri Store settings to App DB");
}

/// Import legacy ~/.dadcam/custom_cameras.json into App DB (one-time migration).
fn import_legacy_devices_at_startup() {
    let conn = match db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(_) => return,
    };

    let count = db::app_schema::import_legacy_devices_json(&conn);
    if count > 0 {
        log::info!("Imported {} legacy camera devices to App DB", count);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize App DB (creates ~/.dadcam/app.db with migrations)
    // Must run before Tauri app starts so commands can use it.
    if let Err(e) = db::app_db::ensure_app_db_initialized() {
        log::warn!("Failed to initialize App DB: {}", e);
        // Non-fatal: app can still run but registry/settings features will fail gracefully
    }

    // Sync bundled camera profiles to App DB (idempotent)
    sync_bundled_profiles_at_startup();

    // Import legacy ~/.dadcam/custom_cameras.json into App DB (one-time)
    import_legacy_devices_at_startup();

    // Check exiftool availability (non-fatal: metadata extraction degrades gracefully)
    if let Err(e) = tools::ensure_exiftool() {
        log::warn!("{}", e);
    }

    // Initialize logging plugin: writes to OS log directory + stdout.
    // macOS: ~/Library/Logs/com.dadcam.app/
    // Windows: %APPDATA%/com.dadcam.app/logs/
    // Linux: ~/.config/com.dadcam.app/logs/
    let log_plugin = tauri_plugin_log::Builder::new()
        .targets([
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir { file_name: None }),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
            tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
        ])
        .max_file_size(5_000_000) // 5MB per log file
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
        .level(if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .build();

    // Restore saved log level from App DB (if set by user).
    // Deferred to setup() because the log plugin must initialize first.
    if let Ok(app_conn) = db::app_db::open_app_db_connection() {
        if let Ok(Some(level)) = db::app_schema::get_setting(&app_conn, "log_level") {
            std::env::set_var("DADCAM_LOG_LEVEL", level);
        }
    }

    tauri::Builder::default()
        .plugin(log_plugin)
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .manage(DbState(Mutex::new(None)))
        .manage(jobs::worker::WorkerState::new())
        .setup(|app| {
            // Apply saved log level (deferred from pre-build so plugin is initialized)
            if let Ok(level_str) = std::env::var("DADCAM_LOG_LEVEL") {
                let filter = match level_str.as_str() {
                    "debug" => log::LevelFilter::Debug,
                    "warn" => log::LevelFilter::Warn,
                    "error" => log::LevelFilter::Error,
                    _ => log::LevelFilter::Info,
                };
                log::set_max_level(filter);
            }

            migrate_tauri_store_to_app_db(app);
            // Spawn background job worker thread
            let worker_state: tauri::State<jobs::worker::WorkerState> = app.state();
            let library_arc = worker_state.library_arc();
            jobs::worker::spawn_worker(app.handle().clone(), library_arc);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Phase 3 commands from commands module
            commands::open_library,
            commands::close_library,
            commands::create_library,
            commands::get_library_root,
            commands::get_clips,
            commands::get_clip,
            commands::get_clips_filtered,
            commands::get_clip_view,
            commands::toggle_tag,
            commands::set_tag,
            // Phase 4 scoring commands
            commands::get_clip_score,
            commands::score_clip,
            commands::get_scoring_status,
            commands::get_best_clips,
            commands::set_score_override,
            commands::clear_score_override,
            commands::queue_scoring_jobs,
            // App settings commands
            commands::get_app_settings,
            commands::save_app_settings,
            commands::get_mode,
            commands::set_mode,
            commands::add_recent_library,
            commands::remove_recent_library,
            commands::get_recent_libraries,
            commands::validate_library_path,
            commands::list_registry_libraries,
            // Stills export command
            commands::export_still,
            // Events commands (Phase 6)
            commands::create_event,
            commands::get_events,
            commands::get_event,
            commands::update_event,
            commands::delete_event,
            commands::add_clips_to_event,
            commands::remove_clips_from_event,
            commands::get_event_clips,
            commands::get_clips_grouped_by_date,
            commands::get_clips_by_date,
            // Ingest verification commands
            commands::get_session_status,
            commands::get_session_by_job,
            commands::export_audit_report,
            commands::wipe_source_files,
            // Ingest commands
            start_ingest,
            get_jobs,
            cancel_job,
            // Licensing commands
            commands::get_license_state,
            commands::activate_license,
            commands::deactivate_license,
            commands::is_feature_allowed,
            // VHS Export commands
            commands::start_vhs_export,
            commands::get_export_history,
            commands::cancel_export,
            // Camera system commands (Phase 2: App DB)
            commands::list_camera_profiles,
            commands::list_camera_devices,
            commands::register_camera_device,
            commands::match_camera,
            commands::import_camera_db,
            commands::export_camera_db,
            commands::create_user_camera_profile,
            commands::update_user_camera_profile,
            commands::delete_user_camera_profile,
            // Dev menu commands (Phase 9)
            commands::test_ffmpeg,
            commands::clear_caches,
            commands::export_database,
            commands::export_exif_dump,
            commands::execute_raw_sql,
            commands::generate_rental_keys,
            commands::get_db_stats,
            // Profile staging commands (spec 3.6)
            commands::stage_profile_edit,
            commands::list_staged_profiles,
            commands::validate_staged_profiles,
            commands::publish_staged_profiles,
            commands::discard_staged_profiles,
            // Diagnostics commands
            commands::get_diagnostics_enabled,
            commands::set_diagnostics_enabled,
            commands::get_log_directory,
            commands::export_logs,
            commands::export_support_bundle,
            commands::get_system_health,
            commands::get_log_level,
            commands::set_log_level,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
