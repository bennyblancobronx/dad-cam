// Dad Cam - App Settings Types

/** App mode: personal (single library, auto-open) or pro (multi-library) */
export type AppMode = 'personal' | 'pro';

/** Recent library entry */
export interface RecentLibrary {
  path: string;
  name: string;
  lastOpened: string;
  clipCount: number;
  thumbnailPath?: string;
}

/** App settings structure */
export interface AppSettings {
  version: number;
  mode: AppMode;
  lastLibraryPath: string | null;
  recentLibraries: RecentLibrary[];
}

/** Default app settings */
export const DEFAULT_SETTINGS: AppSettings = {
  version: 1,
  mode: 'personal',
  lastLibraryPath: null,
  recentLibraries: [],
};
