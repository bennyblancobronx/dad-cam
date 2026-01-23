// Dad Cam - Phase 3 Commands Module
// Tauri commands organized by domain

pub mod library;
pub mod clips;
pub mod tags;

// Re-export all commands for easy registration
pub use library::*;
pub use clips::*;
pub use tags::*;

// Shared database state
use std::sync::Mutex;
use rusqlite::Connection;

/// Shared database connection state managed by Tauri
pub struct DbState(pub Mutex<Option<Connection>>);
