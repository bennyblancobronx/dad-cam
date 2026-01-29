// Dad Cam - Licensing Types

/** License type */
export type LicenseType = 'trial' | 'purchased' | 'rental' | 'dev';

/** Current license state (returned by get_license_state) */
export interface LicenseState {
  licenseType: LicenseType;
  isActive: boolean;
  trialStart: string | null;
  trialDaysRemaining: number | null;
  keyHash: string | null;
}

/** Gated features that require an active license */
export type GatedFeature =
  | 'import'
  | 'scoring'
  | 'camera_registration'
  | 'raw_sql'
  | 'export_original'
  | 'export_rendered';
