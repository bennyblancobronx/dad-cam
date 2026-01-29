// Dad Cam - Welcome Dashboard (Simple Mode)
// Landing page when a project is open, provides quick access to main actions

import { useState, useCallback } from 'react';
import type { LibraryInfo } from '../types/clips';
import type { FeatureFlags } from '../types/settings';
import { ImportDialog } from './ImportDialog';
import { BestClipsPanel } from './BestClipsPanel';
import { isFeatureAllowed } from '../api/licensing';

interface WelcomeDashboardProps {
  library: LibraryInfo;
  onNavigateToClips: () => void;
  onNavigateToStills: () => void;
  /** Feature flags to gate optional sections */
  featureFlags?: FeatureFlags;
}

export function WelcomeDashboard({
  library,
  onNavigateToClips,
  onNavigateToStills,
  featureFlags,
}: WelcomeDashboardProps) {
  const [showImportDialog, setShowImportDialog] = useState(false);
  const [importStatus, setImportStatus] = useState<{
    type: 'success' | 'error';
    message: string;
  } | null>(null);

  // Handle import complete callback
  const handleImportComplete = useCallback(() => {
    setImportStatus({
      type: 'success',
      message: 'Import complete',
    });
    setTimeout(() => setImportStatus(null), 5000);
  }, []);

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
          {library.clipCount} clips in project
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
            onClick={async () => {
              try {
                const allowed = await isFeatureAllowed('import');
                if (!allowed) {
                  setImportStatus({ type: 'error', message: 'Trial expired. Please activate a license to import footage.' });
                  return;
                }
              } catch {
                // If check fails, let backend handle it
              }
              setShowImportDialog(true);
            }}
            title="Import video files from a folder"
          >
            Import Footage
          </button>

          <button
            className="welcome-btn welcome-btn-secondary"
            onClick={handleStills}
            title="Browse clips to export still frames"
          >
            Stills
            <span className="welcome-btn-hint">(S key in player)</span>
          </button>

          <button
            className="welcome-btn welcome-btn-secondary"
            onClick={handleExport}
            title="Select clips to export"
          >
            Export Footage
          </button>

          <button
            className="welcome-btn welcome-btn-ghost"
            onClick={onNavigateToClips}
            title="View all clips in the project"
          >
            Browse All Clips
          </button>
        </div>

        {/* Best Clips section -- gated by featureFlags.bestClips */}
        {featureFlags?.bestClips !== false && library.clipCount > 0 && (
          <BestClipsPanel onClipClick={() => onNavigateToClips()} />
        )}
      </div>

      {showImportDialog && (
        <ImportDialog
          libraryPath={library.rootPath}
          onClose={() => setShowImportDialog(false)}
          onComplete={handleImportComplete}
        />
      )}
    </div>
  );
}
