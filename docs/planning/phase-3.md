Dad Cam - Phase 3 Implementation Guide

Version: 1.0
Target Audience: Developers new to Tauri/React

---

Overview

Phase 3 builds the desktop app shell. Dad Cam becomes a fast video viewer and library for years of footage. No editing, no scoring - just viewing, browsing, and tagging.

When complete, you can:
- Open a library and see all clips in a responsive grid
- Click any thumbnail to play the proxy video
- Hover over thumbnails to scrub through sprite sheets
- Filter clips by All, Favorites, Bad, Unreviewed
- Search by filename or date range
- Toggle Favorite/Bad tags on any clip
- Browse thousands of clips without UI lag

Prerequisites:
- Phase 1 and Phase 2 complete and working
- Rust backend with SQLite database, job system, and preview pipeline
- Test library with ingested clips that have proxies, thumbnails, and sprites
- Basic React/TypeScript knowledge
- Node.js 18+ and npm installed

---

What We're Building

Phase 3 connects the React frontend to the Rust backend:

```
React Frontend (UI)
    |
    v  Tauri invoke()
Rust Backend (Data)
    |
    v  rusqlite
SQLite Database (.dadcam/dadcam.db)
```

The frontend is responsible for:
- Displaying the clip grid with virtualization
- Playing proxy videos
- Showing hover scrub previews via sprite sheets
- Handling user interactions (clicks, hovers, filters)
- Calling Rust via Tauri commands

The backend (already built in Phase 1-2) handles:
- All database queries
- All file operations
- All path resolution

---

Part 1: Frontend Dependencies

1.1 Install Required Packages

Navigate to your project root and install frontend dependencies:

```bash
# Core dependencies
npm install @tanstack/react-virtual
npm install lru-cache

# TypeScript types (already included if using create-tauri-app)
npm install -D @types/react @types/react-dom
```

Package purposes:
- `@tanstack/react-virtual`: Virtualized grid for displaying thousands of clips
- `lru-cache`: Memory-efficient caching for thumbnails

1.2 Why TanStack Virtual?

TanStack Virtual (formerly react-virtual) was chosen over alternatives:

| Library | Bundle Size | Variable Heights | Grid Support | Maintenance |
|---------|-------------|------------------|--------------|-------------|
| TanStack Virtual | ~10kb | Built-in | Built-in | Active |
| react-window | ~6kb | Separate pkg | Built-in | Stable |
| react-virtuoso | ~15kb | Built-in | Limited | Active |

TanStack Virtual handles dynamic content well, supports bidirectional virtualization, and integrates with modern React patterns.

---

Part 2: Tauri Command Layer (Rust Backend)

2.1 Understanding Tauri Commands

Tauri commands are Rust functions that can be called from JavaScript/TypeScript. They run on the backend and return data to the frontend.

Phase 1 built the CLI. Now we expose the same functionality as Tauri commands.

2.2 Create the Commands Module

Create `src-tauri/src/commands/mod.rs`:

```rust
pub mod clips;
pub mod tags;
pub mod library;

// Re-export all commands for easy registration
pub use clips::*;
pub use tags::*;
pub use library::*;
```

2.3 Library Commands

Create `src-tauri/src/commands/library.rs`:

```rust
use crate::db::{self, schema};
use serde::{Deserialize, Serialize};
use tauri::State;
use std::sync::Mutex;
use rusqlite::Connection;

/// Shared database connection state
pub struct DbState(pub Mutex<Option<Connection>>);

/// Library info returned to frontend
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub id: i64,
    pub name: String,
    pub root_path: String,
    pub clip_count: i64,
    pub ingest_mode: String,
}

/// Open a library and store the connection
#[tauri::command]
pub async fn open_library(
    path: String,
    state: State<'_, DbState>,
) -> Result<LibraryInfo, String> {
    let db_path = std::path::Path::new(&path)
        .join(crate::constants::DADCAM_FOLDER)
        .join(crate::constants::DB_FILENAME);

    if !db_path.exists() {
        return Err(format!("No library found at {}", path));
    }

    let conn = db::open_db(&db_path)
        .map_err(|e| format!("Failed to open database: {}", e))?;

    // Get library info
    let library = schema::get_library_by_path(&conn, &path)
        .map_err(|e| format!("Failed to get library: {}", e))?
        .ok_or_else(|| "Library not found in database".to_string())?;

    // Count clips
    let clip_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM clips WHERE library_id = ?1",
            [library.id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count clips: {}", e))?;

    // Store connection in state
    let mut db_lock = state.0.lock().map_err(|_| "Lock error")?;
    *db_lock = Some(conn);

    Ok(LibraryInfo {
        id: library.id,
        name: library.name,
        root_path: library.root_path,
        clip_count,
        ingest_mode: library.ingest_mode,
    })
}

/// Close the current library
#[tauri::command]
pub async fn close_library(state: State<'_, DbState>) -> Result<(), String> {
    let mut db_lock = state.0.lock().map_err(|_| "Lock error")?;
    *db_lock = None;
    Ok(())
}

/// Get library root path from state
#[tauri::command]
pub async fn get_library_path(state: State<'_, DbState>) -> Result<String, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let path: String = conn
        .query_row(
            "SELECT root_path FROM libraries LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get path: {}", e))?;

    Ok(path)
}
```

2.4 Clip Commands

Create `src-tauri/src/commands/clips.rs`:

```rust
use crate::db::schema;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;
use super::DbState;

/// Clip data returned to frontend
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClipView {
    pub id: i64,
    pub title: String,
    pub media_type: String,
    pub duration_ms: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub recorded_at: Option<String>,
    pub thumb_path: Option<String>,
    pub proxy_path: Option<String>,
    pub sprite_path: Option<String>,
    pub sprite_meta_path: Option<String>,
    pub is_favorite: bool,
    pub is_bad: bool,
}

/// Query parameters for clip listing
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipQuery {
    pub offset: i64,
    pub limit: i64,
    pub filter: Option<String>,       // "all", "favorites", "bad", "unreviewed"
    pub search: Option<String>,       // filename search
    pub date_from: Option<String>,    // ISO date string
    pub date_to: Option<String>,      // ISO date string
    pub sort_by: Option<String>,      // "recorded_at", "title", "created_at"
    pub sort_order: Option<String>,   // "asc", "desc"
}

/// Paginated response for clips
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipListResponse {
    pub clips: Vec<ClipView>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// Get clips with pagination and filtering
#[tauri::command]
pub async fn get_clips(
    query: ClipQuery,
    state: State<'_, DbState>,
) -> Result<ClipListResponse, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Get library ID (assume single library for now)
    let library_id: i64 = conn
        .query_row("SELECT id FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| format!("No library: {}", e))?;

    // Build WHERE clause based on filters
    let mut conditions = vec!["c.library_id = ?1".to_string()];
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(library_id)];

    // Filter handling
    if let Some(ref filter) = query.filter {
        match filter.as_str() {
            "favorites" => {
                conditions.push(
                    "EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
                     WHERE ct.clip_id = c.id AND t.name = 'favorite')".to_string()
                );
            }
            "bad" => {
                conditions.push(
                    "EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
                     WHERE ct.clip_id = c.id AND t.name = 'bad')".to_string()
                );
            }
            "unreviewed" => {
                conditions.push(
                    "NOT EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
                     WHERE ct.clip_id = c.id AND t.name IN ('favorite', 'bad'))".to_string()
                );
            }
            _ => {} // "all" - no additional filter
        }
    }

    // Search filter
    if let Some(ref search) = query.search {
        if !search.is_empty() {
            conditions.push(format!(
                "c.title LIKE ?{}",
                params_vec.len() + 1
            ));
            params_vec.push(Box::new(format!("%{}%", search)));
        }
    }

    // Date range filter
    if let Some(ref date_from) = query.date_from {
        conditions.push(format!(
            "c.recorded_at >= ?{}",
            params_vec.len() + 1
        ));
        params_vec.push(Box::new(date_from.clone()));
    }
    if let Some(ref date_to) = query.date_to {
        conditions.push(format!(
            "c.recorded_at <= ?{}",
            params_vec.len() + 1
        ));
        params_vec.push(Box::new(date_to.clone()));
    }

    let where_clause = conditions.join(" AND ");

    // Sort handling
    let sort_column = match query.sort_by.as_deref() {
        Some("title") => "c.title",
        Some("created_at") => "c.created_at",
        _ => "c.recorded_at",
    };
    let sort_order = match query.sort_order.as_deref() {
        Some("asc") => "ASC",
        _ => "DESC",
    };

    // Count total
    let count_sql = format!(
        "SELECT COUNT(*) FROM clips c WHERE {}",
        where_clause
    );
    let total: i64 = conn
        .query_row(&count_sql, rusqlite::params_from_iter(&params_vec), |row| row.get(0))
        .map_err(|e| format!("Count failed: {}", e))?;

    // Build main query
    let sql = format!(
        r#"SELECT
            c.id, c.title, c.media_type, c.duration_ms, c.width, c.height, c.recorded_at,
            (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
             WHERE ca.clip_id = c.id AND ca.role = 'thumb' LIMIT 1) as thumb_path,
            (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
             WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1) as proxy_path,
            (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
             WHERE ca.clip_id = c.id AND ca.role = 'sprite' LIMIT 1) as sprite_path,
            EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
                    WHERE ct.clip_id = c.id AND t.name = 'favorite') as is_favorite,
            EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
                    WHERE ct.clip_id = c.id AND t.name = 'bad') as is_bad
        FROM clips c
        WHERE {}
        ORDER BY {} {} NULLS LAST
        LIMIT ?{} OFFSET ?{}"#,
        where_clause,
        sort_column,
        sort_order,
        params_vec.len() + 1,
        params_vec.len() + 2
    );

    params_vec.push(Box::new(query.limit));
    params_vec.push(Box::new(query.offset));

    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare failed: {}", e))?;

    let clips: Vec<ClipView> = stmt
        .query_map(rusqlite::params_from_iter(&params_vec), |row| {
            let sprite_path: Option<String> = row.get(9)?;
            let sprite_meta_path = sprite_path.as_ref().map(|p| {
                let path = std::path::Path::new(p);
                path.with_extension("json").to_string_lossy().to_string()
            });

            Ok(ClipView {
                id: row.get(0)?,
                title: row.get(1)?,
                media_type: row.get(2)?,
                duration_ms: row.get(3)?,
                width: row.get(4)?,
                height: row.get(5)?,
                recorded_at: row.get(6)?,
                thumb_path: row.get(7)?,
                proxy_path: row.get(8)?,
                sprite_path,
                sprite_meta_path,
                is_favorite: row.get::<_, i32>(10)? == 1,
                is_bad: row.get::<_, i32>(11)? == 1,
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(ClipListResponse {
        clips,
        total,
        offset: query.offset,
        limit: query.limit,
    })
}

/// Get a single clip by ID
#[tauri::command]
pub async fn get_clip(
    clip_id: i64,
    state: State<'_, DbState>,
) -> Result<ClipView, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let sql = r#"SELECT
        c.id, c.title, c.media_type, c.duration_ms, c.width, c.height, c.recorded_at,
        (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
         WHERE ca.clip_id = c.id AND ca.role = 'thumb' LIMIT 1) as thumb_path,
        (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
         WHERE ca.clip_id = c.id AND ca.role = 'proxy' LIMIT 1) as proxy_path,
        (SELECT a.path FROM clip_assets ca JOIN assets a ON ca.asset_id = a.id
         WHERE ca.clip_id = c.id AND ca.role = 'sprite' LIMIT 1) as sprite_path,
        EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
                WHERE ct.clip_id = c.id AND t.name = 'favorite') as is_favorite,
        EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
                WHERE ct.clip_id = c.id AND t.name = 'bad') as is_bad
    FROM clips c
    WHERE c.id = ?1"#;

    conn.query_row(sql, [clip_id], |row| {
        let sprite_path: Option<String> = row.get(9)?;
        let sprite_meta_path = sprite_path.as_ref().map(|p| {
            let path = std::path::Path::new(p);
            path.with_extension("json").to_string_lossy().to_string()
        });

        Ok(ClipView {
            id: row.get(0)?,
            title: row.get(1)?,
            media_type: row.get(2)?,
            duration_ms: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            recorded_at: row.get(6)?,
            thumb_path: row.get(7)?,
            proxy_path: row.get(8)?,
            sprite_path,
            sprite_meta_path,
            is_favorite: row.get::<_, i32>(10)? == 1,
            is_bad: row.get::<_, i32>(11)? == 1,
        })
    })
    .map_err(|e| format!("Clip not found: {}", e))
}
```

