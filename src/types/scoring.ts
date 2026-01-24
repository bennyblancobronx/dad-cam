// Dad Cam - Phase 4 Scoring Types

// Score data for a clip
export interface ClipScore {
  clipId: number;
  overallScore: number;
  sceneScore: number;
  audioScore: number;
  sharpnessScore: number;
  motionScore: number;
  reasons: string[];
  effectiveScore: number;
  hasOverride: boolean;
  overrideType: OverrideType | null;
  overrideValue: number | null;
}

// Override types
export type OverrideType = 'promote' | 'demote' | 'pin';

// Override action (includes clear)
export type OverrideAction = OverrideType | 'clear';

// Request to set a score override
export interface ScoreOverrideRequest {
  clipId: number;
  overrideType: OverrideAction;
  value?: number;
  note?: string;
}

// Library-wide scoring status
export interface ScoringStatus {
  totalClips: number;
  scoredClips: number;
  missingScores: number;
  outdatedScores: number;
  userOverrides: number;
}

// Query parameters for best clips
export interface BestClipsQuery {
  threshold?: number;
  limit?: number;
}

// Best clip entry
export interface BestClipEntry {
  clipId: number;
  title: string;
  durationMs: number | null;
  effectiveScore: number;
  thumbPath: string | null;
}

// Score component breakdown for UI display
export interface ScoreBreakdown {
  label: string;
  value: number;
  weight: number;
  weighted: number;
}

// Helper to get score breakdown from ClipScore
export function getScoreBreakdown(score: ClipScore): ScoreBreakdown[] {
  const weight = 0.25; // Each component is 25%
  return [
    { label: 'Scene', value: score.sceneScore, weight, weighted: score.sceneScore * weight },
    { label: 'Audio', value: score.audioScore, weight, weighted: score.audioScore * weight },
    { label: 'Sharpness', value: score.sharpnessScore, weight, weighted: score.sharpnessScore * weight },
    { label: 'Motion', value: score.motionScore, weight, weighted: score.motionScore * weight },
  ];
}

// Score quality tier based on effective score
export type ScoreTier = 'excellent' | 'good' | 'fair' | 'poor';

export function getScoreTier(score: number): ScoreTier {
  if (score >= 0.8) return 'excellent';
  if (score >= 0.6) return 'good';
  if (score >= 0.4) return 'fair';
  return 'poor';
}

// Color for score tier (for UI)
export function getScoreTierColor(tier: ScoreTier): string {
  switch (tier) {
    case 'excellent': return '#22c55e'; // green-500
    case 'good': return '#3b82f6';      // blue-500
    case 'fair': return '#f59e0b';      // amber-500
    case 'poor': return '#ef4444';      // red-500
  }
}
