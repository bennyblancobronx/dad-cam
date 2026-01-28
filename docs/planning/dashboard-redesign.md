# Dad Cam - Dashboard Redesign Implementation Plan

Version: 1.4
Created: 2026-01-27
Updated: 2026-01-27

---

## Executive Summary

This document outlines all changes required to implement:
1. App-level settings persistence (fix "always shows open library" issue)
2. Personal vs Pro mode distinction
3. Welcome Dashboard (Personal mode)
4. Library Dashboard (Pro mode)
5. Left Navigation Bar with Library, Events, Dates, Settings sections
6. Events system for organizing clips

---

## Table of Contents

1. [Problem Analysis](#1-problem-analysis)
2. [Architecture Overview](#2-architecture-overview)
3. [Database Changes](#3-database-changes)
4. [Backend Changes (Rust)](#4-backend-changes-rust)
5. [Frontend Changes (React)](#5-frontend-changes-react)
6. [Implementation Checklist](#6-implementation-checklist)
7. [File Change Summary](#7-file-change-summary)
8. [Testing Plan](#8-testing-plan)

---

## 1. Problem Analysis

### 1.1 Current State

**Q: Does the app have a persistent database?**

**A: YES** - Each library has its own SQLite database at `.dadcam/dadcam.db` that stores:
- Library metadata
- Clips with full metadata (title, duration, dimensions, recorded_at, etc.)
- Assets with file paths (originals, thumbnails, proxies, sprites)
- Jobs, tags, fingerprints, volumes

**Q: Why do we keep getting the "open library" screen?**

**A:** The app has NO app-level settings persistence. Every launch:
1. `App.tsx` initializes with `library = null`
2. No saved "last opened library" path
3. No "recent libraries" list
4. User must manually select a library every time

### 1.2 Current Data Flow

```
App Launch
    |
    v
App.tsx: library = null
    |
    v
Show "Open Library" screen (ALWAYS)
    |
    v
User selects library folder
    |
    v
open_library() reads .dadcam/dadcam.db
    |
    v
Show LibraryView
```

### 1.3 Target Data Flow

```
App Launch
    |
    v
Load App Settings (Tauri Store)
    |
    +-- mode == 'personal'?
    |       |
    |       +-- Has lastLibrary? --> Auto-open --> Welcome Dashboard
    |       |
    |       +-- No library? --> Create/Open Library prompt
    |
    +-- mode == 'pro'?
            |
            v
        Library Dashboard (multi-library view)
            |
            v
        User selects library --> Welcome Dashboard
```

---

## 2. Architecture Overview

### 2.1 Mode Definitions

| Mode | Target User | Libraries | Features |
|------|-------------|-----------|----------|
| **Personal** | End user (Dad) | Single library | Simplified UI, auto-open last library |
| **Pro** | Developer/Rental house | Multiple libraries | Multi-library management, advanced features |

### 2.2 Dashboard Definitions

| Dashboard | Mode | When Shown | Purpose |
|-----------|------|------------|---------|
| **Welcome Dashboard** | Personal | Inside open library | Main workspace for single library |
| **Library Dashboard** | Pro | App startup | Multi-library selection and management |

### 2.3 Component Hierarchy

```
App.tsx
  |
  +-- [Pro Mode] LibraryDashboard
  |       |
  |       +-- RecentLibraries
  |       +-- NewLibraryButton
  |       +-- OpenLibraryButton
  |
  +-- [Personal Mode OR Library Selected] MainLayout
          |
          +-- LeftNav
          |     +-- LibrarySection
          |     +-- EventsSection
          |     +-- DatesSection
          |     +-- SettingsSection
          |
          +-- ContentArea
                +-- WelcomeDashboard (default)
                +-- ClipGrid (when browsing)
                +-- EventView (when viewing event)
                +-- DateView (when viewing date)
```

---

## 3. Database Changes

### 3.1 App-Level Settings (NOT in SQLite)

App settings are stored via Tauri Store plugin at:
- macOS: `~/Library/Application Support/com.dadcam.app/settings.json`
- Windows: `%APPDATA%\com.dadcam.app\settings.json`
- Linux: `~/.config/com.dadcam.app/settings.json`

**Settings Schema:**
```json
{
  "version": 1,
  "mode": "personal",
  "lastLibraryPath": "/path/to/library",
  "recentLibraries": [
    {
      "path": "/path/to/library",
      "name": "My Videos",
      "lastOpened": "2026-01-27T12:00:00Z",
      "clipCount": 150
    }
  ],
  "maxRecentLibraries": 10
}
```

### 3.2 Migration 3: Events System (per-library SQLite)

```sql
-- Migration 3: Events system for clip organization

-- Events table
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    type TEXT NOT NULL CHECK (type IN ('date_range', 'clip_selection')),
    -- For date_range type
    date_start TEXT,
    date_end TEXT,
    -- Metadata
    color TEXT DEFAULT '#3b82f6',
    icon TEXT DEFAULT 'calendar',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Event clips (for clip_selection type, or manual additions to date_range)
CREATE TABLE event_clips (
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    clip_id INTEGER NOT NULL REFERENCES clips(id) ON DELETE CASCADE,
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (event_id, clip_id)
);

-- Indexes
CREATE INDEX idx_events_library ON events(library_id);
CREATE INDEX idx_events_type ON events(type);
CREATE INDEX idx_events_date_range ON events(date_start, date_end);
CREATE INDEX idx_event_clips_event ON event_clips(event_id);
CREATE INDEX idx_event_clips_clip ON event_clips(clip_id);
```

### 3.3 Migration 4: Library Thumbnails (optional enhancement)

```sql
-- Migration 4: Library thumbnail support

-- Add thumbnail_clip_id to libraries (which clip to use as library thumbnail)
ALTER TABLE libraries ADD COLUMN thumbnail_clip_id INTEGER REFERENCES clips(id);

-- Add cover_image_path for custom library covers
ALTER TABLE libraries ADD COLUMN cover_image_path TEXT;
```

### 3.4 Database Schema Summary

| Table | Purpose | Migration |
|-------|---------|-----------|
| `libraries` | Library metadata | 1 |
| `assets` | File paths (originals, thumbs, proxies, sprites) | 1 |
| `clips` | Video metadata | 1 |
| `clip_assets` | Clip-to-asset mappings | 1 |
| `tags` | Tag definitions | 1 |
| `clip_tags` | Clip-to-tag mappings | 1 |
| `jobs` | Background job queue | 1 |
| `clip_scores` | Heuristic scores | 2 |
| `clip_score_overrides` | User score adjustments | 2 |
| **`events`** | Event definitions | **3 (NEW)** |
| **`event_clips`** | Event-to-clip mappings | **3 (NEW)** |

---

## 4. Backend Changes (Rust)

### 4.1 Dependencies to Add

**Cargo.toml:**
```toml
[dependencies]
tauri-plugin-store = "2"
tauri-plugin-dialog = "2"  # For native save dialog (Stills feature)
```

### 4.2 New Tauri Commands

#### Settings Commands (`src-tauri/src/commands/settings.rs`)

| Command | Parameters | Returns | Purpose |
|---------|------------|---------|---------|
| `get_app_settings` | - | `AppSettings` | Load app settings from store |
| `save_app_settings` | `settings: AppSettings` | `()` | Save app settings to store |
| `get_mode` | - | `String` | Get current mode (personal/pro) |
| `set_mode` | `mode: String` | `()` | Set mode |
| `add_recent_library` | `path, name, clipCount` | `()` | Add/update recent library entry |
| `remove_recent_library` | `path: String` | `()` | Remove from recent libraries |
| `get_recent_libraries` | - | `Vec<RecentLibrary>` | Get recent libraries list |

**Implementation Pattern:**
```rust
use tauri_plugin_store::StoreExt;

#[tauri::command]
pub fn get_app_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    let settings = AppSettings {
        version: store.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32,
        mode: store.get("mode").and_then(|v| v.as_str()).unwrap_or("personal").to_string(),
        last_library_path: store.get("lastLibraryPath").and_then(|v| v.as_str()).map(|s| s.to_string()),
        recent_libraries: // deserialize from store...
    };

    Ok(settings)
}
```

#### Events Commands (`src-tauri/src/commands/events.rs`)

| Command | Parameters | Returns | Purpose |
|---------|------------|---------|---------|
| `create_event` | `name, type, dateStart?, dateEnd?` | `Event` | Create new event |
| `get_events` | - | `Vec<Event>` | List all events in library |
| `get_event` | `eventId: i64` | `Event` | Get single event with clip count |
| `update_event` | `eventId, name?, dateStart?, dateEnd?` | `Event` | Update event |
| `delete_event` | `eventId: i64` | `()` | Delete event |
| `add_clips_to_event` | `eventId, clipIds: Vec<i64>` | `()` | Add clips to event |
| `remove_clips_from_event` | `eventId, clipIds: Vec<i64>` | `()` | Remove clips from event |
| `get_event_clips` | `eventId, offset, limit` | `ClipListResponse` | Get clips in event |
| `get_clips_by_date` | `date: String` | `Vec<ClipView>` | Get clips for specific date |
| `get_clips_grouped_by_date` | `offset, limit` | `DateGroupResponse` | Get clips grouped by date |

### 4.3 New Schema Functions (`src-tauri/src/db/schema.rs`)

```rust
// ----- Events -----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: i64,
    pub library_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub event_type: String,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub color: String,
    pub icon: String,
    pub clip_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

pub fn insert_event(conn: &Connection, event: &NewEvent) -> Result<i64>;
pub fn get_event(conn: &Connection, id: i64) -> Result<Option<Event>>;
pub fn list_events(conn: &Connection, library_id: i64) -> Result<Vec<Event>>;
pub fn update_event(conn: &Connection, id: i64, updates: &EventUpdate) -> Result<()>;
pub fn delete_event(conn: &Connection, id: i64) -> Result<()>;
pub fn add_clips_to_event(conn: &Connection, event_id: i64, clip_ids: &[i64]) -> Result<()>;
pub fn remove_clips_from_event(conn: &Connection, event_id: i64, clip_ids: &[i64]) -> Result<()>;
pub fn get_event_clip_ids(conn: &Connection, event_id: i64) -> Result<Vec<i64>>;
pub fn get_clips_by_date_range(conn: &Connection, library_id: i64, start: &str, end: &str) -> Result<Vec<Clip>>;
```

### 4.4 File Structure

```
src-tauri/src/
  commands/
    mod.rs          # Add: pub mod settings; pub mod events; pub mod stills;
    settings.rs     # NEW: App settings commands
    events.rs       # NEW: Events commands
    stills.rs       # NEW: Export still frame command
  db/
    migrations.rs   # Add: Migration 3 (events)
    schema.rs       # Add: Event structs and functions
```

### 4.5 Stills Command Implementation

**src-tauri/src/commands/stills.rs:**
```rust
use tauri_plugin_dialog::DialogExt;
use std::path::PathBuf;
use crate::tools::ffmpeg_path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StillExportRequest {
    pub clip_id: i64,
    pub timestamp_ms: i64,
    pub format: String,  // "jpg" or "png"
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StillExportResult {
    pub output_path: String,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
}

#[tauri::command]
pub async fn export_still(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::commands::DbState>,
    request: StillExportRequest,
) -> Result<StillExportResult, String> {
    // 1. Get clip and original asset path
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let conn = conn.as_ref().ok_or("No library open")?;

    let clip = crate::db::schema::get_clip(conn, request.clip_id)
        .map_err(|e| e.to_string())?
        .ok_or("Clip not found")?;

    let asset = crate::db::schema::get_asset(conn, clip.original_asset_id)
        .map_err(|e| e.to_string())?
        .ok_or("Original asset not found")?;

    // 2. Verify original file exists
    let library_root = crate::commands::library::get_library_root_internal(&conn)?;
    let original_path = PathBuf::from(&library_root).join(&asset.path);

    if !original_path.exists() {
        return Err(format!("Original file offline: {}", asset.path));
    }

    // 3. Show native save dialog
    let default_name = format!("{}_frame.{}",
        clip.title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_"),
        request.format
    );

    let file_path = app.dialog()
        .file()
        .set_file_name(&default_name)
        .add_filter("Image", &[&request.format])
        .save_file()
        .await
        .ok_or("Save cancelled")?;

    let output_path = file_path.as_path()
        .ok_or("Invalid save path")?
        .to_path_buf();

    // 4. Convert timestamp to seconds
    let timestamp_secs = request.timestamp_ms as f64 / 1000.0;

    // 5. Build FFmpeg command
    let ffmpeg = ffmpeg_path().map_err(|e| e.to_string())?;
    let quality_args: Vec<&str> = match request.format.as_str() {
        "jpg" => vec!["-q:v", "2"],  // High quality JPEG
        "png" => vec!["-compression_level", "6"],
        _ => return Err("Unsupported format. Use jpg or png.".to_string()),
    };

    let status = std::process::Command::new(&ffmpeg)
        .args([
            "-ss", &format!("{:.3}", timestamp_secs),
            "-i", original_path.to_str().unwrap(),
            "-vframes", "1",
        ])
        .args(&quality_args)
        .arg("-y")  // Overwrite
        .arg(output_path.to_str().unwrap())
        .status()
        .map_err(|e| format!("FFmpeg failed to start: {}", e))?;

    if !status.success() {
        return Err("FFmpeg failed to export frame".to_string());
    }

    // 6. Get output file info
    let metadata = std::fs::metadata(&output_path)
        .map_err(|e| format!("Failed to read output: {}", e))?;

    // Get dimensions via ffprobe
    let (width, height) = get_image_dimensions(&output_path).unwrap_or((0, 0));

    Ok(StillExportResult {
        output_path: output_path.to_string_lossy().to_string(),
        width,
        height,
        size_bytes: metadata.len(),
    })
}

fn get_image_dimensions(path: &PathBuf) -> Option<(u32, u32)> {
    let ffprobe = crate::tools::ffprobe_path().ok()?;
    let output = std::process::Command::new(&ffprobe)
        .args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height",
            "-of", "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(',').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().ok()?;
        let h = parts[1].parse().ok()?;
        return Some((w, h));
    }
    None
}
```

**Error Cases Handled:**
- Original file offline/missing
- User cancels save dialog
- Invalid timestamp
- FFmpeg execution failure
- Disk full / write failure
- Unsupported format

---

## 5. Frontend Changes (React)

### 5.1 New TypeScript Types (`src/types/`)

**settings.ts:**
```typescript
export interface AppSettings {
  version: number;
  mode: 'personal' | 'pro';
  lastLibraryPath: string | null;
  recentLibraries: RecentLibrary[];
}

export interface RecentLibrary {
  path: string;
  name: string;
  lastOpened: string;
  clipCount: number;
  thumbnailPath?: string;
}
```

**events.ts:**
```typescript
// Event type constants (use these instead of string literals)
export const EVENT_TYPES = {
  DATE_RANGE: 'date_range',
  CLIP_SELECTION: 'clip_selection',
} as const;

export type EventType = typeof EVENT_TYPES[keyof typeof EVENT_TYPES];

export interface Event {
  id: number;
  libraryId: number;
  name: string;
  description: string | null;
  type: EventType;
  dateStart: string | null;
  dateEnd: string | null;
  color: string;
  icon: string;
  clipCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface DateGroup {
  date: string;
  clipCount: number;
  clips: ClipView[];
}

// Helper to check event type
export function isDateRangeEvent(event: Event): boolean {
  return event.type === EVENT_TYPES.DATE_RANGE;
}

export function isClipSelectionEvent(event: Event): boolean {
  return event.type === EVENT_TYPES.CLIP_SELECTION;
}
```

### 5.2 New API Functions (`src/api/`)

**settings.ts:**
```typescript
export async function getAppSettings(): Promise<AppSettings>;
export async function saveAppSettings(settings: AppSettings): Promise<void>;
export async function getMode(): Promise<'personal' | 'pro'>;
export async function setMode(mode: 'personal' | 'pro'): Promise<void>;
export async function addRecentLibrary(path: string, name: string, clipCount: number): Promise<void>;
export async function getRecentLibraries(): Promise<RecentLibrary[]>;
```

**events.ts:**
```typescript
export async function createEvent(name: string, type: EventType, dateStart?: string, dateEnd?: string): Promise<Event>;
export async function getEvents(): Promise<Event[]>;
export async function getEvent(eventId: number): Promise<Event>;
export async function updateEvent(eventId: number, updates: Partial<Event>): Promise<Event>;
export async function deleteEvent(eventId: number): Promise<void>;
export async function addClipsToEvent(eventId: number, clipIds: number[]): Promise<void>;
export async function removeClipsFromEvent(eventId: number, clipIds: number[]): Promise<void>;
export async function getEventClips(eventId: number, offset: number, limit: number): Promise<ClipListResponse>;
export async function getClipsByDate(date: string): Promise<ClipView[]>;
export async function getClipsGroupedByDate(offset: number, limit: number): Promise<DateGroup[]>;
```

### 5.3 App State Management (`src/context/`)

**AppContext.tsx** - Global state for settings and navigation:

```typescript
import { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { AppSettings, RecentLibrary } from '../types/settings';
import { getAppSettings, saveAppSettings } from '../api/settings';

// Navigation views (state-based routing, not React Router)
export type AppView =
  | { type: 'welcome' }                           // Welcome Dashboard (Personal mode)
  | { type: 'library-dashboard' }                 // Library Dashboard (Pro mode)
  | { type: 'library-setup' }                     // Create/Open library prompt
  | { type: 'clips' }                             // Main clip grid
  | { type: 'event'; eventId: number }            // Event detail view
  | { type: 'date'; date: string }                // Date detail view
  | { type: 'settings' };                         // Settings panel

interface AppContextValue {
  // Settings
  settings: AppSettings | null;
  updateSettings: (updates: Partial<AppSettings>) => Promise<void>;

  // Navigation
  currentView: AppView;
  navigate: (view: AppView) => void;
  goBack: () => void;

  // Library
  isLibraryOpen: boolean;
}

const AppContext = createContext<AppContextValue | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [currentView, setCurrentView] = useState<AppView>({ type: 'welcome' });
  const [viewHistory, setViewHistory] = useState<AppView[]>([]);
  const [isLibraryOpen, setIsLibraryOpen] = useState(false);

  // Load settings on mount
  useEffect(() => {
    getAppSettings().then(setSettings).catch(console.error);
  }, []);

  const updateSettings = async (updates: Partial<AppSettings>) => {
    if (!settings) return;
    const newSettings = { ...settings, ...updates };
    await saveAppSettings(newSettings);
    setSettings(newSettings);
  };

  const navigate = (view: AppView) => {
    setViewHistory(prev => [...prev, currentView]);
    setCurrentView(view);
  };

  const goBack = () => {
    const prev = viewHistory[viewHistory.length - 1];
    if (prev) {
      setViewHistory(h => h.slice(0, -1));
      setCurrentView(prev);
    }
  };

  return (
    <AppContext.Provider value={{
      settings,
      updateSettings,
      currentView,
      navigate,
      goBack,
      isLibraryOpen,
    }}>
      {children}
    </AppContext.Provider>
  );
}

export function useApp() {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error('useApp must be used within AppProvider');
  return ctx;
}
```

**Navigation Pattern:**
- State-based routing (not React Router) - simpler for desktop app
- View history stack for back navigation
- Pro mode: "Back to Libraries" button calls `navigate({ type: 'library-dashboard' })`
- Personal mode: No library switching needed

### 5.4 New Components (`src/components/`)

| Component | File | Purpose |
|-----------|------|---------|
| **MainLayout** | `MainLayout.tsx` | Container with LeftNav + ContentArea |
| **LeftNav** | `LeftNav.tsx` | Left navigation bar |
| **LibrarySection** | `nav/LibrarySection.tsx` | Library info in nav |
| **EventsSection** | `nav/EventsSection.tsx` | Events list in nav |
| **DatesSection** | `nav/DatesSection.tsx` | Date navigation in nav |
| **SettingsSection** | `nav/SettingsSection.tsx` | Settings in nav |
| **WelcomeDashboard** | `WelcomeDashboard.tsx` | Personal mode main view |
| **LibraryDashboard** | `LibraryDashboard.tsx` | Pro mode library selection |
| **LibraryCard** | `LibraryCard.tsx` | Library thumbnail card |
| **EventView** | `EventView.tsx` | View clips in an event |
| **DateView** | `DateView.tsx` | View clips by date |
| **CreateEventModal** | `modals/CreateEventModal.tsx` | Create/edit event |
| **SettingsPanel** | `SettingsPanel.tsx` | Settings configuration |

### 5.4 Component Specifications

#### WelcomeDashboard
```
+------------------------------------------+
|              Welcome Dashboard            |
+------------------------------------------+
|                                          |
|   +------------+  +------------+         |
|   |  Import    |  |   Stills   |         |
|   |  Footage   |  | (S key)    |         |
|   +------------+  +------------+         |
|                                          |
|   +------------+                         |
|   |   Export   |                         |
|   |  Footage   |                         |
|   +------------+                         |
|                                          |
+------------------------------------------+
```

**Stills Feature:**
- Export high-quality still frame from video
- User pauses video, clicks "Stills" or presses S
- Opens save dialog with format options (JPG/PNG)
- Exports current frame at original resolution (not proxy)

**Post-Action Navigation Flows:**

| Action | After Completion | Notes |
|--------|------------------|-------|
| Import Footage | Stay on Welcome Dashboard | Show success toast with counts: "Imported 15 clips (2 skipped)" |
| Stills (from Welcome) | Navigate to clip grid | Open video player for user to select frame, then return |
| Export Footage | Navigate to Export view | Show recipe builder or recent exports |

**Import Footage Flow:**
1. User clicks "Import Footage"
2. Native folder picker opens
3. Ingest runs with progress indicator
4. On completion: success toast + clip count updates
5. Stay on Welcome Dashboard (user may want to import more)

**Stills Flow (from Welcome Dashboard):**
1. User clicks "Stills" button
2. Navigate to clip grid: `navigate({ type: 'clips' })`
3. User selects clip, opens video player
4. User seeks to desired frame, presses S
5. Save dialog appears, user saves
6. Show success toast with file path

#### LibraryDashboard (Pro Mode)
```
+------------------------------------------+
|           Dad Cam - Libraries            |
+------------------------------------------+
|                                          |
|  Recent Libraries                        |
|  +--------+ +--------+ +--------+        |
|  |thumb   | |thumb   | |thumb   |        |
|  |Library1| |Library2| |Library3|        |
|  |150 clips| |89 clips| |42 clips|       |
|  +--------+ +--------+ +--------+        |
|                                          |
|  [+ New Library]  [Open Library]         |
|                                          |
+------------------------------------------+
```

**Pro Mode Navigation:**
When library is open in Pro mode, the Header includes a "Back to Libraries" button:
```
+------------------------------------------+
| [< Libraries]  Library: My Videos        |
+------------------------------------------+
```
Clicking "< Libraries" calls `navigate({ type: 'library-dashboard' })` and closes current library.

#### LeftNav
```
+------------------+
| Library          |
|   My Videos      |
|   150 clips      |
+------------------+
| Events           |
|   + New Event    |
|   > Vacation '25 |
|   > Tim's Party  |
+------------------+
| Dates            |
|   > 2025         |
|     > January    |
|     > February   |
|   > 2024         |
+------------------+
| Settings         |
|   Mode: Personal |
|   [Switch to Pro]|
+------------------+
```

### 5.5 Keyboard Shortcuts

**Existing shortcuts (VideoPlayer.tsx):**
| Key | Action |
|-----|--------|
| Space / K | Play/Pause |
| J | Seek back 10s |
| L | Seek forward 10s |
| M | Mute/Unmute |
| F | Toggle fullscreen |

**New shortcuts to add:**
| Key | Context | Action |
|-----|---------|--------|
| S | VideoPlayer focused | Export still frame (opens save dialog) |
| Escape | Any modal open | Close modal |
| Escape | Event/Date view | Go back to previous view |

**Implementation pattern (in VideoPlayer.tsx):**
```typescript
const handleExportStill = async () => {
  if (!currentClip || !videoRef.current) return;

  const timestampMs = Math.floor(videoRef.current.currentTime * 1000);
  try {
    const result = await exportStill({
      clipId: currentClip.id,
      timestampMs,
      format: 'jpg',  // Default, user can change in save dialog
    });
    // Show success toast
  } catch (err) {
    // Show error toast
  }
};

useEffect(() => {
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.target instanceof HTMLInputElement) return;

    switch (e.key.toLowerCase()) {
      case 's':
        e.preventDefault();
        handleExportStill();
        break;
      // ... existing shortcuts
    }
  };

  window.addEventListener('keydown', handleKeyDown);
  return () => window.removeEventListener('keydown', handleKeyDown);
}, [currentClip]);
```

### 5.6 App.tsx Refactor

```typescript
function App() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [library, setLibrary] = useState<LibraryInfo | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  // Load settings on mount
  useEffect(() => {
    loadAppSettings();
  }, []);

  async function loadAppSettings() {
    try {
      const appSettings = await getAppSettings();
      setSettings(appSettings);

      // Personal mode: auto-open last library
      if (appSettings.mode === 'personal' && appSettings.lastLibraryPath) {
        const lib = await openLibrary(appSettings.lastLibraryPath);
        setLibrary(lib);
      }
    } catch (err) {
      console.error('Failed to load settings:', err);
      // Default to personal mode with no library
      setSettings({ version: 1, mode: 'personal', lastLibraryPath: null, recentLibraries: [] });
    } finally {
      setIsLoading(false);
    }
  }

  if (isLoading) {
    return <LoadingScreen />;
  }

  // Pro mode without library selected: show Library Dashboard
  if (settings?.mode === 'pro' && !library) {
    return <LibraryDashboard settings={settings} onLibrarySelect={handleLibrarySelect} />;
  }

  // Personal mode without library: show create/open prompt
  if (settings?.mode === 'personal' && !library) {
    return <LibrarySetup onLibraryCreated={handleLibrarySelect} />;
  }

  // Library is open: show main layout
  return (
    <MainLayout
      library={library}
      settings={settings}
      onClose={handleCloseLibrary}
      onSettingsChange={handleSettingsChange}
    />
  );
}
```

---

## 6. Implementation Checklist

### Phase 1: App Settings Persistence (Priority: CRITICAL)

- [ ] **1.1** Add `tauri-plugin-store` to Cargo.toml
- [ ] **1.2** Register plugin in `lib.rs`
- [ ] **1.3** Create `src-tauri/src/commands/settings.rs`
  - [ ] `get_app_settings` command
  - [ ] `save_app_settings` command
  - [ ] `get_mode` command
  - [ ] `set_mode` command
  - [ ] `add_recent_library` command
  - [ ] `get_recent_libraries` command
- [ ] **1.4** Register settings commands in `lib.rs`
- [ ] **1.5** Create `src/types/settings.ts`
- [ ] **1.6** Create `src/api/settings.ts`
- [ ] **1.7** Update `App.tsx` to load settings on mount
- [ ] **1.8** Auto-open last library in Personal mode
- [ ] **1.9** Save library to recent list on open

### Phase 2: Mode System (Priority: HIGH)

- [ ] **2.1** Add mode toggle to settings
- [ ] **2.2** Create `SettingsPanel.tsx` component
- [ ] **2.3** Implement mode switching logic in `App.tsx`
- [ ] **2.4** Persist mode preference

### Phase 3: Library Dashboard - Pro Mode (Priority: HIGH)

- [ ] **3.1** Create `LibraryDashboard.tsx`
- [ ] **3.2** Create `LibraryCard.tsx`
- [ ] **3.3** Implement recent libraries grid
- [ ] **3.4** Add "New Library" button with dialog
- [ ] **3.5** Add "Open Library" button with folder picker
- [ ] **3.6** Add library thumbnail support (optional)

### Phase 4: Welcome Dashboard - Personal Mode (Priority: HIGH)

- [ ] **4.1** Create `WelcomeDashboard.tsx`
- [ ] **4.2** Add "Import Footage" button (uses existing ingest)
- [ ] **4.3** Add "Stills" button (export frame from video)
  - [ ] Add `export_still` Tauri command
  - [ ] Add keyboard shortcut (S) in VideoPlayer
  - [ ] Save dialog with JPG/PNG format options
  - [ ] Export at original resolution (from original file, not proxy)
- [ ] **4.4** Add "Export Footage" button (opens clip in export view)

### Phase 5: Left Navigation Bar (Priority: MEDIUM)

- [ ] **5.1** Create `MainLayout.tsx` with left nav + content area
- [ ] **5.2** Create `LeftNav.tsx` container
- [ ] **5.3** Create `nav/LibrarySection.tsx`
- [ ] **5.4** Create `nav/EventsSection.tsx`
- [ ] **5.5** Create `nav/DatesSection.tsx`
- [ ] **5.6** Create `nav/SettingsSection.tsx`
- [ ] **5.7** Add navigation state management
- [ ] **5.8** Style nav with dark theme

### Phase 6: Events System (Priority: MEDIUM)

- [ ] **6.1** Add Migration 3 (events tables) to `migrations.rs`
- [ ] **6.2** Add event schema functions to `schema.rs`
- [ ] **6.3** Create `src-tauri/src/commands/events.rs`
  - [ ] `create_event` command
  - [ ] `get_events` command
  - [ ] `get_event` command
  - [ ] `update_event` command
  - [ ] `delete_event` command
  - [ ] `add_clips_to_event` command
  - [ ] `remove_clips_from_event` command
  - [ ] `get_event_clips` command
- [ ] **6.4** Register events commands in `lib.rs`
- [ ] **6.5** Create `src/types/events.ts`
- [ ] **6.6** Create `src/api/events.ts`
- [ ] **6.7** Create `EventView.tsx`
- [ ] **6.8** Create `modals/CreateEventModal.tsx`
- [ ] **6.9** Add event creation from date range selection
- [ ] **6.10** Add event creation from clip selection

### Phase 7: Dates View (Priority: MEDIUM)

- [ ] **7.1** Add `get_clips_grouped_by_date` command
- [ ] **7.2** Create `DateView.tsx`
- [ ] **7.3** Add date tree navigation in LeftNav
- [ ] **7.4** Implement date grouping (Year > Month > Day)

### Phase 8: Polish & Integration (Priority: LOW) - COMPLETE

- [x] **8.1** Add loading states to all views (CSS classes: skeleton, loading-indicator, loading-inline)
- [x] **8.2** Add error boundaries (ErrorBoundary.tsx - v0.1.24)
- [x] **8.3** Add keyboard shortcuts (VideoPlayer: Space/K/J/L/M/F/S/Escape/N/P, DatesSection tree nav)
- [x] **8.4** Add tooltips and help text (title attributes on all interactive elements)
- [x] **8.5** Performance optimization (TanStack Virtual - v0.1.23)
- [x] **8.6** Update CSS for new layout (App.css comprehensive styles - v0.1.58)

### Phase 9: CLI Parity (Priority: MEDIUM)

- [ ] **9.1** Add Events CLI commands to `cli.rs`
  - [ ] `dadcam event-create`
  - [ ] `dadcam event-list`
  - [ ] `dadcam event-show`
  - [ ] `dadcam event-update`
  - [ ] `dadcam event-delete`
  - [ ] `dadcam event-add-clips`
  - [ ] `dadcam event-remove-clips`

### Phase 10: Error Handling & Validation (Priority: HIGH)

- [ ] **10.1** Add library path validation on startup
  - [ ] Check if lastLibraryPath exists
  - [ ] If not, clear from settings and show picker
- [ ] **10.2** Add recent libraries cleanup
  - [ ] Validate each recent library path exists
  - [ ] Remove or mark stale entries
- [ ] **10.3** Add Tauri store permissions to capabilities
- [ ] **10.4** Add empty state messages
  - [ ] Empty event (no clips)
  - [ ] Empty date (no clips)
  - [ ] Empty library (no clips)

### Phase 11: Export CLI (DEFERRED)

- [ ] **11.1** Implement Phase 5 export commands in CLI
- [ ] **11.2** Implement Phase 6 export history commands in CLI
- [ ] **11.3** Implement Phase 7 batch commands in CLI

---

## 7. File Change Summary

### New Files

| File | Type | Purpose |
|------|------|---------|
| `src-tauri/src/commands/settings.rs` | Rust | App settings commands |
| `src-tauri/src/commands/events.rs` | Rust | Events commands |
| `src-tauri/src/commands/stills.rs` | Rust | Export still frame command |
| `src/types/settings.ts` | TS | Settings type definitions |
| `src/types/events.ts` | TS | Events type definitions |
| `src/api/settings.ts` | TS | Settings API functions |
| `src/api/events.ts` | TS | Events API functions |
| `src/api/stills.ts` | TS | Stills export API function |
| `src/context/AppContext.tsx` | TSX | Global state and navigation |
| `src/components/MainLayout.tsx` | TSX | Main layout with nav |
| `src/components/LeftNav.tsx` | TSX | Left navigation bar |
| `src/components/nav/LibrarySection.tsx` | TSX | Library nav section |
| `src/components/nav/EventsSection.tsx` | TSX | Events nav section |
| `src/components/nav/DatesSection.tsx` | TSX | Dates nav section |
| `src/components/nav/SettingsSection.tsx` | TSX | Settings nav section |
| `src/components/WelcomeDashboard.tsx` | TSX | Personal mode dashboard |
| `src/components/LibraryDashboard.tsx` | TSX | Pro mode dashboard |
| `src/components/LibraryCard.tsx` | TSX | Library thumbnail card |
| `src/components/EventView.tsx` | TSX | Event clips view |
| `src/components/DateView.tsx` | TSX | Date clips view |
| `src/components/modals/CreateEventModal.tsx` | TSX | Create event dialog |
| `src/components/SettingsPanel.tsx` | TSX | Settings configuration |
| `src/components/EmptyState.tsx` | TSX | Reusable empty state component |
| `src/components/UnmountedLibraryView.tsx` | TSX | UI for unmounted volume error |

### Modified Files

| File | Changes |
|------|---------|
| `src-tauri/Cargo.toml` | Add `tauri-plugin-store` and `tauri-plugin-dialog` dependencies |
| `src-tauri/src/lib.rs` | Register new commands, add store + dialog plugins |
| `src-tauri/src/commands/mod.rs` | Export settings, events, and stills modules |
| `src-tauri/src/db/migrations.rs` | Add Migration 3 (events) |
| `src-tauri/src/db/schema.rs` | Add event structs and functions |
| `src-tauri/src/cli.rs` | Add event CLI commands |
| `src-tauri/capabilities/default.json` | Add store + dialog plugin permissions |
| `src/App.tsx` | Complete refactor for mode-based routing |
| `src/App.css` | Add styles for new layout |
| `src/components/LibraryView.tsx` | Integrate into MainLayout (not replace) |
| `src/components/VideoPlayer.tsx` | Add Stills export (S key shortcut) |
| `src/components/ClipGrid.tsx` | Add selection mode for multi-select |

---

## 8. Testing Plan

### Unit Tests

| Test | File | Purpose |
|------|------|---------|
| Settings persistence | `settings_test.rs` | Verify settings save/load |
| Settings corruption recovery | `settings_test.rs` | Verify corrupted JSON returns defaults |
| Event CRUD | `events_test.rs` | Verify event operations |
| Event date range query | `events_test.rs` | Verify date() function works correctly |
| Date grouping | `clips_test.rs` | Verify date grouping logic |
| Stills export | `stills_test.rs` | Verify FFmpeg frame extraction |

### Integration Tests

| Test | Purpose |
|------|---------|
| Personal mode startup | App opens last library automatically |
| Pro mode startup | App shows library dashboard |
| Mode switching | Switching modes persists correctly |
| Event creation | Creating event stores in database |
| Event clips | Adding/removing clips from events |
| Date navigation | Clicking date shows correct clips |

### Manual Test Checklist

**Startup & Settings:**
- [ ] Fresh install: App opens in Personal mode with setup prompt
- [ ] Create library: Library created, opens automatically
- [ ] Close and reopen: Same library opens automatically (Personal mode)
- [ ] Switch to Pro mode: Shows Library Dashboard
- [ ] Settings persist after app restart

**Library Management:**
- [ ] Open different library in Pro mode: Correct library opens
- [ ] Recent libraries: Shows last 10 opened libraries
- [ ] Delete library folder externally, reopen app: Graceful handling, removed from recent
- [ ] Open non-existent path: Shows error, doesn't crash
- [ ] Unmounted external drive: Shows "Library Not Available" UI with retry option
- [ ] Pro mode: "Back to Libraries" button returns to Library Dashboard
- [ ] Corrupted settings.json: App resets to defaults with warning toast

**Events:**
- [ ] Create event (date range): Event shows clips from date range
- [ ] Create event (clip selection): Event shows selected clips
- [ ] Edit event name/dates: Changes saved
- [ ] Delete event: Event removed, clips unchanged
- [ ] Add clips to existing event: Clips appear in event view
- [ ] Remove clips from event: Clips removed from event view
- [ ] Empty event: Shows helpful empty state

**Navigation:**
- [ ] Date navigation: Clicking date filters clips correctly
- [ ] Event navigation: Clicking event shows event clips
- [ ] Library section: Shows current library info
- [ ] Back to dashboard: Returns to welcome/library dashboard

**Stills (Frame Export):**
- [ ] Press S in video player: Opens save dialog
- [ ] Export as JPG: Creates high-quality JPG at original resolution
- [ ] Export as PNG: Creates PNG at original resolution
- [ ] Stills button on Welcome Dashboard: Opens video player for selection
- [ ] Cancel save dialog: No file created, no error
- [ ] Original file offline: Shows "Original file offline" error message
- [ ] Disk full: Shows appropriate error, no partial file left behind

**CLI Parity:**
- [ ] `dadcam event-list`: Shows events
- [ ] `dadcam event-create`: Creates event
- [ ] `dadcam event-show`: Shows event details
- [ ] `dadcam event-delete`: Removes event

---

## Appendix A: Research Notes

### Tauri Store Plugin

**Documentation:** https://v2.tauri.app/plugin/store/

**Installation:**
```bash
cargo add tauri-plugin-store
```

**Usage:**
```rust
// In lib.rs
tauri::Builder::default()
    .plugin(tauri_plugin_store::Builder::new().build())
```

```rust
// In command
use tauri_plugin_store::StoreExt;

let store = app.store("settings.json")?;
store.set("key", value);
store.save()?;
```

### Date Grouping Query

```sql
-- Get clips grouped by date for tree view
SELECT
    strftime('%Y', recorded_at) as year,
    strftime('%m', recorded_at) as month,
    strftime('%d', recorded_at) as day,
    COUNT(*) as clip_count
FROM clips
WHERE library_id = ?
GROUP BY year, month, day
ORDER BY year DESC, month DESC, day DESC;
```

### Event Clips Query (date_range type)

**NOTE:** Use `date()` function for proper date comparison since `recorded_at` is ISO timestamp.

```sql
-- Get clips for a date_range event
-- Use date() to compare just the date portion of ISO timestamps
SELECT c.* FROM clips c
WHERE c.library_id = ?
  AND date(c.recorded_at) >= date(?)  -- event.date_start (YYYY-MM-DD)
  AND date(c.recorded_at) <= date(?)  -- event.date_end (YYYY-MM-DD)
ORDER BY c.recorded_at;

-- Combined with explicit event_clips
SELECT c.* FROM clips c
WHERE c.library_id = ?
  AND (
    -- Clips in date range (inclusive)
    (date(c.recorded_at) >= date(?) AND date(c.recorded_at) <= date(?))
    OR
    -- Explicitly added clips
    c.id IN (SELECT clip_id FROM event_clips WHERE event_id = ?)
  )
ORDER BY c.recorded_at;
```

**Why date() is needed:**
- `recorded_at` stores full ISO timestamps like `2025-01-15T14:30:00`
- Event dates are just `YYYY-MM-DD` strings
- String comparison `2025-01-15T14:30:00 >= 2025-01-15` fails (T > empty)
- `date()` extracts just the date portion for correct comparison

---

## Appendix B: UI Mockups

See Figma/design files (if available) or create based on component specifications in Section 5.4.

---

## 9. AUDIT - Gaps & Missing Items

### 9.1 CRITICAL GAPS - RESOLVED

| # | Gap | Impact | Resolution | Status |
|---|-----|--------|------------|--------|
| **G1** | No CLI commands for Events | CLI users can't manage events | Add Phase 9 CLI commands | PLANNED |
| **G2** | Export CLI not implemented | Phase 5-6 commands missing | Phase 11 (DEFERRED) | DEFERRED |
| **G3** | "Screen Grabs" undefined | Placeholder in plan | Renamed to "Stills" - frame export | RESOLVED |
| **G4** | "Add New Camera" undefined | User requirement | DEFERRED - future feature | DEFERRED |
| **G5** | Library validation missing | App crashes if path deleted | Add Phase 10 validation | PLANNED |
| **G6** | Recent libraries cleanup | Stale entries | Add Phase 10 cleanup | PLANNED |

### 9.2 CLI Commands - Current vs Required

**Currently Implemented (cli.rs):**
```
dadcam init <path>
dadcam ingest <path>
dadcam list
dadcam show <id>
dadcam jobs
dadcam relink-scan <path>
dadcam preview
dadcam preview-status
dadcam invalidate
dadcam cleanup
dadcam check-tools
dadcam score
dadcam score-status
dadcam best-clips
dadcam score-override
```

**Documented in techguide.md but NOT implemented:**
```
# Phase 5-6 Export (NOT IN CLI YET)
dadcam recipe-create
dadcam recipe-list
dadcam recipe-show
dadcam export
dadcam export-list
dadcam export-show
dadcam export-rerun
dadcam export-history
dadcam export-details
dadcam export-open

# Phase 7 Pro Mode (NOT IN CLI YET)
dadcam init-reference
dadcam batch-ingest
dadcam batch-export
dadcam relink
dadcam list-offline
dadcam list-presets
dadcam create-preset
dadcam delete-preset
dadcam volume-info

# Phase 8 ML (NOT IN CLI YET)
dadcam ml-analyze
dadcam ml-status
dadcam train-scoring
dadcam best-clips-ml
```

**NEW CLI Commands Needed for Dashboard Redesign:**
```
# Events (Phase 9 - NEW)
dadcam event-create <name> --type <date_range|clip_selection> [--start <date>] [--end <date>]
dadcam event-list
dadcam event-show <event_id>
dadcam event-update <event_id> [--name <name>] [--start <date>] [--end <date>]
dadcam event-delete <event_id> [--confirm]
dadcam event-add-clips <event_id> <clip_ids...>
dadcam event-remove-clips <event_id> <clip_ids...>

# Camera Profiles (DEFERRED - future feature)
# dadcam camera-list
# dadcam camera-show <profile_id>
# dadcam camera-create <name> --make <make> --model <model>
# dadcam camera-delete <profile_id>

# Settings (Phase 9 - NEW, optional CLI)
dadcam config-get [<key>]
dadcam config-set <key> <value>
dadcam config-reset
```

### 9.3 Questions - RESOLVED

| # | Question | Answer | Action |
|---|----------|--------|--------|
| **Q1** | What is "Screen Grabs"? | Export still frames from video at current playback position | Renamed to "Stills" - export high-quality frame as JPG/PNG |
| **Q2** | What is "Add New Camera"? | Planned future feature | REMOVED from this plan, deferred |
| **Q3** | Should Export CLI be implemented now? | NO - not built yet | DEFERRED to Phase 11 |
| **Q4** | Should activity feed be included? | NO - not requested | REMOVED from plan |

### 9.4 Missing UI Interactions - RESOLVED

| Interaction | Current State | Solution |
|-------------|---------------|----------|
| Clip multi-select | Not implemented | See implementation below |
| Date range picker | Basic inputs in FilterBar | Use native `<input type="date">` (no library needed) |
| Drag-drop clips to events | Not implemented | DEFERRED - Nice-to-have |
| Context menu on clips | Not implemented | DEFERRED - Phase 2 enhancement |
| Keyboard navigation | Partial | Full nav support needed |

**Clip Multi-Select Implementation:**

Add to `ClipGrid.tsx`:
```typescript
interface ClipGridProps {
  // ... existing props
  selectionMode?: boolean;
  selectedClipIds?: Set<number>;
  onSelectionChange?: (clipIds: Set<number>) => void;
}

function ClipGrid({ selectionMode, selectedClipIds, onSelectionChange, ...props }: ClipGridProps) {
  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);

  const handleClipClick = (clipId: number, index: number, e: React.MouseEvent) => {
    if (!selectionMode || !onSelectionChange) {
      // Normal click - open clip
      return;
    }

    const newSelection = new Set(selectedClipIds);

    if (e.shiftKey && lastSelectedIndex !== null) {
      // Range select
      const start = Math.min(lastSelectedIndex, index);
      const end = Math.max(lastSelectedIndex, index);
      for (let i = start; i <= end; i++) {
        newSelection.add(clips[i].id);
      }
    } else if (e.metaKey || e.ctrlKey) {
      // Toggle single
      if (newSelection.has(clipId)) {
        newSelection.delete(clipId);
      } else {
        newSelection.add(clipId);
      }
    } else {
      // Single select (replace)
      newSelection.clear();
      newSelection.add(clipId);
    }

    setLastSelectedIndex(index);
    onSelectionChange(newSelection);
  };

  // Render checkbox overlay when in selection mode
  // ...
}
```

**Date Range Picker:**
Use native HTML5 date inputs (consistent with existing FilterBar pattern):
```tsx
<label>
  Start Date
  <input
    type="date"
    value={dateStart}
    onChange={(e) => setDateStart(e.target.value)}
  />
</label>
<label>
  End Date
  <input
    type="date"
    value={dateEnd}
    onChange={(e) => setDateEnd(e.target.value)}
  />
</label>
```
No external library required. Native inputs work well in Tauri desktop apps.

### 9.5 Missing Error Handling - RESOLVED

| Scenario | Current | Solution |
|----------|---------|----------|
| lastLibraryPath doesn't exist | Crash or error | Graceful fallback to library picker |
| Recent library deleted | Shows stale entry | Remove from list, show indicator |
| Event has no clips | Not handled | Show empty state message |
| Date has no clips | Not handled | Skip in tree or show empty |
| Store file corrupted | Unknown | Reset to defaults with warning |
| **Unmounted volume** | Not handled | Detect and show specific message |

**Unmounted Volume Handling:**

When `lastLibraryPath` points to an unmounted external drive (e.g., `/Volumes/MyDrive/Videos`):

```typescript
// In loadAppSettings() - App.tsx
async function loadAppSettings() {
  try {
    const appSettings = await getAppSettings();
    setSettings(appSettings);

    if (appSettings.mode === 'personal' && appSettings.lastLibraryPath) {
      try {
        const lib = await openLibrary(appSettings.lastLibraryPath);
        setLibrary(lib);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : String(err);

        // Check if it's an unmounted volume (path doesn't exist)
        if (errorMsg.includes('not found') || errorMsg.includes('does not exist')) {
          // Show specific message for unmounted drives
          setUnmountedLibrary({
            path: appSettings.lastLibraryPath,
            name: appSettings.recentLibraries.find(
              r => r.path === appSettings.lastLibraryPath
            )?.name || 'Unknown Library',
          });
        } else {
          setLoadError(errorMsg);
        }
      }
    }
  } catch (err) {
    // Settings file corrupted - reset to defaults
    console.error('Failed to load settings, resetting to defaults:', err);
    const defaultSettings = {
      version: 1,
      mode: 'personal' as const,
      lastLibraryPath: null,
      recentLibraries: [],
    };
    await saveAppSettings(defaultSettings);
    setSettings(defaultSettings);
    // Show warning toast: "Settings were corrupted and have been reset"
  } finally {
    setIsLoading(false);
  }
}
```

**Unmounted Library UI:**
```
+------------------------------------------+
|         Library Not Available            |
+------------------------------------------+
|                                          |
|  "My Videos" is on a drive that's not    |
|  currently connected.                    |
|                                          |
|  Path: /Volumes/MyDrive/Videos           |
|                                          |
|  [Try Again]  [Open Different Library]   |
|                                          |
|  [ ] Remove from recent libraries        |
|                                          |
+------------------------------------------+
```

**Settings Corruption Recovery:**

In `get_app_settings` (Rust):
```rust
#[tauri::command]
pub fn get_app_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    // Try to parse settings, return defaults if corrupted
    match parse_settings_from_store(&store) {
        Ok(settings) => Ok(settings),
        Err(e) => {
            eprintln!("Settings corrupted, returning defaults: {}", e);
            Ok(AppSettings::default())
        }
    }
}
```

### 9.6 Missing Tauri Configuration - RESOLVED

**File: `src-tauri/capabilities/default.json`**

Add these permissions to the existing `permissions` array:
```json
{
  "permissions": [
    // ... existing permissions ...

    // Store plugin (app settings)
    "store:allow-get",
    "store:allow-set",
    "store:allow-save",
    "store:allow-load",
    "store:allow-clear",

    // Dialog plugin (Stills save dialog)
    "dialog:allow-save"
  ]
}
```

**Register plugins in `src-tauri/src/lib.rs`:**
```rust
tauri::Builder::default()
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_dialog::init())
    // ... existing plugins and commands
```

### 9.7 Camera Profile System - DEFERRED

**Current state:**
- `camera_profiles` table exists (Migration 1)
- Default profiles inserted: Sony Handycam, Canon DSLR, Panasonic MiniDV
- Auto-matching during ingest works

**Status:** User management of camera profiles is a PLANNED FUTURE FEATURE, not part of this dashboard redesign.

### 9.8 Stills Feature (Frame Export)

**Purpose:** Export high-quality still frame from video at current playback position.

**Implementation:**
```rust
#[tauri::command]
pub fn export_still(
    clip_id: i64,
    timestamp_ms: i64,
    output_path: String,
    format: String, // "jpg" or "png"
) -> Result<String, String>
```

**FFmpeg command:**
```bash
ffmpeg -ss <timestamp> -i <original_file> -vframes 1 -q:v 2 <output_path>
```

**Key points:**
- Use ORIGINAL file, not proxy (for full resolution)
- Seek to exact timestamp from video player
- Save dialog lets user choose location and format
- Keyboard shortcut: S (when video player focused)

---

## 10. Revised Implementation Phases

### Phase 1: App Settings (CRITICAL) - No changes

### Phase 2: Mode System - No changes

### Phase 3: Library Dashboard (Pro Mode) - No changes

### Phase 4: Welcome Dashboard (Personal Mode)
- Stills feature: Export high-quality frame from video (JPG/PNG)

### Phase 5: Left Navigation - No changes

### Phase 6: Events System
- Add: Clip multi-select mode for clip_selection events
- Add: Date picker component for date_range events

### Phase 7: Dates View - No changes

### Phase 8: Polish - No changes

### Phase 9: CLI Parity (NEW)
- [ ] **9.1** Add Events CLI commands
- [ ] **9.2** Add Settings CLI commands (optional)

### Phase 10: Error Handling & Validation (HIGH)
- [ ] **10.1** Add library path validation on startup
- [ ] **10.2** Add recent libraries cleanup
- [ ] **10.3** Add Tauri store permissions to capabilities
- [ ] **10.4** Add empty state messages

### Phase 11: Export CLI (DEFERRED)
- [ ] **11.1** Implement Export CLI (Phase 5-6 commands)
- [ ] **11.2** Implement Batch operations CLI (Phase 7 commands)
- [ ] **11.3** Implement ML CLI (Phase 8 commands)

---

## Appendix C: Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-27 | Initial plan |
| 1.1 | 2026-01-27 | Audit added: gaps identified, CLI parity analysis |
| 1.2 | 2026-01-27 | User feedback: renamed Screen Grabs to Stills, removed Activity Feed, deferred Camera Profiles |
| 1.3 | 2026-01-27 | Braun Design Language audit: complete design specs added for all components |
| 1.4 | 2026-01-27 | Implementation audit: 16 gaps resolved - added tauri-plugin-dialog, AppContext, navigation system, stills command impl, keyboard shortcuts, multi-select pattern, date picker choice, unmounted volume handling, settings corruption recovery, capabilities config, date() query fix, event type constants, post-action flows, expanded test coverage |

---

## Appendix D: Braun Design Language Specifications

**All UI components MUST follow the Braun Design Language v1.0.0.**

Based on Dieter Rams' 10 Principles of Good Design: "Weniger, aber besser" (Less, but better).

---

### D.1 Typography

**Font: Braun Linear (EXCLUSIVE)** - No other fonts permitted.

```css
font-family: 'Braun Linear', system-ui, sans-serif;
```

| Element | Size | Weight | Line Height | Letter Spacing |
|---------|------|--------|-------------|----------------|
| Page title | 24px | 700 (Bold) | 1.3 | -0.01em |
| Section heading | 20px | 500 (Medium) | 1.4 | -0.01em |
| Subsection | 17px | 500 (Medium) | 1.5 | 0 |
| Body text | 15px | 400 (Regular) | 1.6 | 0 |
| Captions/meta | 13px | 300 (Light) | 1.5 | 0 |
| Labels (UPPERCASE) | 11px | 500 (Medium) | 1.3 | 0.1em |

**Weight mapping for this project:**
- `700 Bold` - Welcome Dashboard title, Library Dashboard title
- `500 Medium` - Buttons, nav items, card titles, modal titles
- `400 Regular` - Body text, descriptions, input values
- `300 Light` - Captions, metadata, clip counts, timestamps, file paths

---

### D.2 Color System

#### Light Mode

| Token | Hex | Use in Dad Cam |
|-------|-----|----------------|
| canvas | #FAFAF8 | App background |
| surface | #F4F4F2 | LeftNav, header backgrounds |
| surface-elevated | #FFFFFF | Cards, modals, dropdowns |
| text | #1C1C1A | Titles, body text |
| text-secondary | #5C5C58 | Descriptions, secondary labels |
| text-muted | #8A8A86 | Captions, clip counts, timestamps |
| border | #E2E1DE | Card borders, dividers |
| border-emphasis | #C0BFBC | Input borders, hover states |

#### Dark Mode

| Token | Value | Use in Dad Cam |
|-------|-------|----------------|
| canvas | #0a0a0b | App background |
| surface | #111113 | LeftNav, header backgrounds |
| surface-elevated | #1A1A1C | Cards, modals |
| text | rgba(250,250,248,0.87) | Titles, body text |
| text-secondary | rgba(250,250,248,0.60) | Descriptions |
| text-muted | rgba(250,250,248,0.38) | Captions, metadata |
| border | #1f1f23 | Card borders, dividers |
| border-emphasis | #3A3A3E | Input borders |

#### Functional Colors (Both Modes)

| Status | Color | Use in Dad Cam |
|--------|-------|----------------|
| success | #22c55e | Job complete, export success |
| warning | #eab308 | Processing, attention needed |
| error | #ef4444 | Job failed, errors |
| info | #3b82f6 | Informational badges |
| accent | #f59e0b (light) / #fbbf24 (dark) | Focus states, single emphasized CTA |

**NEVER use:**
- Pure black (#000000)
- Pure white (#FFFFFF) for backgrounds
- Decorative colors

---

### D.3 Spacing System (8pt Grid)

| Value | Use in Dad Cam |
|-------|----------------|
| 4px | Icon margins |
| 8px | Icon-to-label gaps, tight grouping |
| 12px | Related elements, input padding |
| 16px | Card padding, form field gaps |
| 24px | Section spacing, card grid gaps |
| 32px | Main content padding |
| 48px | Major section spacing |
| 64px | Page-level section gaps |

---

### D.4 Border Radius

| Value | Use |
|-------|-----|
| 4px | Badges, chips, tags |
| 6px | Buttons, inputs |
| 8px | Cards, panels, modals |
| 50% | Avatars, circular icons only |

**NEVER exceed 8px except for full circles.**

---

### D.5 Component Specifications

#### D.5.1 MainLayout

```
Header: height 64px, padding 0 24px, border-bottom 1px solid var(--color-border)
Sidebar (LeftNav): width 240px, padding 24px 16px, border-right 1px solid var(--color-border)
Main content: padding 32px
Background: var(--color-canvas)
```

#### D.5.2 LeftNav

```css
.left-nav {
  width: 240px;
  padding: 24px 16px;
  background: var(--color-surface);
  border-right: 1px solid var(--color-border);
}

.left-nav-section-title {
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--color-text-muted);
  margin-bottom: 8px;
}

.left-nav-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  font-size: 15px;
  font-weight: 500;
  color: var(--color-text-secondary);
  border-radius: 6px;
}

.left-nav-item:hover {
  background: rgba(0,0,0,0.03); /* light */
  color: var(--color-text);
}

.left-nav-item.is-active {
  background: rgba(0,0,0,0.05); /* light */
  color: var(--color-text);
}

/* Dark mode */
[data-theme="dark"] .left-nav-item:hover {
  background: rgba(255,255,255,0.05);
}
```

**LeftNav sections:**

| Section | Icon Size | Title Style |
|---------|-----------|-------------|
| Library | 20x20 | 11px/500/uppercase |
| Events | 20x20 | 11px/500/uppercase |
| Dates | 20x20 | 11px/500/uppercase |
| Settings | 20x20 | 11px/500/uppercase |

#### D.5.3 WelcomeDashboard

```
Layout: Centered content, max-width 800px
Title: 24px / 700 / var(--color-text)
Subtitle: 15px / 400 / var(--color-text-secondary)
Button grid: gap 24px
```

**Welcome Dashboard Buttons:**

| Button | Type | Size |
|--------|------|------|
| Import Footage | btn-primary | Large (52px height) |
| Stills | btn-secondary | Large (52px height) |
| Export Footage | btn-secondary | Large (52px height) |

```css
.welcome-btn {
  min-height: 52px;
  padding: 14px 28px;
  font-size: 17px;
  font-weight: 500;
  border-radius: 6px;
}
```

#### D.5.4 LibraryDashboard (Pro Mode)

```
Title: "dad cam" - 24px / 700 (lowercase as per Braun brand style)
Subtitle: "libraries" - 13px / 300 / var(--color-text-muted)
Card grid: gap 24px, columns auto-fill minmax(280px, 1fr)
```

#### D.5.5 LibraryCard

```css
.library-card {
  background: var(--color-surface-elevated);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  overflow: hidden;
}

.library-card:hover {
  border-color: var(--color-border-emphasis);
}

.library-card.is-selected {
  border-color: var(--color-text);
}

.library-card-image {
  width: 100%;
  aspect-ratio: 16/9;
  object-fit: cover;
  background: var(--color-surface);
}

.library-card-content {
  padding: 16px;
}

.library-card-title {
  font-size: 17px;
  font-weight: 500;
  color: var(--color-text);
  margin-bottom: 4px;
}

.library-card-meta {
  font-size: 13px;
  font-weight: 300;
  color: var(--color-text-muted);
}
```

#### D.5.6 EventView / DateView

```
Header: 64px height, title left-aligned
Title: 20px / 500 / var(--color-text)
Meta info: 13px / 300 / var(--color-text-muted)
Clip grid: gap 24px, auto-fill minmax(240px, 1fr)
Empty state: Centered, 64px icon, 20px/500 title, 15px/400 description
```

#### D.5.7 CreateEventModal

```css
.modal {
  width: 640px;
  background: var(--color-surface-elevated);
  border-radius: 8px;
  /* NO shadows in dark mode */
}

.modal-backdrop {
  background: rgba(0,0,0,0.5); /* light */
  /* background: rgba(0,0,0,0.7); dark */
}

.modal-header {
  padding: 24px 24px 0;
}

.modal-title {
  font-size: 20px;
  font-weight: 500;
}

.modal-body {
  padding: 24px;
}

.modal-footer {
  padding: 0 24px 24px;
  display: flex;
  justify-content: flex-end;
  gap: 12px;
}
```

#### D.5.8 SettingsPanel

```
Layout: Settings nav (200px) + Form content (max-width 640px)
Section title: 24px / 700
Section description: 15px / 400 / var(--color-text-secondary)
Field gap: 16px
Section gap: 48px
```

**Mode Toggle:**

```css
.toggle {
  width: 44px;
  height: 24px;
  border-radius: 9999px; /* Exception for toggles */
}

.toggle-off {
  background: var(--color-border);
}

.toggle-on {
  background: var(--color-text); /* #1C1C1A light, rgba(250,250,248,0.87) dark */
}
```

---

### D.6 Button Specifications

| Variant | Background | Text | Border | Use |
|---------|------------|------|--------|-----|
| btn-primary | #1C1C1A | #FFFFFF | none | Main actions (Import, Save) |
| btn-secondary | transparent | #1C1C1A | 1px #C0BFBC | Secondary actions (Cancel, Stills) |
| btn-ghost | transparent | #5C5C58 | none | Tertiary actions |
| btn-destructive | transparent | #ef4444 | 1px #ef4444 | Delete actions |

**Button Sizes:**

| Size | Height | Padding | Font Size | Use |
|------|--------|---------|-----------|-----|
| Small | 32px | 6px 12px | 13px | Compact UI, table actions |
| Medium | 44px | 10px 20px | 15px | Default |
| Large | 52px | 14px 28px | 17px | Welcome Dashboard CTAs |

**All buttons:**
- font-weight: 500
- border-radius: 6px
- focus: outline 2px solid #f59e0b, offset 2px

---

### D.7 Input Specifications

```css
.input {
  min-height: 44px;
  padding: 10px 12px;
  font-size: 15px;
  font-weight: 400;
  background: var(--color-surface-elevated);
  border: 1px solid var(--color-border-emphasis);
  border-radius: 6px;
}

.input:focus {
  border-color: #f59e0b;
  outline: none;
}

.input-label {
  display: block;
  margin-bottom: 8px;
  font-size: 11px;
  font-weight: 500;
  letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--color-text-secondary);
}

.input-helper {
  margin-top: 4px;
  font-size: 13px;
  font-weight: 300;
  color: var(--color-text-muted);
}
```

---

### D.8 Card Grid (ClipGrid)

```css
.clip-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 24px;
}

.clip-card {
  background: var(--color-surface-elevated);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  overflow: hidden;
}

.clip-card:hover {
  border-color: var(--color-border-emphasis);
}

.clip-card.is-selected {
  border-color: var(--color-text);
}

.clip-thumbnail {
  width: 100%;
  aspect-ratio: 16/9;
  object-fit: cover;
  background: var(--color-surface);
}

.clip-info {
  padding: 12px;
}

.clip-title {
  font-size: 15px;
  font-weight: 500;
  color: var(--color-text);
}

.clip-meta {
  font-size: 13px;
  font-weight: 300;
  color: var(--color-text-muted);
}
```

---

### D.9 Empty States

```css
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 64px 32px;
}

.empty-state-icon {
  width: 64px;
  height: 64px;
  color: var(--color-text-muted);
  margin-bottom: 24px;
  stroke-width: 1.5;
}

.empty-state-title {
  font-size: 20px;
  font-weight: 500;
  color: var(--color-text);
  margin-bottom: 8px;
}

.empty-state-description {
  font-size: 15px;
  font-weight: 400;
  color: var(--color-text-secondary);
  max-width: 400px;
  margin-bottom: 24px;
}
```

**Empty state messages:**

| Context | Title | Description |
|---------|-------|-------------|
| No library | No library selected | Create a new library or open an existing one to get started |
| Empty library | No clips yet | Import footage to add clips to your library |
| Empty event | No clips in event | Add clips to this event using dates or manual selection |
| Empty date | No clips on this date | No footage was recorded on this date |

---

### D.10 Loading States

**Rules:**
- < 300ms: No indicator
- 300ms - 2s: Text "Loading..."
- > 2s: Static progress bar

**NEVER use:** Animated skeletons, shimmer effects, pulse animations, spinners.

```css
.loading-text {
  font-size: 13px;
  font-weight: 300;
  color: var(--color-text-muted);
}

.progress-bar {
  height: 4px;
  background: var(--color-border);
  border-radius: 2px;
}

.progress-bar-fill {
  height: 100%;
  background: var(--color-text);
  /* NO transition - instant updates only */
}
```

---

### D.11 Icons

| Size | Dimensions | Stroke Width | Use |
|------|------------|--------------|-----|
| sm | 16x16 | 1.5px | Badges, compact UI |
| md | 20x20 | 2px | Nav items, buttons |
| lg | 24x24 | 2px | Section headers |
| xl | 32x32 | 2.5px | Empty states |

**Rules:**
- Stroke only, no fills
- Use `currentColor` for stroke
- No decorative icons - every icon must communicate function

---

### D.12 Dark Mode Implementation

```css
:root {
  color-scheme: light dark;
}

[data-theme="dark"] {
  --color-canvas: #0a0a0b;
  --color-surface: #111113;
  --color-surface-elevated: #1A1A1C;
  --color-text: rgba(250, 250, 248, 0.87);
  --color-text-secondary: rgba(250, 250, 248, 0.60);
  --color-text-muted: rgba(250, 250, 248, 0.38);
  --color-border: #1f1f23;
  --color-border-emphasis: #3A3A3E;
}

/* NO shadows in dark mode - use surface lightness for elevation */
[data-theme="dark"] .card,
[data-theme="dark"] .modal {
  box-shadow: none;
}
```

**Elevation in dark mode:**
| Level | Surface | Use |
|-------|---------|-----|
| 0 | #0a0a0b | Canvas |
| 1 | #111113 | Cards, sidebars |
| 2 | #1A1A1C | Elevated cards, dropdowns |
| 3 | #252527 | Modals, popovers |

---

### D.13 Accessibility Requirements

| Element | Minimum Contrast |
|---------|------------------|
| Normal text | 4.5:1 |
| Large text (24px+) | 3:1 |
| UI components | 3:1 |
| Focus indicators | 3:1 |

**Touch targets:** Minimum 44x44px (all buttons, interactive elements)

**Focus states:**
```css
:focus-visible {
  outline: 2px solid #f59e0b;
  outline-offset: 2px;
}
```

---

### D.14 Braun Verification Checklist

Before implementing any UI component, verify:

- [ ] Font is Braun Linear (check ALL text)
- [ ] Font weights match specifications (700/500/400/300 only)
- [ ] All spacing values on 8pt grid
- [ ] No border-radius > 8px (except 50% circles)
- [ ] No pure black (#000) or pure white (#FFF)
- [ ] No decorative colors (color = function only)
- [ ] No decorative shadows in dark mode
- [ ] No animated skeletons or loaders
- [ ] All interactive states defined (hover, focus, active, disabled)
- [ ] Touch targets >= 44px
- [ ] Contrast ratio >= 4.5:1
- [ ] Focus states visible (2px amber outline)
- [ ] Dark mode tested with correct tokens
- [ ] No gradients, glows, or decorative effects

---

### D.15 Anti-Patterns (REJECT)

The following are NOT permitted in Dad Cam UI:

- Colored accent buttons (except amber for single CTA)
- Gradient overlays on imagery
- Decorative shadows or glows
- Border-radius > 8px
- Animated loading skeletons
- Text shadows
- Color for decoration (not function)
- Ornamental icons
- Multiple font families
- Non-grid spacing values
- Pure black (#000) or white (#FFF)
- Spinners or animated loaders

---

### D.16 CSS Custom Properties (Required)

```css
:root {
  /* Colors */
  --color-canvas: #FAFAF8;
  --color-surface: #F4F4F2;
  --color-surface-elevated: #FFFFFF;
  --color-text: #1C1C1A;
  --color-text-secondary: #5C5C58;
  --color-text-muted: #8A8A86;
  --color-border: #E2E1DE;
  --color-border-emphasis: #C0BFBC;

  /* Accent */
  --color-accent: #f59e0b;
  --color-accent-hover: #d97706;

  /* Functional */
  --color-success: #22c55e;
  --color-warning: #eab308;
  --color-error: #ef4444;
  --color-info: #3b82f6;

  /* Spacing */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-6: 24px;
  --space-8: 32px;
  --space-12: 48px;
  --space-16: 64px;

  /* Radius */
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 8px;

  /* Z-index */
  --z-dropdown: 100;
  --z-sticky: 200;
  --z-modal-backdrop: 300;
  --z-modal: 400;
  --z-tooltip: 600;
  --z-toast: 700;
}
```

---

End of Implementation Plan
