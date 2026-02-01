// Background job worker -- polls for pending jobs and processes them.
//
// The worker thread spawns once at app startup and lives for the app lifetime.
// When a library is open, it processes one job per cycle (5s interval).
// When no library is open, it sleeps and does nothing.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;

/// Shared state that tells the worker which library is open.
pub struct WorkerState {
    library_root: Arc<Mutex<Option<PathBuf>>>,
}

impl WorkerState {
    pub fn new() -> Self {
        Self {
            library_root: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the library path (called when a library is opened).
    pub fn set_library(&self, path: PathBuf) {
        let mut guard = self.library_root.lock().unwrap();
        *guard = Some(path);
    }

    /// Clear the library path (called when a library is closed).
    pub fn clear_library(&self) {
        let mut guard = self.library_root.lock().unwrap();
        *guard = None;
    }

    /// Clone the inner Arc for passing to the worker thread.
    pub fn library_arc(&self) -> Arc<Mutex<Option<PathBuf>>> {
        Arc::clone(&self.library_root)
    }
}

/// Spawn the background worker thread. Call once during app setup.
pub fn spawn_worker(app: AppHandle, library_arc: Arc<Mutex<Option<PathBuf>>>) {
    std::thread::Builder::new()
        .name("job-worker".into())
        .spawn(move || {
            worker_loop(app, library_arc);
        })
        .expect("Failed to spawn job worker thread");
}

fn worker_loop(app: AppHandle, library_arc: Arc<Mutex<Option<PathBuf>>>) {
    loop {
        std::thread::sleep(Duration::from_secs(5));

        // Read the current library path (short lock)
        let library_root = {
            let guard = library_arc.lock().unwrap();
            guard.clone()
        };

        let library_root = match library_root {
            Some(p) => p,
            None => continue, // No library open, sleep again
        };

        // Open a short-lived DB connection
        let conn = match crate::db::open_library_db_connection(&library_root) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Job worker: failed to open DB: {}", e);
                continue;
            }
        };

        // Process one job (catch panics so the thread never dies)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::runner::run_next_job(&conn, &library_root, Some(&app))
        }));

        match result {
            Ok(Ok(true)) => {
                // Processed a job -- immediately try the next one (no sleep)
                // by continuing without the sleep at the top.
                // We do this by using a nested loop.
                drain_jobs(&app, &library_arc, &library_root);
            }
            Ok(Ok(false)) => {
                // No pending jobs, will sleep on next iteration
            }
            Ok(Err(e)) => {
                log::error!("Job worker: job failed: {}", e);
            }
            Err(_) => {
                log::error!("Job worker: job panicked (recovered)");
            }
        }
    }
}

/// After processing one job successfully, keep draining until the queue
/// is empty or the library changes. This avoids 5s gaps between jobs.
fn drain_jobs(app: &AppHandle, library_arc: &Arc<Mutex<Option<PathBuf>>>, library_root: &PathBuf) {
    loop {
        // Check that the same library is still open
        let current = {
            let guard = library_arc.lock().unwrap();
            guard.clone()
        };
        if current.as_ref() != Some(library_root) {
            return;
        }

        let conn = match crate::db::open_library_db_connection(library_root) {
            Ok(c) => c,
            Err(_) => return,
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::runner::run_next_job(&conn, library_root, Some(app))
        }));

        match result {
            Ok(Ok(true)) => continue,     // Another job done, keep going
            Ok(Ok(false)) => return,       // Queue empty
            Ok(Err(e)) => {
                log::error!("Job worker: job failed: {}", e);
                return;
            }
            Err(_) => {
                log::error!("Job worker: job panicked (recovered)");
                return;
            }
        }
    }
}
