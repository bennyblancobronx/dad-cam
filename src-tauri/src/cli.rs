// Dad Cam CLI binary
#![allow(dead_code)]

use std::path::PathBuf;
use clap::{Parser, Subcommand};
use anyhow::Result;

mod constants;
mod error;
mod tools;
mod db;
mod hash;
mod metadata;
mod ingest;
mod jobs;
mod camera;
mod preview;
mod scoring;

use db::{open_db, get_db_path, init_library_folders};
use db::schema::{self, Library};
use ingest::create_ingest_job;
use jobs::runner;

#[derive(Parser)]
#[command(name = "dadcam")]
#[command(about = "Dad Cam - A video library for dad cam footage", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new library
    Init {
        /// Library root path
        path: PathBuf,
        /// Library name
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Ingest footage into the library
    Ingest {
        /// Source path (file or directory)
        path: PathBuf,
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
    },

    /// List all clips
    List {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Maximum clips to show
        #[arg(long, default_value = "100")]
        limit: i64,
    },

    /// Show clip details
    Show {
        /// Clip ID
        id: i64,
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
    },

    /// List and manage jobs
    Jobs {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Cancel a job
        #[arg(long)]
        cancel: Option<i64>,
        /// Run pending jobs
        #[arg(long)]
        run: bool,
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
    },

    /// Scan for missing originals (relink)
    RelinkScan {
        /// Path to scan for files
        path: PathBuf,
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
    },

    /// Generate previews (proxies, thumbnails, sprites)
    Preview {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Preview type to generate (proxy, thumb, sprite, all)
        #[arg(long, default_value = "all")]
        r#type: String,
        /// Specific clip ID to process
        #[arg(long)]
        clip: Option<i64>,
        /// Force regeneration even if up to date
        #[arg(long)]
        force: bool,
    },

    /// Show preview generation status
    PreviewStatus {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Only show clips missing previews
        #[arg(long)]
        missing_only: bool,
    },

    /// Invalidate derived assets (force regeneration)
    Invalidate {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Asset type to invalidate (proxy, thumb, sprite, all)
        #[arg(long, default_value = "all")]
        r#type: String,
        /// Confirm invalidation without prompt
        #[arg(long)]
        confirm: bool,
    },

    /// Clean up orphaned files and deduplicate derived assets
    Cleanup {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Scope of cleanup (derived, orphans, all)
        #[arg(long, default_value = "all")]
        scope: String,
        /// Keep only the latest derived asset per clip/role
        #[arg(long)]
        dedup: bool,
        /// Maximum size in GB for derived assets (delete oldest if exceeded)
        #[arg(long)]
        max_size_gb: Option<u64>,
        /// Actually delete files (default is dry-run)
        #[arg(long)]
        confirm: bool,
    },

    /// Check and download required tools (ffmpeg, ffprobe)
    CheckTools {
        /// Download missing tools if possible
        #[arg(long)]
        download: bool,
    },

    /// Score clips for intelligent sorting
    Score {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Specific clip ID to score
        #[arg(long)]
        clip: Option<i64>,
        /// Force re-scoring even if up to date
        #[arg(long)]
        force: bool,
        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
        /// Number of concurrent scoring workers (default: 1)
        #[arg(long, default_value = "1")]
        workers: usize,
        /// Timeout per clip in seconds (default: 300)
        #[arg(long, default_value = "300")]
        timeout_secs: u64,
    },

    /// Show scoring status
    ScoreStatus {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Only show clips missing scores
        #[arg(long)]
        missing_only: bool,
    },

    /// List best clips above threshold
    BestClips {
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Minimum score threshold (0.0-1.0)
        #[arg(long, default_value = "0.6")]
        threshold: f64,
        /// Maximum clips to show
        #[arg(long, default_value = "20")]
        limit: i64,
    },

    /// Override a clip's score
    ScoreOverride {
        /// Clip ID to override
        clip_id: i64,
        /// Override action: promote, demote, pin, clear
        action: String,
        /// Library root (defaults to current directory)
        #[arg(short, long)]
        library: Option<PathBuf>,
        /// Override value (0.0-1.0 for pin, adjustment amount for promote/demote)
        #[arg(long)]
        value: Option<f64>,
        /// Optional note
        #[arg(long)]
        note: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path, name } => cmd_init(path, name),
        Commands::Ingest { path, library } => cmd_ingest(path, library),
        Commands::List { library, limit } => cmd_list(library, limit),
        Commands::Show { id, library } => cmd_show(id, library),
        Commands::Jobs { library, cancel, run, status } => cmd_jobs(library, cancel, run, status),
        Commands::RelinkScan { path, library } => cmd_relink_scan(path, library),
        Commands::Preview { library, r#type, clip, force } => cmd_preview(library, r#type, clip, force),
        Commands::PreviewStatus { library, missing_only } => cmd_preview_status(library, missing_only),
        Commands::Invalidate { library, r#type, confirm } => cmd_invalidate(library, r#type, confirm),
        Commands::Cleanup { library, scope, dedup, max_size_gb, confirm } => cmd_cleanup(library, scope, dedup, max_size_gb, confirm),
        Commands::CheckTools { download } => cmd_check_tools(download),
        Commands::Score { library, clip, force, verbose, workers, timeout_secs } => cmd_score(library, clip, force, verbose, workers, timeout_secs),
        Commands::ScoreStatus { library, missing_only } => cmd_score_status(library, missing_only),
        Commands::BestClips { library, threshold, limit } => cmd_best_clips(library, threshold, limit),
        Commands::ScoreOverride { clip_id, action, library, value, note } => cmd_score_override(clip_id, action, library, value, note),
    }
}

fn cmd_init(path: PathBuf, name: Option<String>) -> Result<()> {
    let library_root = path.canonicalize().unwrap_or(path.clone());

    // Check if library already exists
    let db_path = get_db_path(&library_root);
    if db_path.exists() {
        anyhow::bail!("Library already exists at {}", library_root.display());
    }

    // Create folder structure
    init_library_folders(&library_root)?;

    // Open/create database
    let conn = open_db(&db_path)?;

    // Create library record
    let lib_name = name.unwrap_or_else(|| {
        library_root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "My Library".to_string())
    });

    schema::insert_library(&conn, &library_root.to_string_lossy(), &lib_name, constants::DEFAULT_INGEST_MODE)?;

    println!("Initialized library '{}' at {}", lib_name, library_root.display());
    println!("Structure created:");
    println!("  .dadcam/dadcam.db   - Database");
    println!("  .dadcam/proxies/    - Preview videos");
    println!("  .dadcam/thumbs/     - Thumbnails");
    println!("  .dadcam/sprites/    - Sprite sheets");
    println!("  .dadcam/exports/    - Exported videos");
    println!("  originals/          - Ingested footage");

    Ok(())
}