2.5 Tag Commands

Create `src-tauri/src/commands/tags.rs`:

```rust
use rusqlite::params;
use tauri::State;
use super::DbState;

/// Toggle a tag on a clip (add if missing, remove if present)
#[tauri::command]
pub async fn toggle_tag(
    clip_id: i64,
    tag_name: String,
    state: State<'_, DbState>,
) -> Result<bool, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Get tag ID
    let tag_id: i64 = conn
        .query_row(
            "SELECT id FROM tags WHERE name = ?1",
            [&tag_name],
            |row| row.get(0),
        )
        .map_err(|_| format!("Tag '{}' not found", tag_name))?;

    // Check if tag exists on clip
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM clip_tags WHERE clip_id = ?1 AND tag_id = ?2",
            params![clip_id, tag_id],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if exists {
        // Remove tag
        conn.execute(
            "DELETE FROM clip_tags WHERE clip_id = ?1 AND tag_id = ?2",
            params![clip_id, tag_id],
        )
        .map_err(|e| format!("Failed to remove tag: {}", e))?;
        Ok(false) // Tag is now OFF
    } else {
        // Add tag
        conn.execute(
            "INSERT INTO clip_tags (clip_id, tag_id) VALUES (?1, ?2)",
            params![clip_id, tag_id],
        )
        .map_err(|e| format!("Failed to add tag: {}", e))?;
        Ok(true) // Tag is now ON
    }
}

/// Set a tag to a specific state
#[tauri::command]
pub async fn set_tag(
    clip_id: i64,
    tag_name: String,
    value: bool,
    state: State<'_, DbState>,
) -> Result<(), String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Get tag ID
    let tag_id: i64 = conn
        .query_row(
            "SELECT id FROM tags WHERE name = ?1",
            [&tag_name],
            |row| row.get(0),
        )
        .map_err(|_| format!("Tag '{}' not found", tag_name))?;

    if value {
        // Add tag (ignore if exists)
        conn.execute(
            "INSERT OR IGNORE INTO clip_tags (clip_id, tag_id) VALUES (?1, ?2)",
            params![clip_id, tag_id],
        )
        .map_err(|e| format!("Failed to add tag: {}", e))?;
    } else {
        // Remove tag
        conn.execute(
            "DELETE FROM clip_tags WHERE clip_id = ?1 AND tag_id = ?2",
            params![clip_id, tag_id],
        )
        .map_err(|e| format!("Failed to remove tag: {}", e))?;
    }

    Ok(())
}

/// Get all tags for a clip
#[tauri::command]
pub async fn get_clip_tags(
    clip_id: i64,
    state: State<'_, DbState>,
) -> Result<Vec<String>, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    let mut stmt = conn
        .prepare(
            "SELECT t.name FROM clip_tags ct
             JOIN tags t ON ct.tag_id = t.id
             WHERE ct.clip_id = ?1",
        )
        .map_err(|e| format!("Prepare failed: {}", e))?;

    let tags: Vec<String> = stmt
        .query_map([clip_id], |row| row.get(0))
        .map_err(|e| format!("Query failed: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tags)
}
```

2.6 Register Commands in main.rs

Update `src-tauri/src/main.rs`:

```rust
mod cli;
mod commands;
mod constants;
mod db;
mod error;
mod hash;
mod ingest;
mod jobs;
mod metadata;
mod camera;
mod preview;
mod tools;

use commands::DbState;
use std::sync::Mutex;

fn main() {
    // Check if running as CLI
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && !args[1].starts_with("--") {
        // CLI mode
        if let Err(e) = cli::run() {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    // GUI mode - run Tauri app
    tauri::Builder::default()
        .manage(DbState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            commands::open_library,
            commands::close_library,
            commands::get_library_path,
            commands::get_clips,
            commands::get_clip,
            commands::toggle_tag,
            commands::set_tag,
            commands::get_clip_tags,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

2.7 Update Tauri Permissions

Edit `src-tauri/capabilities/default.json` to allow file access:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default permissions for Dad Cam",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-open",
    {
      "identifier": "fs:allow-read",
      "allow": [
        { "path": "$HOME/**" },
        { "path": "$DOCUMENT/**" },
        { "path": "$DESKTOP/**" }
      ]
    },
    {
      "identifier": "fs:allow-exists",
      "allow": [
        { "path": "$HOME/**" },
        { "path": "$DOCUMENT/**" },
        { "path": "$DESKTOP/**" }
      ]
    }
  ]
}
```

---

Part 3: TypeScript API Layer

3.1 Create Type Definitions

Create `src/types/clips.ts`:

```typescript
// Clip data from backend
export interface ClipView {
  id: number;
  title: string;
  mediaType: 'video' | 'audio' | 'image';
  durationMs: number | null;
  width: number | null;
  height: number | null;
  recordedAt: string | null;
  thumbPath: string | null;
  proxyPath: string | null;
  spritePath: string | null;
  spriteMetaPath: string | null;
  isFavorite: boolean;
  isBad: boolean;
}

// Query parameters for fetching clips
export interface ClipQuery {
  offset: number;
  limit: number;
  filter?: 'all' | 'favorites' | 'bad' | 'unreviewed';
  search?: string;
  dateFrom?: string;
  dateTo?: string;
  sortBy?: 'recorded_at' | 'title' | 'created_at';
  sortOrder?: 'asc' | 'desc';
}

// Paginated response
export interface ClipListResponse {
  clips: ClipView[];
  total: number;
  offset: number;
  limit: number;
}

// Library info
export interface LibraryInfo {
  id: number;
  name: string;
  rootPath: string;
  clipCount: number;
  ingestMode: string;
}

// Sprite sheet metadata (from JSON file)
export interface SpriteMetadata {
  frameCount: number;
  tileWidth: number;
  tileHeight: number;
  fps: number;
  totalWidth: number;
}
```

3.2 Create API Wrapper

Create `src/api/clips.ts`:

```typescript
import { invoke } from '@tauri-apps/api/core';
import type { ClipView, ClipQuery, ClipListResponse, LibraryInfo } from '../types/clips';

// Library operations
export async function openLibrary(path: string): Promise<LibraryInfo> {
  return invoke<LibraryInfo>('open_library', { path });
}

export async function closeLibrary(): Promise<void> {
  return invoke('close_library');
}

export async function getLibraryPath(): Promise<string> {
  return invoke<string>('get_library_path');
}

// Clip operations
export async function getClips(query: ClipQuery): Promise<ClipListResponse> {
  return invoke<ClipListResponse>('get_clips', { query });
}

export async function getClip(clipId: number): Promise<ClipView> {
  return invoke<ClipView>('get_clip', { clipId });
}

// Tag operations
export async function toggleTag(clipId: number, tagName: string): Promise<boolean> {
  return invoke<boolean>('toggle_tag', { clipId, tagName });
}

export async function setTag(clipId: number, tagName: string, value: boolean): Promise<void> {
  return invoke('set_tag', { clipId, tagName, value });
}

export async function getClipTags(clipId: number): Promise<string[]> {
  return invoke<string[]>('get_clip_tags', { clipId });
}
```

3.3 Create Path Utilities

Create `src/utils/paths.ts`:

```typescript
import { convertFileSrc } from '@tauri-apps/api/core';

let libraryRoot: string | null = null;

export function setLibraryRoot(path: string): void {
  libraryRoot = path;
}

export function getLibraryRoot(): string | null {
  return libraryRoot;
}

/**
 * Convert a relative path from the database to an absolute file:// URL
 * that can be used in <img> and <video> src attributes.
 */
export function toAssetUrl(relativePath: string | null): string | null {
  if (!relativePath || !libraryRoot) return null;

  // Join library root with relative path
  const absolutePath = `${libraryRoot}/${relativePath}`;

  // Convert to Tauri asset URL
  return convertFileSrc(absolutePath);
}

/**
 * Convert relative path to absolute filesystem path
 */
export function toAbsolutePath(relativePath: string | null): string | null {
  if (!relativePath || !libraryRoot) return null;
  return `${libraryRoot}/${relativePath}`;
}
```

---

Part 4: Thumbnail Grid with Virtualization

4.1 Understanding Virtualization

With thousands of clips, rendering all thumbnails would crash the browser. Virtualization renders only visible items plus a small buffer.

