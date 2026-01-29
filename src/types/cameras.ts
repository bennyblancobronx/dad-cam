// Camera system types (Phase 5)

export interface CameraProfile {
  id: number;
  name: string;
  version: number;
  matchRules: MatchRules;
  transformRules: TransformRules;
}

export interface MatchRules {
  make?: string[];
  model?: string[];
  codec?: string[];
  container?: string[];
  folderPattern?: string;
  resolution?: { width?: number; height?: number };
}

export interface TransformRules {
  deinterlace?: boolean;
  deinterlaceMode?: string;
  colorSpace?: string;
  lut?: string;
}

export interface CameraDevice {
  id: number;
  uuid: string;
  profileId: number | null;
  serialNumber: string | null;
  fleetLabel: string | null;
  usbFingerprints: string[];
  rentalNotes: string | null;
  createdAt: string;
}

export interface RegisterDeviceParams {
  profileId?: number;
  serialNumber?: string;
  fleetLabel?: string;
  rentalNotes?: string;
  captureUsb: boolean;
}

export interface CameraMatchResult {
  deviceId: number | null;
  profileId: number | null;
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
  profilesCount: number;
  devicesCount: number;
}
