// Camera system types (Phase 2: App DB stable refs)
// Types match Rust backend serde output (camelCase)

export interface CameraProfile {
  profileType: string;   // "bundled" or "user"
  profileRef: string;    // slug (bundled) or uuid (user)
  name: string;
  version: number;
  matchRules: string;    // JSON string
  transformRules: string; // JSON string
}

export interface CameraDevice {
  id: number;
  uuid: string;
  profileType: string;
  profileRef: string;
  serialNumber: string | null;
  fleetLabel: string | null;
  usbFingerprints: string[];
  rentalNotes: string | null;
  createdAt: string;
}

export interface RegisterDeviceParams {
  profileType?: string;
  profileRef?: string;
  serialNumber?: string;
  fleetLabel?: string;
  rentalNotes?: string;
  captureUsb: boolean;
}

export interface CameraMatchResult {
  deviceUuid: string | null;
  profileType: string | null;
  profileRef: string | null;
  profileName: string | null;
  deviceLabel: string | null;
  confidence: number;
  reason: string;
}

export interface ImportCameraDbResult {
  profilesImported: number;
  devicesImported: number;
}

export interface ExportCameraDbResult {
  bundledProfilesCount: number;
  userProfilesCount: number;
  devicesCount: number;
}

// Parsed match rules (for frontend display convenience)
export interface ParsedMatchRules {
  make?: string[];
  model?: string[];
  codec?: string[];
  container?: string[];
  folderPattern?: string;
  minWidth?: number;
  maxWidth?: number;
  minHeight?: number;
  maxHeight?: number;
  frameRate?: number[];
}

/** Parse a matchRules JSON string into a typed object */
export function parseMatchRules(json: string): ParsedMatchRules {
  try {
    return JSON.parse(json) as ParsedMatchRules;
  } catch {
    return {};
  }
}
