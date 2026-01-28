// Dad Cam - Date View (Phase 7)
// Display clips for a specific date

import { useState, useEffect, useCallback } from 'react';
import type { EventClipView } from '../types/events';
import { formatClipTime, parseLocalDate } from '../types/events';
import { getClipsByDate } from '../api/events';
import { toAssetUrl } from '../utils/paths';

const PAGE_SIZE = 50;

function formatDuration(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

function formatDisplayDate(dateStr: string): string {
  // Use parseLocalDate to safely handle YYYY-MM-DD without timezone shift
  const date = parseLocalDate(dateStr);
  return date.toLocaleDateString('en-US', {
    weekday: 'long',
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  });
}

interface DateViewProps {
  date: string;
  onBack: () => void;
  /** Called when user clicks a clip - passes full clip data */
  onClipSelect: (clip: EventClipView) => void;
}

export function DateView({
  date,
  onBack,
  onClipSelect,
}: DateViewProps) {
  const [clips, setClips] = useState<EventClipView[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadClips = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const response = await getClipsByDate(date, 0, PAGE_SIZE);
      setClips(response.clips);
      setTotal(response.total);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load clips');
    } finally {
      setLoading(false);
    }
  }, [date]);

  const loadMore = useCallback(async () => {
    if (loadingMore || clips.length >= total) return;
    try {
      setLoadingMore(true);
      const response = await getClipsByDate(date, clips.length, PAGE_SIZE);
      setClips(prev => [...prev, ...response.clips]);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load more clips');
    } finally {
      setLoadingMore(false);
    }
  }, [date, clips.length, total, loadingMore]);

  useEffect(() => {
    loadClips();
  }, [loadClips]);

  const hasMore = clips.length < total;

  if (error) {
    return (
      <div className="date-view">
        <div className="date-view-header">
          <button className="back-button" onClick={onBack} title="Back to library">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 10H5M5 10l5 5M5 10l5-5" />
            </svg>
            Back
          </button>
        </div>
        <div className="date-view-error">
          <p>{error}</p>
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="date-view">
        <div className="date-view-header">
          <button className="back-button" onClick={onBack} title="Back to library">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 10H5M5 10l5 5M5 10l5-5" />
            </svg>
            Back
          </button>
        </div>
        <div className="date-view-content">
          <div className="date-clip-grid">
            {Array.from({ length: 6 }).map((_, i) => (
              <div key={i} className="date-clip-item" style={{ pointerEvents: 'none' }}>
                <div className="skeleton skeleton-card" />
                <div className="date-clip-info">
                  <div className="skeleton skeleton-text" style={{ width: '80%' }} />
                  <div className="skeleton skeleton-text-sm" />
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="date-view">
      <div className="date-view-header">
        <div className="date-view-header-left">
          <button className="back-button" onClick={onBack} title="Back to library">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 10H5M5 10l5 5M5 10l5-5" />
            </svg>
            Back
          </button>
          <div className="date-view-info">
            <h1 className="date-view-title">{formatDisplayDate(date)}</h1>
            <div className="date-view-meta">
              <span className="date-view-clip-count">{total} clip{total !== 1 ? 's' : ''}</span>
            </div>
          </div>
        </div>
      </div>

      <div className="date-view-content">
        {clips.length === 0 ? (
          <div className="date-empty-state">
            <svg width="64" height="64" viewBox="0 0 64 64" fill="none" stroke="currentColor" strokeWidth="1.5">
              <rect x="8" y="12" width="48" height="40" rx="4" />
              <path d="M8 24h48M20 12v-4M44 12v-4" />
            </svg>
            <h3>No clips on this date</h3>
            <p>No recordings were found for this day.</p>
          </div>
        ) : (
          <>
            <div className="date-clip-grid">
              {clips.map((clip) => (
                <div
                  key={clip.id}
                  className="date-clip-item"
                  onClick={() => onClipSelect(clip)}
                >
                  <div className="date-clip-thumbnail">
                    {clip.thumbnailPath ? (
                      <img
                        src={toAssetUrl(clip.thumbnailPath) ?? undefined}
                        alt={clip.title}
                        className="date-clip-image"
                      />
                    ) : (
                      <div className="date-clip-placeholder">No Preview</div>
                    )}
                    {clip.durationMs && (
                      <div className="date-clip-duration">
                        {formatDuration(clip.durationMs)}
                      </div>
                    )}
                  </div>
                  <div className="date-clip-info">
                    <div className="date-clip-title">{clip.title}</div>
                    {clip.recordedAt && (
                      <div className="date-clip-time">
                        {formatClipTime(clip.recordedAt)}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
            {hasMore && (
              <div className="date-load-more">
                <button
                  className="btn-secondary"
                  onClick={loadMore}
                  disabled={loadingMore}
                  title={`Load ${Math.min(PAGE_SIZE, total - clips.length)} more clips`}
                >
                  {loadingMore ? 'Loading...' : `Load More (${total - clips.length} remaining)`}
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