TanStack Virtual provides a "virtualizer" that:
- Tracks scroll position
- Calculates which items are visible
- Returns only those items for rendering
- Maintains smooth 60fps scrolling

4.2 Create the Thumbnail Cache

Create `src/utils/thumbnailCache.ts`:

```typescript
import { LRUCache } from 'lru-cache';

interface CachedImage {
  url: string;
  loaded: boolean;
  error: boolean;
}

// LRU cache with max 500 entries (adjust based on available memory)
const cache = new LRUCache<string, CachedImage>({
  max: 500,
  // Optionally set maxSize for memory-based eviction
  // maxSize: 100 * 1024 * 1024, // 100MB
  // sizeCalculation: (value) => estimateImageSize(value),
});

// Preload queue for background loading
const preloadQueue: string[] = [];
let isPreloading = false;

/**
 * Get a cached image or start loading it
 */
export function getThumbnail(url: string): CachedImage {
  const cached = cache.get(url);
  if (cached) return cached;

  // Create placeholder and start loading
  const placeholder: CachedImage = {
    url,
    loaded: false,
    error: false,
  };
  cache.set(url, placeholder);

  // Load image
  const img = new Image();
  img.onload = () => {
    cache.set(url, { url, loaded: true, error: false });
  };
  img.onerror = () => {
    cache.set(url, { url, loaded: false, error: true });
  };
  img.src = url;

  return placeholder;
}

/**
 * Preload thumbnails in background (for smooth scrolling)
 */
export function preloadThumbnails(urls: string[]): void {
  urls.forEach(url => {
    if (!cache.has(url) && !preloadQueue.includes(url)) {
      preloadQueue.push(url);
    }
  });

  if (!isPreloading) {
    processPreloadQueue();
  }
}

async function processPreloadQueue(): Promise<void> {
  isPreloading = true;

  while (preloadQueue.length > 0) {
    const batch = preloadQueue.splice(0, 10); // Process 10 at a time

    await Promise.all(
      batch.map(url => new Promise<void>(resolve => {
        const img = new Image();
        img.onload = () => {
          cache.set(url, { url, loaded: true, error: false });
          resolve();
        };
        img.onerror = () => {
          cache.set(url, { url, loaded: false, error: true });
          resolve();
        };
        img.src = url;
      }))
    );

    // Small delay to prevent overwhelming the system
    await new Promise(r => setTimeout(r, 50));
  }

  isPreloading = false;
}

/**
 * Clear the cache (e.g., when switching libraries)
 */
export function clearThumbnailCache(): void {
  cache.clear();
  preloadQueue.length = 0;
}

/**
 * Get cache statistics
 */
export function getCacheStats(): { size: number; maxSize: number } {
  return {
    size: cache.size,
    maxSize: cache.max,
  };
}
```

4.3 Create the Clip Grid Component

Create `src/components/ClipGrid.tsx`:

```typescript
import { useRef, useCallback, useEffect, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import type { ClipView } from '../types/clips';
import { toAssetUrl } from '../utils/paths';
import { preloadThumbnails } from '../utils/thumbnailCache';
import { ClipThumbnail } from './ClipThumbnail';

interface ClipGridProps {
  clips: ClipView[];
  totalClips: number;
  onLoadMore: () => void;
  onClipClick: (clip: ClipView) => void;
  onTagToggle: (clipId: number, tag: 'favorite' | 'bad') => void;
  isLoading: boolean;
  columnCount?: number;
  itemHeight?: number;
  gap?: number;
}

export function ClipGrid({
  clips,
  totalClips,
  onLoadMore,
  onClipClick,
  onTagToggle,
  isLoading,
  columnCount = 4,
  itemHeight = 200,
  gap = 16,
}: ClipGridProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);

  // Calculate responsive column count
  useEffect(() => {
    const updateWidth = () => {
      if (parentRef.current) {
        const width = parentRef.current.offsetWidth;
        setContainerWidth(width);
      }
    };
    updateWidth();
    window.addEventListener('resize', updateWidth);
    return () => window.removeEventListener('resize', updateWidth);
  }, []);

  // Calculate items per row based on container width
  const itemWidth = (containerWidth - gap * (columnCount + 1)) / columnCount;
  const rowCount = Math.ceil(clips.length / columnCount);

  // Create virtualizer for rows
  const rowVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => itemHeight + gap,
    overscan: 3, // Render 3 extra rows above/below viewport
  });

  // Load more when approaching end
  const virtualItems = rowVirtualizer.getVirtualItems();
  const lastItem = virtualItems[virtualItems.length - 1];

  useEffect(() => {
    if (!lastItem) return;

    const lastRowIndex = lastItem.index;
    const totalRows = Math.ceil(totalClips / columnCount);

    // Load more when within 5 rows of the end
    if (lastRowIndex >= rowCount - 5 && clips.length < totalClips && !isLoading) {
      onLoadMore();
    }
  }, [lastItem, clips.length, totalClips, rowCount, columnCount, isLoading, onLoadMore]);

  // Preload thumbnails for visible + nearby items
  useEffect(() => {
    if (virtualItems.length === 0) return;

    const firstRow = virtualItems[0].index;
    const lastRow = virtualItems[virtualItems.length - 1].index;

    // Include 2 rows buffer
    const startIdx = Math.max(0, (firstRow - 2) * columnCount);
    const endIdx = Math.min(clips.length, (lastRow + 3) * columnCount);

    const urlsToPreload = clips
      .slice(startIdx, endIdx)
      .map(clip => toAssetUrl(clip.thumbPath))
      .filter((url): url is string => url !== null);

    preloadThumbnails(urlsToPreload);
  }, [virtualItems, clips, columnCount]);

  return (
    <div
      ref={parentRef}
      className="clip-grid-container"
      style={{
        height: '100%',
        overflow: 'auto',
        contain: 'strict',
      }}
    >
      <div
        style={{
          height: `${rowVirtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualItems.map(virtualRow => {
          const rowIndex = virtualRow.index;
          const startIndex = rowIndex * columnCount;
          const rowClips = clips.slice(startIndex, startIndex + columnCount);

          return (
            <div
              key={virtualRow.key}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
                display: 'flex',
                gap: `${gap}px`,
                padding: `0 ${gap}px`,
              }}
            >
              {rowClips.map((clip, colIndex) => (
                <ClipThumbnail
                  key={clip.id}
                  clip={clip}
                  width={itemWidth}
                  height={itemHeight}
                  onClick={() => onClipClick(clip)}
                  onFavoriteToggle={() => onTagToggle(clip.id, 'favorite')}
                  onBadToggle={() => onTagToggle(clip.id, 'bad')}
                />
              ))}
            </div>
          );
        })}
      </div>

      {isLoading && (
        <div className="loading-indicator">
          Loading more clips...
        </div>
      )}
    </div>
  );
}
```

4.4 Create the Thumbnail Component

Create `src/components/ClipThumbnail.tsx`:

```typescript
import { useState, useCallback } from 'react';
import type { ClipView } from '../types/clips';
import { toAssetUrl } from '../utils/paths';
import { SpriteHover } from './SpriteHover';

interface ClipThumbnailProps {
  clip: ClipView;
  width: number;
  height: number;
  onClick: () => void;
  onFavoriteToggle: () => void;
  onBadToggle: () => void;
}

export function ClipThumbnail({
  clip,
  width,
  height,
  onClick,
  onFavoriteToggle,
  onBadToggle,
}: ClipThumbnailProps) {
  const [isHovering, setIsHovering] = useState(false);
  const [imageError, setImageError] = useState(false);

  const thumbUrl = toAssetUrl(clip.thumbPath);
  const spriteUrl = toAssetUrl(clip.spritePath);
  const spriteMetaUrl = toAssetUrl(clip.spriteMetaPath);

  const handleMouseEnter = useCallback(() => {
    setIsHovering(true);
  }, []);

  const handleMouseLeave = useCallback(() => {
    setIsHovering(false);
  }, []);

  const formatDuration = (ms: number | null): string => {
    if (!ms) return '';
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
  };

  return (
    <div
      className="clip-thumbnail"
      style={{
        width: `${width}px`,
        height: `${height}px`,
        position: 'relative',
        cursor: 'pointer',
        borderRadius: '8px',
        overflow: 'hidden',
        backgroundColor: '#1a1a1a',
      }}
      onClick={onClick}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      {/* Thumbnail or Sprite Hover */}
      {isHovering && spriteUrl && clip.mediaType === 'video' ? (
        <SpriteHover
          spriteUrl={spriteUrl}
          spriteMetaUrl={spriteMetaUrl}
          width={width}
          height={height - 40} // Leave room for info bar
        />
      ) : (
        <div
          className="thumbnail-image"
          style={{
            width: '100%',
            height: `${height - 40}px`,
            backgroundImage: thumbUrl && !imageError ? `url(${thumbUrl})` : 'none',
            backgroundSize: 'cover',
            backgroundPosition: 'center',
            backgroundColor: '#2a2a2a',
          }}
        >
          {(!thumbUrl || imageError) && (
            <div className="placeholder" style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              height: '100%',
              color: '#666',
            }}>
              No Preview
            </div>
          )}
        </div>
      )}

      {/* Duration badge */}
      {clip.durationMs && (
        <div
          className="duration-badge"
          style={{
            position: 'absolute',
            bottom: '48px',
            right: '8px',
            backgroundColor: 'rgba(0, 0, 0, 0.7)',
            color: 'white',
            padding: '2px 6px',
            borderRadius: '4px',
            fontSize: '12px',
          }}
        >
          {formatDuration(clip.durationMs)}
        </div>
      )}

      {/* Info bar */}
      <div
        className="info-bar"
        style={{
          position: 'absolute',
          bottom: 0,
          left: 0,
          right: 0,
          height: '40px',
          backgroundColor: '#1a1a1a',
          padding: '4px 8px',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
        }}
      >
        <span
          className="title"
          style={{
            color: 'white',
            fontSize: '13px',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
            flex: 1,
          }}
        >
          {clip.title}
        </span>

        {/* Tag buttons */}
        <div className="tag-buttons" style={{ display: 'flex', gap: '4px' }}>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onFavoriteToggle();
            }}
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              padding: '4px',
              color: clip.isFavorite ? '#ff4444' : '#666',
              fontSize: '16px',
            }}
            title={clip.isFavorite ? 'Remove from favorites' : 'Add to favorites'}
          >
            {clip.isFavorite ? '\u2665' : '\u2661'}
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              onBadToggle();
            }}
            style={{
              background: 'none',
              border: 'none',
              cursor: 'pointer',
              padding: '4px',
              color: clip.isBad ? '#ffaa00' : '#666',
              fontSize: '16px',
            }}
            title={clip.isBad ? 'Unmark as bad' : 'Mark as bad'}
          >
            {clip.isBad ? '\u2718' : '\u2717'}
          </button>
        </div>
      </div>

      {/* Hover indicator */}
      {isHovering && (
        <div
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            border: '2px solid #4a9eff',
            borderRadius: '8px',
            pointerEvents: 'none',
          }}
        />
      )}
    </div>
  );
}
```

---

Part 5: Sprite Sheet Hover Scrubbing

5.1 Understanding Sprite Scrubbing

Sprite scrubbing shows a preview of the video as the user moves their mouse across the thumbnail. Instead of loading a video, we show different frames from a pre-generated sprite sheet.

How it works:
1. Mouse enters thumbnail area
2. Load sprite metadata (frame count, tile size)
3. Calculate which frame based on mouse X position
4. Update background-position to show that frame
5. All happens client-side, no network calls

5.2 Create the Sprite Hover Component

Create `src/components/SpriteHover.tsx`:

```typescript
import { useState, useEffect, useCallback, useRef } from 'react';
import type { SpriteMetadata } from '../types/clips';

