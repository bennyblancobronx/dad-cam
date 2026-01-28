// Dad Cam - Add to Event Modal (Phase 6)
// Modal for selecting which event to add clips to

import { useState, useEffect, useCallback } from 'react';
import type { EventView } from '../../types/events';
import { getEvents, addClipsToEvent } from '../../api/events';

interface AddToEventModalProps {
  clipIds: number[];
  onClose: () => void;
  onAdded: () => void;
}

export function AddToEventModal({ clipIds, onClose, onAdded }: AddToEventModalProps) {
  const [events, setEvents] = useState<EventView[]>([]);
  const [loading, setLoading] = useState(true);
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedEventId, setSelectedEventId] = useState<number | null>(null);

  const loadEvents = useCallback(async () => {
    try {
      setLoading(true);
      const eventsList = await getEvents();
      // Filter to only show clip_selection events (can't add to date_range)
      const selectionEvents = eventsList.filter(e => e.eventType === 'clip_selection');
      setEvents(selectionEvents);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load events');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadEvents();
  }, [loadEvents]);

  const handleAdd = async () => {
    if (!selectedEventId) return;

    try {
      setAdding(true);
      setError(null);
      await addClipsToEvent(selectedEventId, clipIds);
      onAdded();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to add clips to event');
    } finally {
      setAdding(false);
    }
  };

  const handleBackdropClick = (e: React.MouseEvent) => {
    if (e.target === e.currentTarget) {
      onClose();
    }
  };

  // Handle Escape key
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !adding) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, adding]);

  return (
    <div className="modal-backdrop" onClick={handleBackdropClick}>
      <div className="modal add-to-event-modal">
        <div className="modal-header">
          <h2 className="modal-title">Add to Event</h2>
          <button className="modal-close" onClick={onClose} title="Close (Escape)">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 5L5 15M5 5l10 10" />
            </svg>
          </button>
        </div>

        <div className="modal-body">
          {error && <div className="error-message">{error}</div>}

          <p className="add-to-event-count">
            {clipIds.length} clip{clipIds.length !== 1 ? 's' : ''} selected
          </p>

          {loading ? (
            <div className="add-to-event-loading">Loading events...</div>
          ) : events.length === 0 ? (
            <div className="add-to-event-empty">
              <p>No manual selection events available.</p>
              <p className="add-to-event-hint">
                Create a "Manual Selection" event first, then add clips to it.
              </p>
            </div>
          ) : (
            <div className="event-list">
              {events.map((event) => (
                <label
                  key={event.id}
                  className={`event-list-item ${selectedEventId === event.id ? 'is-selected' : ''}`}
                >
                  <input
                    type="radio"
                    name="event"
                    value={event.id}
                    checked={selectedEventId === event.id}
                    onChange={() => setSelectedEventId(event.id)}
                    disabled={adding}
                  />
                  <span
                    className="event-list-color"
                    style={{ backgroundColor: event.color }}
                  />
                  <span className="event-list-name">{event.name}</span>
                  <span className="event-list-count">{event.clipCount} clips</span>
                </label>
              ))}
            </div>
          )}
        </div>

        <div className="modal-footer">
          <button
            type="button"
            className="secondary-button"
            onClick={onClose}
            disabled={adding}
          >
            Cancel
          </button>
          <button
            type="button"
            className="primary-button"
            onClick={handleAdd}
            disabled={adding || !selectedEventId || events.length === 0}
          >
            {adding ? 'Adding...' : 'Add to Event'}
          </button>
        </div>
      </div>
    </div>
  );
}
