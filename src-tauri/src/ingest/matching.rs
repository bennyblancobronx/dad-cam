// Camera matching and stable ref resolution for ingest pipeline

use rusqlite::Connection;
use crate::metadata::MediaMetadata;
use crate::constants::{MATCHER_VERSION, MATCH_SCORE_THRESHOLD, MATCH_MAX_SCORE};

/// Candidate result from matching (used for audit trail).
#[derive(Debug, Clone)]
pub(crate) struct MatchCandidateResult {
    pub slug: String,
    pub profile_type: String, // "bundled" or "user"
    pub score: f64,
    pub rejected: bool,
    pub reject_reason: Option<String>,
    pub matched_rules: Vec<String>,
    pub failed_rules: Vec<String>,
}

/// Full matching result with audit data.
pub(crate) struct MatchingResult {
    pub profile_type: String,
    pub profile_ref: String,
    pub device_uuid: Option<String>,
    pub confidence: f64,
    pub match_source: String,
    pub candidates: Vec<MatchCandidateResult>,
}

/// Convert raw specificity score to confidence (G3).
pub(crate) fn score_to_confidence(score: f64) -> f64 {
    (score / MATCH_MAX_SCORE).min(0.95)
}

/// Resolve camera match to stable refs using App DB priority order (spec section 7.2):
/// 1. Registered device match (USB fingerprint -> device UUID -> assigned profile if set)
/// 2. User profiles rules engine (match_rules from App DB user_profiles)
/// 3. Bundled profiles rules engine (match_rules from App DB bundled_profiles)
/// 4. Generic fallback (generic-fallback)
///
/// Also resolves legacy library-local profile_id by name for backward compat.
/// Returns (profile_type, profile_ref, device_uuid).
pub(crate) fn resolve_stable_camera_refs(
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
    legacy_device_id: Option<i64>,
    usb_fingerprints: Option<&[String]>,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> (Option<String>, Option<String>, Option<String>) {
    let app_conn = match crate::db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(_) => return resolve_stable_refs_fallback(lib_conn, legacy_profile_id, legacy_device_id),
    };

    // Priority 1: Registered device by USB fingerprint
    if let Some(fps) = usb_fingerprints {
        for fp in fps {
            if let Ok(Some(device)) = crate::db::app_schema::find_device_by_usb_fingerprint_app(&app_conn, fp) {
                if device.profile_type != "none" && !device.profile_ref.is_empty() {
                    return (
                        Some(device.profile_type.clone()),
                        Some(device.profile_ref.clone()),
                        Some(device.uuid),
                    );
                }
                let device_uuid = Some(device.uuid.clone());
                let (ptype, pref) = resolve_profile_from_app_db(
                    &app_conn, metadata, source_folder, lib_conn, legacy_profile_id,
                );
                return (Some(ptype), Some(pref), device_uuid);
            }
        }
    }

    // Priority 1b: Device by serial number
    if let Some(ref serial) = metadata.serial_number {
        if let Ok(Some(device)) = crate::db::app_schema::find_device_by_serial_app(&app_conn, serial) {
            if device.profile_type != "none" && !device.profile_ref.is_empty() {
                return (
                    Some(device.profile_type.clone()),
                    Some(device.profile_ref.clone()),
                    Some(device.uuid),
                );
            }
            let device_uuid = Some(device.uuid.clone());
            let (ptype, pref) = resolve_profile_from_app_db(
                &app_conn, metadata, source_folder, lib_conn, legacy_profile_id,
            );
            return (Some(ptype), Some(pref), device_uuid);
        }
    }

    // No device match -- resolve profile from App DB, device from legacy
    let (ptype, pref) = resolve_profile_from_app_db(
        &app_conn, metadata, source_folder, lib_conn, legacy_profile_id,
    );

    let device_uuid = legacy_device_id.and_then(|did| {
        lib_conn.query_row(
            "SELECT uuid FROM camera_devices WHERE id = ?1",
            [did],
            |row| row.get::<_, String>(0),
        ).ok()
    });

    (Some(ptype), Some(pref), device_uuid)
}