interface SpriteHoverProps {
  spriteUrl: string | null;
  spriteMetaUrl: string | null;
  width: number;
  height: number;
}

export function SpriteHover({
  spriteUrl,
  spriteMetaUrl,
  width,
  height,
}: SpriteHoverProps) {
  const [metadata, setMetadata] = useState<SpriteMetadata | null>(null);
  const [currentFrame, setCurrentFrame] = useState(0);
  const [spriteLoaded, setSpriteLoaded] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  // Load sprite metadata
  useEffect(() => {
    if (!spriteMetaUrl) return;

    fetch(spriteMetaUrl)
      .then(res => res.json())
      .then((data: SpriteMetadata) => {
        setMetadata(data);
      })
      .catch(err => {
        console.error('Failed to load sprite metadata:', err);
      });
  }, [spriteMetaUrl]);

  // Preload sprite image
  useEffect(() => {
    if (!spriteUrl) return;

    const img = new Image();
    img.onload = () => setSpriteLoaded(true);
    img.src = spriteUrl;
  }, [spriteUrl]);

  // Handle mouse movement
  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!metadata || !containerRef.current) return;

    const rect = containerRef.current.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const percentage = Math.max(0, Math.min(1, x / rect.width));

    // Calculate frame index
    const frameIndex = Math.floor(percentage * metadata.frameCount);
    setCurrentFrame(Math.min(frameIndex, metadata.frameCount - 1));
  }, [metadata]);

  if (!spriteUrl || !spriteLoaded || !metadata) {
    return (
      <div
        style={{
          width: '100%',
          height: `${height}px`,
          backgroundColor: '#2a2a2a',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <span style={{ color: '#666' }}>Loading...</span>
      </div>
    );
  }

  // Calculate background position for current frame
  // Sprites are arranged horizontally, so we shift left
  const xOffset = currentFrame * metadata.tileWidth;

  // Calculate scale to fit container
  const scale = height / metadata.tileHeight;
  const scaledTileWidth = metadata.tileWidth * scale;

  return (
    <div
      ref={containerRef}
      onMouseMove={handleMouseMove}
      style={{
        width: '100%',
        height: `${height}px`,
        overflow: 'hidden',
        position: 'relative',
      }}
    >
      <div
        style={{
          width: `${scaledTileWidth}px`,
          height: `${height}px`,
          backgroundImage: `url(${spriteUrl})`,
          backgroundSize: `${metadata.totalWidth * scale}px ${height}px`,
          backgroundPosition: `-${xOffset * scale}px 0`,
          backgroundRepeat: 'no-repeat',
        }}
      />

      {/* Frame indicator */}
      <div
        style={{
          position: 'absolute',
          bottom: '4px',
          left: '4px',
          backgroundColor: 'rgba(0, 0, 0, 0.7)',
          color: 'white',
          padding: '2px 6px',
          borderRadius: '4px',
          fontSize: '11px',
        }}
      >
        Frame {currentFrame + 1}/{metadata.frameCount}
      </div>

      {/* Progress bar */}
      <div
        style={{
          position: 'absolute',
          bottom: 0,
          left: 0,
          right: 0,
          height: '3px',
          backgroundColor: 'rgba(255, 255, 255, 0.2)',
        }}
      >
        <div
          style={{
            width: `${((currentFrame + 1) / metadata.frameCount) * 100}%`,
            height: '100%',
            backgroundColor: '#4a9eff',
            transition: 'width 50ms ease-out',
          }}
        />
      </div>
    </div>
  );
}
```

---

Part 6: Video Player

6.1 Create the Video Player Component

Create `src/components/VideoPlayer.tsx`:

```typescript
import { useRef, useState, useEffect, useCallback } from 'react';
import type { ClipView } from '../types/clips';
import { toAssetUrl } from '../utils/paths';

interface VideoPlayerProps {
  clip: ClipView;
  onClose: () => void;
  onPrevious?: () => void;
  onNext?: () => void;
  hasPrevious?: boolean;
  hasNext?: boolean;
}

export function VideoPlayer({
  clip,
  onClose,
  onPrevious,
  onNext,
  hasPrevious = false,
  hasNext = false,
}: VideoPlayerProps) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [isMuted, setIsMuted] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const proxyUrl = toAssetUrl(clip.proxyPath);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'Escape':
          onClose();
          break;
        case ' ':
          e.preventDefault();
          togglePlayPause();
          break;
        case 'ArrowLeft':
          if (e.shiftKey && hasPrevious) {
            onPrevious?.();
          } else {
            seek(-5);
          }
          break;
        case 'ArrowRight':
          if (e.shiftKey && hasNext) {
            onNext?.();
          } else {
            seek(5);
          }
          break;
        case 'm':
          toggleMute();
          break;
        case 'f':
          toggleFullscreen();
          break;
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, hasPrevious, hasNext, onPrevious, onNext]);

  const togglePlayPause = useCallback(() => {
    if (!videoRef.current) return;
    if (isPlaying) {
      videoRef.current.pause();
    } else {
      videoRef.current.play();
    }
    setIsPlaying(!isPlaying);
  }, [isPlaying]);

  const seek = useCallback((delta: number) => {
    if (!videoRef.current) return;
    videoRef.current.currentTime = Math.max(
      0,
      Math.min(duration, videoRef.current.currentTime + delta)
    );
  }, [duration]);

  const toggleMute = useCallback(() => {
    if (!videoRef.current) return;
    videoRef.current.muted = !isMuted;
    setIsMuted(!isMuted);
  }, [isMuted]);

  const toggleFullscreen = useCallback(() => {
    if (!videoRef.current) return;
    if (document.fullscreenElement) {
      document.exitFullscreen();
    } else {
      videoRef.current.requestFullscreen();
    }
  }, []);

  const handleTimeUpdate = useCallback(() => {
    if (videoRef.current) {
      setCurrentTime(videoRef.current.currentTime);
    }
  }, []);

  const handleLoadedMetadata = useCallback(() => {
    if (videoRef.current) {
      setDuration(videoRef.current.duration);
    }
  }, []);

  const handleSeek = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const time = parseFloat(e.target.value);
    if (videoRef.current) {
      videoRef.current.currentTime = time;
    }
    setCurrentTime(time);
  }, []);

  const handleVolumeChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const vol = parseFloat(e.target.value);
    if (videoRef.current) {
      videoRef.current.volume = vol;
    }
    setVolume(vol);
  }, []);

  const formatTime = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  if (!proxyUrl) {
    return (
      <div className="video-player-overlay" style={overlayStyle}>
        <div className="video-player-container" style={containerStyle}>
          <button onClick={onClose} style={closeButtonStyle}>Close</button>
          <div style={{ color: 'white', textAlign: 'center', padding: '40px' }}>
            No proxy video available. Run preview generation first.
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="video-player-overlay" style={overlayStyle} onClick={onClose}>
      <div
        className="video-player-container"
        style={containerStyle}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div style={headerStyle}>
          <span style={{ color: 'white', fontSize: '16px' }}>{clip.title}</span>
          <button onClick={onClose} style={closeButtonStyle}>X</button>
        </div>

        {/* Video */}
        <div style={{ flex: 1, display: 'flex', justifyContent: 'center', alignItems: 'center', backgroundColor: '#000' }}>
          {error ? (
            <div style={{ color: '#ff4444', padding: '20px' }}>
              Error loading video: {error}
            </div>
          ) : (
            <video
              ref={videoRef}
              src={proxyUrl}
              style={{ maxWidth: '100%', maxHeight: '100%' }}
              onTimeUpdate={handleTimeUpdate}
              onLoadedMetadata={handleLoadedMetadata}
              onPlay={() => setIsPlaying(true)}
              onPause={() => setIsPlaying(false)}
              onError={(e) => setError('Failed to load video')}
              autoPlay
            />
          )}
        </div>

        {/* Controls */}
        <div style={controlsStyle}>
          {/* Navigation */}
          <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
            <button
              onClick={onPrevious}
              disabled={!hasPrevious}
              style={navButtonStyle}
            >
              Prev
            </button>
            <button onClick={togglePlayPause} style={playButtonStyle}>
              {isPlaying ? 'Pause' : 'Play'}
            </button>
            <button
              onClick={onNext}
              disabled={!hasNext}
              style={navButtonStyle}
            >
              Next
            </button>
          </div>

          {/* Seek bar */}
          <div style={{ flex: 1, display: 'flex', alignItems: 'center', gap: '8px', margin: '0 16px' }}>
            <span style={{ color: 'white', fontSize: '12px' }}>{formatTime(currentTime)}</span>
            <input
              type="range"
              min="0"
              max={duration || 0}
              step="0.1"
              value={currentTime}
              onChange={handleSeek}
              style={{ flex: 1 }}
            />
            <span style={{ color: 'white', fontSize: '12px' }}>{formatTime(duration)}</span>
          </div>

          {/* Volume */}
          <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <button onClick={toggleMute} style={controlButtonStyle}>
              {isMuted ? 'Unmute' : 'Mute'}
            </button>
            <input
              type="range"
              min="0"
              max="1"
              step="0.1"
              value={isMuted ? 0 : volume}
              onChange={handleVolumeChange}
              style={{ width: '80px' }}
            />
            <button onClick={toggleFullscreen} style={controlButtonStyle}>
              Fullscreen
            </button>
          </div>
        </div>

        {/* Keyboard shortcuts hint */}
        <div style={hintsStyle}>
          Space: Play/Pause | Arrow keys: Seek | Shift+Arrows: Prev/Next | M: Mute | F: Fullscreen | Esc: Close
        </div>
      </div>
    </div>
  );
}

