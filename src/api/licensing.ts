// Dad Cam - Licensing API
// Calls to Tauri backend for license state and activation

import { invoke } from '@tauri-apps/api/core';
import type { LicenseState, GatedFeature } from '../types/licensing';

/** Get current license state */
export async function getLicenseState(): Promise<LicenseState> {
  return invoke<LicenseState>('get_license_state');
}

/** Activate a license key */
export async function activateLicense(key: string): Promise<LicenseState> {
  return invoke<LicenseState>('activate_license', { key });
}

/** Deactivate the current license (returns new trial state for cache sync) */
export async function deactivateLicense(): Promise<LicenseState> {
  return invoke<LicenseState>('deactivate_license');
}

/** Check if a feature is allowed under current license */
export async function isFeatureAllowed(feature: GatedFeature): Promise<boolean> {
  return invoke<boolean>('is_feature_allowed', { feature });
}
