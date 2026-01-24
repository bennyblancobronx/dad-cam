// Dad Cam - Commands Module
// Tauri commands organized by domain

pub mod library;
pub mod clips;
pub mod tags;
pub mod scoring;

// Re-export all commands for easy registration
pub use library::*;
pub use clips::*;
pub use tags::*;
pub use scoring::*;

// Shared database state
use std::sync::Mutex;
use rusqlite::Connection;

/// Shared database connection state managed by Tauri
pub struct DbState(pub Mutex<Option<Connection>>);
