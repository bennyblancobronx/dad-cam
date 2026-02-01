// Dad Cam - Diagnostics API
// Get/set crash reporting preference, log directory, log export.

import { invoke } from '@tauri-apps/api/core';

export async function getDiagnosticsEnabled(): Promise<boolean> {
  return invoke<boolean>('get_diagnostics_enabled');
}

export async function setDiagnosticsEnabled(enabled: boolean): Promise<void> {
  return invoke('set_diagnostics_enabled', { enabled });
}

export async function getLogDirectory(): Promise<string> {
  return invoke<string>('get_log_directory');
}

export async function exportLogs(targetDir: string): Promise<number> {
  return invoke<number>('export_logs', { targetDir });
}
