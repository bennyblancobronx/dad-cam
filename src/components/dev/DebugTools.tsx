// Dad Cam - Debug Tools (Dev Menu sub-component)

import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';

interface ToolStatus {
  name: string;
  available: boolean;
  path: string;
  version: string | null;
}

interface DebugToolsProps {
  showMessage: (msg: string) => void;
  showError: (msg: string) => void;
}

export function DebugTools({ showMessage, showError }: DebugToolsProps) {
  const [toolStatus, setToolStatus] = useState<ToolStatus[] | null>(null);
  const [dbStats, setDbStats] = useState<string | null>(null);
  const [sqlInput, setSqlInput] = useState('');
  const [sqlResult, setSqlResult] = useState<string | null>(null);
  const [clipIdInput, setClipIdInput] = useState('');

  // Log viewer state
  const [logs, setLogs] = useState<string[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);

  // Listen for job-progress events as a live log source
  useEffect(() => {
    let cleanup: (() => void) | undefined;

    listen<{ jobId: string; phase: string; message: string }>('job-progress', (event) => {
      const p = event.payload;
      const line = `[${new Date().toLocaleTimeString()}] ${p.phase}: ${p.message}`;
      setLogs((prev) => {
        const next = [...prev, line];
        // Keep last 200 lines
        return next.length > 200 ? next.slice(-200) : next;
      });
    }).then((unlisten) => {
      cleanup = unlisten;
    });

    return () => { cleanup?.(); };
  }, []);

  // Auto-scroll logs
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

  const handleTestTools = async () => {
    try {
      const status = await invoke<ToolStatus[]>('test_ffmpeg');
      setToolStatus(status);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to test tools');
    }
  };

  const handleClearCaches = async () => {
    try {
      const result = await invoke<string>('clear_caches');
      showMessage(result);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to clear caches');
    }
  };

  const handleDbStats = async () => {
    try {
      const stats = await invoke<string>('get_db_stats');
      setDbStats(stats);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to get stats');
    }
  };

  const handleExportDb = async () => {
    try {
      const outputPath = await save({
        title: 'Export Database',
        defaultPath: 'dadcam.db',
        filters: [{ name: 'SQLite Database', extensions: ['db'] }],
      });
      if (!outputPath) return;
      await invoke('export_database', { outputPath });
      showMessage(`Database exported to ${outputPath}`);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to export database');
    }
  };

  const handleExportExif = async () => {
    const clipId = parseInt(clipIdInput);
    if (isNaN(clipId) || clipId <= 0) {
      showError('Enter a valid clip ID');
      return;
    }

    try {
      const outputPath = await save({
        title: 'Export EXIF Dump',
        defaultPath: `clip_${clipId}_exif.json`,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!outputPath) return;
      await invoke('export_exif_dump', { clipId, outputPath });
      showMessage(`EXIF data exported to ${outputPath}`);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to export EXIF data');
    }
  };

  const handleRunSql = async () => {
    if (!sqlInput.trim()) return;
    try {
      const result = await invoke<string>('execute_raw_sql', { sql: sqlInput });
      setSqlResult(result);
    } catch (err) {
      setSqlResult(typeof err === 'string' ? err : 'Query failed');
    }
  };

  return (
    <div className="settings-section">
      <h2 className="settings-section-title">Debug Tools</h2>
      <p className="settings-section-description">
        System diagnostics, logs, and database tools.
      </p>

      {/* Log Viewer */}
      <div className="devmenu-form-group">
        <label className="devmenu-label">
          Live Logs ({logs.length} entries)
          {logs.length > 0 && (
            <button
              className="devmenu-link-btn"
              onClick={() => setLogs([])}
              style={{ marginLeft: 12 }}
            >
              Clear
            </button>
          )}
        </label>
        <div className="devmenu-log-viewer">
          {logs.length === 0 ? (
            <span className="devmenu-hint">No log entries yet. Logs appear during import, export, and scoring operations.</span>
          ) : (
            logs.map((line, i) => (
              <div key={i} className="devmenu-log-line">{line}</div>
            ))
          )}
          <div ref={logEndRef} />
        </div>
      </div>

      {/* Tool Status */}
      <div className="devmenu-form-group">
        <label className="devmenu-label">External Tools</label>
        <button className="secondary-button" onClick={handleTestTools}>
          Test FFmpeg / FFprobe / ExifTool
        </button>

        {toolStatus && (
          <div className="devmenu-tool-status">
            {toolStatus.map((tool) => (
              <div key={tool.name} className="devmenu-info-row">
                <span className={`devmenu-status-dot ${tool.available ? 'is-ok' : 'is-err'}`} />
                <span className="devmenu-info-label">{tool.name}</span>
                <span className="devmenu-info-value devmenu-mono">
                  {tool.available
                    ? (tool.version || tool.path)
                    : 'Not found'}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Cache / DB Actions */}
      <div className="devmenu-form-group">
        <label className="devmenu-label">Cache & Storage</label>
        <div className="devmenu-actions">
          <button className="secondary-button" onClick={handleClearCaches}>
            Clear Caches
          </button>
          <button className="secondary-button" onClick={handleDbStats}>
            Database Stats
          </button>
          <button className="secondary-button" onClick={handleExportDb}>
            Export Database
          </button>
        </div>

        {dbStats && (
          <pre className="devmenu-pre">{dbStats}</pre>
        )}
      </div>

      {/* Export EXIF Dump */}
      <div className="devmenu-form-group">
        <label className="devmenu-label">Export EXIF Dump</label>
        <div className="devmenu-inline">
          <input
            type="number"
            className="devmenu-input devmenu-input-sm"
            value={clipIdInput}
            onChange={(e) => setClipIdInput(e.target.value)}
            placeholder="Clip ID"
            min={1}
          />
          <button
            className="secondary-button"
            onClick={handleExportExif}
            disabled={!clipIdInput.trim()}
          >
            Export EXIF
          </button>
        </div>
        <span className="devmenu-hint">Extract raw ExifTool JSON from a clip's original file</span>
      </div>

      {/* Raw SQL */}
      <div className="devmenu-form-group">
        <label className="devmenu-label">Raw SQL (Dev license only)</label>
        <textarea
          className="devmenu-textarea"
          value={sqlInput}
          onChange={(e) => setSqlInput(e.target.value)}
          placeholder="SELECT * FROM clips LIMIT 10"
          rows={3}
        />
        <button
          className="secondary-button"
          onClick={handleRunSql}
          disabled={!sqlInput.trim()}
          style={{ marginTop: 8 }}
        >
          Execute
        </button>

        {sqlResult && (
          <pre className="devmenu-pre devmenu-pre-scroll">{sqlResult}</pre>
        )}
      </div>
    </div>
  );
}