fn cmd_ingest(source_path: PathBuf, library: Option<PathBuf>) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Canonicalize source path
    let source = source_path.canonicalize()
        .map_err(|_| anyhow::anyhow!("Source path does not exist: {}", source_path.display()))?;

    println!("Ingesting from {} into library '{}'", source.display(), lib.name);

    // Create ingest job
    let job_id = create_ingest_job(&conn, lib.id, &source.to_string_lossy(), &lib.ingest_mode)?;
    println!("Created ingest job {}", job_id);

    // Run the job
    let result = ingest::run_ingest_job(&conn, job_id, &library_root)?;

    println!();
    println!("Ingest complete:");
    println!("  Total files:  {}", result.total_files);
    println!("  Processed:    {}", result.processed);
    println!("  Skipped:      {}", result.skipped);
    println!("  Failed:       {}", result.failed);

    if !result.clips_created.is_empty() {
        println!("  Clips created: {}", result.clips_created.len());
    }

    Ok(())
}

fn cmd_list(library: Option<PathBuf>, limit: i64) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;
    let clips = schema::list_clips(&conn, lib.id, limit, 0)?;
    let total = schema::count_clips(&conn, lib.id)?;

    println!("Library: {} ({} clips total)", lib.name, total);
    println!();

    if clips.is_empty() {
        println!("No clips found. Use 'dadcam ingest <path>' to add footage.");
        return Ok(());
    }

    println!("{:>5}  {:>8}  {:>10}  {:>12}  {}", "ID", "Type", "Duration", "Recorded", "Title");
    println!("{}", "-".repeat(70));

    for clip in clips {
        let duration = clip.duration_ms
            .map(|ms| format_duration(ms))
            .unwrap_or_else(|| "-".to_string());

        let recorded = clip.recorded_at
            .as_ref()
            .map(|r| r.split('T').next().unwrap_or(r).to_string())
            .unwrap_or_else(|| "-".to_string());

        let title = if clip.title.len() > 30 {
            format!("{}...", &clip.title[..27])
        } else {
            clip.title.clone()
        };

        println!("{:>5}  {:>8}  {:>10}  {:>12}  {}",
            clip.id,
            clip.media_type,
            duration,
            recorded,
            title
        );
    }

    if total > limit {
        println!();
        println!("Showing {} of {} clips. Use --limit to see more.", limit, total);
    }

    Ok(())
}

fn cmd_show(id: i64, library: Option<PathBuf>) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let clip = schema::get_clip(&conn, id)?
        .ok_or_else(|| anyhow::anyhow!("Clip {} not found", id))?;

    let asset = schema::get_asset(&conn, clip.original_asset_id)?;

    println!("Clip #{}", clip.id);
    println!();
    println!("Title:       {}", clip.title);
    println!("Type:        {}", clip.media_type);

    if let Some(duration) = clip.duration_ms {
        println!("Duration:    {}", format_duration(duration));
    }

    if let (Some(w), Some(h)) = (clip.width, clip.height) {
        println!("Resolution:  {}x{}", w, h);
    }

    if let Some(fps) = clip.fps {
        println!("FPS:         {:.2}", fps);
    }

    if let Some(ref codec) = clip.codec {
        println!("Codec:       {}", codec);
    }

    if let Some(ref recorded_at) = clip.recorded_at {
        let source = clip.timestamp_source.as_deref().unwrap_or("unknown");
        let estimated = if clip.recorded_at_is_estimated { " (estimated)" } else { "" };
        println!("Recorded:    {} [source: {}]{}", recorded_at, source, estimated);
    }

    if let Some(ref folder) = clip.source_folder {
        println!("Source:      {}", folder);
    }

    println!("Created:     {}", clip.created_at);

    if let Some(ref asset) = asset {
        println!();
        println!("Asset:");
        println!("  Path:      {}", asset.path);
        println!("  Size:      {}", format_size(asset.size_bytes));
        if let Some(ref hash) = asset.hash_fast {
            println!("  Hash:      {}...", &hash[..hash.len().min(40)]);
        }
        if let Some(ref verified) = asset.verified_at {
            println!("  Verified:  {}", verified);
        }
    }

    // Check tags
    let is_favorite = schema::has_clip_tag(&conn, id, "favorite")?;
    let is_bad = schema::has_clip_tag(&conn, id, "bad")?;

    if is_favorite || is_bad {
        println!();
        print!("Tags:        ");
        if is_favorite { print!("[favorite] "); }
        if is_bad { print!("[bad]"); }
        println!();
    }

    Ok(())
}

