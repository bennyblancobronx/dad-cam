// Dad Cam - Phase 4 Scoring Status Component
import { useState, useEffect, useCallback } from 'react';
import type { ScoringStatus } from '../types/scoring';
import { getScoringStatus, queueScoringJobs } from '../api/scoring';

interface ScoringStatusBarProps {
  onRefresh?: () => void;
}

export function ScoringStatusBar({ onRefresh }: ScoringStatusBarProps) {
  const [status, setStatus] = useState<ScoringStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [queueing, setQueueing] = useState(false);

  const loadStatus = useCallback(async () => {
    try {
      const result = await getScoringStatus();
      setStatus(result);
    } catch (err) {
      console.error('Failed to load scoring status:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadStatus();
  }, [loadStatus]);

  const handleQueueAll = async () => {
    if (queueing) return;
    setQueueing(true);
    try {
      const count = await queueScoringJobs();
      alert(`Queued ${count} clips for scoring`);
      loadStatus();
      onRefresh?.();
    } catch (err) {
      alert(`Failed to queue scoring jobs: ${err}`);
    } finally {
      setQueueing(false);
    }
  };

  if (loading || !status) {
    return (
      <div style={styles.container}>
        <span style={styles.loading}>Loading scoring status...</span>
      </div>
    );
  }

  const progress = status.totalClips > 0
    ? (status.scoredClips / status.totalClips) * 100
    : 0;

  const needsScoring = status.missingScores + status.outdatedScores;

  return (
    <div style={styles.container}>
      <div style={styles.stats}>
        <div style={styles.stat}>
          <span style={styles.statValue}>{status.scoredClips}</span>
          <span style={styles.statLabel}>Scored</span>
        </div>
        <div style={styles.stat}>
          <span style={styles.statValue}>{status.totalClips}</span>
          <span style={styles.statLabel}>Total</span>
        </div>
        {status.missingScores > 0 && (
          <div style={styles.stat}>
            <span style={{ ...styles.statValue, color: '#f59e0b' }}>{status.missingScores}</span>
            <span style={styles.statLabel}>Missing</span>
          </div>
        )}
        {status.outdatedScores > 0 && (
          <div style={styles.stat}>
            <span style={{ ...styles.statValue, color: '#888' }}>{status.outdatedScores}</span>
            <span style={styles.statLabel}>Outdated</span>
          </div>
        )}
        {status.userOverrides > 0 && (
          <div style={styles.stat}>
            <span style={{ ...styles.statValue, color: '#4a9eff' }}>{status.userOverrides}</span>
            <span style={styles.statLabel}>Overrides</span>
          </div>
        )}
      </div>

      <div style={styles.progressSection}>
        <div style={styles.progressTrack}>
          <div style={{ ...styles.progressFill, width: `${progress}%` }} />
        </div>
        <span style={styles.progressLabel}>{progress.toFixed(0)}%</span>
      </div>

      {needsScoring > 0 && (
        <button
          style={styles.queueButton}
          onClick={handleQueueAll}
          disabled={queueing}
        >
          {queueing ? 'Queueing...' : `Score ${needsScoring} Clips`}
        </button>
      )}
    </div>
  );
}

// Compact inline status indicator
interface ScoringProgressProps {
  scored: number;
  total: number;
}

export function ScoringProgress({ scored, total }: ScoringProgressProps) {
  const progress = total > 0 ? (scored / total) * 100 : 0;

  return (
    <div style={styles.inlineProgress}>
      <div style={styles.inlineTrack}>
        <div style={{ ...styles.inlineFill, width: `${progress}%` }} />
      </div>
      <span style={styles.inlineLabel}>{scored}/{total} scored</span>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: 'flex',
    alignItems: 'center',
    gap: '16px',
    padding: '12px 16px',
    backgroundColor: '#1a1a1a',
    borderRadius: '8px',
    flexWrap: 'wrap',
  },
  loading: {
    color: '#666',
    fontSize: '13px',
  },
  stats: {
    display: 'flex',
    gap: '16px',
  },
  stat: {
    display: 'flex',
    flexDirection: 'column',
    alignItems: 'center',
    gap: '2px',
  },
  statValue: {
    fontSize: '18px',
    fontWeight: 600,
    color: '#fff',
  },
  statLabel: {
    fontSize: '11px',
    color: '#666',
    textTransform: 'uppercase',
    letterSpacing: '0.5px',
  },
  progressSection: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    flex: 1,
    minWidth: '120px',
  },
  progressTrack: {
    flex: 1,
    height: '6px',
    backgroundColor: '#333',
    borderRadius: '3px',
    overflow: 'hidden',
  },
  progressFill: {
    height: '100%',
    backgroundColor: '#22c55e',
    borderRadius: '3px',
    transition: 'width 0.3s ease',
  },
  progressLabel: {
    fontSize: '13px',
    color: '#888',
    minWidth: '36px',
    textAlign: 'right',
  },
  queueButton: {
    padding: '8px 16px',
    fontSize: '13px',
    fontWeight: 500,
    border: 'none',
    borderRadius: '6px',
    backgroundColor: '#4a9eff',
    color: '#fff',
    cursor: 'pointer',
    transition: 'background-color 0.2s',
    whiteSpace: 'nowrap',
  },
  inlineProgress: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
  },
  inlineTrack: {
    width: '60px',
    height: '4px',
    backgroundColor: '#333',
    borderRadius: '2px',
    overflow: 'hidden',
  },
  inlineFill: {
    height: '100%',
    backgroundColor: '#22c55e',
    borderRadius: '2px',
  },
  inlineLabel: {
    fontSize: '12px',
    color: '#888',
  },
};
