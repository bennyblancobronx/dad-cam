// Dad Cam - VHS Export Types

export type SelectionMode = 'date_range' | 'event' | 'favorites' | 'score' | 'all';
export type ExportOrdering = 'chronological' | 'score_desc' | 'score_asc' | 'shuffle';

export interface VhsExportParams {
  selectionMode: SelectionMode;
  selectionParams: Record<string, unknown>;
  ordering: ExportOrdering;
  titleText: string | null;
  outputPath: string;
  libraryPath: string;
  /** Crossfade blend duration in ms (from devMenu.jlBlendMs). Default: 500 */
  blendDurationMs?: number;
  /** Title overlay start time in seconds (from devMenu.titleStartSeconds). Default: 5 */
  titleStartSeconds?: number;
}

export interface ExportHistoryEntry {
  id: number;
  outputPath: string;
  createdAt: string;
  selectionMode: string;
  ordering: string;
  titleText: string | null;
  resolution: string | null;
  isWatermarked: boolean;
  status: string;
  durationMs: number | null;
  fileSizeBytes: number | null;
  clipCount: number | null;
  errorMessage: string | null;
  completedAt: string | null;
}
