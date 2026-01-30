// Dad Cam - Job types

/** Progress payload emitted by Rust during long-running operations */
export interface JobProgress {
  jobId: string;
  phase: string;
  current: number;
  total: number;
  percent: number;
  message: string;
  isCancelled: boolean;
  isError: boolean;
  errorMessage: string | null;
}

/** Camera breakdown entry returned from ingest */
export interface CameraBreakdownEntry {
  name: string;
  count: number;
}

/** Response from start_ingest command */
export interface IngestResponse {
  jobId: number;
  totalFiles: number;
  processed: number;
  skipped: number;
  failed: number;
  clipsCreated: number[];
  cameraBreakdown: CameraBreakdownEntry[];
  sessionId: number | null;
  sidecarCount: number;
  sidecarFailed: number;
}