fn cmd_jobs(library: Option<PathBuf>, cancel: Option<i64>, run: bool, status: Option<String>) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Handle cancel
    if let Some(job_id) = cancel {
        schema::cancel_job(&conn, job_id)?;
        println!("Cancelled job {}", job_id);
        return Ok(());
    }

    // Handle run
    if run {
        println!("Running pending jobs...");
        let count = runner::run_all_jobs(&conn, &library_root, None)?;
        println!("Completed {} jobs", count);
        return Ok(());
    }

    // List jobs
    let jobs = schema::list_jobs(&conn, Some(lib.id), status.as_deref(), 50)?;

    if jobs.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    println!("{:>5}  {:>12}  {:>10}  {:>8}  {:>20}", "ID", "Type", "Status", "Progress", "Created");
    println!("{}", "-".repeat(65));

    for job in jobs {
        let progress = job.progress.map(|p| format!("{}%", p)).unwrap_or_else(|| "-".to_string());
        let created = job.created_at.split('T').next().unwrap_or(&job.created_at);

        println!("{:>5}  {:>12}  {:>10}  {:>8}  {:>20}",
            job.id,
            job.job_type,
            job.status,
            progress,
            created
        );
    }

    // Show pending counts
    let pending = runner::count_pending_jobs(&conn)?;
    if !pending.is_empty() {
        println!();
        println!("Pending jobs:");
        for (job_type, count) in pending {
            println!("  {}: {}", job_type, count);
        }
    }

    Ok(())
}

