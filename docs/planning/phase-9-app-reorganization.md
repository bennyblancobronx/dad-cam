# Dad Cam - Phase 9: App Reorganization

Version: 1.1.0
Status: Implemented
Created: 2026-01-28
Updated: 2026-01-29 (audit corrections)

---

## Overview

Dad Cam is a paid tool for making old school cameras useable in the modern day, plus automatic "VHS edits". We rent out dad cameras and need an app to automate the process and make it useable for end users.

**Business Model:**
- Paid app for anyone ($99 one-time)
- Free during rental (pre-generated keys)
- Free trial that blocks import/export after 14 days
- Cross-platform: Mac, Linux, Windows

**Core Features:**
- VHS Edits: J & L audio blending in one long timeline
- Opening title text (starts 5 seconds in, adjustable in dev menu)
- Sidecar files and EXIF dumps
- Dev menu for formulas and master camera library

---

## Master Checklist

### Phase 9A: First-Run Experience
- [ ] Create FirstRunWizard component
- [ ] Add "Simple" vs "Advanced" mode selection screen
- [ ] Simple mode: prompt for folder location once, create Default Project
- [ ] Advanced mode: navigate to Projects dashboard
- [ ] Store first-run-completed flag in settings
- [ ] Skip wizard on subsequent launches

### Phase 9B: Theme System (Light/Dark Mode)
- [ ] Define light mode CSS variables (Braun Light Mode)
- [ ] Set light mode as default
- [ ] Add theme toggle to settings (Advanced mode only)
- [ ] Hide theme toggle in Simple mode (always light)
- [ ] Store theme preference in settings
- [ ] Apply theme class to root element

### Phase 9C: Terminology Rename
- [x] Rename AppMode: 'personal' to 'simple', 'pro' to 'advanced'
- [ ] Rename LibraryView to ProjectView (deferred -- cosmetic rename done last per implementation guide)
- [ ] Rename LibraryDashboard to ProjectsDashboard (deferred)
- [ ] Rename LibraryCard to ProjectCard (deferred)
- [ ] Rename LibrarySection to ProjectSection (deferred)
- [ ] Update all UI labels: "Library" to "Project" (deferred)
- [ ] Update "Open Library" to "Open Project" (Advanced) (deferred)
- [ ] Update welcome text and help strings (deferred)

NOTE: Per implementation guide, data/type renames (AppMode enum, RecentLibrary->RecentProject) are done in Step 1. Cosmetic file renames and UI label changes are deferred to the very end to minimize merge conflicts.

### Phase 9D: Feature Toggles (Settings)
- [ ] Add featureFlags to AppSettings type
- [ ] Add toggle: screenGrabs (default: true in Advanced, hidden in Simple)
- [ ] Add toggle: faceDetection (default: true in Advanced, hidden in Simple)
- [ ] Add toggle: bestClips (default: true in Advanced, hidden in Simple)
- [ ] Add toggle: camerasTab (default: false in Simple, true in Advanced)
- [ ] Conditionally render features based on flags
- [ ] Settings UI: show toggles in Advanced mode only

### Phase 9E: Camera System Foundation
- [ ] Create cameras.ts types (CameraProfile, CameraMatch, etc.)
- [ ] Create cameras.ts API (Tauri commands)
- [ ] Bundle canonical.json camera database (7,500+ cameras)
- [ ] Implement camera matching logic (EXIF Make + Model)
- [ ] Generic camera profile fallback (silent, no prompts)
- [ ] Store camera matches in database

### Phase 9F: Cameras Tab (Advanced Mode)
- [ ] Create CamerasTab component
- [ ] Group clips by detected camera
- [ ] Show camera info: make, model, clip count
- [ ] Unknown cameras show as "Unknown Camera" with generic icon
- [ ] Hide tab in Simple mode (per feature flag)

### Phase 9G: Camera Registration (Dev Menu)
- [ ] USB device detection (macOS, Windows, Linux)
- [ ] USB fingerprint extraction (VID, PID, Serial)
- [ ] EXIF dump from sample files
- [ ] Camera profile form UI
- [ ] Save to custom_cameras.json
- [ ] Battery/charger dropdowns (build-as-you-go)
- [ ] Memory card fallback flow

### Phase 9H: Export Dialog Updates
- [ ] Add title text input field to export dialog
- [ ] Title is optional (blank = no title)
- [ ] Title starts at 5 seconds (from dev menu default)
- [ ] Implement J & L audio blending (verify crossfade implementation)
- [ ] Title text overlay using FFmpeg drawtext filter

