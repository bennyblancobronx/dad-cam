// Dad Cam - VHS Export Dialog
import { useState, useEffect, useCallback } from 'react';
import { listen } from '@tauri-apps/api/event';
import { save } from '@tauri-apps/plugin-dialog';
import type { SelectionMode, ExportOrdering, ExportHistoryEntry } from '../types/export';
import type { JobProgress } from '../types/jobs';
import { startVhsExport, getExportHistory, cancelExport } from '../api/export';
import { getAppSettings } from '../api/settings';
import { getEvents } from '../api/events';
import type { EventView } from '../types/events';
import { ExportHistory } from './ExportHistory';

interface ExportDialogProps {
  libraryPath: string;
  mode: 'simple' | 'advanced';
  onClose: () => void;
}

export function ExportDialog({ libraryPath, mode, onClose }: ExportDialogProps) {
  // Form state
  const [selectionMode, setSelectionMode] = useState<SelectionMode>('all');
  const [ordering, setOrdering] = useState<ExportOrdering>('chronological');
  const [titleText, setTitleText] = useState('');
  const [outputPath, setOutputPath] = useState('');

  // Date range params
  const [dateFrom, setDateFrom] = useState('');
  const [dateTo, setDateTo] = useState('');

  // Score params (advanced only)
  const [scoreThreshold, setScoreThreshold] = useState(0.6);

  // Event params
  const [eventId, setEventId] = useState('');

  // Export state
  const [isExporting, setIsExporting] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  // History
  const [history, setHistory] = useState<ExportHistoryEntry[]>([]);

  // Events list for dropdown
  const [events, setEvents] = useState<EventView[]>([]);

  // Dev menu values (loaded from settings)
  const [blendDurationMs, setBlendDurationMs] = useState(500);
  const [titleStartSeconds, setTitleStartSeconds] = useState(5);

  // Load history, events, and dev menu settings on mount
  useEffect(() => {
    getExportHistory(10).then(setHistory).catch(() => {});
    getEvents().then(setEvents).catch(() => {});
    getAppSettings().then((settings) => {
      if (settings.devMenu) {
        if (typeof settings.devMenu.jlBlendMs === 'number') {
          setBlendDurationMs(settings.devMenu.jlBlendMs);
        }
        if (typeof settings.devMenu.titleStartSeconds === 'number') {
          setTitleStartSeconds(settings.devMenu.titleStartSeconds);
        }
      }
    }).catch(() => {});
  }, []);

  // Listen for progress events
  useEffect(() => {
    if (!jobId) return;

    const unlisten = listen<JobProgress>('job-progress', (event) => {
      if (event.payload.jobId === jobId) {
        setProgress(event.payload);

        if (event.payload.phase === 'complete') {
          setIsExporting(false);
          setJobId(null);
          // Refresh history
          getExportHistory(10).then(setHistory).catch(() => {});
        }

        if (event.payload.isError) {
          setIsExporting(false);
          setError(event.payload.errorMessage || 'Export failed');
        }

        if (event.payload.isCancelled) {
          setIsExporting(false);
          setJobId(null);
        }
      }
    });

    return () => {
      unlisten.then(fn => fn());
    };
  }, [jobId]);

  // Pick output path
  const handlePickOutput = useCallback(async () => {
    const path = await save({
      title: 'Save VHS Export',
      defaultPath: 'export.mp4',
      filters: [{ name: 'MP4 Video', extensions: ['mp4'] }],
    });
    if (path) {
      setOutputPath(path);
    }
  }, []);

  // Start export
  const handleExport = useCallback(async () => {
    if (!outputPath) {
      setError('Choose an output location first');
      return;
    }

    setError(null);
    setIsExporting(true);
    setProgress(null);

    // Build selection params
    const selectionParams: Record<string, unknown> = {};
    if (selectionMode === 'date_range') {
      selectionParams.dateFrom = dateFrom;
      selectionParams.dateTo = dateTo;
    } else if (selectionMode === 'event') {
      selectionParams.eventId = parseInt(eventId, 10) || 0;
    } else if (selectionMode === 'score') {
      selectionParams.threshold = scoreThreshold;
    }

    try {
      const id = await startVhsExport({
        selectionMode,
        selectionParams,
        ordering,
        titleText: titleText || null,
        outputPath,
        libraryPath,
        blendDurationMs,
        titleStartSeconds,
      });
      setJobId(id);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Export failed');
      setIsExporting(false);
    }
  }, [outputPath, selectionMode, ordering, titleText, libraryPath, dateFrom, dateTo, eventId, scoreThreshold]);

  // Cancel export
  const handleCancel = useCallback(async () => {
    if (jobId) {
      await cancelExport(jobId);
    }
  }, [jobId]);

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !isExporting) {
        onClose();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [isExporting, onClose]);

  return (
    <div className="modal-backdrop" onClick={!isExporting ? onClose : undefined}>
      <div className="modal export-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="modal-header">
          <h2 className="modal-title">VHS Export</h2>
          {!isExporting && (
            <button className="modal-close" onClick={onClose} title="Close (Escape)">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M4 4l8 8M12 4l-8 8" />
              </svg>
            </button>
          )}
        </div>

        <div className="modal-body">
          {/* Selection Mode */}
          <div className="form-group">
            <label className="form-label">CLIP SELECTION</label>
            <select
              value={selectionMode}
              onChange={(e) => setSelectionMode(e.target.value as SelectionMode)}
              disabled={isExporting}
              className="form-select"
            >
              <option value="all">All Clips</option>
              <option value="favorites">Favorites Only</option>
              <option value="date_range">Date Range</option>
              <option value="event">Event</option>
              {mode === 'advanced' && <option value="score">Score Threshold</option>}
            </select>
          </div>

          {/* Date range sub-controls */}
          {selectionMode === 'date_range' && (
            <div className="form-group form-row">
              <div>
                <label className="form-label">FROM</label>
                <input
                  type="date"
                  value={dateFrom}
                  onChange={(e) => setDateFrom(e.target.value)}
                  disabled={isExporting}
                  className="form-input"
                />
              </div>
              <div>
                <label className="form-label">TO</label>
                <input
                  type="date"
                  value={dateTo}
                  onChange={(e) => setDateTo(e.target.value)}
                  disabled={isExporting}
                  className="form-input"
                />
              </div>
            </div>
          )}

          {/* Event selector */}
          {selectionMode === 'event' && (
            <div className="form-group">
              <label className="form-label">EVENT</label>
              <select
                value={eventId}
                onChange={(e) => setEventId(e.target.value)}
                disabled={isExporting}
                className="form-select"
              >
                <option value="">-- Select an event --</option>
                {events.map((ev) => (
                  <option key={ev.id} value={String(ev.id)}>{ev.name}</option>
                ))}
              </select>
            </div>
          )}

          {/* Score threshold (advanced only) */}
          {selectionMode === 'score' && (
            <div className="form-group">
              <label className="form-label">MINIMUM SCORE: {(scoreThreshold * 100).toFixed(0)}%</label>
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={scoreThreshold}
                onChange={(e) => setScoreThreshold(parseFloat(e.target.value))}
                disabled={isExporting}
                className="form-range"
              />
            </div>
          )}

          {/* Ordering */}
          <div className="form-group">
            <label className="form-label">ORDER</label>
            <select
              value={ordering}
              onChange={(e) => setOrdering(e.target.value as ExportOrdering)}
              disabled={isExporting}
              className="form-select"
            >
              <option value="chronological">Chronological</option>
              <option value="score_desc">Best First</option>
              <option value="score_asc">Worst First</option>
              <option value="shuffle">Shuffle</option>
            </select>
          </div>

          {/* Title text */}
          <div className="form-group">
            <label className="form-label">TITLE OVERLAY (optional)</label>
            <input
              type="text"
              value={titleText}
              onChange={(e) => setTitleText(e.target.value)}
              disabled={isExporting}
              className="form-input"
              placeholder="Text shown at start of video"
            />
          </div>

          {/* Output path */}
          <div className="form-group">
            <label className="form-label">OUTPUT FILE</label>
            <div className="form-row">
              <input
                type="text"
                value={outputPath}
                readOnly
                className="form-input"
                placeholder="Choose save location..."
                style={{ flex: 1 }}
              />
              <button
                onClick={handlePickOutput}
                disabled={isExporting}
                className="secondary-button"
                style={{ padding: '6px 12px' }}
              >
                Browse
              </button>
            </div>
          </div>

          {/* Error */}
          {error && (
            <div className="error-message" style={{ marginTop: '8px' }}>
              {error}
            </div>
          )}

          {/* Progress */}
          {isExporting && progress && (
            <div className="export-progress" style={{ marginTop: '12px' }}>
              <div className="progress-bar-track">
                <div
                  className="progress-bar-fill"
                  style={{ width: `${progress.percent}%` }}
                />
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginTop: '4px', fontSize: '13px', color: 'var(--color-text-secondary)' }}>
                <span>{progress.message}</span>
                <span>{Math.round(progress.percent)}%</span>
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="modal-footer" style={{ marginTop: '16px' }}>
            {isExporting ? (
              <button onClick={handleCancel} className="secondary-button">
                Cancel Export
              </button>
            ) : (
              <>
                <button onClick={onClose} className="secondary-button">
                  Close
                </button>
                <button
                  onClick={handleExport}
                  disabled={!outputPath}
                  className="primary-button"
                >
                  Export
                </button>
              </>
            )}
          </div>

          {/* Export History */}
          {!isExporting && <ExportHistory history={history} />}
        </div>
      </div>
    </div>
  );
}

