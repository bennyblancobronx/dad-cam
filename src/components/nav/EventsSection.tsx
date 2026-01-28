// Dad Cam - Left Nav Events Section (Phase 6)
// Shows events list with create event button

import { useState, useEffect, useCallback } from 'react';
import type { EventView } from '../../types/events';
import { getEvents, deleteEvent } from '../../api/events';
import { CreateEventModal } from '../modals/CreateEventModal';
import { EditEventModal } from '../modals/EditEventModal';

interface EventsSectionProps {
  onNavigateToEvent?: (eventId: number) => void;
}

export function EventsSection({ onNavigateToEvent }: EventsSectionProps) {
  const [events, setEvents] = useState<EventView[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [editingEvent, setEditingEvent] = useState<EventView | null>(null);
  const [contextMenuEvent, setContextMenuEvent] = useState<number | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<EventView | null>(null);
  const [deleting, setDeleting] = useState(false);

  const loadEvents = useCallback(async () => {
    try {
      setLoading(true);
      const eventsList = await getEvents();
      setEvents(eventsList);
    } catch (err) {
      console.error('Failed to load events:', err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadEvents();
  }, [loadEvents]);

  const handleEventClick = (eventId: number) => {
    if (onNavigateToEvent) {
      onNavigateToEvent(eventId);
    }
  };

  const handleContextMenu = (e: React.MouseEvent, eventId: number) => {
    e.preventDefault();
    setContextMenuEvent(contextMenuEvent === eventId ? null : eventId);
  };

  const handleDeleteClick = (event: EventView) => {
    setConfirmDelete(event);
    setContextMenuEvent(null);
  };

  const handleConfirmDelete = async () => {
    if (!confirmDelete) return;

    try {
      setDeleting(true);
      await deleteEvent(confirmDelete.id);
      await loadEvents();
      setConfirmDelete(null);
    } catch (err) {
      console.error('Failed to delete event:', err);
    } finally {
      setDeleting(false);
    }
  };

  const handleEventCreated = () => {
    loadEvents();
  };

  const handleEditEvent = (event: EventView) => {
    setEditingEvent(event);
    setContextMenuEvent(null);
  };

  const handleEventUpdated = () => {
    loadEvents();
  };

  // Close context menu when clicking elsewhere
  useEffect(() => {
    const handleClickOutside = () => setContextMenuEvent(null);
    document.addEventListener('click', handleClickOutside);
    return () => document.removeEventListener('click', handleClickOutside);
  }, []);

  return (
    <div className="nav-section">
      <div className="nav-section-header">
        <svg className="nav-section-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
          <rect x="3" y="4" width="14" height="13" rx="2" />
          <path d="M3 8h14M7 2v4M13 2v4" />
        </svg>
        <h3 className="nav-section-title">Events</h3>
      </div>

      <button
        className="nav-item nav-add-button"
        onClick={() => setShowCreateModal(true)}
        title="Create a new event to organize clips"
      >
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M8 3v10M3 8h10" />
        </svg>
        New Event
      </button>

      {loading ? (
        <div className="nav-loading">Loading...</div>
      ) : events.length === 0 ? (
        <div className="nav-empty">
          <span className="nav-empty-text">No events yet</span>
        </div>
      ) : (
        <div className="nav-events-list">
          {events.map((event) => (
            <div key={event.id} className="nav-event-item-wrapper">
              <button
                className="nav-item nav-event-item"
                onClick={() => handleEventClick(event.id)}
                onContextMenu={(e) => handleContextMenu(e, event.id)}
                title={`View ${event.name} (right-click for options)`}
              >
                <span
                  className="nav-event-color"
                  style={{ backgroundColor: event.color }}
                />
                <span className="nav-event-name">{event.name}</span>
                <span className="nav-event-count">{event.clipCount}</span>
              </button>
              {contextMenuEvent === event.id && (
                <div className="nav-context-menu" onClick={(e) => e.stopPropagation()}>
                  <button
                    className="nav-context-item"
                    onClick={() => handleEditEvent(event)}
                  >
                    Edit Event
                  </button>
                  <button
                    className="nav-context-item nav-context-delete"
                    onClick={() => handleDeleteClick(event)}
                  >
                    Delete Event
                  </button>
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {showCreateModal && (
        <CreateEventModal
          onClose={() => setShowCreateModal(false)}
          onCreated={handleEventCreated}
        />
      )}

      {editingEvent && (
        <EditEventModal
          event={editingEvent}
          onClose={() => setEditingEvent(null)}
          onUpdated={handleEventUpdated}
        />
      )}

      {/* Delete Confirmation Dialog */}
      {confirmDelete && (
        <>
          <div className="modal-backdrop" onClick={() => !deleting && setConfirmDelete(null)} />
          <div className="confirm-dialog">
            <h3 className="confirm-dialog-title">Delete Event?</h3>
            <p className="confirm-dialog-message">
              Are you sure you want to delete "{confirmDelete.name}"?
              {confirmDelete.clipCount > 0 && (
                <> This event contains {confirmDelete.clipCount} clip{confirmDelete.clipCount !== 1 ? 's' : ''}.</>
              )}
              {' '}This action cannot be undone.
            </p>
            <div className="confirm-dialog-actions">
              <button
                className="secondary-button"
                onClick={() => setConfirmDelete(null)}
                disabled={deleting}
              >
                Cancel
              </button>
              <button
                className="primary-button confirm-dialog-delete"
                onClick={handleConfirmDelete}
                disabled={deleting}
              >
                {deleting ? 'Deleting...' : 'Delete'}
              </button>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
