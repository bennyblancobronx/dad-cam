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

// Shared database state
use std::sync::Mutex;
use rusqlite::Connection;

/// Shared database connection state managed by Tauri
pub struct DbState(pub Mutex<Option<Connection>>);
