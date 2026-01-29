// Dad Cam - Import Dialog
// Modal with setup, progress, and summary phases

import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { getEvents } from '../api/events';
import { cancelJob } from '../api/jobs';
import type { EventView } from '../types/events';
import type { JobProgress, IngestResponse } from '../types/jobs';

type DialogPhase = 'setup' | 'progress' | 'summary';

interface ImportDialogProps {
  libraryPath: string;
  /** Whether to show camera breakdown in summary */
  showCameraBreakdown?: boolean;
  onClose: () => void;
  onComplete: () => void;
}

export function ImportDialog({
  libraryPath,
  showCameraBreakdown = false,
  onClose,
  onComplete,
}: ImportDialogProps) {
  const [phase, setPhase] = useState<DialogPhase>('setup');

  // Setup phase state
  const [sourcePath, setSourcePath] = useState<string | null>(null);
  const [events, setEvents] = useState<EventView[]>([]);
  const [eventChoice, setEventChoice] = useState<'none' | 'existing' | 'new'>('none');
  const [selectedEventId, setSelectedEventId] = useState<number | null>(null);
  const [newEventName, setNewEventName] = useState('');

  // Progress phase state
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);

  // Summary phase state
  const [result, setResult] = useState<IngestResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const unlistenRef = useRef<(() => void) | null>(null);

  // Load events on mount
  useEffect(() => {
    getEvents()
      .then(setEvents)
      .catch(() => setEvents([]));
  }, []);

  // Cleanup progress listener on unmount
  useEffect(() => {
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, []);

  // Pick folder
  const handlePickFolder = useCallback(async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select Folder to Import',
    });
    if (selected) {
      setSourcePath(selected as string);
    }
  }, []);

  // Start import
  const handleStartImport = useCallback(async () => {
    if (!sourcePath) return;

    setPhase('progress');
    setError(null);

    // Listen for progress events
    const unlisten = await listen<JobProgress>('job-progress', (event) => {
      setProgress(event.payload);
      if (event.payload.jobId && !jobId) {
        setJobId(event.payload.jobId);
      }
    });
    unlistenRef.current = unlisten;

    try {
      const response = await invoke<IngestResponse>('start_ingest', {
        sourcePath,
        libraryPath,
        eventId: eventChoice === 'existing' ? selectedEventId : null,
        newEventName: eventChoice === 'new' && newEventName.trim() ? newEventName.trim() : null,
      });

      setResult(response);
      setPhase('summary');
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Import failed');
      setPhase('summary');
    } finally {
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
    }
  }, [sourcePath, libraryPath, eventChoice, selectedEventId, newEventName, jobId]);

  // Cancel import
  const handleCancel = useCallback(async () => {
    if (jobId) {
      try {
        await cancelJob(jobId);
      } catch {
        // Ignore cancel errors
      }
    }
  }, [jobId]);

  // Done - close dialog and notify parent
  const handleDone = useCallback(() => {
    onComplete();
    onClose();
  }, [onComplete, onClose]);

  // Folder display name
  const folderName = sourcePath
    ? sourcePath.split(/[/\\]/).filter(Boolean).pop() || sourcePath
    : null;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal-content"
        style={{ maxWidth: '480px', width: '100%' }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="modal-header">
          <h2 className="modal-title">Import Footage</h2>
          {phase === 'setup' && (
            <button className="modal-close" onClick={onClose}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M4 4l8 8M12 4l-8 8" />
              </svg>
            </button>
          )}
        </div>

        {/* Setup Phase */}
        {phase === 'setup' && (
          <div className="modal-body" style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            {/* Folder picker */}
            <div>
              <label style={{ display: 'block', marginBottom: '6px', fontSize: '14px', color: 'var(--color-text-secondary)' }}>
                Source Folder
              </label>
              <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
                <button onClick={handlePickFolder} className="secondary-button" style={{ padding: '8px 14px', whiteSpace: 'nowrap' }}>
                  Choose Folder
                </button>
                {folderName && (
                  <span style={{ fontSize: '14px', color: 'var(--color-text)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {folderName}
                  </span>
                )}
              </div>
            </div>

            {/* Event assignment */}
            <div>
              <label style={{ display: 'block', marginBottom: '6px', fontSize: '14px', color: 'var(--color-text-secondary)' }}>
                Assign to Event
              </label>
              <select
                value={eventChoice === 'existing' ? `existing-${selectedEventId}` : eventChoice}
                onChange={(e) => {
                  const val = e.target.value;
                  if (val === 'none') {
                    setEventChoice('none');
                    setSelectedEventId(null);
                  } else if (val === 'new') {
                    setEventChoice('new');
                    setSelectedEventId(null);
                  } else if (val.startsWith('existing-')) {
                    setEventChoice('existing');
                    setSelectedEventId(parseInt(val.replace('existing-', ''), 10));
                  }
                }}
                style={{
                  width: '100%',
                  padding: '8px 12px',
                  borderRadius: '6px',
                  border: '1px solid var(--color-border)',
                  background: 'var(--color-bg-secondary)',
                  color: 'var(--color-text)',
                  fontSize: '14px',
                }}
              >
                <option value="none">No event</option>
                {events.map((ev) => (
                  <option key={ev.id} value={`existing-${ev.id}`}>
                    {ev.name}
                  </option>
                ))}
                <option value="new">Create new event...</option>
              </select>

              {/* New event name input */}
              {eventChoice === 'new' && (
                <input
                  type="text"
                  placeholder="Event name"
                  value={newEventName}
                  onChange={(e) => setNewEventName(e.target.value)}
                  autoFocus
                  style={{
                    width: '100%',
                    marginTop: '8px',
                    padding: '8px 12px',
                    borderRadius: '6px',
                    border: '1px solid var(--color-border)',
                    background: 'var(--color-bg-secondary)',
                    color: 'var(--color-text)',
                    fontSize: '14px',
                    boxSizing: 'border-box',
                  }}
                />
              )}
            </div>

            {/* Start button */}
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '8px', marginTop: '8px' }}>
              <button className="secondary-button" style={{ padding: '8px 16px' }} onClick={onClose}>
                Cancel
              </button>
              <button
                className="welcome-btn welcome-btn-primary"
                style={{ padding: '8px 20px', fontSize: '14px' }}
                disabled={!sourcePath || (eventChoice === 'new' && !newEventName.trim())}
                onClick={handleStartImport}
              >
                Start Import
              </button>
            </div>
          </div>
        )}

        {/* Progress Phase */}
        {phase === 'progress' && (
          <div className="modal-body" style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
            <div style={{ fontSize: '14px', color: 'var(--color-text-secondary)' }}>
              {progress?.message || 'Starting import...'}
            </div>

            {/* Progress bar */}
            <div style={{ width: '100%', height: '8px', borderRadius: '4px', background: 'var(--color-bg-tertiary)', overflow: 'hidden' }}>
              <div
                style={{
                  width: `${progress?.percent || 0}%`,
                  height: '100%',
                  borderRadius: '4px',
                  background: 'var(--color-primary)',
                  transition: 'width 0.3s ease',
                }}
              />
            </div>

            {/* Count */}
            <div style={{ fontSize: '13px', color: 'var(--color-text-secondary)', textAlign: 'center' }}>
              {progress ? `${progress.current} / ${progress.total}` : '...'}
            </div>

            {/* Cancel */}
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <button className="secondary-button" style={{ padding: '8px 16px' }} onClick={handleCancel}>
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Summary Phase */}
        {phase === 'summary' && (
          <div className="modal-body" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
            {error ? (
              <div style={{ color: 'var(--color-error)', fontSize: '14px' }}>
                {error}
              </div>
            ) : result ? (
              <>
                <div style={{ fontSize: '16px', fontWeight: 600 }}>
                  {result.processed} clip{result.processed !== 1 ? 's' : ''} imported
                </div>
                {(result.skipped > 0 || result.failed > 0) && (
                  <div style={{ fontSize: '14px', color: 'var(--color-text-secondary)' }}>
                    {result.skipped > 0 && <span>{result.skipped} skipped (duplicates)</span>}
                    {result.skipped > 0 && result.failed > 0 && <span> / </span>}
                    {result.failed > 0 && <span style={{ color: 'var(--color-error)' }}>{result.failed} failed</span>}
                  </div>
                )}

                {/* Event assignment info */}
                {eventChoice !== 'none' && result.processed > 0 && (
                  <div style={{ fontSize: '14px', color: 'var(--color-text-secondary)' }}>
                    Added to event: {eventChoice === 'new' ? newEventName : events.find(e => e.id === selectedEventId)?.name}
                  </div>
                )}

                {/* Camera breakdown (Advanced mode only) */}
                {showCameraBreakdown && result.cameraBreakdown.length > 0 && (
                  <div style={{ marginTop: '4px' }}>
                    <div style={{ fontSize: '13px', fontWeight: 500, color: 'var(--color-text-secondary)', marginBottom: '4px' }}>
                      Camera breakdown
                    </div>
                    {result.cameraBreakdown.map((cam) => (
                      <div
                        key={cam.name}
                        style={{
                          display: 'flex',
                          justifyContent: 'space-between',
                          fontSize: '13px',
                          padding: '2px 0',
                          color: 'var(--color-text-secondary)',
                        }}
                      >
                        <span>{cam.name}</span>
                        <span>{cam.count}</span>
                      </div>
                    ))}
                  </div>
                )}
              </>
            ) : (
              <div style={{ fontSize: '14px', color: 'var(--color-text-secondary)' }}>
                Import completed.
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: '8px' }}>
              <button
                className="welcome-btn welcome-btn-primary"
                style={{ padding: '8px 20px', fontSize: '14px' }}
                onClick={handleDone}
              >
                Done
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
