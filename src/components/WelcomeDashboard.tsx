// Dad Cam - Welcome Dashboard (Personal Mode)
// Landing page when a library is open, provides quick access to main actions

import { useState, useCallback } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import type { LibraryInfo } from '../types/clips';

interface WelcomeDashboardProps {
  library: LibraryInfo;
  onNavigateToClips: () => void;
  onNavigateToStills: () => void;
}

export function WelcomeDashboard({
  library,
  onNavigateToClips,
  onNavigateToStills,
}: WelcomeDashboardProps) {
  const [isImporting, setIsImporting] = useState(false);
  const [importStatus, setImportStatus] = useState<{
    type: 'success' | 'error';
    message: string;
  } | null>(null);

  // Handle import footage
  const handleImport = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder to Import',
      });

      if (!selected) return;

      setIsImporting(true);
      setImportStatus(null);

      const result = await invoke<{
        jobId: number;
        totalFiles: number;
        processed: number;
        skipped: number;
        failed: number;
        clipsCreated: number[];
      }>('start_ingest', {
        sourcePath: selected,
        libraryPath: library.rootPath,
      });

      setImportStatus({
        type: 'success',
        message: `Imported ${result.processed} clips${result.skipped > 0 ? ` (${result.skipped} skipped)` : ''}`,
      });

      // Clear status after 5 seconds
      setTimeout(() => setImportStatus(null), 5000);
    } catch (err) {
      setImportStatus({
        type: 'error',
        message: typeof err === 'string' ? err : err instanceof Error ? err.message : 'Import failed',
      });
    } finally {
      setIsImporting(false);
    }
  }, [library.rootPath]);

  // Handle stills - navigate to clip grid for frame selection
  const handleStills = useCallback(() => {
    onNavigateToStills();
  }, [onNavigateToStills]);

  // Handle export footage (placeholder for now)
  const handleExport = useCallback(() => {
    // Navigate to clips for export selection
    onNavigateToClips();
  }, [onNavigateToClips]);

  return (
    <div className="welcome-dashboard">
      <div className="welcome-dashboard-content">
        <h1 className="welcome-dashboard-title">{library.name}</h1>
        <p className="welcome-dashboard-subtitle">
          {library.clipCount} clips in library
        </p>

        {/* Status message */}
        {importStatus && (
          <div
            className={`welcome-status ${importStatus.type === 'error' ? 'welcome-status-error' : 'welcome-status-success'}`}
          >
            {importStatus.message}
          </div>
        )}

        {/* Action buttons */}
        <div className="welcome-button-grid">
          <button
            className="welcome-btn welcome-btn-primary"
            onClick={handleImport}
            disabled={isImporting}
            title="Import video files from a folder"
          >
            {isImporting ? 'Importing...' : 'Import Footage'}
          </button>

          <button
            className="welcome-btn welcome-btn-secondary"
            onClick={handleStills}
            disabled={isImporting}
            title="Browse clips to export still frames"
          >
            Stills
            <span className="welcome-btn-hint">(S key in player)</span>
          </button>

          <button
            className="welcome-btn welcome-btn-secondary"
            onClick={handleExport}
            disabled={isImporting}
            title="Select clips to export"
          >
            Export Footage
          </button>

          <button
            className="welcome-btn welcome-btn-ghost"
            onClick={onNavigateToClips}
            disabled={isImporting}
            title="View all clips in the library"
          >
            Browse All Clips
          </button>
        </div>
      </div>
    </div>
  );
}
