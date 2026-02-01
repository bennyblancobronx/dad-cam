// Dad Cam - Commands Module
// Tauri commands organized by domain

pub mod library;
pub mod clips;
pub mod tags;
pub mod scoring;
pub mod settings;
pub mod stills;
pub mod events;
pub mod licensing;
pub mod export;
pub mod cameras;
pub mod devmenu;
pub mod diagnostics;

// Re-export all commands for easy registration
pub use library::*;
pub use clips::*;
pub use tags::*;
pub use scoring::*;
pub use settings::*;
pub use stills::*;
pub use events::*;
pub use licensing::*;
pub use export::*;
pub use cameras::*;
pub use devmenu::*;
pub use diagnostics::*;

// Shared database state
use std::sync::Mutex;
use std::path::PathBuf;
use rusqlite::Connection;

/// Library path state managed by Tauri.
/// Stores only the library root path, NOT a Connection (spec 3.4).
/// Each command opens a short-lived connection via connect().
pub struct DbState(pub Mutex<Option<PathBuf>>);

impl DbState {
    /// Open a short-lived library DB connection from the stored path.
    /// Returns error if no library is open.
    pub fn connect(&self) -> Result<Connection, String> {
        let guard = self.0.lock().map_err(|e| e.to_string())?;
        let library_root = guard.as_ref().ok_or("No library open")?;
        crate::db::open_library_db_connection(library_root)
            .map_err(|e| e.to_string())
    }

    /// Get the stored library root path, if any.
    pub fn library_root(&self) -> Result<Option<PathBuf>, String> {
        let guard = self.0.lock().map_err(|e| e.to_string())?;
        Ok(guard.clone())
    }
}
