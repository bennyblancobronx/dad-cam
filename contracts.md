Dad Cam App — Contracts (Non-Negotiables)

These decisions are final. Do not revisit unless explicitly reopening.

Version: 1.0

---

1. Library Root Model

- One library = one folder
- All Dad Cam data lives in `.dadcam/` at library root
- Structure:
  - `.dadcam/dadcam.db` (SQLite database)
  - `.dadcam/proxies/` (generated preview videos)
  - `.dadcam/thumbs/` (poster frames)
  - `.dadcam/sprites/` (hover scrub sheets)
  - `.dadcam/exports/` (rendered outputs)
- Originals stored in `originals/` at library root (when copied)

---

2. Ingest Modes

- DEFAULT: Copy into library (`originals/` folder)
- ADVANCED: Reference in place (pro mode, files stay where they are)
- Mode is set per-library, not per-clip
- Same defaults for all users (no consumer vs pro split)

---

3. Supported Formats

- Anything ffmpeg supports
- No whitelist, no restrictions
- If ffprobe can read it, we accept it

---

4. Outlier Media Types

- Audio-only files: ACCEPTED (flagged as outlier)
- Image files: ACCEPTED (flagged as outlier)
- These are edge cases but valid for dad cam archives

---

5. Hashing Strategy

- Algorithm: BLAKE3 (chunked)
- During ingest: Fast chunked hash (first 1MB + last 1MB + file size)
- Background job: Full file BLAKE3 (optional, queued)
- Dedup rule: Same hash = same clip (user can override)

---

6. Verification

- After copy: Hash verify (default ON)
- Stored in DB: `verified_at` timestamp or NULL
- Failure: Log error, mark asset as unverified, do not delete

---

7. Sidecar Policy

- Preserve ALL unknown files in source folder structure
- Never flatten AVCHD/BDMV/etc. structures
- Sidecars travel with their parent video
- Unknown file types kept alongside originals

---

8. Timestamp Precedence

Order of trust for clip date/time:
1. Embedded metadata (ffprobe/exiftool)
2. Folder name parsing (e.g., `2019-07-04/`)
3. Filesystem modified date (last resort)

Store which source was used in DB.

---

9. Event Grouping

- Primary: Folder-based (each source folder = one event)
- Secondary: Time-gap grouping (clips >4 hours apart = new event)
- User can override event assignment
- Events are for organization, not a hard constraint

---

10. Pipeline Versioning

- Global `PIPELINE_VERSION` integer (starts at 1)
- Bumping version invalidates:
  - Proxies
  - Thumbnails
  - Sprites
  - Scoring caches
- Derived assets regenerate on next access or background job
- Original assets are NEVER invalidated

---

11. Camera Profiles

- Format: JSON
- Stored in DB with version number
- Matching uses: metadata hints, codec/container hints, folder structure hints
- Match results stored with confidence score and reasons

---

12. Originals Preservation (NON-NEGOTIABLE)

- Original files are NEVER deleted by the app
- NEVER modified
- NEVER moved without explicit user action
- This is absolute and cannot be overridden

---

13. No Cloud Dependency (NON-NEGOTIABLE)

- App works 100% offline
- No account required
- No telemetry
- No external API calls for core functionality
- User data never leaves the machine

---

14. No NLE Lock-in (NON-NEGOTIABLE)

- Exports use standard formats (H.264, ProRes)
- No proprietary project files required
- SQLite DB is inspectable
- User can always access their originals directly

---

15. Cross-Platform (NON-NEGOTIABLE)

- macOS, Windows, Linux
- Same features on all platforms
- Same library format (portable between OSes)

---

16. Crash Safety (NON-NEGOTIABLE)

- Ingest is resumable after crash/disconnect
- Per-file state stored in DB
- USB disconnect mid-ingest does not corrupt library
- Jobs have durable state with retry logic

---

17. Database

- SQLite (single file)
- Location: `.dadcam/dadcam.db`
- Migrations: Numbered, forward-only
- No ORM abstractions in core logic
- DB is the source of truth

---

18. External Tools

- ffmpeg: Video processing, proxy generation, export
- ffprobe: Metadata extraction, format detection
- exiftool: Camera metadata, dates, make/model
- All tools bundled with app (not system-installed)

---

End of Contracts