// Styles
const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  top: 0,
  left: 0,
  right: 0,
  bottom: 0,
  backgroundColor: 'rgba(0, 0, 0, 0.9)',
  display: 'flex',
  justifyContent: 'center',
  alignItems: 'center',
  zIndex: 1000,
};

const containerStyle: React.CSSProperties = {
  width: '90vw',
  height: '90vh',
  maxWidth: '1400px',
  display: 'flex',
  flexDirection: 'column',
  backgroundColor: '#1a1a1a',
  borderRadius: '8px',
  overflow: 'hidden',
};

const headerStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  padding: '12px 16px',
  backgroundColor: '#2a2a2a',
};

const closeButtonStyle: React.CSSProperties = {
  background: 'none',
  border: 'none',
  color: 'white',
  fontSize: '18px',
  cursor: 'pointer',
  padding: '4px 8px',
};

const controlsStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  padding: '12px 16px',
  backgroundColor: '#2a2a2a',
};

const playButtonStyle: React.CSSProperties = {
  padding: '8px 16px',
  backgroundColor: '#4a9eff',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  cursor: 'pointer',
};

const navButtonStyle: React.CSSProperties = {
  padding: '8px 12px',
  backgroundColor: '#444',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  cursor: 'pointer',
};

const controlButtonStyle: React.CSSProperties = {
  padding: '4px 8px',
  backgroundColor: '#444',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  cursor: 'pointer',
  fontSize: '12px',
};

const hintsStyle: React.CSSProperties = {
  padding: '8px 16px',
  backgroundColor: '#1a1a1a',
  color: '#666',
  fontSize: '11px',
  textAlign: 'center',
};
```

---

Part 7: Main Application Component

7.1 Create the Library View

Create `src/components/LibraryView.tsx`:

```typescript
import { useState, useEffect, useCallback } from 'react';
import type { ClipView, ClipQuery, LibraryInfo } from '../types/clips';
import { getClips, toggleTag } from '../api/clips';
import { setLibraryRoot } from '../utils/paths';
import { ClipGrid } from './ClipGrid';
import { VideoPlayer } from './VideoPlayer';
import { FilterBar } from './FilterBar';

interface LibraryViewProps {
  library: LibraryInfo;
  onCloseLibrary: () => void;
}

const PAGE_SIZE = 50;

export function LibraryView({ library, onCloseLibrary }: LibraryViewProps) {
  const [clips, setClips] = useState<ClipView[]>([]);
  const [totalClips, setTotalClips] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [selectedClip, setSelectedClip] = useState<ClipView | null>(null);
  const [filter, setFilter] = useState<ClipQuery['filter']>('all');
  const [search, setSearch] = useState('');
  const [dateFrom, setDateFrom] = useState<string | undefined>();
  const [dateTo, setDateTo] = useState<string | undefined>();
  const [sortBy, setSortBy] = useState<ClipQuery['sortBy']>('recorded_at');
  const [sortOrder, setSortOrder] = useState<ClipQuery['sortOrder']>('desc');

  // Set library root for path resolution
  useEffect(() => {
    setLibraryRoot(library.rootPath);
  }, [library.rootPath]);

  // Load clips
  const loadClips = useCallback(async (reset: boolean = false) => {
    setIsLoading(true);

    try {
      const offset = reset ? 0 : clips.length;
      const query: ClipQuery = {
        offset,
        limit: PAGE_SIZE,
        filter,
        search: search || undefined,
        dateFrom,
        dateTo,
        sortBy,
        sortOrder,
      };

      const response = await getClips(query);

      if (reset) {
        setClips(response.clips);
      } else {
        setClips(prev => [...prev, ...response.clips]);
      }
      setTotalClips(response.total);
    } catch (err) {
      console.error('Failed to load clips:', err);
    } finally {
      setIsLoading(false);
    }
  }, [clips.length, filter, search, dateFrom, dateTo, sortBy, sortOrder]);

  // Initial load and reload on filter change
  useEffect(() => {
    loadClips(true);
  }, [filter, search, dateFrom, dateTo, sortBy, sortOrder]);

  // Handle load more
  const handleLoadMore = useCallback(() => {
    if (!isLoading && clips.length < totalClips) {
      loadClips(false);
    }
  }, [isLoading, clips.length, totalClips, loadClips]);

  // Handle clip click
  const handleClipClick = useCallback((clip: ClipView) => {
    setSelectedClip(clip);
  }, []);

  // Handle tag toggle
  const handleTagToggle = useCallback(async (clipId: number, tag: 'favorite' | 'bad') => {
    try {
      const newValue = await toggleTag(clipId, tag);

      // Update local state
      setClips(prev =>
        prev.map(clip => {
          if (clip.id !== clipId) return clip;
          return {
            ...clip,
            isFavorite: tag === 'favorite' ? newValue : clip.isFavorite,
            isBad: tag === 'bad' ? newValue : clip.isBad,
          };
        })
      );
    } catch (err) {
      console.error('Failed to toggle tag:', err);
    }
  }, []);

  // Navigate to previous/next clip in player
  const currentClipIndex = selectedClip
    ? clips.findIndex(c => c.id === selectedClip.id)
    : -1;

  const handlePreviousClip = useCallback(() => {
    if (currentClipIndex > 0) {
      setSelectedClip(clips[currentClipIndex - 1]);
    }
  }, [currentClipIndex, clips]);

  const handleNextClip = useCallback(() => {
    if (currentClipIndex < clips.length - 1) {
      setSelectedClip(clips[currentClipIndex + 1]);
    }
  }, [currentClipIndex, clips]);

  return (
    <div className="library-view" style={{
      display: 'flex',
      flexDirection: 'column',
      height: '100vh',
      backgroundColor: '#0a0a0a',
    }}>
      {/* Header */}
      <header style={{
        padding: '12px 16px',
        backgroundColor: '#1a1a1a',
        borderBottom: '1px solid #2a2a2a',
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
      }}>
        <div>
          <h1 style={{ margin: 0, fontSize: '18px', color: 'white' }}>
            {library.name}
          </h1>
          <span style={{ color: '#888', fontSize: '13px' }}>
            {totalClips.toLocaleString()} clips
          </span>
        </div>
        <button
          onClick={onCloseLibrary}
          style={{
            padding: '8px 16px',
            backgroundColor: '#333',
            border: 'none',
            borderRadius: '4px',
            color: 'white',
            cursor: 'pointer',
          }}
        >
          Close Library
        </button>
      </header>

      {/* Filter bar */}
      <FilterBar
        filter={filter}
        onFilterChange={setFilter}
        search={search}
        onSearchChange={setSearch}
        sortBy={sortBy}
        onSortByChange={setSortBy}
        sortOrder={sortOrder}
        onSortOrderChange={setSortOrder}
        dateFrom={dateFrom}
        onDateFromChange={setDateFrom}
        dateTo={dateTo}
        onDateToChange={setDateTo}
      />

      {/* Clip grid */}
      <main style={{ flex: 1, overflow: 'hidden' }}>
        <ClipGrid
          clips={clips}
          totalClips={totalClips}
          onLoadMore={handleLoadMore}
          onClipClick={handleClipClick}
          onTagToggle={handleTagToggle}
          isLoading={isLoading}
        />
      </main>

      {/* Video player modal */}
      {selectedClip && (
        <VideoPlayer
          clip={selectedClip}
          onClose={() => setSelectedClip(null)}
          onPrevious={handlePreviousClip}
          onNext={handleNextClip}
          hasPrevious={currentClipIndex > 0}
          hasNext={currentClipIndex < clips.length - 1}
        />
      )}
    </div>
  );
}
```

7.2 Create the Filter Bar Component

Create `src/components/FilterBar.tsx`:

```typescript
import type { ClipQuery } from '../types/clips';

interface FilterBarProps {
  filter: ClipQuery['filter'];
  onFilterChange: (filter: ClipQuery['filter']) => void;
  search: string;
  onSearchChange: (search: string) => void;
  sortBy: ClipQuery['sortBy'];
  onSortByChange: (sortBy: ClipQuery['sortBy']) => void;
  sortOrder: ClipQuery['sortOrder'];
  onSortOrderChange: (sortOrder: ClipQuery['sortOrder']) => void;
  dateFrom?: string;
  onDateFromChange: (date?: string) => void;
  dateTo?: string;
  onDateToChange: (date?: string) => void;
}

