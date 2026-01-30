# importplan.md
**Dad Cam App — Gold-Standard Import + Verification Plan (Auditable Checklist)**  
**Goal:** reliably ingest media from removable devices (SD cards, cameras, phones, USB storage) into the library with **bit-level integrity + completeness proof**, so you can confidently mark a session **SAFE TO WIPE** (optional) without losing memories.

---

## 0) Definitions (what “gold standard” means)
- **Integrity (bit-level):** the bytes in the library copy are *identical* to the bytes on the source device at the time of ingest. Proof uses **full-file cryptographic hashes** (fixity).  
  - Why: comparing a newly computed checksum/hash to a known value is standard fixity auditing to detect any bit changes. [R3][R4][R7]
- **Completeness:** you imported *all eligible files that existed on the device at the start of the session*, and you detected if the device changed during ingest.  
  - Why: “every file we copied is intact” is not the same as “we copied everything that was there.” A manifest + rescan closes that gap. [R4]
- **Crash-safety:** ingest never produces a “final” library file unless it’s fully verified; partial copies only exist as temp files.  
  - Why: atomic replace patterns use temp-write + fsync + rename + fsync(dir) to avoid half-written finals. [R1][R2]

---

## 1) Non-negotiable promises (invariants)
These are the “auditable guarantees” your code must uphold:

1. **Never mark a file verified unless source-hash == dest-hash.**
2. **Never show SAFE TO WIPE unless every manifest entry is verified AND the rescan matches.**
3. **Never write directly to the final destination path.** (temp → verify → atomic rename)
4. **Dedup never skips verification.** (fast-hash can only propose candidates; full-hash must confirm)
5. **Every decision has evidence** stored in DB + exportable logs.

---

## 2) Threat model (what can go wrong)
You’re defending against:
- Silent corruption during read or write (bad SD sector, flaky USB reader, cable issues)
- Partial writes / power loss / app crash mid-copy
- User ejects the card early
- Files added/modified while ingest runs (camera still writing)
- Dedup false matches from “fast hash” or identical size/mtime
- Filesystem metadata not persisted (directory entry not durable unless fsynced) [R1]

---

## 3) Required data model (minimal additions)
> If you already have an “ingest job” table, these can be merged into it. The key is **session-level roll-up** + **manifest**.

### 3.1 IngestSession
- `id`
- `source_root` (mount/path)
- `device_fingerprint` (best-effort: volume UUID/serial + label + capacity)
- `started_at`, `finished_at`
- `status`: `discovering | ingesting | verifying | complete | failed`
- `safe_to_wipe_at` (nullable)
- `manifest_hash` (hash of manifest file, for audit immutability)
- `rescan_hash` (hash of rescan snapshot)

### 3.2 IngestManifestEntry (one row per discovered eligible file)
- `session_id`
- `relative_path`
- `size_bytes`
- `mtime`
- `hash_fast` (optional; for quick UX + candidate lookup)
- `hash_source_full` (**required for SAFE TO WIPE**)
- `asset_id` (nullable until mapped)
- `result`: `copied_verified | dedup_verified | skipped_ineligible | failed | changed`
- `error_code`, `error_detail`

### 3.3 Asset verification fields (if not present)
- `hash_full` (dest full hash)
- `verified_at`
- `verified_method`: `copy_readback | dedup_match`
- `verification_error` (nullable)

---

## 4) Canonical pipeline (gold-standard)
### 4.1 Session: Discover → Manifest
**Checklist**
- [ ] Mount/source selected; record `device_fingerprint`.
- [ ] Discover eligible files (walk). For each file: write a manifest entry (`relative_path`, `size`, `mtime`).  
- [ ] Freeze “session baseline” by hashing the serialized manifest into `manifest_hash`.

**Why**
- Completeness requires knowing what “all files” means at a point in time. [R4]

**Evidence**
- `IngestSession.manifest_hash`
- Exportable `manifest.jsonl` (see §9)

