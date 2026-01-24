// Dad Cam - Phase 4 Best Clips Panel
import { useState, useEffect, useCallback } from 'react';
import type { BestClipEntry } from '../types/scoring';
import { getBestClips } from '../api/scoring';
import { getScoreTier, getScoreTierColor } from '../types/scoring';
import { toAssetUrl } from '../utils/paths';

interface BestClipsPanelProps {
  onClipClick?: (clipId: number) => void;
  threshold?: number;
  limit?: number;
}

export function BestClipsPanel({
  onClipClick,
  threshold = 0.6,
  limit = 20,
}: BestClipsPanelProps) {
  const [clips, setClips] = useState<BestClipEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentThreshold, setCurrentThreshold] = useState(threshold);

  const loadBestClips = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await getBestClips({ threshold: currentThreshold, limit });
      setClips(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load best clips');
    } finally {
      setLoading(false);
    }
  }, [currentThreshold, limit]);

  useEffect(() => {
    loadBestClips();
  }, [loadBestClips]);

  const formatDuration = (ms: number | null): string => {
    if (!ms) return '--:--';
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const remainingSeconds = seconds % 60;
    return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
  };

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <h3 style={styles.title}>Best Clips</h3>
        <div style={styles.controls}>
          <label style={styles.thresholdLabel}>
            Min Score:
            <input
              type="range"
              min="0"
              max="100"
              value={currentThreshold * 100}
              onChange={(e) => setCurrentThreshold(parseInt(e.target.value, 10) / 100)}
              style={styles.slider}
            />
            <span style={styles.thresholdValue}>{(currentThreshold * 100).toFixed(0)}%</span>
          </label>
          <button onClick={loadBestClips} style={styles.refreshButton} disabled={loading}>
            {loading ? 'Loading...' : 'Refresh'}
          </button>
        </div>
      </div>

      {error && <div style={styles.error}>{error}</div>}

      {!loading && clips.length === 0 && (
        <div style={styles.empty}>
          No clips found above {(currentThreshold * 100).toFixed(0)}% threshold.
          <br />
          Try lowering the threshold or score more clips.
        </div>
      )}

      <div style={styles.grid}>
        {clips.map((clip) => {
          const tier = getScoreTier(clip.effectiveScore);
          const color = getScoreTierColor(tier);

          return (
            <div
              key={clip.clipId}
              style={styles.card}
              onClick={() => onClipClick?.(clip.clipId)}
            >
              <div style={styles.thumbContainer}>
                {clip.thumbPath ? (
                  <img
                    src={toAssetUrl(clip.thumbPath) ?? undefined}
                    alt={clip.title}
                    style={styles.thumb}
                  />
                ) : (
                  <div style={styles.thumbPlaceholder}>No Preview</div>
                )}
                <div style={{ ...styles.scoreBadge, backgroundColor: color }}>
                  {(clip.effectiveScore * 100).toFixed(0)}
                </div>
              </div>
              <div style={styles.cardInfo}>
                <div style={styles.cardTitle} title={clip.title}>
                  {clip.title.length > 30 ? `${clip.title.slice(0, 27)}...` : clip.title}
                </div>
                <div style={styles.cardMeta}>
                  {formatDuration(clip.durationMs)}
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {clips.length > 0 && (
        <div style={styles.footer}>
          Showing {clips.length} clips above {(currentThreshold * 100).toFixed(0)}%
        </div>
      )}
    </div>
  );
}

// Compact version for sidebar or panel
interface BestClipsListProps {
  onClipClick?: (clipId: number) => void;
  limit?: number;
}

export function BestClipsList({ onClipClick, limit = 10 }: BestClipsListProps) {
  const [clips, setClips] = useState<BestClipEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getBestClips({ threshold: 0.6, limit })
      .then(setClips)
      .catch(() => setClips([]))
      .finally(() => setLoading(false));
  }, [limit]);

  if (loading) {
    return <div style={styles.listLoading}>Loading best clips...</div>;
  }

  if (clips.length === 0) {
    return <div style={styles.listEmpty}>No scored clips yet</div>;
  }

  return (
    <div style={styles.list}>
      {clips.map((clip, index) => {
        const tier = getScoreTier(clip.effectiveScore);
        const color = getScoreTierColor(tier);

        return (
          <div
            key={clip.clipId}
            style={styles.listItem}
            onClick={() => onClipClick?.(clip.clipId)}
          >
            <span style={styles.listRank}>#{index + 1}</span>
            <span style={{ ...styles.listScore, color }}>{(clip.effectiveScore * 100).toFixed(0)}</span>
            <span style={styles.listTitle}>{clip.title}</span>
          </div>
        );
      })}
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    backgroundColor: '#1a1a1a',
    borderRadius: '8px',
    padding: '16px',
  },
  header: {
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: '16px',
    flexWrap: 'wrap',
    gap: '12px',
  },
  title: {
    margin: 0,
    fontSize: '18px',
    fontWeight: 600,
    color: '#fff',
  },
  controls: {
    display: 'flex',
    alignItems: 'center',
    gap: '16px',
  },
  thresholdLabel: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    fontSize: '13px',
    color: '#888',
  },
  slider: {
    width: '100px',
  },
  thresholdValue: {
    minWidth: '40px',
    textAlign: 'right',
    color: '#ccc',
  },
  refreshButton: {
    padding: '6px 12px',
    fontSize: '13px',
    border: '1px solid #444',
    borderRadius: '6px',
    backgroundColor: '#2a2a2a',
    color: '#ccc',
    cursor: 'pointer',
  },
  error: {
    backgroundColor: '#442222',
    color: '#ff8888',
    padding: '12px',
    borderRadius: '6px',
    marginBottom: '16px',
    fontSize: '13px',
  },
  empty: {
    textAlign: 'center',
    color: '#666',
    padding: '32px 16px',
    fontSize: '14px',
  },
  grid: {
    display: 'grid',
    gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))',
    gap: '16px',
  },
  card: {
    backgroundColor: '#2a2a2a',
    borderRadius: '8px',
    overflow: 'hidden',
    cursor: 'pointer',
    transition: 'transform 0.2s, box-shadow 0.2s',
  },
  thumbContainer: {
    position: 'relative',
    aspectRatio: '16/9',
    backgroundColor: '#333',
  },
  thumb: {
    width: '100%',
    height: '100%',
    objectFit: 'cover',
  },
  thumbPlaceholder: {
    width: '100%',
    height: '100%',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    color: '#666',
    fontSize: '12px',
  },
  scoreBadge: {
    position: 'absolute',
    top: '8px',
    right: '8px',
    padding: '4px 8px',
    borderRadius: '4px',
    color: '#fff',
    fontSize: '12px',
    fontWeight: 600,
  },
  cardInfo: {
    padding: '10px 12px',
  },
  cardTitle: {
    fontSize: '13px',
    fontWeight: 500,
    color: '#fff',
    marginBottom: '4px',
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  cardMeta: {
    fontSize: '12px',
    color: '#888',
  },
  footer: {
    marginTop: '16px',
    textAlign: 'center',
    fontSize: '13px',
    color: '#666',
  },
  list: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  listItem: {
    display: 'flex',
    alignItems: 'center',
    gap: '8px',
    padding: '8px 12px',
    backgroundColor: '#2a2a2a',
    borderRadius: '6px',
    cursor: 'pointer',
  },
  listRank: {
    fontSize: '12px',
    color: '#666',
    minWidth: '24px',
  },
  listScore: {
    fontSize: '13px',
    fontWeight: 600,
    minWidth: '32px',
  },
  listTitle: {
    fontSize: '13px',
    color: '#ccc',
    flex: 1,
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
  },
  listLoading: {
    textAlign: 'center',
    color: '#666',
    padding: '16px',
    fontSize: '13px',
  },
  listEmpty: {
    textAlign: 'center',
    color: '#666',
    padding: '16px',
    fontSize: '13px',
  },
};
