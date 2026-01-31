// Dad Cam - Phase 3 Path Utilities
import { convertFileSrc } from '@tauri-apps/api/core';

let libraryRoot: string | null = null;

export function setLibraryRoot(path: string): void {
  libraryRoot = path;
}

/**
 * Convert a relative path from the database to an absolute file:// URL
 * that can be used in <img> and <video> src attributes.
 */
export function toAssetUrl(relativePath: string | null): string | null {
  if (!relativePath || !libraryRoot) return null;

  // Join library root with relative path
  const absolutePath = `${libraryRoot}/${relativePath}`;

  // Convert to Tauri asset URL
  return convertFileSrc(absolutePath);
}

/**
 * Get sprite metadata path from sprite path
 */
export function getSpriteMetaPath(spritePath: string | null): string | null {
  if (!spritePath) return null;
  // Replace .jpg extension with .json
  return spritePath.replace(/\.jpg$/, '.json');
}
