// Dad Cam - Phase 4 Score Badge Component
import { useMemo } from 'react';
import type { ClipScore } from '../types/scoring';
import { getScoreTier, getScoreTierColor } from '../types/scoring';

interface ScoreBadgeProps {
  score: ClipScore | null;
  size?: 'small' | 'medium' | 'large';
  showOverride?: boolean;
}

export function ScoreBadge({ score, size = 'medium', showOverride = true }: ScoreBadgeProps) {
  const { displayScore, tier, color, hasOverride } = useMemo(() => {
    if (!score) {
      return { displayScore: null, tier: null, color: '#666666', hasOverride: false };
    }
    const effectiveScore = score.effectiveScore;
    const tier = getScoreTier(effectiveScore);
    const color = getScoreTierColor(tier);
    return {
      displayScore: effectiveScore,
      tier,
      color,
      hasOverride: score.hasOverride,
    };
  }, [score]);

  if (displayScore === null) {
    return (
      <div className={`score-badge score-badge--${size} score-badge--unscored`}>
        <span className="score-badge__value">--</span>
      </div>
    );
  }

  const sizeStyles = {
    small: { fontSize: '12px', padding: '2px 6px', minWidth: '36px' },
    medium: { fontSize: '14px', padding: '4px 10px', minWidth: '48px' },
    large: { fontSize: '18px', padding: '6px 14px', minWidth: '60px' },
  };

  return (
    <div
      className={`score-badge score-badge--${size} score-badge--${tier}`}
      style={{
        ...sizeStyles[size],
        backgroundColor: `${color}20`,
        color: color,
        border: `1px solid ${color}40`,
        borderRadius: '6px',
        fontWeight: 600,
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: '4px',
      }}
      title={`Score: ${(displayScore * 100).toFixed(0)}%${hasOverride ? ' (override applied)' : ''}`}
    >
      <span className="score-badge__value">
        {(displayScore * 100).toFixed(0)}
      </span>
      {showOverride && hasOverride && (
        <span className="score-badge__override" style={{ fontSize: '0.8em' }}>
          {score?.overrideType === 'pin' ? '\u{1F4CC}' : score?.overrideType === 'promote' ? '\u2191' : '\u2193'}
        </span>
      )}
    </div>
  );
}

// Inline score indicator for grid thumbnails
interface ScoreIndicatorProps {
  score: number | null;
  hasOverride?: boolean;
}

export function ScoreIndicator({ score, hasOverride = false }: ScoreIndicatorProps) {
  if (score === null) return null;

  const tier = getScoreTier(score);
  const color = getScoreTierColor(tier);

  return (
    <div
      className="score-indicator"
      style={{
        position: 'absolute',
        top: '8px',
        right: '8px',
        backgroundColor: `${color}cc`,
        color: '#ffffff',
        fontSize: '11px',
        fontWeight: 600,
        padding: '2px 6px',
        borderRadius: '4px',
        display: 'flex',
        alignItems: 'center',
        gap: '2px',
      }}
    >
      {(score * 100).toFixed(0)}
      {hasOverride && <span style={{ fontSize: '9px' }}>*</span>}
    </div>
  );
}
