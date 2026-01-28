// Dad Cam - Edit Event Modal (Phase 6)
// Modal for editing an existing event

import { useState, useEffect } from 'react';
import { updateEvent } from '../../api/events';
import { EVENT_TYPES, type EventView } from '../../types/events';

interface EditEventModalProps {
  event: EventView;
  onClose: () => void;
  onUpdated: () => void;
}

export function EditEventModal({ event, onClose, onUpdated }: EditEventModalProps) {
  const [name, setName] = useState(event.name);
  const [description, setDescription] = useState(event.description || '');
  const [dateStart, setDateStart] = useState(event.dateStart || '');
  const [dateEnd, setDateEnd] = useState(event.dateEnd || '');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isDateRangeEvent = event.eventType === EVENT_TYPES.DATE_RANGE;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!name.trim()) {
      setError('Event name is required');
      return;
    }

    if (isDateRangeEvent) {
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

      await updateEvent(event.id, {
        name: name.trim(),
        description: description.trim() || undefined,
        dateStart: isDateRangeEvent ? dateStart : undefined,
        dateEnd: isDateRangeEvent ? dateEnd : undefined,
      });

      onUpdated();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update event');
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
      <div className="modal edit-event-modal">
        <div className="modal-header">
          <h2 className="modal-title">Edit Event</h2>
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
              <div className="event-type-display">
                {isDateRangeEvent ? 'Date Range' : 'Manual Selection'}
              </div>
            </div>

            {isDateRangeEvent && (
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
              {loading ? 'Saving...' : 'Save Changes'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