### 4.2 File: Copy with read-back verification (streaming)
**Do this for every manifest entry not excluded as ineligible.**

**Algorithm (must be streaming, not read_to_end)**
1. Open source file; re-stat; compare to manifest `(size, mtime)`.
   - If changed → mark `result=changed`, block SAFE TO WIPE.
2. Create dest temp file on the SAME filesystem as final library path: `dest.final.<uuid>.tmp`
3. Stream loop:
   - read chunk from source
   - update **BLAKE3** hasher (source)
   - write chunk to temp dest
4. `fsync(temp dest file)`
   - Why: fsync flushes modified file data + metadata so it’s retrievable after crash. [R1]
5. Close temp dest.
6. Read temp dest back from disk (stream) and compute dest full hash.
7. Compare `hash_source_full == hash_dest_full`
   - If mismatch: delete temp dest, mark failed.
   - If match: **atomic rename** temp → final path, then `fsync(parent dir)`.
     - Why: rename is atomic replacement; fsync(dir) is required because fsync(file) doesn’t ensure the directory entry reached disk. [R1][R2]
8. Write/update DB:
   - Asset `hash_full = hash_dest_full`
   - Asset `verified_at = now`, `verified_method = copy_readback`
   - ManifestEntry `hash_source_full`, `asset_id`, `result=copied_verified`

**Why (hashing + compare)**
- Fixity (hash compare) is a standard way to detect any bit-level change; identical files produce identical checksums. [R3][R4][R7]
- BLAKE3 is designed to be fast + parallelizable and supports incremental/streaming hashing. [R5][R6]

**Evidence**
- Per file: stored `hash_source_full`, `hash_full`, `verified_at`, and an audit record showing compare result.

### 4.3 File: Dedup that is still safe-to-wipe
If fast hash finds candidates:
- [ ] Compute **full source hash anyway** (stream source once)
- [ ] Compare to candidate asset’s stored `hash_full`
- [ ] If match: link manifest entry to existing asset, set `result=dedup_verified`, `verified_method=dedup_match`
- [ ] If no match: treat as unique → do full copy pipeline above

**Why**
- Fast hashes/fingerprints are useful for speed, but only full-file hashes can prove bit-level identity for wipe decisions. [R3][R4]

**Evidence**
- ManifestEntry shows `dedup_verified` with `asset_id` and `hash_source_full`

### 4.4 Session: Rescan gate for SAFE TO WIPE
After ingest attempts complete:
- [ ] Rescan source device for eligible files and build a rescan snapshot (same schema as manifest).
- [ ] Compare rescan snapshot to original manifest:
  - No missing entries (from manifest)
  - No new eligible files added
  - No changed metadata that indicates modifications
- [ ] Only if:
  - all manifest entries are `copied_verified` or `dedup_verified`
  - AND rescan matches
  → set `safe_to_wipe_at = now` and show SAFE TO WIPE.

**Why**
- Completeness requires proving the device didn’t change during ingest and you accounted for every baseline file. [R4]

**Evidence**
- `IngestSession.rescan_hash`, and a diff report (0 differences required for SAFE)

---

## 5) Deletion/Wipe workflow (only after SAFE TO WIPE)
Default policy can still be “never delete,” but if the user requests it:

**Checklist**
- [ ] Require SAFE TO WIPE state (hard gate).
- [ ] Present a human-readable wipe report: counts + any excluded items.
- [ ] Perform delete in a deterministic order from manifest entries (relative paths).
- [ ] After delete, optional final rescan to confirm emptiness (or confirm those files missing).
- [ ] Log every delete outcome.

**Why**
- Wiping without prior integrity + completeness guarantees defeats the whole system purpose.

**Evidence**
- `wipe_report.json` including deleted paths + success/failure per item.

---

