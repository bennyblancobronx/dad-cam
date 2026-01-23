// Dad Cam CLI binary

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

    // Insert default camera profiles
    camera::insert_default_profiles(&conn)?;

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
        let count = runner::run_all_jobs(&conn, &library_root)?;
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
