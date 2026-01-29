// Dad Cam - App Settings Types (v2)

/** App mode: simple (single project, auto-open) or advanced (multi-project) */
export type AppMode = 'simple' | 'advanced';

/** Recent project entry */
export interface RecentProject {
  path: string;
  name: string;
  lastOpened: string;
  clipCount: number;
  thumbnailPath?: string;
}

/** Feature flags */
export interface FeatureFlags {
  screenGrabs: boolean;
  faceDetection: boolean;
  bestClips: boolean;
  camerasTab: boolean;
}

/** Score weights for the scoring engine */
export interface ScoreWeights {
  scene: number;
  audio: number;
  sharpness: number;
  motion: number;
}

/** Dev menu settings */
export interface DevMenuSettings {
  titleStartSeconds: number;
  jlBlendMs: number;
  scoreWeights: ScoreWeights;
  watermarkText: string | null;
}

/** Cached license state (non-secret summary) */
export interface LicenseStateCache {
  licenseType: 'trial' | 'purchased' | 'rental' | 'dev';
  isActive: boolean;
  daysRemaining: number | null;
}

/** App settings structure (v2) */
export interface AppSettings {
  version: number;
  mode: AppMode;
  firstRunCompleted: boolean;
  theme: 'light' | 'dark';
  defaultProjectPath: string | null;
  recentProjects: RecentProject[];
  featureFlags: FeatureFlags;
  devMenu: DevMenuSettings;
  licenseStateCache: LicenseStateCache | null;
}

/** Default feature flags for Simple mode */
export const DEFAULT_FEATURE_FLAGS_SIMPLE: FeatureFlags = {
  screenGrabs: true,
  faceDetection: false,
  bestClips: true,
  camerasTab: false,
};

/** Default feature flags for Advanced mode */
export const DEFAULT_FEATURE_FLAGS_ADVANCED: FeatureFlags = {
  screenGrabs: true,
  faceDetection: true,
  bestClips: true,
  camerasTab: true,
};

/** Default dev menu settings */
export const DEFAULT_DEV_MENU: DevMenuSettings = {
  titleStartSeconds: 5.0,
  jlBlendMs: 500,
  scoreWeights: { scene: 0.25, audio: 0.25, sharpness: 0.25, motion: 0.25 },
  watermarkText: null,
};

/** Default app settings */
export const DEFAULT_SETTINGS: AppSettings = {
  version: 2,
  mode: 'simple',
  firstRunCompleted: false,
  theme: 'light',
  defaultProjectPath: null,
  recentProjects: [],
  featureFlags: DEFAULT_FEATURE_FLAGS_SIMPLE,
  devMenu: DEFAULT_DEV_MENU,
  licenseStateCache: null,
};