/// Full matching with audit trail (for sidecar matchAudit section).
pub(crate) fn resolve_stable_camera_refs_with_audit(
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
    legacy_device_id: Option<i64>,
    usb_fingerprints: Option<&[String]>,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> MatchingResult {
    let app_conn = match crate::db::app_db::open_app_db_connection() {
        Ok(c) => c,
        Err(_) => {
            return MatchingResult {
                profile_type: "bundled".to_string(),
                profile_ref: "generic-fallback".to_string(),
                device_uuid: resolve_legacy_device_uuid(lib_conn, legacy_device_id),
                confidence: 0.1,
                match_source: "generic_fallback".to_string(),
                candidates: Vec::new(),
            };
        }
    };

    // Priority 1: Registered device by USB fingerprint
    if let Some(fps) = usb_fingerprints {
        for fp in fps {
            if let Ok(Some(device)) = crate::db::app_schema::find_device_by_usb_fingerprint_app(&app_conn, fp) {
                if device.profile_type != "none" && !device.profile_ref.is_empty() {
                    return MatchingResult {
                        profile_type: device.profile_type.clone(),
                        profile_ref: device.profile_ref.clone(),
                        device_uuid: Some(device.uuid),
                        confidence: 1.0,
                        match_source: "registered_device_usb".to_string(),
                        candidates: Vec::new(),
                    };
                }
            }
        }
    }

    // Priority 1b: Device by serial number
    if let Some(ref serial) = metadata.serial_number {
        if let Ok(Some(device)) = crate::db::app_schema::find_device_by_serial_app(&app_conn, serial) {
            if device.profile_type != "none" && !device.profile_ref.is_empty() {
                return MatchingResult {
                    profile_type: device.profile_type.clone(),
                    profile_ref: device.profile_ref.clone(),
                    device_uuid: Some(device.uuid),
                    confidence: 0.95,
                    match_source: "registered_device_serial".to_string(),
                    candidates: Vec::new(),
                };
            }
        }
    }

    // No device match -- run profile matching with full audit
    let result = resolve_profile_with_audit(&app_conn, metadata, source_folder, lib_conn, legacy_profile_id);

    let device_uuid = legacy_device_id.and_then(|did| {
        lib_conn.query_row(
            "SELECT uuid FROM camera_devices WHERE id = ?1",
            [did],
            |row| row.get::<_, String>(0),
        ).ok()
    });

    MatchingResult {
        profile_type: result.0,
        profile_ref: result.1,
        device_uuid,
        confidence: result.2,
        match_source: result.3,
        candidates: result.4,
    }
}

/// Resolve profile using App DB priority with full audit trail.
/// Returns (profile_type, profile_ref, confidence, match_source, candidates).
fn resolve_profile_with_audit(
    app_conn: &Connection,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
) -> (String, String, f64, String, Vec<MatchCandidateResult>) {
    let mut all_candidates = Vec::new();

    // Priority 2: User profiles match_rules
    if let Ok(user_profiles) = crate::db::app_schema::list_user_profiles(app_conn) {
        let user_candidates = evaluate_user_profiles(&user_profiles, metadata, source_folder);
        all_candidates.extend(user_candidates);
    }

    // Priority 3: Bundled profiles match_rules
    if let Ok(bundled) = crate::db::app_schema::list_bundled_profiles(app_conn) {
        let bundled_candidates = evaluate_bundled_profiles(&bundled, metadata, source_folder);
        all_candidates.extend(bundled_candidates);
    }

    // Find best non-rejected candidate above threshold
    let best = all_candidates.iter()
        .filter(|c| !c.rejected && c.score >= MATCH_SCORE_THRESHOLD)
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(winner) = best {
        let confidence = score_to_confidence(winner.score);
        return (
            winner.profile_type.clone(),
            winner.slug.clone(),
            confidence,
            if winner.profile_type == "user" { "user_profile" } else { "bundled_profile" }.to_string(),
            all_candidates,
        );
    }

    // Fallback: resolve legacy profile_id by name
    if let Ok(bundled) = crate::db::app_schema::list_bundled_profiles(app_conn) {
        if let Some(pid) = legacy_profile_id {
            if let Ok(name) = lib_conn.query_row(
                "SELECT name FROM camera_profiles WHERE id = ?1",
                [pid],
                |row| row.get::<_, String>(0),
            ) {
                if let Some(bp) = bundled.iter().find(|b| {
                    b.name.eq_ignore_ascii_case(&name) || b.slug.eq_ignore_ascii_case(&name)
                }) {
                    return ("bundled".to_string(), bp.slug.clone(), 0.3, "legacy_name".to_string(), all_candidates);
                }
                if let Ok(ups) = crate::db::app_schema::list_user_profiles(app_conn) {
                    if let Some(up) = ups.iter().find(|u| u.name.eq_ignore_ascii_case(&name)) {
                        return ("user".to_string(), up.uuid.clone(), 0.3, "legacy_name".to_string(), all_candidates);
                    }
                }
            }
        }
    }

    // Priority 4: Generic fallback
    ("bundled".to_string(), "generic-fallback".to_string(), 0.1, "generic_fallback".to_string(), all_candidates)
}

/// Evaluate all user profiles, returning candidates with scores and reject info.
fn evaluate_user_profiles(
    profiles: &[crate::db::app_schema::AppUserProfile],
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> Vec<MatchCandidateResult> {
    let mut results = Vec::new();
    for profile in profiles {
        let rules: serde_json::Value = serde_json::from_str(&profile.match_rules).unwrap_or_default();

        // Phase 1: Reject check
        if let Some((rejected, reason)) = check_reject_rules(&rules, metadata) {
            if rejected {
                results.push(MatchCandidateResult {
                    slug: profile.uuid.clone(),
                    profile_type: "user".to_string(),
                    score: 0.0,
                    rejected: true,
                    reject_reason: Some(reason),
                    matched_rules: Vec::new(),
                    failed_rules: Vec::new(),
                });
                continue;
            }
        }

        // Phase 2: Score
        let (score, matched, failed) = score_match_rules_detailed(&rules, metadata, source_folder);
        results.push(MatchCandidateResult {
            slug: profile.uuid.clone(),
            profile_type: "user".to_string(),
            score,
            rejected: false,
            reject_reason: None,
            matched_rules: matched,
            failed_rules: failed,
        });
    }
    results
}

/// Evaluate all bundled profiles, returning candidates with scores and reject info.
fn evaluate_bundled_profiles(
    profiles: &[crate::db::app_schema::AppBundledProfile],
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> Vec<MatchCandidateResult> {
    let mut results = Vec::new();
    for profile in profiles {
        let rules: serde_json::Value = serde_json::from_str(&profile.match_rules).unwrap_or_default();

        // Phase 1: Reject check
        if let Some((rejected, reason)) = check_reject_rules(&rules, metadata) {
            if rejected {
                results.push(MatchCandidateResult {
                    slug: profile.slug.clone(),
                    profile_type: "bundled".to_string(),
                    score: 0.0,
                    rejected: true,
                    reject_reason: Some(reason),
                    matched_rules: Vec::new(),
                    failed_rules: Vec::new(),
                });
                continue;
            }
        }

        // Phase 2: Score
        let (score, matched, failed) = score_match_rules_detailed(&rules, metadata, source_folder);
        results.push(MatchCandidateResult {
            slug: profile.slug.clone(),
            profile_type: "bundled".to_string(),
            score,
            rejected: false,
            reject_reason: None,
            matched_rules: matched,
            failed_rules: failed,
        });
    }
    results
}

/// Phase 1: Check reject rules. Returns Some((true, reason)) if rejected.
fn check_reject_rules(
    rules: &serde_json::Value,
    metadata: &MediaMetadata,
) -> Option<(bool, String)> {
    let obj = rules.as_object()?;

    // reject_codec
    if let Some(reject_codecs) = obj.get("reject_codec").or_else(|| obj.get("rejectCodec")).and_then(|v| v.as_array()) {
        if let Some(ref codec) = metadata.codec {
            for rc in reject_codecs {
                if let Some(s) = rc.as_str() {
                    if codec.eq_ignore_ascii_case(s) {
                        return Some((true, format!("reject_codec: {} matches {}", codec, s)));
                    }
                }
            }
        }
    }

    // reject_container
    if let Some(reject_containers) = obj.get("reject_container").or_else(|| obj.get("rejectContainer")).and_then(|v| v.as_array()) {
        if let Some(ref container) = metadata.container {
            let parts: Vec<&str> = container.split(',').map(|s| s.trim()).collect();
            for rc in reject_containers {
                if let Some(s) = rc.as_str() {
                    if parts.iter().any(|p| p.eq_ignore_ascii_case(s)) {
                        return Some((true, format!("reject_container: {} matches {}", container, s)));
                    }
                }
            }
        }
    }

    // reject_model
    if let Some(reject_models) = obj.get("reject_model").or_else(|| obj.get("rejectModel")).and_then(|v| v.as_array()) {
        if let Some(ref model) = metadata.camera_model {
            for rm in reject_models {
                if let Some(s) = rm.as_str() {
                    if model.to_lowercase().contains(&s.to_lowercase()) {
                        return Some((true, format!("reject_model: {} contains {}", model, s)));
                    }
                }
            }
        }
    }

    None
}

/// Resolve profile using App DB priority: user profiles > bundled profiles > legacy name > fallback.
fn resolve_profile_from_app_db(
    app_conn: &Connection,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
) -> (String, String) {
    // Priority 2: User profiles match_rules
    if let Ok(user_profiles) = crate::db::app_schema::list_user_profiles(app_conn) {
        if let Some(matched) = match_app_profile_rules(&user_profiles, metadata, source_folder) {
            return ("user".to_string(), matched);
        }
    }

    // Priority 3: Bundled profiles match_rules
    if let Ok(bundled) = crate::db::app_schema::list_bundled_profiles(app_conn) {
        if let Some(matched) = match_bundled_profile_rules(&bundled, metadata, source_folder) {
            return ("bundled".to_string(), matched);
        }

        // Fallback: resolve legacy profile_id by name against bundled/user
        if let Some(pid) = legacy_profile_id {
            if let Ok(name) = lib_conn.query_row(
                "SELECT name FROM camera_profiles WHERE id = ?1",
                [pid],
                |row| row.get::<_, String>(0),
            ) {
                if let Some(bp) = bundled.iter().find(|b| {
                    b.name.eq_ignore_ascii_case(&name) || b.slug.eq_ignore_ascii_case(&name)
                }) {
                    return ("bundled".to_string(), bp.slug.clone());
                }
                if let Ok(ups) = crate::db::app_schema::list_user_profiles(app_conn) {
                    if let Some(up) = ups.iter().find(|u| u.name.eq_ignore_ascii_case(&name)) {
                        return ("user".to_string(), up.uuid.clone());
                    }
                }
            }
        }
    }

    // Priority 4: Generic fallback
    ("bundled".to_string(), "generic-fallback".to_string())
}

/// Match metadata against App DB user profiles' match_rules.
/// Returns the UUID of the best matching user profile above threshold.
/// Now includes reject rules (Phase 1) before scoring (Phase 2).
pub(crate) fn match_app_profile_rules(
    profiles: &[crate::db::app_schema::AppUserProfile],
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> Option<String> {
    let mut best: Option<(i32, f64, &str)> = None;

    for profile in profiles {
        let rules: serde_json::Value = serde_json::from_str(&profile.match_rules).unwrap_or_default();

        // Phase 1: Reject check
        if let Some((rejected, _)) = check_reject_rules(&rules, metadata) {
            if rejected { continue; }
        }

        // Phase 2: Score
        let score = score_match_rules(&rules, metadata, source_folder);

        // Phase 3: Threshold
        if score >= MATCH_SCORE_THRESHOLD {
            let is_better = best.map_or(true, |(bv, bs, br)| {
                profile.version > bv
                    || (profile.version == bv && score > bs)
                    || (profile.version == bv && (score - bs).abs() < f64::EPSILON && profile.uuid.as_str() < br)
            });
            if is_better {
                best = Some((profile.version, score, &profile.uuid));
            }
        }
    }

    best.map(|(_, _, uuid)| uuid.to_string())
}

/// Match metadata against App DB bundled profiles' match_rules.
/// Returns the slug of the best matching bundled profile above threshold.
/// Now includes reject rules (Phase 1) before scoring (Phase 2).
pub(crate) fn match_bundled_profile_rules(
    profiles: &[crate::db::app_schema::AppBundledProfile],
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> Option<String> {
    let mut best: Option<(i32, f64, &str)> = None;

    for profile in profiles {
        let rules: serde_json::Value = serde_json::from_str(&profile.match_rules).unwrap_or_default();

        // Phase 1: Reject check
        if let Some((rejected, _)) = check_reject_rules(&rules, metadata) {
            if rejected { continue; }
        }

        // Phase 2: Score
        let score = score_match_rules(&rules, metadata, source_folder);

        // Phase 3: Threshold
        if score >= MATCH_SCORE_THRESHOLD {
            let is_better = best.map_or(true, |(bv, bs, br)| {
                profile.version > bv
                    || (profile.version == bv && score > bs)
                    || (profile.version == bv && (score - bs).abs() < f64::EPSILON && profile.slug.as_str() < br)
            });
            if is_better {
                best = Some((profile.version, score, &profile.slug));
            }
        }
    }

    best.map(|(_, _, slug)| slug.to_string())
}

/// Score how well a match_rules JSON object matches the given metadata.
/// Keys are ANDed; within a key, arrays are ORed; strings are case-insensitive (spec 7.3).
/// Returns 0.0 if any specified key fails to match.
/// Score uses Appendix A specificity weights:
///   +5 make+model, +3 folderPattern, +3 codec+container,
///   +2 resolution constraints, +1 frameRate
pub(crate) fn score_match_rules(
    rules: &serde_json::Value,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> f64 {
    let (score, _, _) = score_match_rules_detailed(rules, metadata, source_folder);
    score
}

/// Detailed scoring that returns (score, matched_rules, failed_rules).
fn score_match_rules_detailed(
    rules: &serde_json::Value,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
) -> (f64, Vec<String>, Vec<String>) {
    let obj = match rules.as_object() {
        Some(o) if !o.is_empty() => o,
        _ => return (0.0, Vec::new(), Vec::new()),
    };

    let mut total_keys = 0usize;
    let mut matched_keys = 0usize;
    let mut specificity = 0.0f64;
    let mut matched_rules = Vec::new();
    let mut failed_rules = Vec::new();

    // make
    let make_matched = if let Some(makes) = obj.get("make").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref cam_make) = metadata.camera_make {
            if makes.iter().any(|m| {
                m.as_str().map_or(false, |s| cam_make.to_lowercase().contains(&s.to_lowercase()))
            }) {
                matched_keys += 1;
                matched_rules.push("make".to_string());
                true
            } else {
                failed_rules.push("make".to_string());
                false
            }
        } else {
            failed_rules.push("make".to_string());
            false
        }
    } else { false };

    // model
    let model_matched = if let Some(models) = obj.get("model").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref cam_model) = metadata.camera_model {
            if models.iter().any(|m| {
                m.as_str().map_or(false, |s| cam_model.to_lowercase().contains(&s.to_lowercase()))
            }) {
                matched_keys += 1;
                matched_rules.push("model".to_string());
                true
            } else {
                failed_rules.push("model".to_string());
                false
            }
        } else {
            failed_rules.push("model".to_string());
            false
        }
    } else { false };

    if make_matched && model_matched {
        specificity += 5.0;
    } else if make_matched || model_matched {
        specificity += 2.0;
    }

    // codec
    let codec_matched = if let Some(codecs) = obj.get("codec").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref codec) = metadata.codec {
            if codecs.iter().any(|c| {
                c.as_str().map_or(false, |s| codec.eq_ignore_ascii_case(s))
            }) {
                matched_keys += 1;
                matched_rules.push("codec".to_string());
                true
            } else {
                failed_rules.push("codec".to_string());
                false
            }
        } else {
            failed_rules.push("codec".to_string());
            false
        }
    } else { false };

    // container
    let container_matched = if let Some(containers) = obj.get("container").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(ref container) = metadata.container {
            let parts: Vec<&str> = container.split(',').map(|s| s.trim()).collect();
            if containers.iter().any(|c| {
                c.as_str().map_or(false, |s| parts.iter().any(|p| p.eq_ignore_ascii_case(s)))
            }) {
                matched_keys += 1;
                matched_rules.push("container".to_string());
                true
            } else {
                failed_rules.push("container".to_string());
                false
            }
        } else {
            failed_rules.push("container".to_string());
            false
        }
    } else { false };

    if codec_matched && container_matched {
        specificity += 3.0;
    } else if codec_matched || container_matched {
        specificity += 1.5;
    }

    // folderPattern
    if let Some(pattern) = obj.get("folderPattern").and_then(|v| v.as_str()) {
        total_keys += 1;
        if let Some(folder) = source_folder {
            if let Ok(re) = regex::RegexBuilder::new(pattern).case_insensitive(true).build() {
                if re.is_match(folder) {
                    matched_keys += 1;
                    matched_rules.push("folderPattern".to_string());
                    specificity += 3.0;
                } else {
                    failed_rules.push("folderPattern".to_string());
                }
            } else {
                failed_rules.push("folderPattern".to_string());
            }
        } else {
            failed_rules.push("folderPattern".to_string());
        }
    }

    // Resolution constraints
    let has_resolution_rule = obj.contains_key("minWidth") || obj.contains_key("maxWidth")
        || obj.contains_key("minHeight") || obj.contains_key("maxHeight");
    if has_resolution_rule {
        total_keys += 1;
        let w = metadata.width.unwrap_or(0);
        let h = metadata.height.unwrap_or(0);
        let mut res_ok = true;
        if let Some(min_w) = obj.get("minWidth").and_then(|v| v.as_i64()) {
            if (w as i64) < min_w { res_ok = false; }
        }
        if let Some(max_w) = obj.get("maxWidth").and_then(|v| v.as_i64()) {
            if (w as i64) > max_w { res_ok = false; }
        }
        if let Some(min_h) = obj.get("minHeight").and_then(|v| v.as_i64()) {
            if (h as i64) < min_h { res_ok = false; }
        }
        if let Some(max_h) = obj.get("maxHeight").and_then(|v| v.as_i64()) {
            if (h as i64) > max_h { res_ok = false; }
        }
        if res_ok {
            matched_keys += 1;
            matched_rules.push("resolution".to_string());
            specificity += 2.0;
        } else {
            failed_rules.push("resolution".to_string());
        }
    }

    // frameRate
    if let Some(rates) = obj.get("frameRate").and_then(|v| v.as_array()) {
        total_keys += 1;
        if let Some(fps) = metadata.fps {
            if rates.iter().any(|r| {
                r.as_f64().map_or(false, |expected| (fps - expected).abs() <= 0.5)
            }) {
                matched_keys += 1;
                matched_rules.push("frameRate".to_string());
                specificity += 1.0;
            } else {
                failed_rules.push("frameRate".to_string());
            }
        } else {
            failed_rules.push("frameRate".to_string());
        }
    }

    if total_keys == 0 {
        return (0.0, matched_rules, failed_rules);
    }

    // All keys must match (AND semantics per spec 7.3)
    if matched_keys == total_keys {
        (specificity, matched_rules, failed_rules)
    } else {
        (0.0, matched_rules, failed_rules)
    }
}

