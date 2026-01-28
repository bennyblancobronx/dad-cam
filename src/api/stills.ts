// Dad Cam - Stills Export API
// Frontend API for exporting still frames from video clips

import { invoke } from '@tauri-apps/api/core';

/** Request to export a still frame */
export interface StillExportRequest {
  clipId: number;
  timestampMs: number;
  outputPath: string;
  format: 'jpg' | 'png';
}

/** Result of still frame export */
export interface StillExportResult {
  outputPath: string;
  width: number;
  height: number;
  sizeBytes: number;
}

/**
 * Export a still frame from a video clip
 * Uses the original video file (not proxy) for maximum quality
 */
export async function exportStill(request: StillExportRequest): Promise<StillExportResult> {
  return await invoke<StillExportResult>('export_still', { request });
}
