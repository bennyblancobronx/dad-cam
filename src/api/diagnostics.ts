// Dad Cam - Diagnostics API
// Crash reporting preference, log directory, log export, support bundle, system health, log level.

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

export async function exportSupportBundle(targetDir: string): Promise<string> {
  return invoke<string>('export_support_bundle', { targetDir });
}

export interface SystemHealth {
  pendingJobs: [string, number][];
  failedJobs24h: number;
  lastError: string | null;
  originalsSize: string;
  derivedSize: string;
  dbSize: string;
}

export async function getSystemHealth(): Promise<SystemHealth> {
  return invoke<SystemHealth>('get_system_health');
}

export async function getLogLevel(): Promise<string> {
  return invoke<string>('get_log_level');
}

export async function setLogLevel(level: string): Promise<void> {
  return invoke('set_log_level', { level });
}
