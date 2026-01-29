// Dad Cam - Licensing System
// Trial (14 days), Purchased ($99), Rental (key-based), Dev (internal)
// Keys validated offline via BLAKE3 keyed hash (contract 13: no network)

use serde::{Deserialize, Serialize};
use chrono::Utc;

/// License key prefixes
const PREFIX_PURCHASED: &str = "DCAM-P-";
const PREFIX_RENTAL: &str = "DCAM-R-";
const PREFIX_DEV: &str = "DCAM-D-";

/// Trial duration in days
const TRIAL_DAYS: i32 = 14;

/// Keychain service name
const KEYCHAIN_SERVICE: &str = "com.dadcam.app";

/// Keychain account names
const KEYCHAIN_KEY_ACCOUNT: &str = "license-key";
const KEYCHAIN_TRIAL_ACCOUNT: &str = "trial-start";

/// Secret key for BLAKE3 keyed hashing (32 bytes)
/// Light friction, not DRM -- per project philosophy
const VALIDATION_SECRET: [u8; 32] = [
    0xd4, 0xa1, 0xdc, 0x4a, 0x6d, 0x0e, 0x83, 0x9f,
    0x7b, 0x21, 0x55, 0xc8, 0xe3, 0x47, 0x91, 0x0c,
    0xf6, 0x38, 0xba, 0x2d, 0x69, 0x14, 0xa7, 0xe5,
    0x3c, 0x80, 0xfb, 0x52, 0x06, 0xcd, 0x9e, 0x73,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LicenseType {
    Trial,
    Purchased,
    Rental,
    Dev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseState {
    pub license_type: LicenseType,
    pub is_active: bool,
    pub trial_start: Option<String>,
    pub trial_days_remaining: Option<i32>,
    pub key_hash: Option<String>,
}

/// Get the current license state
pub fn check_license() -> LicenseState {
    // Check for stored key first
    if let Some(key) = get_stored_key() {
        if let Some(license_type) = validate_key(&key) {
            let hash = blake3::hash(key.as_bytes());
            let key_hash = format!("{}", hash)[..16].to_string();
            return LicenseState {
                license_type,
                is_active: true,
                trial_start: None,
                trial_days_remaining: None,
                key_hash: Some(key_hash),
            };
        }
    }

    // No valid key -- check trial
    let trial_start = get_or_create_trial_start();
    let days_remaining = calculate_trial_days_remaining(&trial_start);
    let is_active = days_remaining > 0;

    LicenseState {
        license_type: LicenseType::Trial,
        is_active,
        trial_start: Some(trial_start),
        trial_days_remaining: Some(days_remaining),
        key_hash: None,
    }
}

/// Activate a license key
pub fn activate_key(key: &str) -> Result<LicenseState, String> {
    let key = key.trim();
    let license_type = validate_key(key)
        .ok_or_else(|| "Invalid license key".to_string())?;

    // Store key in keychain
    store_key(key).map_err(|e| format!("Failed to store key: {}", e))?;

    let hash = blake3::hash(key.as_bytes());
    let key_hash = format!("{}", hash)[..16].to_string();

    Ok(LicenseState {
        license_type,
        is_active: true,
        trial_start: None,
        trial_days_remaining: None,
        key_hash: Some(key_hash),
    })
}

/// Deactivate current license (remove key from keychain)
pub fn deactivate() -> Result<(), String> {
    delete_stored_key().map_err(|e| format!("Failed to remove key: {}", e))
}

/// Check if a gated feature is allowed given current license state
pub fn is_allowed(feature: &str) -> bool {
    let state = check_license();

    match feature {
        "import" => state.is_active,
        "scoring" => state.is_active,
        "camera_registration" => state.is_active,
        "raw_sql" => state.license_type == LicenseType::Dev,
        "export_original" => true, // Always allowed (non-hostage rule, contract 12)
        "export_rendered" => true, // Always allowed, but watermarked when inactive
        _ => state.is_active,
    }
}

/// Check if rendered exports should have watermark + 720p cap
pub fn should_watermark() -> bool {
    let state = check_license();
    !state.is_active
}

/// Generate a license key of the given type (for dev menu / rental key generation)
pub fn generate_key(license_type: &LicenseType) -> String {
    let prefix = match license_type {
        LicenseType::Purchased => PREFIX_PURCHASED,
        LicenseType::Rental => PREFIX_RENTAL,
        LicenseType::Dev => PREFIX_DEV,
        LicenseType::Trial => return String::new(),
    };

    // Generate random payload using uuid
    let payload = uuid::Uuid::new_v4().to_string().replace('-', "");
    let body = format!("{}{}", prefix, payload);
    let checksum = compute_checksum(&body);
    format!("{}-{}", body, checksum)
}

/// Generate multiple rental keys
pub fn generate_rental_keys(count: u32) -> Vec<String> {
    (0..count).map(|_| generate_key(&LicenseType::Rental)).collect()
}

// --- Key validation ---

/// Validate a license key format and BLAKE3 keyed hash checksum
/// Returns the license type if valid, None if invalid
fn validate_key(key: &str) -> Option<LicenseType> {
    let key = key.trim();

    // Determine type from prefix
    let (license_type, prefix_len) = if key.starts_with(PREFIX_DEV) {
        (LicenseType::Dev, PREFIX_DEV.len())
    } else if key.starts_with(PREFIX_PURCHASED) {
        (LicenseType::Purchased, PREFIX_PURCHASED.len())
    } else if key.starts_with(PREFIX_RENTAL) {
        (LicenseType::Rental, PREFIX_RENTAL.len())
    } else {
        return None;
    };

    // Split: everything before last '-' is the body, after is checksum
    let last_dash = key.rfind('-')?;
    if last_dash <= prefix_len {
        return None;
    }

    let body = &key[..last_dash];
    let checksum = &key[last_dash + 1..];

    if checksum.len() != 8 {
        return None;
    }

    // Verify BLAKE3 keyed hash
    let computed = compute_checksum(body);
    if computed == checksum {
        Some(license_type)
    } else {
        None
    }
}

/// Compute 8-char hex checksum for a key body using BLAKE3 keyed hash
fn compute_checksum(body: &str) -> String {
    let mut hasher = blake3::Hasher::new_keyed(&VALIDATION_SECRET);
    hasher.update(body.as_bytes());
    let hash = hasher.finalize();
    format!("{}", hash)[..8].to_string()
}

// --- Keychain operations ---

fn get_stored_key() -> Option<String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_KEY_ACCOUNT).ok()?;
    entry.get_password().ok()
}

