// Camera system API wrappers (Phase 5)

import { invoke } from '@tauri-apps/api/core';
import type {
  CameraProfile,
  CameraDevice,
  RegisterDeviceParams,
  CameraMatchResult,
  ImportCameraDbResult,
  ExportCameraDbResult,
} from '../types/cameras';

export async function listCameraProfiles(): Promise<CameraProfile[]> {
  return invoke<CameraProfile[]>('list_camera_profiles');
}

export async function listCameraDevices(): Promise<CameraDevice[]> {
  return invoke<CameraDevice[]>('list_camera_devices');
}

export async function registerCameraDevice(params: RegisterDeviceParams): Promise<CameraDevice> {
  return invoke<CameraDevice>('register_camera_device', { params });
}

export async function matchCamera(clipId: number): Promise<CameraMatchResult> {
  return invoke<CameraMatchResult>('match_camera', { clipId });
}

export async function importCameraDb(jsonPath: string): Promise<ImportCameraDbResult> {
  return invoke<ImportCameraDbResult>('import_camera_db', { jsonPath });
}

export async function exportCameraDb(outputPath: string): Promise<ExportCameraDbResult> {
  return invoke<ExportCameraDbResult>('export_camera_db', { outputPath });
}