### Phase 9I: Dev Menu
- [ ] Create DevMenu component
- [ ] Access: Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win/Linux)
- [ ] Alternative access: Settings > About > click version 7 times
- [ ] Formulas section:
  - [ ] Title start time (devMenu.titleStartSeconds, default: 5 seconds)
  - [ ] J & L blend duration (devMenu.jlBlendMs, default: 500ms)
  - [ ] Score weights (scene/audio/sharpness/motion)
  - [ ] Watermark text override (devMenu.watermarkText)
- [ ] Camera Database section:
  - [ ] View all cameras (bundled + custom)
  - [ ] Register new camera
  - [ ] Import/Export JSON
- [ ] License Tools section:
  - [ ] View current license state
  - [ ] Generate rental keys (batch)
  - [ ] Clear license (test trial)
  - [ ] Set dev mode
- [ ] Debug section:
  - [ ] FFmpeg test (test_ffmpeg command)
  - [ ] Clear caches (clear_caches command)
  - [ ] Export database (export_database command)
  - [ ] Export EXIF dump (export_exif_dump command)
  - [ ] Raw SQL query (execute_raw_sql, dev key only)
  - [ ] Database stats (get_db_stats command)

### Phase 9J: Licensing System
- [ ] Trial start date storage (keychain, survives settings deletion)
- [ ] Days remaining calculation (14-day trial)
- [ ] Trial UI: TrialBanner with countdown and "Enter License Key" CTA
- [ ] Soft lock after 14 days:
  - [ ] CAN view/browse/play clips
  - [ ] CAN export originals (file copy -- non-hostage rule, contract 12)
  - [ ] CAN export rendered outputs WITH watermark + 720p max cap
  - [ ] CANNOT import new footage
  - [ ] CANNOT run new auto-edit jobs (scoring, face detection, refresh best clips)
  - [ ] CANNOT register cameras (reading EXIF is OK; saving to custom DB is not)
- [ ] License key prefixes: DCAM-P- (purchased), DCAM-R- (rental), DCAM-D- (dev)
- [ ] Key validation (BLAKE3 keyed hash, local only, no phone home -- contract 13)
- [ ] Key storage in system keychain (keyring crate)
- [ ] LicenseKeyModal for key entry (accessible from trial banner + dev menu)

### Phase 9K: Import UI (GUI)
- [ ] Create ImportFootage component/dialog
- [ ] Source selection: folder picker
- [ ] Event assignment: select existing or create new
- [ ] Progress display during import
- [ ] Camera auto-detection results display (Advanced mode)
- [ ] Import complete summary

### Phase 9L: Events Enhancement
- [ ] Event selection in GUI (existing)
- [ ] Event selection on import (new)
- [ ] Multi-camera organization within events
- [ ] No multi-view camera support (noted as future)

---

## Detailed Specifications

### User Experience Modes

#### Simple Mode (Default for consumers)
- Single project only ("Default Project")
- Cameras tab hidden
- Generic camera profile (silent fallback)
- Light mode only (no theme toggle)
- Feature toggles hidden (all features enabled but not configurable)
- First run: prompt for folder location once, never ask again
- No Projects dashboard - goes straight to project

#### Advanced Mode (For professionals)
- Multiple projects supported
- Cameras tab visible
- Register specific cameras
- Enable/disable features:
  - Screen grabs
  - Face detection
  - Best clips scoring
  - Cameras tab
- Theme toggle available (light/dark)
- Projects dashboard on launch
- Full dev menu access

### Theme System

#### Light Mode (Default)
```css
:root {
  --color-canvas: #FAFAF8;
  --color-surface: #FFFFFF;
  --color-surface-elevated: #FFFFFF;
  --color-text: rgba(10, 10, 11, 0.87);
  --color-text-secondary: rgba(10, 10, 11, 0.60);
  --color-text-muted: rgba(10, 10, 11, 0.38);
  --color-border: #E5E5E5;
  --color-border-emphasis: #D4D4D4;
  /* Accent and functional colors remain same */
}
```

#### Dark Mode (Advanced only)
```css
:root.dark-mode {
  --color-canvas: #0a0a0b;
  --color-surface: #111113;
  --color-surface-elevated: #1A1A1C;
  --color-text: rgba(250, 250, 248, 0.87);
  --color-text-secondary: rgba(250, 250, 248, 0.60);
  --color-text-muted: rgba(250, 250, 248, 0.38);
  --color-border: #1f1f23;
  --color-border-emphasis: #3A3A3E;
}
```

