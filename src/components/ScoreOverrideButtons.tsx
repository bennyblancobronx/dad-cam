// Dad Cam - Phase 4 Score Override Buttons
import { useState } from 'react';
import type { ClipScore, OverrideAction } from '../types/scoring';

interface ScoreOverrideButtonsProps {
  clipId: number;
  score: ClipScore | null;
  onOverride: (clipId: number, action: OverrideAction, value?: number) => Promise<void>;
  disabled?: boolean;
  compact?: boolean;
}

export function ScoreOverrideButtons({
  clipId,
  score,
  onOverride,
  disabled = false,
  compact = false,
}: ScoreOverrideButtonsProps) {
  const [loading, setLoading] = useState<OverrideAction | null>(null);

  const handleAction = async (action: OverrideAction, value?: number) => {
    if (disabled || loading) return;
    setLoading(action);
    try {
      await onOverride(clipId, action, value);
    } finally {
      setLoading(null);
    }
  };

  const hasOverride = score?.hasOverride ?? false;
  const overrideType = score?.overrideType;

  const buttonStyle = (active: boolean): React.CSSProperties => ({
    ...styles.button,
    ...(compact ? styles.buttonCompact : {}),
    ...(active ? styles.buttonActive : {}),
    opacity: disabled ? 0.5 : 1,
    cursor: disabled ? 'not-allowed' : 'pointer',
  });

  return (
    <div style={compact ? styles.containerCompact : styles.container}>
      <button
        style={buttonStyle(overrideType === 'promote')}
        onClick={() => handleAction('promote', 0.2)}
        disabled={disabled || loading !== null}
        title="Boost score by 20%"
      >
        {loading === 'promote' ? '...' : compact ? '+' : 'Promote'}
      </button>

      <button
        style={buttonStyle(overrideType === 'demote')}
        onClick={() => handleAction('demote', 0.2)}
        disabled={disabled || loading !== null}
        title="Reduce score by 20%"
      >
        {loading === 'demote' ? '...' : compact ? '-' : 'Demote'}
      </button>

      {!compact && (
        <button
          style={buttonStyle(overrideType === 'pin')}
          onClick={() => {
            const value = prompt('Pin to score (0-100):', '80');
            if (value !== null) {
              const numValue = parseInt(value, 10) / 100;
              if (numValue >= 0 && numValue <= 1) {
                handleAction('pin', numValue);
              }
            }
          }}
          disabled={disabled || loading !== null}
          title="Pin to a specific score"
        >
          {loading === 'pin' ? '...' : 'Pin'}
        </button>
      )}

      {hasOverride && (
        <button
          style={{ ...styles.button, ...styles.buttonClear, ...(compact ? styles.buttonCompact : {}) }}
          onClick={() => handleAction('clear')}
          disabled={disabled || loading !== null}
          title="Clear override"
        >
          {loading === 'clear' ? '...' : compact ? 'x' : 'Clear'}
        </button>
      )}
    </div>
  );
}

// Inline override indicator for detail views
interface OverrideIndicatorProps {
  score: ClipScore;
}

export function OverrideIndicator({ score }: OverrideIndicatorProps) {
  if (!score.hasOverride) return null;

  const { overrideType, overrideValue } = score;
  const value = overrideValue ?? 0;

  let text = '';
  let color = '#888';

  switch (overrideType) {
    case 'promote':
      text = `+${(value * 100).toFixed(0)}%`;
      color = '#22c55e';
      break;
    case 'demote':
      text = `-${(value * 100).toFixed(0)}%`;
      color = '#ef4444';
      break;
    case 'pin':
      text = `Pinned: ${(value * 100).toFixed(0)}%`;
      color = '#f59e0b';
      break;
  }

  return (
    <span style={{ ...styles.indicator, color }}>
      {text}
    </span>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    gap: '8px',
    flexWrap: 'wrap',
  },
  containerCompact: {
    display: 'flex',
    gap: '4px',
  },
  button: {
    padding: '8px 16px',
    fontSize: '13px',
    fontWeight: 500,
    border: '1px solid #444',
    borderRadius: '6px',
    backgroundColor: '#2a2a2a',
    color: '#ccc',
    transition: 'all 0.2s',
  },
  buttonCompact: {
    padding: '4px 8px',
    fontSize: '12px',
    minWidth: '28px',
  },
  buttonActive: {
    backgroundColor: '#3a3a3a',
    borderColor: '#4a9eff',
    color: '#4a9eff',
  },
  buttonClear: {
    borderColor: '#666',
    color: '#888',
  },
  indicator: {
    fontSize: '12px',
    fontWeight: 500,
  },
};
