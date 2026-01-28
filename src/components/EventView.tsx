// Dad Cam - Event View (Phase 6)
// Display clips for a specific event

import { useState, useEffect, useCallback } from 'react';
import type { EventView as EventViewType, EventClipView } from '../types/events';
import { getEvent, getEventClips, removeClipsFromEvent } from '../api/events';
import { formatDateRange, isDateRangeEvent, formatClipDate } from '../types/events';
import { toAssetUrl } from '../utils/paths';
import { EditEventModal } from './modals/EditEventModal';

function formatDuration(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

interface EventViewProps {
  eventId: number;
  onBack: () => void;
  /** Called when user clicks a clip - passes full clip data */
  onClipSelect: (clip: EventClipView) => void;
}

export function EventView({
  eventId,
  onBack,
  onClipSelect,
}: EventViewProps) {
  const [event, setEvent] = useState<EventViewType | null>(null);
  const [clips, setClips] = useState<EventClipView[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedClips, setSelectedClips] = useState<Set<number>>(new Set());
  const [showEditModal, setShowEditModal] = useState(false);
  const [removing, setRemoving] = useState(false);

  const loadEvent = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const eventData = await getEvent(eventId);
      setEvent(eventData);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load event');
    }
  }, [eventId]);

  const loadClips = useCallback(async () => {
    try {
      const response = await getEventClips(eventId, 0, 100);
      setClips(response.clips);
      setTotal(response.total);
    } catch (err) {
      console.error('Failed to load event clips:', err);
    } finally {
      setLoading(false);
    }
  }, [eventId]);

  useEffect(() => {
    loadEvent();
    loadClips();
  }, [loadEvent, loadClips]);

  const handleClipClick = (clip: EventClipView) => {
    if (selectionMode) {
      setSelectedClips((prev) => {
        const next = new Set(prev);
        if (next.has(clip.id)) {
          next.delete(clip.id);
        } else {
          next.add(clip.id);
        }
        return next;
      });
    } else {
      onClipSelect(clip);
    }
  };

  const handleRemoveSelected = async () => {
    if (selectedClips.size === 0 || removing) return;

    try {
      setRemoving(true);
      await removeClipsFromEvent(eventId, Array.from(selectedClips));
      setSelectedClips(new Set());
      setSelectionMode(false);
      await loadEvent();
      await loadClips();
    } catch (err) {
      console.error('Failed to remove clips:', err);
    } finally {
      setRemoving(false);
    }
  };

  const handleCancelSelection = () => {
    setSelectionMode(false);
    setSelectedClips(new Set());
  };

  const handleEventUpdated = async () => {
    await loadEvent();
  };

  if (error) {
    return (
      <div className="event-view">
        <div className="event-view-header">
          <button className="back-button" onClick={onBack} title="Back to library">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 10H5M5 10l5 5M5 10l5-5" />
            </svg>
            Back
          </button>
        </div>
        <div className="event-view-error">
          <p>{error}</p>
        </div>
      </div>
    );
  }

  if (loading || !event) {
    return (
      <div className="event-view">
        <div className="event-view-header">
          <button className="back-button" onClick={onBack} title="Back to library">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 10H5M5 10l5 5M5 10l5-5" />
            </svg>
            Back
          </button>
        </div>
        <div className="event-view-content" style={{ padding: 'var(--space-6)' }}>
          <div className="event-clip-grid">
            {Array.from({ length: 6 }).map((_, i) => (
              <div key={i} className="event-clip-item" style={{ pointerEvents: 'none' }}>
                <div className="skeleton skeleton-card" />
                <div className="event-clip-info">
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
    <div className="event-view">
      <div className="event-view-header">
        <div className="event-view-header-left">
          <button className="back-button" onClick={onBack} title="Back to library">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 10H5M5 10l5 5M5 10l5-5" />
            </svg>
            Back
          </button>
          <div className="event-view-info">
            <h1 className="event-view-title">{event.name}</h1>
            <div className="event-view-meta">
              <span className="event-view-clip-count">{total} clips</span>
              {isDateRangeEvent(event) && event.dateStart && event.dateEnd && (
                <>
                  <span className="event-view-separator">|</span>
                  <span className="event-view-dates">
                    {formatDateRange(event.dateStart, event.dateEnd)}
                  </span>
                </>
              )}
            </div>
          </div>
        </div>
        <div className="event-view-header-right">
          {selectionMode ? (
            <>
              <span className="selection-count">{selectedClips.size} selected</span>
              <button
                className="secondary-button"
                onClick={handleRemoveSelected}
                disabled={selectedClips.size === 0 || removing}
                title="Remove selected clips from this event"
              >
                {removing ? 'Removing...' : 'Remove from Event'}
              </button>
              <button className="secondary-button" onClick={handleCancelSelection} disabled={removing} title="Cancel selection">
                Cancel
              </button>
            </>
          ) : (
            <>
              <button
                className="secondary-button"
                onClick={() => setShowEditModal(true)}
                title="Edit event name and details"
              >
                Edit
              </button>
              <button
                className="secondary-button"
                onClick={() => setSelectionMode(true)}
                disabled={clips.length === 0}
                title="Select clips to remove from event"
              >
                Select Clips
              </button>
            </>
          )}
        </div>
      </div>

      {event.description && (
        <div className="event-view-description">
          <p>{event.description}</p>
        </div>
      )}

      <div className="event-view-content">
        {clips.length === 0 ? (
          <div className="event-empty-state">
            <svg width="64" height="64" viewBox="0 0 64 64" fill="none" stroke="currentColor" strokeWidth="1.5">
              <rect x="8" y="12" width="48" height="40" rx="4" />
              <path d="M8 24h48M20 12v-4M44 12v-4" />
            </svg>
            <h3>No clips in this event</h3>
            <p>
              {isDateRangeEvent(event)
                ? 'No clips were recorded during this date range.'
                : 'Add clips to this event using the selection tool.'}
            </p>
          </div>
        ) : (
          <div className="event-clip-grid">
            {clips.map((clip) => (
              <div
                key={clip.id}
                className={`event-clip-item ${selectionMode ? 'selection-mode' : ''} ${
                  selectedClips.has(clip.id) ? 'is-selected' : ''
                }`}
                onClick={() => handleClipClick(clip)}
              >
                {selectionMode && (
                  <div className="clip-checkbox">
                    <input
                      type="checkbox"
                      checked={selectedClips.has(clip.id)}
                      onChange={() => {}}
                      tabIndex={-1}
                    />
                  </div>
                )}
                <div className="event-clip-thumbnail">
                  {clip.thumbnailPath ? (
                    <img
                      src={toAssetUrl(clip.thumbnailPath) ?? undefined}
                      alt={clip.title}
                      className="event-clip-image"
                    />
                  ) : (
                    <div className="event-clip-placeholder">No Preview</div>
                  )}
                  {clip.durationMs && (
                    <div className="event-clip-duration">
                      {formatDuration(clip.durationMs)}
                    </div>
                  )}
                </div>
                <div className="event-clip-info">
                  <div className="event-clip-title">{clip.title}</div>
                  {clip.recordedAt && (
                    <div className="event-clip-date">
                      {formatClipDate(clip.recordedAt)}
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Edit Event Modal */}
      {showEditModal && event && (
        <EditEventModal
          event={event}
          onClose={() => setShowEditModal(false)}
          onUpdated={handleEventUpdated}
        />
      )}
    </div>
  );
}
