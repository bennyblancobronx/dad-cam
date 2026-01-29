// Dad Cam - Jobs API
// Calls to Tauri backend for job management

import { invoke } from '@tauri-apps/api/core';

/** Cancel a running job by its ID */
export async function cancelJob(jobId: string): Promise<void> {
  await invoke('cancel_job', { jobId });
}