## 6) Error handling requirements (no silent failure)
**Must implement**
- If read-back compare fails: mark file failed, keep source untouched, do not mark verified.
- If user ejects device mid-session: session ends as not safe; list remaining unverified.
- If I/O errors occur: capture errno + path in error_detail; do not retry infinitely.
- If out-of-space: fail session; do not delete any source files.

**Evidence**
- Structured error codes in manifest entries and session summary.

---

## 7) UX requirements (reduce user-caused risk)
- Show a persistent banner: **“Keep device connected until verification completes.”**
- Per-session status: `Discovering → Copying → Read-back verifying → Rescanning → SAFE TO WIPE`
- A single button: **Export audit report** (manifest + verification results + rescan diff)

---

## 8) Performance notes (still gold-standard)
- Streaming avoids 1× file-size RAM spikes.
- BLAKE3 is fast by design and supports incremental hashing (so you can hash while copying). [R5][R6]
- Read-back adds 1 extra sequential read on destination — accepted cost for gold standard.

---

## 9) Required audit artifacts (exportable)
Export per session (single folder/zip):
- `session.json` (device_fingerprint, start/end, safe_to_wipe_at)
- `manifest.jsonl` (baseline)
- `results.jsonl` (per-file: hashes, method, timestamps, errors)
- `rescan.jsonl`
- `rescan_diff.json` (must be empty for SAFE)
- `wipe_report.json` (if wipe performed)

Also store `manifest_hash` and `rescan_hash` (BLAKE3 of the normalized JSONL) for tamper evidence.

---

## 10) Test plan (must pass before shipping)
**Integrity**
- [ ] Corrupt dest after copy (flip a byte) → read-back compare must fail; no final file accepted.
**Crash safety**
- [ ] Simulate crash mid-copy → only temp file exists; on restart session can resume or clean temp safely.
**Dedup correctness**
- [ ] Two different files that share first/last MB + size → fast-hash collision must not skip; full hash resolves.
**Completeness**
- [ ] Add a new file to device after manifest → rescan diff must block SAFE TO WIPE.

---

## 11) “Done” definition (acceptance criteria)
A session is SAFE TO WIPE iff:
- `forall manifest_entry: result in {copied_verified, dedup_verified}`
- `rescan_diff == empty`
- all verified assets have `hash_full` and `verified_at` set
- audit artifacts export succeeds

---

# References
(Used to justify *why* each step exists. URLs are in a code block for easy copy/paste.)

```text
[R1] Linux man-pages: fsync(2) — notes that fsync flushes data/metadata and that directory entries may require fsync on the directory too.
https://man7.org/linux/man-pages/man2/fsync.2.html

[R2] Linux man-pages: rename(2) — states that if newpath exists it is atomically replaced (atomic rename semantics on local filesystems).
https://man7.org/linux/man-pages/man2/rename.2.html

[R3] Library of Congress (The Signal): “File Fixity and Data Integrity” — checksum comparison as fixity checking/auditing; bit change changes checksum.
https://blogs.loc.gov/thesignal/2014/04/protect-your-data-file-fixity-and-data-integrity/

[R4] NDSA Fixity Guidance Report (PDF) — fixity as bit-level integrity; checksums/hashes provide evidence one set of bits is identical to another.
https://www.digitalpreservation.gov/documents/NDSA-Fixity-Guidance-Report-final100214.pdf

[R5] IETF Internet-Draft: “The BLAKE3 Hashing Framework” — BLAKE3 specified as secure, fast, parallelizable.
https://www.ietf.org/archive/id/draft-aumasson-blake3-00.html

[R6] BLAKE3 project documentation — notes speed, security, and incremental/streaming capability due to Merkle-tree structure.
https://github.com/BLAKE3-team/BLAKE3

[R7] DPC Digital Preservation Handbook: Fixity and checksums — compute checksums for copies and compare to known-good reference.
https://www.dpconline.org/handbook/technical-solutions-and-tools/fixity-and-checksums
```
