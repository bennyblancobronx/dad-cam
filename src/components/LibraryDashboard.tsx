// Dad Cam - Project Dashboard (Advanced Mode)
// Multi-project selection view with card grid

import { useState, useEffect, useCallback } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import type { LibraryInfo } from '../types/clips';
import type { AppSettings, RegistryLibraryEntry } from '../types/settings';
import { openLibrary, createLibrary } from '../api/clips';
import {
  addRecentLibrary,
  validateLibraryPath,
  removeRecentLibrary,
  getAppSettings,
  listRegistryLibraries,
} from '../api/settings';
import { clearThumbnailCache } from '../utils/thumbnailCache';
import { LibraryCard } from './LibraryCard';

interface LibraryDashboardProps {
  settings: AppSettings;
  onLibrarySelect: (library: LibraryInfo) => void;
  onSettingsChange: (settings: AppSettings) => void;
  onNavigateToSettings: () => void;
  onNavigateToCameras?: () => void;
}

export function LibraryDashboard({
  settings,
  onLibrarySelect,
  onSettingsChange,
  onNavigateToSettings,
  onNavigateToCameras,
}: LibraryDashboardProps) {
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [libraryPath, setLibraryPath] = useState('');
  const [newLibraryName, setNewLibraryName] = useState('');
  const [registryLibraries, setRegistryLibraries] = useState<RegistryLibraryEntry[]>([]);

  // Load registry libraries on mount
  useEffect(() => {
    listRegistryLibraries()
      .then(setRegistryLibraries)
      .catch((err) => console.error('Failed to load registry libraries:', err));
  }, []);

  // Refresh registry after library operations
  const refreshRegistry = useCallback(async () => {
    try {
      const libs = await listRegistryLibraries();
      setRegistryLibraries(libs);
    } catch {
      // Best-effort
    }
  }, []);

  // Open existing library via folder picker
  const handleOpenLibrary = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Dad Cam Project',
      });

      if (!selected) {
        setIsLoading(false);
        return;
      }

      const lib = await openLibrary(selected as string);
      clearThumbnailCache();

      // Add to recent libraries
      await addRecentLibrary(selected as string, lib.name, lib.clipCount);
      const updatedSettings = await getAppSettings();
      onSettingsChange(updatedSettings);
      refreshRegistry();

      onLibrarySelect(lib);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to open project'
      );
    } finally {
      setIsLoading(false);
    }
  }, [onLibrarySelect, onSettingsChange, refreshRegistry]);

  // Open a recent library
  const handleSelectRecent = useCallback(async (path: string, name: string) => {
    setIsLoading(true);
    setError(null);

    try {
      const isValid = await validateLibraryPath(path);

      if (!isValid) {
        setError(`"${name}" is not available. The drive may not be connected.`);
        setIsLoading(false);
        return;
      }

      const lib = await openLibrary(path);
      clearThumbnailCache();

      // Update recent libraries
      await addRecentLibrary(path, lib.name, lib.clipCount);
      const updatedSettings = await getAppSettings();
      onSettingsChange(updatedSettings);
      refreshRegistry();

      onLibrarySelect(lib);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to open project'
      );
    } finally {
      setIsLoading(false);
    }
  }, [onLibrarySelect, onSettingsChange, refreshRegistry]);

  // Browse for folder
  const handleBrowseFolder = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder for New Project',
      });
      if (selected) {
        setLibraryPath(selected as string);
      }
    } catch (err) {
      console.error('Failed to open folder picker:', err);
    }
  }, []);

  // Create new library
  const handleCreateLibrary = useCallback(async () => {
    if (!libraryPath.trim() || !newLibraryName.trim()) {
      setError('Please select a folder and enter a project name');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const lib = await createLibrary(libraryPath.trim(), newLibraryName.trim());
      clearThumbnailCache();

      // Add to recent libraries
      await addRecentLibrary(libraryPath.trim(), lib.name, lib.clipCount);
      const updatedSettings = await getAppSettings();
      onSettingsChange(updatedSettings);
      refreshRegistry();

      setShowCreateForm(false);
      setLibraryPath('');
      setNewLibraryName('');

      onLibrarySelect(lib);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to create project'
      );
    } finally {
      setIsLoading(false);
    }
  }, [libraryPath, newLibraryName, onLibrarySelect, onSettingsChange, refreshRegistry]);

  // Remove a library from recent list
  const handleRemoveRecent = useCallback(async (path: string, e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await removeRecentLibrary(path);
      const updatedSettings = await getAppSettings();
      onSettingsChange(updatedSettings);
      refreshRegistry();
    } catch (err) {
      console.error('Failed to remove library:', err);
    }
  }, [onSettingsChange, refreshRegistry]);

  return (
    <div className="library-dashboard">
      {/* Header */}
      <header className="library-dashboard-header">
        <div className="library-dashboard-titles">
          <h1 className="library-dashboard-title">dad cam</h1>
          <span className="library-dashboard-subtitle">projects</span>
        </div>
        <button
          className="settings-icon-button"
          onClick={onNavigateToSettings}
          aria-label="Open settings"
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="10" cy="10" r="3" />
            <path d="M10 1v2M10 17v2M1 10h2M17 10h2M3.5 3.5l1.4 1.4M15.1 15.1l1.4 1.4M3.5 16.5l1.4-1.4M15.1 4.9l1.4-1.4" />
          </svg>
        </button>
      </header>

      {/* Error message */}
      {error && (
        <div className="error-message library-dashboard-error">
          {error}
        </div>
      )}

      {/* Main content */}
      <main className="library-dashboard-content">
        {/* Create form */}
        {showCreateForm ? (
          <div className="library-create-form">
            <h2 className="library-create-title">Create New Project</h2>

            <div className="input-group">
              <label htmlFor="new-library-path">Location</label>
              <div className="input-with-button">
                <input
                  id="new-library-path"
                  type="text"
                  placeholder="Select a folder..."
                  value={libraryPath}
                  onChange={(e) => setLibraryPath(e.target.value)}
                  disabled={isLoading}
                  readOnly
                />
                <button
                  className="browse-button"
                  onClick={handleBrowseFolder}
                  disabled={isLoading}
                  type="button"
                >
                  Browse
                </button>
              </div>
            </div>

            <div className="input-group">
              <label htmlFor="library-name">Name</label>
              <input
                id="library-name"
                type="text"
                placeholder="My Video Project"
                value={newLibraryName}
                onChange={(e) => setNewLibraryName(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleCreateLibrary()}
                disabled={isLoading}
              />
            </div>

            <div className="button-group">
              <button
                className="primary-button"
                onClick={handleCreateLibrary}
                disabled={isLoading}
              >
                {isLoading ? 'Creating...' : 'Create Project'}
              </button>
              <button
                className="secondary-button"
                onClick={() => {
                  setShowCreateForm(false);
                  setLibraryPath('');
                  setNewLibraryName('');
                  setError(null);
                }}
                disabled={isLoading}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <>
            {/* Recent Libraries Grid -- from App DB registry */}
            {registryLibraries.length > 0 && (
              <section className="library-section">
                <h2 className="library-section-title">Recent Projects</h2>
                <div className="library-grid">
                  {registryLibraries.map((lib) => (
                    <div key={lib.libraryUuid} className="library-grid-item">
                      <LibraryCard
                        library={{
                          path: lib.path,
                          name: lib.label || lib.path.split('/').pop() || 'Untitled',
                          lastOpened: lib.lastOpenedAt || '',
                          clipCount: lib.clipCount,
                          thumbnailPath: lib.thumbnailPath || undefined,
                        }}
                        onSelect={() => handleSelectRecent(lib.path, lib.label || lib.path.split('/').pop() || 'Untitled')}
                        isLoading={isLoading}
                      />
                      {lib.isMissing && (
                        <div className="library-missing-badge" title="Library not found at this path">
                          Missing
                        </div>
                      )}
                      <button
                        className="library-remove-button"
                        onClick={(e) => handleRemoveRecent(lib.path, e)}
                        aria-label={`Remove ${lib.label || 'library'} from recent`}
                        title="Remove from recent"
                      >
                        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                          <path d="M4 4l8 8M12 4l-8 8" />
                        </svg>
                      </button>
                    </div>
                  ))}
                </div>
              </section>
            )}

            {/* Actions */}
            <section className="library-actions">
              <button
                className="library-action-button library-action-new"
                onClick={() => setShowCreateForm(true)}
                disabled={isLoading}
                title="Create a new video project"
              >
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M12 5v14M5 12h14" />
                </svg>
                <span>New Project</span>
              </button>
              <button
                className="library-action-button library-action-open"
                onClick={handleOpenLibrary}
                disabled={isLoading}
                title="Open an existing project folder"
              >
                <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M3 7a2 2 0 012-2h14a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                  <path d="M8 12h8" />
                </svg>
                <span>{isLoading ? 'Opening...' : 'Open Project'}</span>
              </button>
              {/* Cameras button -- works without library open (Phase 6) */}
              {settings.featureFlags.camerasTab && onNavigateToCameras && (
                <button
                  className="library-action-button"
                  onClick={onNavigateToCameras}
                  disabled={isLoading}
                  title="Manage camera profiles and devices"
                >
                  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <rect x="2" y="6" width="20" height="12" rx="2" />
                    <circle cx="12" cy="12" r="3" />
                    <path d="M17 6V4h-4" />
                  </svg>
                  <span>Cameras</span>
                </button>
              )}
            </section>

            {/* Empty state */}
            {registryLibraries.length === 0 && (
              <div className="library-empty-state">
                <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
                  <path d="M3 7a2 2 0 012-2h14a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
                  <path d="M3 7l9 6 9-6" />
                </svg>
                <h3>No recent projects</h3>
                <p>Create a new project or open an existing one to get started.</p>
              </div>
            )}
          </>
        )}
      </main>
    </div>
  );
}
