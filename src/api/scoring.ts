// Dad Cam - Phase 4 Scoring API
import { invoke } from '@tauri-apps/api/core';
import type {
  ClipScore,
  ScoreOverrideRequest,
  ScoringStatus,
  BestClipsQuery,
  BestClipEntry,
} from '../types/scoring';

// Get score for a specific clip
export async function getClipScore(clipId: number): Promise<ClipScore | null> {
  return invoke<ClipScore | null>('get_clip_score', { clipId });
}

// Score a specific clip (runs analysis)
export async function scoreClip(
  libraryPath: string,
  clipId: number,
  force: boolean = false
): Promise<ClipScore> {
  return invoke<ClipScore>('score_clip', { libraryPath, clipId, force });
}

// Get library-wide scoring status
export async function getScoringStatus(): Promise<ScoringStatus> {
  return invoke<ScoringStatus>('get_scoring_status');
}

// Get best clips above threshold
export async function getBestClips(query?: BestClipsQuery): Promise<BestClipEntry[]> {
  return invoke<BestClipEntry[]>('get_best_clips', { query: query ?? {} });
}

// Set a score override (promote, demote, pin, or clear)
export async function setScoreOverride(request: ScoreOverrideRequest): Promise<ClipScore> {
  return invoke<ClipScore>('set_score_override', { request });
}

// Clear a score override
export async function clearScoreOverride(clipId: number): Promise<boolean> {
  return invoke<boolean>('clear_score_override', { clipId });
}

// Queue scoring jobs for all unscored clips
export async function queueScoringJobs(): Promise<number> {
  return invoke<number>('queue_scoring_jobs');
}

// Convenience functions for common override actions

export async function promoteClip(
  clipId: number,
  value: number = 0.2,
  note?: string
): Promise<ClipScore> {
  return setScoreOverride({ clipId, overrideType: 'promote', value, note });
}

export async function demoteClip(
  clipId: number,
  value: number = 0.2,
  note?: string
): Promise<ClipScore> {
  return setScoreOverride({ clipId, overrideType: 'demote', value, note });
}

export async function pinClipScore(
  clipId: number,
  value: number,
  note?: string
): Promise<ClipScore> {
  return setScoreOverride({ clipId, overrideType: 'pin', value, note });
}

export async function clearClipOverride(clipId: number): Promise<ClipScore> {
  return setScoreOverride({ clipId, overrideType: 'clear' });
}