export function FilterBar({
  filter,
  onFilterChange,
  search,
  onSearchChange,
  sortBy,
  onSortByChange,
  sortOrder,
  onSortOrderChange,
  dateFrom,
  onDateFromChange,
  dateTo,
  onDateToChange,
}: FilterBarProps) {
  const filters: { value: ClipQuery['filter']; label: string }[] = [
    { value: 'all', label: 'All Clips' },
    { value: 'favorites', label: 'Favorites' },
    { value: 'bad', label: 'Bad' },
    { value: 'unreviewed', label: 'Unreviewed' },
  ];

  return (
    <div
      className="filter-bar"
      style={{
        padding: '12px 16px',
        backgroundColor: '#1a1a1a',
        borderBottom: '1px solid #2a2a2a',
        display: 'flex',
        gap: '16px',
        alignItems: 'center',
        flexWrap: 'wrap',
      }}
    >
      {/* Filter buttons */}
      <div style={{ display: 'flex', gap: '4px' }}>
        {filters.map(f => (
          <button
            key={f.value}
            onClick={() => onFilterChange(f.value)}
            style={{
              padding: '6px 12px',
              backgroundColor: filter === f.value ? '#4a9eff' : '#333',
              border: 'none',
              borderRadius: '4px',
              color: 'white',
              cursor: 'pointer',
              fontSize: '13px',
            }}
          >
            {f.label}
          </button>
        ))}
      </div>

      {/* Search */}
      <div style={{ flex: 1, minWidth: '200px', maxWidth: '400px' }}>
        <input
          type="text"
          placeholder="Search by filename..."
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          style={{
            width: '100%',
            padding: '8px 12px',
            backgroundColor: '#333',
            border: 'none',
            borderRadius: '4px',
            color: 'white',
            fontSize: '13px',
          }}
        />
      </div>

      {/* Date range */}
      <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
        <span style={{ color: '#888', fontSize: '13px' }}>Date:</span>
        <input
          type="date"
          value={dateFrom || ''}
          onChange={(e) => onDateFromChange(e.target.value || undefined)}
          style={dateInputStyle}
        />
        <span style={{ color: '#666' }}>to</span>
        <input
          type="date"
          value={dateTo || ''}
          onChange={(e) => onDateToChange(e.target.value || undefined)}
          style={dateInputStyle}
        />
      </div>

      {/* Sort */}
      <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
        <span style={{ color: '#888', fontSize: '13px' }}>Sort:</span>
        <select
          value={sortBy}
          onChange={(e) => onSortByChange(e.target.value as ClipQuery['sortBy'])}
          style={selectStyle}
        >
          <option value="recorded_at">Date Recorded</option>
          <option value="title">Title</option>
          <option value="created_at">Date Added</option>
        </select>
        <button
          onClick={() => onSortOrderChange(sortOrder === 'asc' ? 'desc' : 'asc')}
          style={{
            padding: '6px 10px',
            backgroundColor: '#333',
            border: 'none',
            borderRadius: '4px',
            color: 'white',
            cursor: 'pointer',
            fontSize: '12px',
          }}
        >
          {sortOrder === 'asc' ? 'Oldest First' : 'Newest First'}
        </button>
      </div>
    </div>
  );
}

const dateInputStyle: React.CSSProperties = {
  padding: '6px 8px',
  backgroundColor: '#333',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  fontSize: '13px',
};

const selectStyle: React.CSSProperties = {
  padding: '6px 8px',
  backgroundColor: '#333',
  border: 'none',
  borderRadius: '4px',
  color: 'white',
  fontSize: '13px',
};
```

7.3 Create the Main App Component

Create `src/App.tsx`:

```typescript
import { useState, useCallback } from 'react';
import type { LibraryInfo } from './types/clips';
import { openLibrary, closeLibrary } from './api/clips';
import { LibraryView } from './components/LibraryView';
import { clearThumbnailCache } from './utils/thumbnailCache';
import { open } from '@tauri-apps/plugin-dialog';

function App() {
  const [library, setLibrary] = useState<LibraryInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const handleOpenLibrary = useCallback(async () => {
    setError(null);
    setIsLoading(true);

    try {
      // Open folder picker
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Dad Cam Library',
      });

      if (!selected) {
        setIsLoading(false);
        return;
      }

      // Open the library
      const info = await openLibrary(selected as string);
      setLibrary(info);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, []);

  const handleCloseLibrary = useCallback(async () => {
    try {
      await closeLibrary();
      clearThumbnailCache();
      setLibrary(null);
    } catch (err) {
      console.error('Failed to close library:', err);
    }
  }, []);

  // Show library view if open
  if (library) {
    return (
      <LibraryView
        library={library}
        onCloseLibrary={handleCloseLibrary}
      />
    );
  }

  // Show welcome screen
  return (
    <div
      className="welcome-screen"
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        height: '100vh',
        backgroundColor: '#0a0a0a',
        color: 'white',
      }}
    >
      <h1 style={{ fontSize: '48px', marginBottom: '16px' }}>Dad Cam</h1>
      <p style={{ color: '#888', marginBottom: '32px' }}>
        A video library for dad cam footage
      </p>

      <button
        onClick={handleOpenLibrary}
        disabled={isLoading}
        style={{
          padding: '16px 32px',
          fontSize: '18px',
          backgroundColor: '#4a9eff',
          border: 'none',
          borderRadius: '8px',
          color: 'white',
          cursor: isLoading ? 'wait' : 'pointer',
          opacity: isLoading ? 0.7 : 1,
        }}
      >
        {isLoading ? 'Opening...' : 'Open Library'}
      </button>

      {error && (
        <div
          style={{
            marginTop: '24px',
            padding: '16px',
            backgroundColor: '#331111',
            borderRadius: '8px',
            color: '#ff6666',
            maxWidth: '400px',
            textAlign: 'center',
          }}
        >
          {error}
        </div>
      )}

      <div style={{ marginTop: '48px', color: '#666', fontSize: '13px' }}>
        <p>To get started:</p>
        <ol style={{ textAlign: 'left' }}>
          <li>Initialize a library: <code>dadcam init /path/to/library</code></li>
          <li>Ingest footage: <code>dadcam ingest /path/to/footage</code></li>
          <li>Generate previews: <code>dadcam preview</code></li>
          <li>Open the library folder here</li>
        </ol>
      </div>
    </div>
  );
}

export default App;
```

7.4 Add Dialog Plugin

Install the Tauri dialog plugin for folder selection:

```bash
# In project root
npm install @tauri-apps/plugin-dialog

# In src-tauri
cd src-tauri
cargo add tauri-plugin-dialog
```

Update `src-tauri/src/main.rs` to register the plugin:

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())  // Add this line
    .manage(DbState(Mutex::new(None)))
    // ... rest of builder
```

Update `src-tauri/capabilities/default.json`:

```json
{
  "permissions": [
    "core:default",
    "shell:allow-open",
    "dialog:allow-open",
    // ... existing permissions
  ]
}
```

---

Part 8: Styling

8.1 Global Styles

Create `src/index.css`:

```css
* {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen,
    Ubuntu, Cantarell, 'Open Sans', 'Helvetica Neue', sans-serif;
  background-color: #0a0a0a;
  color: white;
  overflow: hidden;
}

/* Scrollbar styling */
::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

::-webkit-scrollbar-track {
  background: #1a1a1a;
}

::-webkit-scrollbar-thumb {
  background: #444;
  border-radius: 4px;
}

::-webkit-scrollbar-thumb:hover {
  background: #555;
}

/* Input styling */
input[type="text"],
input[type="date"],
select {
  outline: none;
}

input[type="text"]:focus,
input[type="date"]:focus,
select:focus {
  box-shadow: 0 0 0 2px rgba(74, 158, 255, 0.3);
}

/* Button hover states */
button:hover:not(:disabled) {
  filter: brightness(1.1);
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Range input styling */
input[type="range"] {
  -webkit-appearance: none;
  background: #333;
  height: 4px;
  border-radius: 2px;
}

input[type="range"]::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 12px;
  height: 12px;
  background: #4a9eff;
  border-radius: 50%;
  cursor: pointer;
}

/* Loading indicator */
.loading-indicator {
  text-align: center;
  padding: 20px;
  color: #888;
}
```

Update `src/main.tsx` to import styles:

```typescript
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

---

Part 9: Testing Your Implementation

9.1 Build and Run

```bash
# Build Rust backend
cd src-tauri
cargo build

# Run in development mode (from project root)
cd ..
npm run tauri dev
```

9.2 Test Workflow

1. Initialize a test library with clips (if not already done):
   ```bash
   ./target/debug/dadcam init ~/test-library --name "Test Library"
   ./target/debug/dadcam ingest ~/test-footage --library ~/test-library
   ./target/debug/dadcam preview --library ~/test-library
   ```

2. Launch the app and open the test library

3. Test grid scrolling:
   - Scroll quickly through the grid
   - Verify thumbnails load smoothly
   - Check that scrolling stays at 60fps

4. Test hover scrubbing:
   - Hover over a video thumbnail
   - Move mouse left/right
   - Verify sprite frames change smoothly

5. Test video playback:
   - Click a thumbnail
   - Verify proxy video plays
   - Test keyboard shortcuts (Space, arrows, M, F, Esc)
   - Test previous/next navigation

6. Test filtering:
   - Click each filter button (All, Favorites, Bad, Unreviewed)
   - Use the search box
   - Set date range filters
   - Change sort options

7. Test tagging:
   - Click heart icon to toggle favorite
   - Click X icon to toggle bad
   - Switch to Favorites filter, verify tagged clips appear
   - Switch to Bad filter, verify marked clips appear

9.3 Performance Testing

Test with a large library (1000+ clips):

```bash
# Create test data
for i in {1..1000}; do
  # Copy test video with different names
  cp test.mp4 ~/test-footage/clip_$i.mp4
done

# Ingest all
./target/debug/dadcam ingest ~/test-footage

# Generate all previews
./target/debug/dadcam preview -t all
```

Verify:
- [ ] Initial load completes in under 3 seconds
- [ ] Scrolling stays smooth at 60fps
- [ ] Memory usage stays under 500MB
- [ ] No UI freezing during scroll

---

Part 10: Checklist

Before moving to Phase 4, verify:

**Rust Backend:**
- [ ] All commands compile without errors
- [ ] DbState properly manages connection lifecycle
- [ ] Commands handle errors gracefully with clear messages
- [ ] Pagination works correctly (offset, limit)
- [ ] Filters work: all, favorites, bad, unreviewed
- [ ] Search filter works (partial filename match)
- [ ] Date range filter works
- [ ] Sort options work (recorded_at, title, created_at)
- [ ] Tag toggle updates database correctly

**TypeScript Frontend:**
- [ ] Type definitions match Rust structs
- [ ] API wrapper functions handle errors
- [ ] Path utilities correctly convert relative paths to asset URLs

**Virtualized Grid:**
- [ ] TanStack Virtual properly virtualizes rows
- [ ] Only visible thumbnails are rendered
- [ ] Scrolling is smooth at 60fps
- [ ] Load more triggers when approaching end
- [ ] Thumbnail preloading works for nearby items

**Thumbnail Loading:**
- [ ] LRU cache limits memory usage
- [ ] Cached thumbnails load instantly
- [ ] Failed thumbnail loads show placeholder
- [ ] Cache clears when switching libraries

**Sprite Hover:**
- [ ] Sprite metadata loads correctly
- [ ] Mouse movement updates frame smoothly
- [ ] Frame indicator shows current position
- [ ] Falls back gracefully if sprite missing

**Video Player:**
- [ ] Proxy video plays on click
- [ ] Play/pause works (button and space bar)
- [ ] Seek bar works (click and drag)
- [ ] Volume control works
- [ ] Mute toggle works
- [ ] Fullscreen toggle works
- [ ] Previous/next navigation works
- [ ] Escape closes player

**Filter Bar:**
- [ ] Filter buttons highlight active filter
- [ ] Search input debounces properly
- [ ] Date inputs work
- [ ] Sort dropdown works
- [ ] Sort order toggle works

**Overall UX:**
- [ ] App opens quickly
- [ ] Library picker works
- [ ] Responsive layout adapts to window size
- [ ] No UI freezing during operations
- [ ] Error messages are clear and helpful

---

Part 11: Operational Hardening

11.1 Error Boundaries

Wrap components in error boundaries to prevent crashes:

Create `src/components/ErrorBoundary.tsx`:

```typescript
import { Component, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { hasError: false, error: null };

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('Error caught by boundary:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        this.props.fallback || (
          <div style={{ padding: '20px', color: '#ff6666' }}>
            <h2>Something went wrong</h2>
            <pre>{this.state.error?.message}</pre>
          </div>
        )
      );
    }

    return this.props.children;
  }
}
```

11.2 Debounced Search

Prevent excessive queries by debouncing search input:

```typescript
import { useState, useEffect } from 'react';

