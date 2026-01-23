// Dad Cam - Phase 3 API Layer
import { invoke } from '@tauri-apps/api/core';
import type { ClipView, ClipQuery, ClipListResponse, LibraryInfo } from '../types/clips';

// Library operations
export async function openLibrary(path: string): Promise<LibraryInfo> {
  return invoke<LibraryInfo>('open_library', { path });
}

export async function closeLibrary(): Promise<void> {
  return invoke('close_library');
}

export async function createLibrary(path: string, name: string): Promise<LibraryInfo> {
  return invoke<LibraryInfo>('create_library', { path, name });
}

export async function getLibraryRoot(): Promise<string> {
  return invoke<string>('get_library_root');
}

// Clip operations
export async function getClipsFiltered(query: ClipQuery): Promise<ClipListResponse> {
  return invoke<ClipListResponse>('get_clips_filtered', { query });
}

export async function getClipView(clipId: number): Promise<ClipView> {
  return invoke<ClipView>('get_clip_view', { id: clipId });
}

// Tag operations
export async function toggleTag(clipId: number, tag: string): Promise<boolean> {
  return invoke<boolean>('toggle_tag', { clipId, tag });
}

export async function setTag(clipId: number, tag: string, value: boolean): Promise<boolean> {
  return invoke<boolean>('set_tag', { clipId, tag, value });
}