### Settings Type Updates

```typescript
export type AppMode = 'simple' | 'advanced';

export interface FeatureFlags {
  screenGrabs: boolean;
  faceDetection: boolean;
  bestClips: boolean;
  camerasTab: boolean;
}

export interface LicenseStateCache {
  licenseType: 'trial' | 'purchased' | 'rental' | 'dev';
  isActive: boolean;
  daysRemaining: number | null;
}

export interface AppSettings {
  version: number;
  mode: AppMode;
  firstRunCompleted: boolean;
  theme: 'light' | 'dark';
  defaultProjectPath: string | null;
  recentProjects: RecentProject[];
  featureFlags: FeatureFlags;
  devMenu: DevMenuSettings;
  licenseStateCache: LicenseStateCache | null;
}

export interface DevMenuSettings {
  titleStartSeconds: number;      // seconds, default 5
  jlBlendMs: number;              // ms, default 500
  scoreWeights: ScoreWeights;
  watermarkText: string | null;
}

export const DEFAULT_SETTINGS: AppSettings = {
  version: 2,
  mode: 'simple',
  theme: 'light',
  firstRunCompleted: false,
  defaultProjectPath: null,
  recentProjects: [],
  featureFlags: {
    screenGrabs: true,
    faceDetection: false,  // off in Simple mode
    bestClips: true,
    camerasTab: false,     // hidden in Simple mode
  },
  devMenu: {
    titleStartSeconds: 5,
    jlBlendMs: 500,
    scoreWeights: {
      scene: 0.25,
      audio: 0.25,
      sharpness: 0.25,
      motion: 0.25,
    },
    watermarkText: null,
  },
  licenseStateCache: null,
};
```

### VHS Edit Specifications

**J & L Audio Blending:**
- J-cut: Audio from next clip starts before video transition
- L-cut: Audio from current clip continues after video transition
- Default blend duration: 500ms (adjustable in dev menu)

**FFmpeg Implementation:**
```bash
# J-cut: Audio leads video by blend_duration/2
# L-cut: Audio trails video by blend_duration/2
# Combined with video crossfade (xfade) and audio crossfade (acrossfade)
```

**Opening Title:**
- Input: plain text field in export dialog
- Timing: starts at 5 seconds (adjustable)
- Duration: 3 seconds (fade in 0.5s, hold 2s, fade out 0.5s)
- Style: centered, Braun typography, semi-transparent background
- FFmpeg: drawtext filter

### Camera System

**Detection Priority:**
1. Custom cameras (USB fingerprint) - 100% confidence
2. Custom cameras (serial number) - 95% confidence
3. Custom cameras (make + model) - 80% confidence
4. Bundled database (make + model) - 80% confidence
5. Bundled database (filename pattern) - 70% confidence
6. Unknown - generic profile - 0% confidence (silent)

**Generic Camera Profile:**
```json
{
  "id": "generic",
  "name": "Unknown Camera",
  "make": null,
  "model": null,
  "deinterlace": "auto",
  "lut": null,
  "notes": "Generic fallback profile"
}
```

**Import Behavior:**
- Scan files for EXIF camera data
- Match against database
- Unknown cameras use generic profile silently (no prompt)
- Store match results in clips table

### Development Scope

**Supported (Phase 9):**
- USB cameras
- Memory cards (SD, CF, etc.)

**Not Supported (Future):**
- FireWire cameras
- VHS/MiniDV tape capture
- Multi-view camera sync

---

## File Structure Changes

NOTE: File renames (LibraryView->ProjectView, etc.) are deferred to end per implementation guide.
Current file names reflect what was built.

