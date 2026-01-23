# Dad Cam

A cross-platform desktop app for importing, organizing, viewing, and auto-editing footage from old-school digital cameras.

## What It Does

- Drop in years of footage
- Browse and watch like a personal archive
- Automatically find the best moments
- Generate nostalgic long-form "VHS-style" edits

Not a full video editor. A video viewer + clipper + auto-editor.

## Core Loop

```
Ingest > Index > Preview > Pick Best > Auto-Edit > Export
```

## Tech Stack

- Framework: Tauri 2.0
- Frontend: React + TypeScript
- Backend: Rust
- Database: SQLite
- Video: FFmpeg (bundled)

## Status

**Version: 0.1.14** - Documentation and planning phase

All implementation guides complete for Phases 0-8. Ready to code.

## Documentation

| File | Purpose |
|------|---------|
| `about.md` | Product definition |
| `contracts.md` | Non-negotiable architectural decisions |
| `techguide.md` | Technical manual and CLI reference |
| `changelog.md` | Version history |
| `docs/planning/` | Phase implementation guides |

## Key Principles

- Originals never deleted or modified
- Works 100% offline, no account required
- No telemetry, no cloud dependency
- Cross-platform (macOS, Windows, Linux)
- Crash-safe, resumable operations

## License

Proprietary. All rights reserved. See LICENSE file.