function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState(value);

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => clearTimeout(handler);
  }, [value, delay]);

  return debouncedValue;
}

// In FilterBar, use debounced search:
const debouncedSearch = useDebounce(search, 300);
useEffect(() => {
  onSearchChange(debouncedSearch);
}, [debouncedSearch]);
```

11.3 Request Cancellation

Cancel in-flight requests when filters change:

```typescript
// In LibraryView
const abortControllerRef = useRef<AbortController | null>(null);

const loadClips = useCallback(async (reset: boolean = false) => {
  // Cancel previous request
  if (abortControllerRef.current) {
    abortControllerRef.current.abort();
  }
  abortControllerRef.current = new AbortController();

  // ... rest of function
}, [/* deps */]);
```

---

Part 12: Module Registration

12.1 Update main.rs Module List

```rust
mod cli;
mod commands;  // Add this line
mod constants;
mod db;
mod error;
mod hash;
mod ingest;
mod jobs;
mod metadata;
mod camera;
mod preview;
mod tools;
```

12.2 Final Project Structure

After Phase 3, your project structure should be:

```
dad-cam/
  src/                          # React frontend
    api/
      clips.ts                  # API wrapper functions
    components/
      App.tsx                   # Main app component
      LibraryView.tsx           # Library browser
      ClipGrid.tsx              # Virtualized thumbnail grid
      ClipThumbnail.tsx         # Single thumbnail component
      SpriteHover.tsx           # Sprite scrubbing component
      VideoPlayer.tsx           # Video player modal
      FilterBar.tsx             # Filter controls
      ErrorBoundary.tsx         # Error boundary
    types/
      clips.ts                  # TypeScript interfaces
    utils/
      paths.ts                  # Path utilities
      thumbnailCache.ts         # LRU thumbnail cache
    main.tsx                    # React entry point
    index.css                   # Global styles
  src-tauri/
    src/
      commands/
        mod.rs                  # Commands module
        library.rs              # Library commands
        clips.rs                # Clip commands
        tags.rs                 # Tag commands
      # ... existing modules from Phase 1-2
    Cargo.toml
    tauri.conf.json
    capabilities/
      default.json              # Updated permissions
```

---

Part 13: Helper Binary Bundling

13.1 Overview

Dad Cam requires external tools (ffmpeg, ffprobe, exiftool) to be bundled with the app. These cannot rely on system-installed versions because:
1. Users may not have them installed
2. Version differences cause inconsistent behavior
3. Cross-platform requirement demands predictable tooling

Phase 2 already implemented the tools module (`src-tauri/src/tools.rs`) for CLI usage. Phase 3 reuses that same infrastructure - no new bundling code is needed, but the Tauri build must be configured correctly.

13.2 Tauri Sidecar Configuration

The sidecar binaries are configured in `src-tauri/tauri.conf.json`:

```json
{
  "bundle": {
    "externalBin": [
      "binaries/ffmpeg",
      "binaries/ffprobe",
      "binaries/exiftool"
    ]
  }
}
```

Binary naming convention (Tauri requirement):
- `binaries/ffmpeg-x86_64-pc-windows-msvc.exe` (Windows x64)
- `binaries/ffmpeg-x86_64-apple-darwin` (macOS Intel)
- `binaries/ffmpeg-aarch64-apple-darwin` (macOS Apple Silicon)
- `binaries/ffmpeg-x86_64-unknown-linux-gnu` (Linux x64)

Same pattern for ffprobe and exiftool.

13.3 Binary Resolution at Runtime

The existing `tools.rs` module handles resolution. For GUI mode, the same functions work:

```rust
// In src-tauri/src/tools.rs (already implemented in Phase 2)
pub fn resolve_ffmpeg() -> Result<PathBuf> {
    // 1. Check sidecar location first
    // 2. Fall back to PATH if running in dev mode
    // ...
}
```

GUI mode uses the same resolution logic - no changes needed.

13.4 First-Launch Download (Optional)

For smaller initial download, binaries can be downloaded on first launch using `ffmpeg-sidecar` crate:

```rust
use ffmpeg_sidecar::command::ffmpeg_is_installed;
use ffmpeg_sidecar::download::auto_download;

async fn ensure_ffmpeg() -> Result<(), String> {
    if !ffmpeg_is_installed() {
        auto_download().map_err(|e| format!("Failed to download ffmpeg: {}", e))?;
    }
    Ok(())
}
```

This is already configured in Phase 2. The GUI app will work with bundled binaries or auto-download if missing.

13.5 Verification

To verify bundling works:

```bash
# Build release
npm run tauri build

# Check bundle contents (macOS)
ls "src-tauri/target/release/bundle/macos/Dad Cam.app/Contents/Resources/"

# Should see:
# ffmpeg-aarch64-apple-darwin (or x86_64)
# ffprobe-aarch64-apple-darwin
# exiftool-aarch64-apple-darwin
```

---

Part 14: Archive and Delete Behavior (Stub)

14.1 Overview

Phase 3 includes basic tagging (favorite/bad). A future "Personal Mode" may allow:
- Archiving clips (hide from default view)
- Deleting clips (remove from library, NOT originals)

Per contracts.md: Original files are NEVER deleted by the app.

Phase 3 stubs this functionality without implementing destructive operations.

14.2 Archived Tag (Database Ready)

The tags table (created in Phase 1) already supports custom tags. Add the "archived" tag:

```sql
-- Run during library initialization (Phase 1 schema)
INSERT OR IGNORE INTO tags (name, is_system) VALUES ('archived', 1);
```

14.3 Stub UI Elements

Add disabled "Archive" button to ClipThumbnail.tsx:

```typescript
// In src/components/ClipThumbnail.tsx, inside tag-buttons div:

{/* Archive button - stubbed for future Personal Mode */}
<button
  onClick={(e) => {
    e.stopPropagation();
    // TODO: Phase 7 - Personal Mode
    console.log('Archive not yet implemented');
  }}
  disabled={true}
  style={{
    background: 'none',
    border: 'none',
    cursor: 'not-allowed',
    padding: '4px',
    color: '#444',
    fontSize: '14px',
    opacity: 0.5,
  }}
  title="Archive (coming in Personal Mode)"
>
  A
</button>
```

14.4 Filter Support (Pre-wired)

The filter bar already supports custom filters. When Personal Mode is implemented, add "archived" filter:

```typescript
// In FilterBar.tsx, when Personal Mode is enabled:
const filters: { value: ClipQuery['filter']; label: string }[] = [
  { value: 'all', label: 'All Clips' },
  { value: 'favorites', label: 'Favorites' },
  { value: 'bad', label: 'Bad' },
  { value: 'unreviewed', label: 'Unreviewed' },
  // { value: 'archived', label: 'Archived' },  // Uncomment for Personal Mode
];
```

14.5 Backend Query Support (Pre-wired)

The clips.rs get_clips command already handles tag-based filtering. No changes needed - "archived" filter will work once enabled:

```rust
// In src-tauri/src/commands/clips.rs, inside get_clips match:
"archived" => {
    conditions.push(
        "EXISTS (SELECT 1 FROM clip_tags ct JOIN tags t ON ct.tag_id = t.id
         WHERE ct.clip_id = c.id AND t.name = 'archived')".to_string()
    );
}
```

14.6 Delete Behavior (NOT IMPLEMENTED)

Delete functionality is explicitly NOT implemented in Phase 3. Future implementation must:
1. Only delete database records and derived assets (proxies, thumbs, sprites)
2. NEVER delete original files (contracts.md non-negotiable)
3. Require explicit user confirmation
4. Be reversible (soft delete with recovery option)

This is deferred to Phase 7 (Personal Mode) per development-plan.md.

---

Resources

- [Tauri 2.0 Documentation](https://v2.tauri.app/)
- [Tauri Calling Rust from Frontend](https://v2.tauri.app/develop/calling-rust/)
- [TanStack Virtual Documentation](https://tanstack.com/virtual/latest)
- [LRU Cache npm package](https://www.npmjs.com/package/lru-cache)
- [Sprite Sheet Hover Previews](https://dev.to/speaklouder/how-video-platforms-show-instant-hover-previews-using-sprite-sheets-in-nodejs-2l0l)
- [Tauri SQL Plugin](https://v2.tauri.app/plugin/sql/)
- [Tauri Sidecar Binaries](https://v2.tauri.app/develop/sidecar/)
- [ffmpeg-sidecar Crate](https://crates.io/crates/ffmpeg-sidecar)

---

Next Steps

After Phase 3 is complete:
- Phase 4: Scoring Engine (heuristic scoring for "Best Clips")
- Phase 5: Auto-Edit Engine (VHS Mode generation)
- Phase 6: Export System

The UI you built will be extended to show:
- Best Clips view with score threshold slider
- VHS Mode generation wizard
- Export history and controls

See development-plan.md for the full roadmap.

---

Part 10: Shippable Hardening (Preview Readiness + Path Resolution)

Phase 3 must not assume proxies/thumbs/sprites already exist. The UI must stay stable if previews are missing,
stale, or still generating. We solve this by adding:

1) A Rust-side asset URL resolver (cross-platform `file://` URLs)
2) A preview readiness API (status + optional enqueue)
3) A lightweight background enqueue on library open (no UI lockups)
4) Explicit UI states for missing / generating / unavailable assets

This keeps the “viewer first” promise while staying fully offline and contract-compliant.

---