/// Fallback when App DB is unavailable: resolve from legacy library DB refs only.
fn resolve_stable_refs_fallback(
    lib_conn: &Connection,
    legacy_profile_id: Option<i64>,
    legacy_device_id: Option<i64>,
) -> (Option<String>, Option<String>, Option<String>) {
    let device_uuid = resolve_legacy_device_uuid(lib_conn, legacy_device_id);
    let _pid = legacy_profile_id;
    (Some("bundled".to_string()), Some("generic-fallback".to_string()), device_uuid)
}

fn resolve_legacy_device_uuid(lib_conn: &Connection, legacy_device_id: Option<i64>) -> Option<String> {
    legacy_device_id.and_then(|did| {
        lib_conn.query_row(
            "SELECT uuid FROM camera_devices WHERE id = ?1",
            [did],
            |row| row.get::<_, String>(0),
        ).ok()
    })
}

/// Build a MatchAudit sidecar section from matching results.
pub(crate) fn build_match_audit(
    result: &MatchingResult,
    metadata: &MediaMetadata,
    source_folder: Option<&str>,
    ffprobe_ext: &crate::metadata::ffprobe::FFprobeExtendedFields,
    exif_ext: &crate::metadata::exiftool::ExifExtendedMetadata,
) -> super::sidecar::MatchAudit {
    let input_sig = super::sidecar::MatchInputSignature {
        make: metadata.camera_make.clone(),
        model: metadata.camera_model.clone(),
        serial: metadata.serial_number.clone(),
        codec: metadata.codec.clone(),
        container: metadata.container.clone(),
        width: metadata.width,
        height: metadata.height,
        fps: metadata.fps,
        field_order: ffprobe_ext.field_order.clone(),
        compressor_id: exif_ext.compressor_id.clone(),
        folder_path: source_folder.map(|s| s.to_string()),
    };

    let candidates: Vec<super::sidecar::MatchCandidate> = result.candidates.iter().map(|c| {
        super::sidecar::MatchCandidate {
            slug: c.slug.clone(),
            score: c.score,
            rejected: c.rejected,
            reject_reason: c.reject_reason.clone(),
            matched_rules: c.matched_rules.clone(),
            failed_rules: c.failed_rules.clone(),
        }
    }).collect();

    let assignment_reason = if result.match_source == "generic_fallback" {
        "No profile scored above threshold, using generic fallback".to_string()
    } else {
        format!("{} match (confidence {:.2})", result.match_source, result.confidence)
    };

    super::sidecar::MatchAudit {
        matched_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        matcher_version: MATCHER_VERSION,
        match_source: result.match_source.clone(),
        input_signature: input_sig,
        candidates,
        winner: super::sidecar::MatchWinner {
            slug: result.profile_ref.clone(),
            confidence: result.confidence,
            assignment_reason,
        },
    }
}