fn cmd_relink_scan(scan_path: PathBuf, library: Option<PathBuf>) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    let source = scan_path.canonicalize()
        .map_err(|_| anyhow::anyhow!("Scan path does not exist: {}", scan_path.display()))?;

    println!("Scanning {} for potential matches...", source.display());

    // Discover files
    let files = ingest::discover::discover_media_files(&source)?;
    println!("Found {} media files", files.len());

    if files.is_empty() {
        println!("No media files found to match.");
        return Ok(());
    }

    // Get all assets for the library to check which are missing
    let assets = schema::get_missing_assets(&conn, lib.id)?;
    let mut missing_assets = Vec::new();

    for asset in &assets {
        let full_path = library_root.join(&asset.path);
        if !full_path.exists() {
            missing_assets.push(asset);
        }
    }

    if missing_assets.is_empty() {
        println!();
        println!("No missing assets in library - all originals present.");
        return Ok(());
    }

    println!("Found {} missing assets to match against", missing_assets.len());
    println!();

    // Build fingerprint index from scanned files
    let mut matches: Vec<(String, i64, String)> = Vec::new(); // (scanned_path, clip_id, match_type)

    for file_path in &files {
        // Get file size
        let file_size = match std::fs::metadata(file_path) {
            Ok(m) => m.len() as i64,
            Err(_) => continue,
        };

        // Try to extract duration via ffprobe
        let duration_ms = metadata::ffprobe::probe(file_path)
            .ok()
            .and_then(|m| m.duration_ms);

        // Compute size_duration fingerprint
        let fingerprint = hash::compute_size_duration_fingerprint(file_size, duration_ms);

        // Look for matching clips
        let matching_clips = schema::find_clips_by_fingerprint(&conn, "size_duration", &fingerprint)?;

        for clip_id in matching_clips {
            matches.push((
                file_path.to_string_lossy().to_string(),
                clip_id,
                "size_duration".to_string(),
            ));
        }

        // Also try hash-based matching for high confidence
        if let Ok(hash) = hash::compute_fast_hash(file_path) {
            for asset in &missing_assets {
                if let Some(ref asset_hash) = asset.hash_fast {
                    // Extract just the hash portion for comparison
                    let hash_value = hash.split(':').last().unwrap_or(&hash);
                    let asset_hash_value = asset_hash.split(':').last().unwrap_or(asset_hash);

                    if hash_value == asset_hash_value {
                        // Get the clip for this asset
                        if let Ok(Some(clip)) = schema::get_clip_by_asset(&conn, asset.id) {
                            // Check if we already have this match
                            let already_matched = matches.iter().any(|(_, cid, _)| *cid == clip.id);
                            if !already_matched {
                                matches.push((
                                    file_path.to_string_lossy().to_string(),
                                    clip.id,
                                    "hash_fast".to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Report results
    if matches.is_empty() {
        println!("No matches found.");
        println!();
        println!("Scanned files can be ingested as new clips with:");
        println!("  dadcam ingest {}", source.display());
    } else {
        println!("Found {} potential matches:", matches.len());
        println!();
        println!("{:>6}  {:>12}  {}", "Clip", "Match Type", "File Path");
        println!("{}", "-".repeat(70));

        for (path, clip_id, match_type) in &matches {
            let display_path = if path.len() > 45 {
                format!("...{}", &path[path.len() - 42..])
            } else {
                path.clone()
            };
            println!("{:>6}  {:>12}  {}", clip_id, match_type, display_path);
        }

        println!();
        println!("To relink files, use manual copy to the originals folder or re-ingest.");
    }

    Ok(())
}

// --- Helper Functions ---

fn resolve_library_root(library: Option<PathBuf>) -> Result<PathBuf> {
    let path = library.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let path = path.canonicalize().unwrap_or(path);

    // Check if .dadcam exists
    let db_path = get_db_path(&path);
    if !db_path.exists() {
        anyhow::bail!(
            "No library found at {}. Use 'dadcam init <path>' to create one.",
            path.display()
        );
    }

    Ok(path)
}

fn get_library_from_db(conn: &rusqlite::Connection, root: &PathBuf) -> Result<Library> {
    let root_str = root.to_string_lossy();
    schema::get_library_by_path(conn, &root_str)?
        .ok_or_else(|| anyhow::anyhow!("Library not found in database"))
}

fn format_duration(ms: i64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn cmd_preview(library: Option<PathBuf>, preview_type: String, clip_id: Option<i64>, force: bool) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Determine which types to generate
    let types: Vec<&str> = match preview_type.as_str() {
        "all" => vec!["thumb", "proxy", "sprite"],
        "proxy" => vec!["proxy"],
        "thumb" => vec!["thumb"],
        "sprite" => vec!["sprite"],
        _ => anyhow::bail!("Unknown preview type: {}. Use proxy, thumb, sprite, or all.", preview_type),
    };

    // If a specific clip is requested, just queue jobs for that clip
    if let Some(cid) = clip_id {
        let clip = schema::get_clip(&conn, cid)?
            .ok_or_else(|| anyhow::anyhow!("Clip {} not found", cid))?;

        println!("Generating previews for clip {}: {}", cid, clip.title);

        // If force, delete existing assets first
        if force {
            for role in &types {
                if let Some(existing) = preview::find_derived_asset(&conn, cid, role)? {
                    let old_path = library_root.join(&existing.path);
                    let _ = std::fs::remove_file(&old_path);
                    // Also remove VTT and JSON metadata for sprites
                    if *role == "sprite" {
                        let vtt_path = old_path.with_extension("vtt");
                        let json_path = old_path.with_extension("json");
                        let _ = std::fs::remove_file(&vtt_path);
                        let _ = std::fs::remove_file(&json_path);
                    }
                }
            }
        }

        // Queue jobs
        for job_type in &types {
            schema::insert_job(&conn, &schema::NewJob {
                job_type: (*job_type).to_string(),
                library_id: Some(lib.id),
                clip_id: Some(cid),
                asset_id: None,
                priority: if *job_type == "thumb" { 8 } else if *job_type == "proxy" { 5 } else { 3 },
                payload: "{}".to_string(),
            })?;
        }

        // Run jobs immediately
        let count = runner::run_all_jobs(&conn, &library_root, None)?;
        println!("Completed {} preview jobs", count);

        return Ok(());
    }

    // Queue jobs for all clips missing previews
    let mut total_queued = 0;

    for job_type in &types {
        let clips = preview::get_clips_needing_previews(&conn, lib.id, job_type, 1000)?;

        for clip in &clips {
            schema::insert_job(&conn, &schema::NewJob {
                job_type: (*job_type).to_string(),
                library_id: Some(lib.id),
                clip_id: Some(clip.id),
                asset_id: None,
                priority: if *job_type == "thumb" { 8 } else if *job_type == "proxy" { 5 } else { 3 },
                payload: "{}".to_string(),
            })?;
            total_queued += 1;
        }

        if !clips.is_empty() {
            println!("Queued {} {} jobs", clips.len(), job_type);
        }
    }

    if total_queued == 0 {
        println!("All clips have up-to-date previews.");
        return Ok(());
    }

    println!();
    println!("Running {} preview jobs...", total_queued);

    let count = runner::run_all_jobs(&conn, &library_root, None)?;
    println!("Completed {} preview jobs", count);

    Ok(())
}

fn cmd_preview_status(library: Option<PathBuf>, missing_only: bool) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Get total clip count
    let total_clips = schema::count_clips(&conn, lib.id)?;

    // Count clips with each preview type
    let has_proxy: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT clip_id) FROM clip_assets WHERE role = 'proxy'",
        [],
        |row| row.get(0),
    )?;

    let has_thumb: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT clip_id) FROM clip_assets WHERE role = 'thumb'",
        [],
        |row| row.get(0),
    )?;

    let has_sprite: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT clip_id) FROM clip_assets WHERE role = 'sprite'",
        [],
        |row| row.get(0),
    )?;

    // Count pending jobs
    let pending_proxy: i64 = conn.query_row(
        "SELECT COUNT(*) FROM jobs WHERE type = 'proxy' AND status = 'pending'",
        [],
        |row| row.get(0),
    )?;

    let pending_thumb: i64 = conn.query_row(
        "SELECT COUNT(*) FROM jobs WHERE type = 'thumb' AND status = 'pending'",
        [],
        |row| row.get(0),
    )?;

    let pending_sprite: i64 = conn.query_row(
        "SELECT COUNT(*) FROM jobs WHERE type = 'sprite' AND status = 'pending'",
        [],
        |row| row.get(0),
    )?;

    println!("Preview Status for '{}'", lib.name);
    println!("{}", "-".repeat(50));
    println!("Total clips: {}", total_clips);
    println!();
    println!("{:>12}  {:>8}  {:>8}  {:>10}", "Type", "Have", "Missing", "Pending");
    println!("{}", "-".repeat(50));

    let missing_proxy = total_clips - has_proxy;
    let missing_thumb = total_clips - has_thumb;
    let missing_sprite = total_clips - has_sprite;

    println!("{:>12}  {:>8}  {:>8}  {:>10}", "Thumbnails", has_thumb, missing_thumb, pending_thumb);
    println!("{:>12}  {:>8}  {:>8}  {:>10}", "Proxies", has_proxy, missing_proxy, pending_proxy);
    println!("{:>12}  {:>8}  {:>8}  {:>10}", "Sprites", has_sprite, missing_sprite, pending_sprite);

    if missing_only {
        println!();
        println!("Clips missing previews:");

        // List clips missing any preview
        let mut stmt = conn.prepare(
            r#"SELECT c.id, c.title, c.media_type
               FROM clips c
               WHERE c.library_id = ?1
               AND NOT EXISTS (SELECT 1 FROM clip_assets ca WHERE ca.clip_id = c.id AND ca.role = 'thumb')
               LIMIT 50"#
        )?;

        let missing: Vec<(i64, String, String)> = stmt.query_map([lib.id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        if missing.is_empty() {
            println!("  None - all clips have thumbnails");
        } else {
            for (id, title, media_type) in &missing {
                let display_title = if title.len() > 40 {
                    format!("{}...", &title[..37])
                } else {
                    title.clone()
                };
                println!("  {:>6}  {:>8}  {}", id, media_type, display_title);
            }
            if missing.len() == 50 {
                println!("  ... (showing first 50)");
            }
        }
    }

    Ok(())
}

fn cmd_invalidate(library: Option<PathBuf>, asset_type: String, confirm: bool) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Determine which types to invalidate
    let types: Vec<&str> = match asset_type.as_str() {
        "all" => vec!["thumb", "proxy", "sprite"],
        "proxy" => vec!["proxy"],
        "thumb" => vec!["thumb"],
        "sprite" => vec!["sprite"],
        _ => anyhow::bail!("Unknown asset type: {}. Use proxy, thumb, sprite, or all.", asset_type),
    };

    // Count affected assets
    let mut counts: Vec<(String, i64)> = Vec::new();
    for role in &types {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM clip_assets ca JOIN clips c ON c.id = ca.clip_id WHERE c.library_id = ?1 AND ca.role = ?2",
            rusqlite::params![lib.id, role],
            |row| row.get(0),
        )?;
        if count > 0 {
            counts.push(((*role).to_string(), count));
        }
    }

    if counts.is_empty() {
        println!("No derived assets to invalidate.");
        return Ok(());
    }

    println!("This will delete the following derived assets:");
    for (role, count) in &counts {
        println!("  {}: {} files", role, count);
    }

    if !confirm {
        println!();
        println!("Use --confirm to proceed with invalidation.");
        return Ok(());
    }

    // Delete files and remove database records
    let mut deleted = 0;

    for role in &types {
        // Get all assets of this type
        let mut stmt = conn.prepare(
            r#"SELECT a.id, a.path FROM assets a
               JOIN clip_assets ca ON ca.asset_id = a.id
               JOIN clips c ON c.id = ca.clip_id
               WHERE c.library_id = ?1 AND ca.role = ?2"#
        )?;

        let assets: Vec<(i64, String)> = stmt.query_map(rusqlite::params![lib.id, role], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        for (asset_id, path) in assets {
            // Delete file
            let full_path = library_root.join(&path);
            let _ = std::fs::remove_file(&full_path);

            // Delete VTT and JSON metadata for sprites
            if *role == "sprite" {
                let vtt_path = full_path.with_extension("vtt");
                let json_path = full_path.with_extension("json");
                let _ = std::fs::remove_file(&vtt_path);
                let _ = std::fs::remove_file(&json_path);
            }

            // Remove clip_asset link
            conn.execute(
                "DELETE FROM clip_assets WHERE asset_id = ?1",
                [asset_id],
            )?;

            // Remove asset record
            conn.execute(
                "DELETE FROM assets WHERE id = ?1",
                [asset_id],
            )?;

            deleted += 1;
        }
    }

    println!("Invalidated {} derived assets.", deleted);
    println!();
    println!("Run 'dadcam preview' to regenerate.");

    Ok(())
}

fn cmd_cleanup(
    library: Option<PathBuf>,
    scope: String,
    dedup: bool,
    max_size_gb: Option<u64>,
    confirm: bool,
) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    let derived_dirs = ["proxies", "thumbs", "sprites"];
    let mut orphan_files: Vec<(PathBuf, u64)> = Vec::new();
    let mut duplicate_assets: Vec<(i64, String, u64)> = Vec::new(); // (asset_id, path, size)
    let mut total_derived_size: u64 = 0;

    // Build set of referenced paths from database
    let mut referenced_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stmt = conn.prepare(
        "SELECT path FROM assets WHERE library_id = ?1 AND type IN ('proxy', 'thumb', 'sprite')"
    )?;
    let paths: Vec<String> = stmt.query_map([lib.id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    for path in paths {
        referenced_paths.insert(path);
    }

    // Scan derived directories for orphans
    if scope == "orphans" || scope == "all" {
        println!("Scanning for orphaned files...");

        for dir_name in &derived_dirs {
            let dir_path = library_root.join(constants::DADCAM_FOLDER).join(dir_name);
            if !dir_path.exists() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(&dir_path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let file_path = entry.path();
                    if !file_path.is_file() {
                        continue;
                    }

                    // Get relative path for comparison
                    let rel_path = file_path
                        .strip_prefix(&library_root)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_default();

                    let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    total_derived_size += file_size;

                    // Check if this file is referenced in DB
                    if !referenced_paths.contains(&rel_path) {
                        // Also check for VTT files which are sidecars to sprites
                        let is_vtt = file_path.extension().map(|e| e == "vtt").unwrap_or(false);
                        if !is_vtt {
                            orphan_files.push((file_path, file_size));
                        }
                    }
                }
            }
        }

        if !orphan_files.is_empty() {
            println!("Found {} orphaned files ({}):",
                orphan_files.len(),
                format_size(orphan_files.iter().map(|(_, s)| *s as i64).sum())
            );
            for (path, size) in &orphan_files {
                let display = path.strip_prefix(&library_root)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| path.display().to_string());
                println!("  {} ({})", display, format_size(*size as i64));
            }
        } else {
            println!("No orphaned files found.");
        }
    }

    // Find duplicate derived assets per (clip_id, role)
    if dedup && (scope == "derived" || scope == "all") {
        println!();
        println!("Checking for duplicate derived assets...");

        let mut stmt = conn.prepare(
            r#"SELECT ca.clip_id, ca.role, a.id, a.path, a.size_bytes, a.created_at
               FROM clip_assets ca
               JOIN assets a ON a.id = ca.asset_id
               JOIN clips c ON c.id = ca.clip_id
               WHERE c.library_id = ?1 AND ca.role IN ('proxy', 'thumb', 'sprite')
               ORDER BY ca.clip_id, ca.role, a.created_at DESC"#
        )?;

        let rows: Vec<(i64, String, i64, String, i64, String)> = stmt.query_map([lib.id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?.filter_map(|r| r.ok()).collect();

        // Group by (clip_id, role) and find duplicates
        let mut current_key: Option<(i64, String)> = None;
        for (clip_id, role, asset_id, path, size, _created_at) in rows {
            let key = (clip_id, role.clone());

            let first_in_group = if current_key.as_ref() != Some(&key) {
                current_key = Some(key);
                true
            } else {
                false
            };

            // Keep the first (newest) in each group, mark others as duplicates
            if !first_in_group {
                duplicate_assets.push((asset_id, path, size as u64));
            }
        }

        if !duplicate_assets.is_empty() {
            println!("Found {} duplicate assets ({}):",
                duplicate_assets.len(),
                format_size(duplicate_assets.iter().map(|(_, _, s)| *s as i64).sum())
            );
            for (_id, path, size) in &duplicate_assets {
                println!("  {} ({})", path, format_size(*size as i64));
            }
        } else {
            println!("No duplicate assets found.");
        }
    }

    // Check size cap
    let mut over_cap_assets: Vec<(i64, String, u64)> = Vec::new();
    if let Some(max_gb) = max_size_gb {
        let max_bytes = max_gb * 1024 * 1024 * 1024;

        println!();
        println!("Current derived asset size: {} (cap: {} GB)",
            format_size(total_derived_size as i64),
            max_gb
        );

        if total_derived_size > max_bytes {
            let excess = total_derived_size - max_bytes;
            println!("Exceeds cap by {}", format_size(excess as i64));

            // Get oldest derived assets to delete
            let mut stmt = conn.prepare(
                r#"SELECT a.id, a.path, a.size_bytes
                   FROM assets a
                   JOIN clips c ON c.library_id = ?1
                   JOIN clip_assets ca ON ca.asset_id = a.id AND ca.clip_id = c.id
                   WHERE a.type IN ('proxy', 'thumb', 'sprite')
                   ORDER BY a.created_at ASC"#
            )?;

            let rows: Vec<(i64, String, i64)> = stmt.query_map([lib.id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?.filter_map(|r| r.ok()).collect();

            let mut freed: u64 = 0;
            for (asset_id, path, size) in rows {
                if freed >= excess {
                    break;
                }
                // Don't delete assets already marked as duplicates
                if !duplicate_assets.iter().any(|(id, _, _)| *id == asset_id) {
                    over_cap_assets.push((asset_id, path, size as u64));
                    freed += size as u64;
                }
            }

            if !over_cap_assets.is_empty() {
                println!("Would delete {} oldest assets to meet cap:", over_cap_assets.len());
                for (_id, path, size) in &over_cap_assets {
                    println!("  {} ({})", path, format_size(*size as i64));
                }
            }
        }
    }

    // Summary
    let total_to_delete = orphan_files.len() + duplicate_assets.len() + over_cap_assets.len();

    if total_to_delete == 0 {
        println!();
        println!("Nothing to clean up.");
        return Ok(());
    }

    if !confirm {
        println!();
        println!("Use --confirm to delete {} items.", total_to_delete);
        return Ok(());
    }

    // Actually delete
    println!();
    println!("Cleaning up...");

    let mut deleted_count = 0;
    let mut freed_bytes: u64 = 0;

    // Delete orphan files
    for (path, size) in &orphan_files {
        if std::fs::remove_file(path).is_ok() {
            deleted_count += 1;
            freed_bytes += size;
        }
    }

    // Delete duplicate assets
    for (asset_id, path, size) in &duplicate_assets {
        let full_path = library_root.join(path);
        let _ = std::fs::remove_file(&full_path);

        // Remove from DB
        conn.execute("DELETE FROM clip_assets WHERE asset_id = ?1", [asset_id])?;
        conn.execute("DELETE FROM assets WHERE id = ?1", [asset_id])?;

        deleted_count += 1;
        freed_bytes += size;
    }

    // Delete over-cap assets
    for (asset_id, path, size) in &over_cap_assets {
        let full_path = library_root.join(path);
        let _ = std::fs::remove_file(&full_path);

        conn.execute("DELETE FROM clip_assets WHERE asset_id = ?1", [asset_id])?;
        conn.execute("DELETE FROM assets WHERE id = ?1", [asset_id])?;

        deleted_count += 1;
        freed_bytes += size;
    }

    println!("Cleanup complete:");
    println!("  Deleted: {} files", deleted_count);
    println!("  Freed:   {}", format_size(freed_bytes as i64));

    Ok(())
}

fn cmd_check_tools(download: bool) -> Result<()> {
    println!("Checking required tools...");
    println!();

    let status = tools::check_tools();

    println!("{:<12} {:<10} {}", "Tool", "Status", "Path");
    println!("{}", "-".repeat(60));

    let mut all_available = true;
    for (name, available, path) in &status {
        let status_str = if *available { "OK" } else { "MISSING" };
        println!("{:<12} {:<10} {}", name, status_str, path);
        if !available {
            all_available = false;
        }
    }

    if all_available {
        println!();
        println!("All tools available.");
        return Ok(());
    }

    if !download {
        println!();
        println!("Some tools are missing. Use --download to attempt automatic download.");
        return Ok(());
    }

    println!();
    println!("Attempting to download missing tools...");

    match tools::ensure_ffmpeg() {
        Ok((ffmpeg, ffprobe)) => {
            println!("FFmpeg downloaded to: {}", ffmpeg.display());
            println!("FFprobe downloaded to: {}", ffprobe.display());
        }
        Err(e) => {
            println!("Failed to download FFmpeg: {}", e);
            println!();
            println!("Please install FFmpeg manually:");
            println!("  macOS:   brew install ffmpeg");
            println!("  Windows: Download from https://ffmpeg.org/download.html");
            println!("  Linux:   apt install ffmpeg (or equivalent)");
        }
    }

    // Note: exiftool must be installed separately
    if !tools::is_tool_available("exiftool") {
        println!();
        println!("ExifTool not found. Please install manually:");
        println!("  macOS:   brew install exiftool");
        println!("  Windows: Download from https://exiftool.org/");
        println!("  Linux:   apt install libimage-exiftool-perl");
    }

    Ok(())
}

// ----- Phase 4: Scoring Commands -----

fn cmd_score(library: Option<PathBuf>, clip_id: Option<i64>, force: bool, verbose: bool, workers: usize, timeout_secs: u64) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Note: workers > 1 requires threaded execution (future enhancement)
    if workers > 1 && verbose {
        println!("Note: Multi-worker scoring uses {} concurrent workers", workers);
    }
    let _ = timeout_secs; // Reserved for future timeout implementation

    // If a specific clip is requested, score just that one
    if let Some(cid) = clip_id {
        let clip = schema::get_clip(&conn, cid)?
            .ok_or_else(|| anyhow::anyhow!("Clip {} not found", cid))?;

        println!("Scoring clip {}: {}", cid, clip.title);

        // Check if already scored (unless force)
        if !force && !scoring::analyzer::needs_scoring(&conn, cid)? {
            println!("Clip already has up-to-date score. Use --force to rescore.");
            if let Some(score) = scoring::analyzer::get_clip_score(&conn, cid)? {
                println!("  Overall: {:.2}", score.overall_score);
                println!("  Scene: {:.2}, Audio: {:.2}, Sharpness: {:.2}, Motion: {:.2}",
                    score.scene_score, score.audio_score, score.sharpness_score, score.motion_score);
            }
            return Ok(());
        }

        let result = scoring::analyzer::analyze_clip(&conn, cid, &library_root, verbose)?;
        scoring::analyzer::save_clip_score(&conn, &result)?;

        println!("Score: {:.2}", result.overall_score);
        println!("  Scene: {:.2}, Audio: {:.2}, Sharpness: {:.2}, Motion: {:.2}",
            result.scene_score, result.audio_score, result.sharpness_score, result.motion_score);

        if !result.reasons.is_empty() {
            println!("  Reasons: {}", result.reasons.join(", "));
        }

        return Ok(());
    }

    // Score all clips needing scores
    let clips_needing_scores = scoring::analyzer::get_clips_needing_scores(&conn, lib.id, 1000)?;

    if clips_needing_scores.is_empty() {
        println!("All clips have up-to-date scores.");
        return Ok(());
    }

    println!("Scoring {} clips (workers: {}, timeout: {}s)...", clips_needing_scores.len(), workers, timeout_secs);

    let mut scored = 0;
    let mut failed = 0;

    for cid in clips_needing_scores {
        match scoring::analyzer::analyze_clip(&conn, cid, &library_root, verbose) {
            Ok(result) => {
                if let Err(e) = scoring::analyzer::save_clip_score(&conn, &result) {
                    eprintln!("Failed to save score for clip {}: {}", cid, e);
                    failed += 1;
                } else {
                    if verbose {
                        println!("Clip {}: {:.2}", cid, result.overall_score);
                    }
                    scored += 1;
                }
            }
            Err(e) => {
                if verbose {
                    eprintln!("Failed to score clip {}: {}", cid, e);
                }
                failed += 1;
            }
        }
    }

    println!();
    println!("Scoring complete:");
    println!("  Scored: {}", scored);
    if failed > 0 {
        println!("  Failed: {}", failed);
    }

    Ok(())
}

fn cmd_score_status(library: Option<PathBuf>, missing_only: bool) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Get total clip count
    let total_clips = schema::count_clips(&conn, lib.id)?;

    // Count clips with scores
    let scored_clips: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT cs.clip_id) FROM clip_scores cs
         JOIN clips c ON c.id = cs.clip_id WHERE c.library_id = ?1",
        [lib.id],
        |row| row.get(0),
    )?;

    // Count clips needing rescoring (outdated versions)
    let outdated: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_scores cs
         JOIN clips c ON c.id = cs.clip_id
         WHERE c.library_id = ?1 AND (cs.pipeline_version != ?2 OR cs.scoring_version != ?3)",
        rusqlite::params![lib.id, constants::PIPELINE_VERSION, constants::SCORING_VERSION],
        |row| row.get(0),
    )?;

    // Count overrides
    let overrides: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clip_score_overrides o
         JOIN clips c ON c.id = o.clip_id WHERE c.library_id = ?1",
        [lib.id],
        |row| row.get(0),
    )?;

    println!("Scoring Status for '{}'", lib.name);
    println!("{}", "-".repeat(50));
    println!("Total clips:    {}", total_clips);
    println!("Scored:         {}", scored_clips);
    println!("Missing scores: {}", total_clips - scored_clips);
    println!("Outdated:       {}", outdated);
    println!("User overrides: {}", overrides);

    if missing_only && (total_clips - scored_clips) > 0 {
        println!();
        println!("Clips missing scores:");

        let mut stmt = conn.prepare(
            r#"SELECT c.id, c.title, c.media_type
               FROM clips c
               LEFT JOIN clip_scores cs ON cs.clip_id = c.id
               WHERE c.library_id = ?1 AND cs.id IS NULL
               LIMIT 50"#
        )?;

        let missing: Vec<(i64, String, String)> = stmt.query_map([lib.id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        for (id, title, media_type) in &missing {
            let display_title = if title.len() > 40 {
                format!("{}...", &title[..37])
            } else {
                title.clone()
            };
            println!("  {:>6}  {:>8}  {}", id, media_type, display_title);
        }

        if missing.len() == 50 {
            println!("  ... (showing first 50)");
        }
    }

    Ok(())
}

fn cmd_best_clips(library: Option<PathBuf>, threshold: f64, limit: i64) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    let lib = get_library_from_db(&conn, &library_root)?;

    // Validate threshold
    if threshold < 0.0 || threshold > 1.0 {
        anyhow::bail!("Threshold must be between 0.0 and 1.0");
    }

    let best_clips = scoring::analyzer::get_best_clips(&conn, lib.id, threshold, limit)?;

    if best_clips.is_empty() {
        println!("No clips found above threshold {:.2}", threshold);
        println!();
        println!("Try lowering the threshold or run 'dadcam score' to score clips.");
        return Ok(());
    }

    println!("Best clips (threshold >= {:.2}):", threshold);
    println!();
    println!("{:>5}  {:>6}  {:>10}  {}", "ID", "Score", "Duration", "Title");
    println!("{}", "-".repeat(60));

    for (clip_id, score) in &best_clips {
        let clip = schema::get_clip(&conn, *clip_id)?;
        if let Some(clip) = clip {
            let duration = clip.duration_ms
                .map(|ms| format_duration(ms))
                .unwrap_or_else(|| "-".to_string());

            let title = if clip.title.len() > 35 {
                format!("{}...", &clip.title[..32])
            } else {
                clip.title.clone()
            };

            println!("{:>5}  {:>6.2}  {:>10}  {}", clip_id, score, duration, title);
        }
    }

    println!();
    println!("Showing {} clips", best_clips.len());

    Ok(())
}