fn store_key(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_KEY_ACCOUNT)
        .map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

fn delete_stored_key() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_KEY_ACCOUNT)
        .map_err(|e| e.to_string())?;
    match entry.delete_password() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already gone, that's fine
        Err(e) => Err(e.to_string()),
    }
}

fn get_trial_start() -> Option<String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_TRIAL_ACCOUNT).ok()?;
    entry.get_password().ok()
}

fn set_trial_start(date: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_TRIAL_ACCOUNT)
        .map_err(|e| e.to_string())?;
    entry.set_password(date).map_err(|e| e.to_string())
}

fn get_or_create_trial_start() -> String {
    if let Some(start) = get_trial_start() {
        return start;
    }

    let today = Utc::now().format("%Y-%m-%d").to_string();
    let _ = set_trial_start(&today);
    today
}

fn calculate_trial_days_remaining(trial_start: &str) -> i32 {
    let start = match chrono::NaiveDate::parse_from_str(trial_start, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return 0,
    };

    let today = Utc::now().date_naive();
    let elapsed = (today - start).num_days() as i32;
    let remaining = TRIAL_DAYS - elapsed;
    remaining.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_validate_purchased() {
        let key = generate_key(&LicenseType::Purchased);
        assert!(key.starts_with("DCAM-P-"));
        assert!(validate_key(&key).is_some());
        assert_eq!(validate_key(&key).unwrap(), LicenseType::Purchased);
    }

    #[test]
    fn test_generate_and_validate_rental() {
        let key = generate_key(&LicenseType::Rental);
        assert!(key.starts_with("DCAM-R-"));
        assert!(validate_key(&key).is_some());
        assert_eq!(validate_key(&key).unwrap(), LicenseType::Rental);
    }

    #[test]
    fn test_generate_and_validate_dev() {
        let key = generate_key(&LicenseType::Dev);
        assert!(key.starts_with("DCAM-D-"));
        assert!(validate_key(&key).is_some());
        assert_eq!(validate_key(&key).unwrap(), LicenseType::Dev);
    }

    #[test]
    fn test_invalid_key_rejected() {
        assert!(validate_key("not-a-key").is_none());
        assert!(validate_key("DCAM-P-fake-12345678").is_none());
        assert!(validate_key("").is_none());
    }

    #[test]
    fn test_tampered_key_rejected() {
        let key = generate_key(&LicenseType::Purchased);
        // Flip one character in the payload
        let mut tampered = key.clone();
        let bytes = unsafe { tampered.as_bytes_mut() };
        let idx = PREFIX_PURCHASED.len() + 2;
        bytes[idx] = if bytes[idx] == b'a' { b'b' } else { b'a' };
        assert!(validate_key(&tampered).is_none());
    }

    #[test]
    fn test_trial_days_remaining() {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        assert_eq!(calculate_trial_days_remaining(&today), TRIAL_DAYS);

        // 7 days ago
        let seven_ago = (Utc::now() - chrono::Duration::days(7))
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(calculate_trial_days_remaining(&seven_ago), 7);

        // 14 days ago (expired)
        let expired = (Utc::now() - chrono::Duration::days(14))
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(calculate_trial_days_remaining(&expired), 0);

        // 30 days ago (well expired)
        let old = (Utc::now() - chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(calculate_trial_days_remaining(&old), 0);
    }
}