```
src/
├── components/
│   ├── FirstRunWizard.tsx          # NEW
│   ├── LibraryView.tsx             # EXISTING (rename to ProjectView deferred)
│   ├── LibraryDashboard.tsx        # EXISTING (rename to ProjectsDashboard deferred)
│   ├── LibraryCard.tsx             # EXISTING (rename to ProjectCard deferred)
│   ├── CamerasView.tsx             # NEW
│   ├── DevMenu.tsx                 # NEW
│   ├── ExportDialog.tsx            # NEW (full VHS export dialog)
│   ├── ExportHistory.tsx           # NEW
│   ├── ImportDialog.tsx            # NEW
│   ├── TrialBanner.tsx             # NEW
│   ├── dev/
│   │   ├── FormulasEditor.tsx      # NEW
│   │   ├── CameraDbManager.tsx     # NEW
│   │   ├── LicenseTools.tsx        # NEW
│   │   └── DebugTools.tsx          # NEW
│   ├── modals/
│   │   └── LicenseKeyModal.tsx     # NEW
│   └── nav/
│       ├── LibrarySection.tsx      # UPDATED (rename to ProjectSection deferred)
│       └── ...
├── types/
│   ├── settings.ts                 # UPDATED (v2)
│   ├── cameras.ts                  # NEW
│   ├── licensing.ts                # NEW
│   ├── export.ts                   # NEW
│   ├── jobs.ts                     # NEW
│   └── ...
└── api/
    ├── cameras.ts                  # NEW
    ├── licensing.ts                # NEW
    ├── export.ts                   # NEW
    ├── jobs.ts                     # NEW
    ├── settings.ts                 # UPDATED
    └── ...

src-tauri/src/
├── licensing/
│   └── mod.rs                      # NEW
├── export/
│   ├── mod.rs                      # NEW
│   ├── ffmpeg_builder.rs           # NEW
│   ├── timeline.rs                 # NEW
│   └── watermark.rs                # NEW
├── camera/
│   ├── mod.rs                      # EXISTING
│   ├── devices.rs                  # NEW
│   ├── matcher.rs                  # NEW
│   └── bundled.rs                  # NEW
├── ingest/
│   ├── mod.rs                      # UPDATED
│   └── sidecar.rs                  # NEW
├── jobs/
│   ├── mod.rs                      # UPDATED
│   ├── runner.rs                   # UPDATED
│   └── progress.rs                 # NEW
├── commands/
│   ├── licensing.rs                # NEW
│   ├── export.rs                   # NEW
│   ├── cameras.rs                  # NEW
│   └── devmenu.rs                  # NEW
└── ...

resources/
└── cameras/
    └── canonical.json              # NEW (bundled camera DB)
```

---

## Migration Notes

### Settings Migration (v1 to v2)

Migration is implemented in Rust (`src-tauri/src/commands/settings.rs` lines 164-206).
Settings are in the Tauri Store (JSON file), NOT SQLite. This is NOT a database migration.

Logic:
- `personal` -> `Simple`, `pro` -> `Advanced`
- `lastLibraryPath` -> `defaultProjectPath`
- `recentLibraries` -> `recentProjects`
- `firstRunCompleted` = true if v1 settings existed (existing user skips wizard)
- `theme` = "light" (new default)
- `featureFlags` = mode-appropriate defaults
- `devMenu` = defaults (titleStartSeconds: 5, jlBlendMs: 500, equal score weights)
- `licenseStateCache` = null
- Old v1 keys (`lastLibraryPath`, `recentLibraries`) cleaned up after migration

---

## Implementation Order

1. **Phase 9B: Theme System** - Switch to light mode default first
2. **Phase 9C: Terminology Rename** - Clean refactor, no new features
3. **Phase 9D: Feature Toggles** - Settings infrastructure
4. **Phase 9A: First-Run Experience** - Depends on mode/settings
5. **Phase 9E: Camera System Foundation** - Database and matching
6. **Phase 9F: Cameras Tab** - UI for camera organization
7. **Phase 9H: Export Dialog Updates** - Title input
8. **Phase 9I: Dev Menu** - Formulas and tools
9. **Phase 9G: Camera Registration** - USB detection
10. **Phase 9J: Licensing System** - Trial and purchase
11. **Phase 9K: Import UI** - GUI import flow
12. **Phase 9L: Events Enhancement** - Multi-camera in events

---

## Open Questions

1. ~~Default Project location~~ - **DECIDED: Prompt for folder once**
2. ~~Opening title input location~~ - **DECIDED: In export dialog**
3. ~~Unknown camera handling~~ - **DECIDED: Generic profile silently**
4. Title font/style - use Braun typography or allow customization?
5. Rental key expiration - should rental keys expire?
6. Multi-machine licenses - one key per machine or transferable?

---

## References

- docs/planning/pro-register-camera.md - Licensing and camera registration details
- about.md - Product definition
- contracts.md - Architectural decisions (non-negotiables)
- techguide.md - Technical implementation details

---

End of Spec