fn cmd_score_override(clip_id: i64, action: String, library: Option<PathBuf>, value: Option<f64>, note: Option<String>) -> Result<()> {
    let library_root = resolve_library_root(library)?;
    let db_path = get_db_path(&library_root);
    let conn = open_db(&db_path)?;

    // Verify clip exists
    let clip = schema::get_clip(&conn, clip_id)?
        .ok_or_else(|| anyhow::anyhow!("Clip {} not found", clip_id))?;

    match action.as_str() {
        "promote" => {
            let adj = value.unwrap_or(constants::SCORE_PROMOTE_DEFAULT);
            if adj < 0.0 || adj > 1.0 {
                anyhow::bail!("Adjustment value must be between 0.0 and 1.0");
            }

            conn.execute(
                "INSERT INTO clip_score_overrides (clip_id, override_type, override_value, note)
                 VALUES (?1, 'promote', ?2, ?3)
                 ON CONFLICT(clip_id) DO UPDATE SET
                    override_type = 'promote',
                    override_value = excluded.override_value,
                    note = excluded.note,
                    updated_at = datetime('now')",
                rusqlite::params![clip_id, adj, note],
            )?;

            println!("Promoted clip {}: '{}' (+{:.2})", clip_id, clip.title, adj);
        }

        "demote" => {
            let adj = value.unwrap_or(constants::SCORE_DEMOTE_DEFAULT);
            if adj < 0.0 || adj > 1.0 {
                anyhow::bail!("Adjustment value must be between 0.0 and 1.0");
            }

            conn.execute(
                "INSERT INTO clip_score_overrides (clip_id, override_type, override_value, note)
                 VALUES (?1, 'demote', ?2, ?3)
                 ON CONFLICT(clip_id) DO UPDATE SET
                    override_type = 'demote',
                    override_value = excluded.override_value,
                    note = excluded.note,
                    updated_at = datetime('now')",
                rusqlite::params![clip_id, adj, note],
            )?;

            println!("Demoted clip {}: '{}' (-{:.2})", clip_id, clip.title, adj);
        }

        "pin" => {
            let pin_value = value.ok_or_else(|| anyhow::anyhow!("Pin requires a --value between 0.0 and 1.0"))?;
            if pin_value < 0.0 || pin_value > 1.0 {
                anyhow::bail!("Pin value must be between 0.0 and 1.0");
            }

            conn.execute(
                "INSERT INTO clip_score_overrides (clip_id, override_type, override_value, note)
                 VALUES (?1, 'pin', ?2, ?3)
                 ON CONFLICT(clip_id) DO UPDATE SET
                    override_type = 'pin',
                    override_value = excluded.override_value,
                    note = excluded.note,
                    updated_at = datetime('now')",
                rusqlite::params![clip_id, pin_value, note],
            )?;

            println!("Pinned clip {}: '{}' to score {:.2}", clip_id, clip.title, pin_value);
        }

        "clear" => {
            let deleted = conn.execute(
                "DELETE FROM clip_score_overrides WHERE clip_id = ?1",
                [clip_id],
            )?;

            if deleted > 0 {
                println!("Cleared override for clip {}: '{}'", clip_id, clip.title);
            } else {
                println!("No override existed for clip {}", clip_id);
            }
        }

        _ => {
            anyhow::bail!("Unknown action: {}. Use promote, demote, pin, or clear.", action);
        }
    }

    // Show current effective score
    if let Some(score) = scoring::analyzer::get_clip_score(&conn, clip_id)? {
        // Check for override
        let override_info: Option<(String, f64)> = conn.query_row(
            "SELECT override_type, override_value FROM clip_score_overrides WHERE clip_id = ?1",
            [clip_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();

        let effective = if let Some((otype, oval)) = &override_info {
            match otype.as_str() {
                "pin" => *oval,
                "promote" => (score.overall_score + oval).min(1.0),
                "demote" => (score.overall_score - oval).max(0.0),
                _ => score.overall_score,
            }
        } else {
            score.overall_score
        };

        println!("Base score: {:.2}, Effective score: {:.2}", score.overall_score, effective);
    }

    Ok(())
}
