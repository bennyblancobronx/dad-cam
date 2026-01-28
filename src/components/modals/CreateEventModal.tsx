// Dad Cam - Create Event Modal (Phase 6)
// Modal for creating a new event

import { useState, useEffect } from 'react';
import { createEvent } from '../../api/events';
import { EVENT_TYPES, type EventType } from '../../types/events';

interface CreateEventModalProps {
  onClose: () => void;
  onCreated: () => void;
}

export function CreateEventModal({ onClose, onCreated }: CreateEventModalProps) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [eventType, setEventType] = useState<EventType>(EVENT_TYPES.DATE_RANGE);
  const [dateStart, setDateStart] = useState('');
  const [dateEnd, setDateEnd] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!name.trim()) {
      setError('Event name is required');
      return;
    }

    if (eventType === EVENT_TYPES.DATE_RANGE) {
      if (!dateStart || !dateEnd) {
        setError('Start and end dates are required for date range events');
        return;
      }
      if (dateStart > dateEnd) {
        setError('Start date must be before end date');
        return;
      }
    }

    try {
      setLoading(true);
      setError(null);

      await createEvent(name.trim(), eventType, {
        description: description.trim() || undefined,
        dateStart: eventType === EVENT_TYPES.DATE_RANGE ? dateStart : undefined,
        dateEnd: eventType === EVENT_TYPES.DATE_RANGE ? dateEnd : undefined,
      });

      onCreated();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create event');
    } finally {
      setLoading(false);
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
      if (e.key === 'Escape' && !loading) {
        onClose();
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, loading]);

  return (
    <div className="modal-backdrop" onClick={handleBackdropClick}>
      <div className="modal create-event-modal">
        <div className="modal-header">
          <h2 className="modal-title">Create Event</h2>
          <button className="modal-close" onClick={onClose} title="Close (Escape)">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 5L5 15M5 5l10 10" />
            </svg>
          </button>
        </div>

        <form onSubmit={handleSubmit}>
          <div className="modal-body">
            {error && <div className="error-message">{error}</div>}

            <div className="input-group">
              <label htmlFor="event-name">Event Name</label>
              <input
                id="event-name"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="Vacation 2025"
                disabled={loading}
                autoFocus
              />
            </div>

            <div className="input-group">
              <label htmlFor="event-description">Description (optional)</label>
              <input
                id="event-description"
                type="text"
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="Family trip to the beach"
                disabled={loading}
              />
            </div>

            <div className="input-group">
              <label>Event Type</label>
              <div className="event-type-options">
                <label className={`event-type-option ${eventType === EVENT_TYPES.DATE_RANGE ? 'is-selected' : ''}`}>
                  <input
                    type="radio"
                    name="eventType"
                    value={EVENT_TYPES.DATE_RANGE}
                    checked={eventType === EVENT_TYPES.DATE_RANGE}
                    onChange={() => setEventType(EVENT_TYPES.DATE_RANGE)}
                    disabled={loading}
                  />
                  <div className="event-type-content">
                    <span className="event-type-title">Date Range</span>
                    <span className="event-type-desc">Include all clips recorded between two dates</span>
                  </div>
                </label>
                <label className={`event-type-option ${eventType === EVENT_TYPES.CLIP_SELECTION ? 'is-selected' : ''}`}>
                  <input
                    type="radio"
                    name="eventType"
                    value={EVENT_TYPES.CLIP_SELECTION}
                    checked={eventType === EVENT_TYPES.CLIP_SELECTION}
                    onChange={() => setEventType(EVENT_TYPES.CLIP_SELECTION)}
                    disabled={loading}
                  />
                  <div className="event-type-content">
                    <span className="event-type-title">Manual Selection</span>
                    <span className="event-type-desc">Manually select which clips to include</span>
                  </div>
                </label>
              </div>
            </div>

            {eventType === EVENT_TYPES.DATE_RANGE && (
              <div className="date-range-inputs">
                <div className="input-group">
                  <label htmlFor="date-start">Start Date</label>
                  <input
                    id="date-start"
                    type="date"
                    value={dateStart}
                    onChange={(e) => setDateStart(e.target.value)}
                    disabled={loading}
                  />
                </div>
                <div className="input-group">
                  <label htmlFor="date-end">End Date</label>
                  <input
                    id="date-end"
                    type="date"
                    value={dateEnd}
                    onChange={(e) => setDateEnd(e.target.value)}
                    disabled={loading}
                  />
                </div>
              </div>
            )}
          </div>

          <div className="modal-footer">
            <button
              type="button"
              className="secondary-button"
              onClick={onClose}
              disabled={loading}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="primary-button"
              disabled={loading || !name.trim()}
            >
              {loading ? 'Creating...' : 'Create Event'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
