// Dad Cam - VHS Export API
import { invoke } from '@tauri-apps/api/core';
import type { VhsExportParams, ExportHistoryEntry } from '../types/export';

/** Start a VHS export. Returns a job ID for progress tracking. */
export async function startVhsExport(params: VhsExportParams): Promise<string> {
  return invoke<string>('start_vhs_export', { params });
}

/** Get recent export history entries. */
export async function getExportHistory(limit?: number): Promise<ExportHistoryEntry[]> {
  return invoke<ExportHistoryEntry[]>('get_export_history', { limit: limit ?? 20 });
}

/** Cancel an in-progress export by job ID. */
export async function cancelExport(jobId: string): Promise<boolean> {
  return invoke<boolean>('cancel_export', { jobId });
}
