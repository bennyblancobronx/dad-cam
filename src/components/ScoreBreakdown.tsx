// Dad Cam - Phase 4 Score Breakdown Panel
import type { ClipScore } from '../types/scoring';
import { getScoreBreakdown, getScoreTier, getScoreTierColor } from '../types/scoring';

interface ScoreBreakdownProps {
  score: ClipScore;
  showReasons?: boolean;
}

export function ScoreBreakdown({ score, showReasons = true }: ScoreBreakdownProps) {
  const breakdown = getScoreBreakdown(score);
  const overallTier = getScoreTier(score.effectiveScore);
  const overallColor = getScoreTierColor(overallTier);

  return (
    <div className="score-breakdown" style={styles.container}>
      {/* Overall Score */}
      <div style={styles.overallSection}>
        <div style={styles.overallLabel}>Overall Score</div>
        <div style={{ ...styles.overallValue, color: overallColor }}>
          {(score.effectiveScore * 100).toFixed(0)}%
        </div>
        {score.hasOverride && (
          <div style={styles.overrideInfo}>
            {score.overrideType === 'pin' && `Pinned to ${((score.overrideValue ?? 0) * 100).toFixed(0)}%`}
            {score.overrideType === 'promote' && `+${((score.overrideValue ?? 0) * 100).toFixed(0)}% boost`}
            {score.overrideType === 'demote' && `-${((score.overrideValue ?? 0) * 100).toFixed(0)}% reduction`}
          </div>
        )}
      </div>

      {/* Component Breakdown */}
      <div style={styles.breakdownSection}>
        {breakdown.map((item) => {
          const itemTier = getScoreTier(item.value);
          const itemColor = getScoreTierColor(itemTier);
          const percentage = item.value * 100;

          return (
            <div key={item.label} style={styles.breakdownItem}>
              <div style={styles.breakdownHeader}>
                <span style={styles.breakdownLabel}>{item.label}</span>
                <span style={{ ...styles.breakdownValue, color: itemColor }}>
                  {percentage.toFixed(0)}%
                </span>
              </div>
              <div style={styles.progressTrack}>
                <div
                  style={{
                    ...styles.progressFill,
                    width: `${percentage}%`,
                    backgroundColor: itemColor,
                  }}
                />
              </div>
            </div>
          );
        })}
      </div>

      {/* Reasons */}
      {showReasons && score.reasons.length > 0 && (
        <div style={styles.reasonsSection}>
          <div style={styles.reasonsLabel}>Analysis Notes</div>
          <ul style={styles.reasonsList}>
            {score.reasons.map((reason, index) => (
              <li key={index} style={styles.reasonItem}>{reason}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    backgroundColor: '#1a1a1a',
    borderRadius: '8px',
    padding: '16px',
  },
  overallSection: {
    textAlign: 'center',
    marginBottom: '20px',
    paddingBottom: '16px',
    borderBottom: '1px solid #333',
  },
  overallLabel: {
    fontSize: '12px',
    color: '#888',
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
    marginBottom: '4px',
  },
  overallValue: {
    fontSize: '36px',
    fontWeight: 700,
  },
  overrideInfo: {
    fontSize: '12px',
    color: '#888',
    marginTop: '4px',
  },
  breakdownSection: {
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
  },
  breakdownItem: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  breakdownHeader: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  breakdownLabel: {
    fontSize: '13px',
    color: '#ccc',
  },
  breakdownValue: {
    fontSize: '13px',
    fontWeight: 600,
  },
  progressTrack: {
    height: '4px',
    backgroundColor: '#333',
    borderRadius: '2px',
    overflow: 'hidden',
  },
  progressFill: {
    height: '100%',
    borderRadius: '2px',
    transition: 'width 0.3s ease',
  },
  reasonsSection: {
    marginTop: '16px',
    paddingTop: '16px',
    borderTop: '1px solid #333',
  },
  reasonsLabel: {
    fontSize: '12px',
    color: '#888',
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
    marginBottom: '8px',
  },
  reasonsList: {
    margin: 0,
    padding: '0 0 0 16px',
    fontSize: '13px',
    color: '#aaa',
  },
  reasonItem: {
    marginBottom: '4px',
  },
};