10.1 Asset URL Resolver (Rust)

The DB stores relative paths with POSIX separators (Phase 0 contract). The frontend needs a URL it can load
(`asset://` or `file://`). Keep it simple and use `tauri::api::path::normalize_path` + `tauri::Url::from_file_path`.

Create `src-tauri/src/commands/assets.rs`:

```rust
use tauri::State;
use serde::{Deserialize, Serialize};

use super::DbState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetUrlResponse {
    pub url: String,
}

/// Convert a stored relative DB path (POSIX separators) into a file:// URL usable by the UI.
#[tauri::command]
pub async fn resolve_asset_url(
    rel_path: String,
    state: State<'_, DbState>,
) -> Result<AssetUrlResponse, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Library root
    let root: String = conn
        .query_row("SELECT root_path FROM libraries LIMIT 1", [], |row| row.get(0))
        .map_err(|e| format!("Failed to get library root: {}", e))?;

    // Convert DB posix path into OS path safely
    let abs_path = std::path::Path::new(&root).join(rel_path.replace('/', &std::path::MAIN_SEPARATOR.to_string()));

    let url = tauri::Url::from_file_path(&abs_path)
        .map_err(|_| format!("Failed to convert to file URL: {}", abs_path.display()))?
        .to_string();

    Ok(AssetUrlResponse { url })
}
```

Register the module:
- Add `pub mod assets;` under `src-tauri/src/commands/mod.rs`
- Re-export with `pub use assets::*;`

Why this matters:
- Windows paths work (no broken separators)
- Frontend doesn’t guess filesystem rules
- You can later swap this for a custom protocol without refactoring UI

---

10.2 Preview Status API (Rust)

We need the UI to know whether a clip is ready to display:
- thumb available?
- proxy available?
- sprite available?
- are they stale?
- are jobs queued/running?

Create `src-tauri/src/commands/preview.rs`:

```rust
use serde::{Deserialize, Serialize};
use tauri::State;

use super::DbState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStatus {
    pub clip_id: i64,
    pub has_thumb: bool,
    pub has_proxy: bool,
    pub has_sprite: bool,
    pub thumb_stale: bool,
    pub proxy_stale: bool,
    pub sprite_stale: bool,
    pub queued_jobs: Vec<String>,   // ["thumb", "proxy", "sprite"]
    pub running_jobs: Vec<String>,
}

/// Read preview availability + staleness for a clip.
/// Staleness is defined exactly the same as Phase 2 invalidate logic (pipeline_version + derived_params hash).
#[tauri::command]
pub async fn get_preview_status(
    clip_id: i64,
    state: State<'_, DbState>,
) -> Result<PreviewStatus, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Reuse Phase 2 preview status queries if you already implemented them for CLI.
    // Minimal approach: determine "has_*" by presence of clip_assets rows, and staleness by comparing
    // asset.pipeline_version with constants::PIPELINE_VERSION.
    let pipeline_version = crate::constants::PIPELINE_VERSION as i64;

    let has_thumb: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM clip_assets ca JOIN assets a ON ca.asset_id=a.id
           WHERE ca.clip_id=?1 AND ca.role='thumb'
         )",
        [clip_id],
        |row| row.get::<_, i64>(0),
    ).map_err(|e| format!("status query failed: {}", e))? == 1;

    let has_proxy: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM clip_assets ca JOIN assets a ON ca.asset_id=a.id
           WHERE ca.clip_id=?1 AND ca.role='proxy'
         )",
        [clip_id],
        |row| row.get::<_, i64>(0),
    ).map_err(|e| format!("status query failed: {}", e))? == 1;

    let has_sprite: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM clip_assets ca JOIN assets a ON ca.asset_id=a.id
           WHERE ca.clip_id=?1 AND ca.role='sprite'
         )",
        [clip_id],
        |row| row.get::<_, i64>(0),
    ).map_err(|e| format!("status query failed: {}", e))? == 1;

    // Staleness (simple, version-based). If you have derived_params hashing fully implemented, extend this
    // to compare expected DerivedParams hash vs stored.
    let thumb_stale: bool = conn.query_row(
        "SELECT COALESCE((
           SELECT a.pipeline_version < ?2 FROM clip_assets ca JOIN assets a ON ca.asset_id=a.id
           WHERE ca.clip_id=?1 AND ca.role='thumb'
           ORDER BY a.created_at DESC LIMIT 1
         ), 0)",
        rusqlite::params![clip_id, pipeline_version],
        |row| row.get::<_, i64>(0),
    ).map_err(|e| format!("stale query failed: {}", e))? == 1;

    let proxy_stale: bool = conn.query_row(
        "SELECT COALESCE((
           SELECT a.pipeline_version < ?2 FROM clip_assets ca JOIN assets a ON ca.asset_id=a.id
           WHERE ca.clip_id=?1 AND ca.role='proxy'
           ORDER BY a.created_at DESC LIMIT 1
         ), 0)",
        rusqlite::params![clip_id, pipeline_version],
        |row| row.get::<_, i64>(0),
    ).map_err(|e| format!("stale query failed: {}", e))? == 1;

    let sprite_stale: bool = conn.query_row(
        "SELECT COALESCE((
           SELECT a.pipeline_version < ?2 FROM clip_assets ca JOIN assets a ON ca.asset_id=a.id
           WHERE ca.clip_id=?1 AND ca.role='sprite'
           ORDER BY a.created_at DESC LIMIT 1
         ), 0)",
        rusqlite::params![clip_id, pipeline_version],
        |row| row.get::<_, i64>(0),
    ).map_err(|e| format!("stale query failed: {}", e))? == 1;

    // Jobs in queue / running (reuse Phase 1 jobs table)
    let mut queued_jobs = Vec::new();
    let mut running_jobs = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT job_type, status FROM jobs WHERE clip_id=?1 AND job_type IN ('thumb','proxy','sprite')
         AND status IN ('queued','running')"
    ).map_err(|e| format!("jobs query failed: {}", e))?;

    let rows = stmt.query_map([clip_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }).map_err(|e| format!("jobs query failed: {}", e))?;

    for r in rows.flatten() {
        if r.1 == "queued" { queued_jobs.push(r.0); }
        if r.1 == "running" { running_jobs.push(r.0); }
    }

    Ok(PreviewStatus {
        clip_id,
        has_thumb,
        has_proxy,
        has_sprite,
        thumb_stale,
        proxy_stale,
        sprite_stale,
        queued_jobs,
        running_jobs,
    })
}
```

Register:
- Add `pub mod preview;` and `pub use preview::*;` in `commands/mod.rs`
- Add it to `tauri::generate_handler![...]` list

---

10.3 Ensure Previews (Optional Enqueue) (Rust)

Now provide an action the UI can call when it sees missing previews.

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnsurePreviewsResponse {
    pub clip_id: i64,
    pub enqueued: Vec<String>, // roles enqueued
}

/// Enqueue missing/stale preview jobs for a clip.
/// Uses Phase 1 job system + Phase 2 generators. Safe to call repeatedly.
#[tauri::command]
pub async fn ensure_clip_previews(
    clip_id: i64,
    state: State<'_, DbState>,
) -> Result<EnsurePreviewsResponse, String> {
    let db_lock = state.0.lock().map_err(|_| "Lock error")?;
    let conn = db_lock.as_ref().ok_or("No library open")?;

    // Use the same logic as Phase 2 CLI preview command:
    // - compute expected DerivedParams
    // - if missing/stale, enqueue jobs (thumb first, then sprite, then proxy)
    // Minimal implementation: if missing or stale by pipeline_version, queue.
    let status = get_preview_status(clip_id, state).await?;

    let mut enqueued = Vec::new();

    // Pseudocode: insert jobs if not already queued/running
    // (Implement using your Phase 1 jobs::enqueue_job helper.)
    let want_thumb = !status.has_thumb || status.thumb_stale;
    let want_sprite = !status.has_sprite || status.sprite_stale;
    let want_proxy = !status.has_proxy || status.proxy_stale;

    if want_thumb { crate::jobs::enqueue_preview_job(conn, clip_id, "thumb").map_err(|e| e.to_string())?; enqueued.push("thumb".into()); }
    if want_sprite { crate::jobs::enqueue_preview_job(conn, clip_id, "sprite").map_err(|e| e.to_string())?; enqueued.push("sprite".into()); }
    if want_proxy { crate::jobs::enqueue_preview_job(conn, clip_id, "proxy").map_err(|e| e.to_string())?; enqueued.push("proxy".into()); }

    Ok(EnsurePreviewsResponse { clip_id, enqueued })
}
```

Notes:
- Always enqueue **thumb first** so the grid becomes usable ASAP.
- Keep your Phase 2 concurrency guardrails (default one ffmpeg job at a time).

---

10.4 Background Enqueue on Library Open (Frontend)

After `open_library`, kick off a background request:
- fetch first N clips (e.g., first 100)
- call `ensure_clip_previews` for clips missing thumbs
- let the normal job runner generate previews

This makes first-time user experience “just works”.

Minimal approach:
- On app open: only ensure thumbs for the first screen.
- When scrolling: opportunistically ensure previews for visible clips.

---

10.5 Frontend Changes (Stable UI States)

Update your `ClipCard` UI states:

- If `thumb_url` missing: show skeleton + “Generating thumbnail…” if queued/running
- If `proxy_url` missing: clicking opens player with “Generating proxy…” overlay (don’t crash)
- If reference mode and source missing: show “Source offline” state, and disable playback

Implementation pattern:
- `get_clips` returns relative paths (as today)
- UI calls `resolve_asset_url(rel_path)` lazily and caches results
- UI calls `get_preview_status(clip_id)` when needed (hover/click/visible)

---

10.6 Explicit Limitation: Single Open Library

Phase 3 supports:
- One active library open at a time
- Library switching via `open_library()` replacing DbState connection

Document this clearly so future Phase 7+ multi-library work is additive.

---

10.7 Update Testing Checklist (Hardening)

Add these tests to Part 9:

1) Open library with no previews:
- grid loads, shows placeholders, and begins generating thumbs

2) Delete a proxy file on disk:
- player shows error state and offers “Regenerate previews” (calls ensure_clip_previews)

3) Unplug a reference-mode drive:
- app does not crash; shows “Source offline” state

4) Upgrade `PIPELINE_VERSION`:
- stale previews are detected and regenerated (status shows stale before regen)

---

End of Phase 3 Hardening


---

End of Phase 3 Implementation Guide
