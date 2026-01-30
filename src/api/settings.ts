// Dad Cam - Settings API
// Calls to Tauri backend for app settings persistence

import { invoke } from '@tauri-apps/api/core';
import type { AppSettings, AppMode, RecentProject, RegistryLibraryEntry } from '../types/settings';
import { DEFAULT_SETTINGS } from '../types/settings';

/** Get app settings from persistent store */
export async function getAppSettings(): Promise<AppSettings> {
  try {
    return await invoke<AppSettings>('get_app_settings');
  } catch (err) {
    console.error('Failed to get app settings, using defaults:', err);
    return DEFAULT_SETTINGS;
  }
}

/** Save app settings to persistent store */
export async function saveAppSettings(settings: AppSettings): Promise<void> {
  await invoke('save_app_settings', { settings });
}

/** Get current mode */
export async function getMode(): Promise<AppMode> {
  const mode = await invoke<string>('get_mode');
  return mode as AppMode;
}

/** Set mode */
export async function setMode(mode: AppMode): Promise<void> {
  await invoke('set_mode', { mode });
}

/** Add or update a recent project entry */
export async function addRecentLibrary(
  path: string,
  name: string,
  clipCount: number
): Promise<void> {
  await invoke('add_recent_library', { path, name, clipCount });
}

/** Remove a project from recent list */
export async function removeRecentLibrary(path: string): Promise<void> {
  await invoke('remove_recent_library', { path });
}

/** Get recent projects list */
export async function getRecentLibraries(): Promise<RecentProject[]> {
  return await invoke<RecentProject[]>('get_recent_libraries');
}

/** Validate if a library path exists and is valid */
export async function validateLibraryPath(path: string): Promise<boolean> {
  return await invoke<boolean>('validate_library_path', { path });
}

/** List libraries from App DB registry (Phase 6: survives library deletion) */
export async function listRegistryLibraries(): Promise<RegistryLibraryEntry[]> {
  return await invoke<RegistryLibraryEntry[]>('list_registry_libraries');
}
