# Observability Gaps -- Implementation Checklist

Tracks the remaining monitoring/logging/debugging gaps identified in the v0.1.154 audit.

---

## 1. Replace eprintln! in cli.rs

The CLI binary does not use tauri-plugin-log (it is a standalone binary).
Replace eprintln! with stderr-printing that is consistent with the rest of CLI output.

- [x] cli.rs:1297 -- multi-worker note: change to `println!` (it is informational, not an error)
- [x] cli.rs:1350 -- save score failure: keep as eprintln (stderr is correct for errors in CLI)
- [x] cli.rs:1361 -- score clip failure: keep as eprintln (stderr is correct for errors in CLI)
- [x] scoring/tests.rs -- leave as-is (test skip messages via eprintln is standard Rust practice)
- [x] Verify: `cargo build --bin dadcam` compiles clean

**Decision:** Line 1297 is informational output going to stderr -- change to println. Lines 1350/1361 are genuine error output -- eprintln is correct for CLI. Test file unchanged.

---

## 2. One-Click Support Bundle Export

Replace the current 3-step process (export logs, export DB, test tools separately) with a single command that collects everything into one folder.

### Backend (diagnostics.rs)

- [x] Add `export_support_bundle` command
- [x] Collects: log files, DB stats, tool versions, OS info, app version, library stats (if open)
- [x] Writes a `support-bundle/` folder to user-chosen location
- [x] Includes a `summary.txt` with human-readable system info
- [x] Register command in lib.rs invoke_handler

### Frontend (diagnostics API + Settings UI)

- [x] Add `exportSupportBundle` to api/diagnostics.ts
- [x] Add "Export Support Bundle" button in Settings > About > Diagnostics section
- [x] Button triggers folder picker then calls command
- [x] Show success/error message

### Verification

- [ ] Button appears in Settings > About
- [ ] Clicking exports folder with: log files, summary.txt
- [ ] summary.txt contains: app version, OS, tools status, DB stats (if library open)

---

## 3. Toast Notifications for Background Job Failures

When background jobs fail, users have no way to know unless they check the dev menu logs.

### Frontend

- [x] Add a `ToastNotification` component (simple bar at top/bottom, auto-dismiss after 8s)
- [x] In App.tsx, listen for `job-progress` events where `isError === true`
- [x] Show toast: "[job type] failed: [error message]"
- [x] Toast has dismiss button
- [x] Multiple toasts stack (max 3 visible)
- [x] CSS for toast in App.css

### Verification

- [ ] Simulated error event shows toast
- [ ] Toast auto-dismisses after 8s
- [ ] Toast has manual dismiss
- [ ] Does not interfere with normal UI flow

---

## 4. System Health Panel in Dev Menu

Add a "System Health" section to DebugTools showing live system status.

### Backend (diagnostics.rs)

- [x] Add `get_system_health` command
- [x] Returns: pending jobs by type, failed jobs count (last 24h), library disk usage (originals + derived), last error message

### Frontend (DebugTools.tsx)

- [x] Add "System Health" section at top of debug tools
- [x] Show pending jobs breakdown (thumb: X, proxy: Y, etc.)
- [x] Show failed jobs count (last 24h)
- [x] Show disk usage: originals size, derived size, total
- [x] Show last error from jobs table
- [x] Auto-refresh button (manual, not polling)

### Verification

- [ ] Health section appears in dev menu
- [ ] Shows accurate pending/failed counts
- [ ] Shows disk usage
- [ ] Refresh button works

---

## 5. Runtime Log Level Toggle

Currently log level is compile-time (Debug for dev, Info for release). Add runtime override.

### Backend (diagnostics.rs)

- [x] Add `set_log_level` command (accepts: "debug", "info", "warn", "error")
- [x] Add `get_log_level` command
- [x] Store preference in App DB app_settings
- [x] Apply via `log::set_max_level()` at runtime
- [x] On startup, read saved level from App DB and apply (after log plugin init)

### Frontend

- [x] Add `setLogLevel` / `getLogLevel` to api/diagnostics.ts
- [x] Add log level selector in Dev Menu (Debug Tools section)
- [x] Dropdown: Debug / Info / Warn / Error
- [x] Shows current level
- [x] Change takes effect immediately

### Verification

- [ ] Changing to Debug shows more log output in dev menu live logs
- [ ] Changing to Warn suppresses info-level messages
- [ ] Level persists across app restart
- [ ] Default is Info for release builds

---

## Audit Checklist (run after all items complete)

- [x] `cargo build` -- no warnings from changed files
- [x] `cargo build --bin dadcam` -- CLI builds clean (0 warnings)
- [x] `npm run build` -- frontend builds clean
- [ ] Support bundle export produces complete output (requires manual test)
- [ ] Toast appears on simulated job failure (requires manual test)
- [ ] System health panel shows accurate data (requires manual test)
- [ ] Log level toggle changes output in real time (requires manual test)
- [x] No new files in project root
- [x] changelog.md updated with version bump
- [ ] techguide.md updated if needed (new commands documented)
