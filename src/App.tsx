// Dad Cam - Main App with Settings Persistence
import { useState, useCallback, useEffect } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import type { LibraryInfo } from './types/clips';
import type { AppSettings } from './types/settings';
import { DEFAULT_SETTINGS } from './types/settings';
import { openLibrary, closeLibrary, createLibrary } from './api/clips';
import {
  getAppSettings,
  addRecentLibrary,
  validateLibraryPath,
  removeRecentLibrary,
} from './api/settings';
import { clearThumbnailCache } from './utils/thumbnailCache';
import { LibraryView } from './components/LibraryView';
import { LibraryDashboard } from './components/LibraryDashboard';
import { SettingsPanel } from './components/SettingsPanel';
import { ErrorBoundary } from './components/ErrorBoundary';
import './App.css';

/** Unmounted library state for error recovery */
interface UnmountedLibrary {
  path: string;
  name: string;
}

function App() {
  // Core state
  const [library, setLibrary] = useState<LibraryInfo | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Form state
  const [libraryPath, setLibraryPath] = useState('');
  const [newLibraryName, setNewLibraryName] = useState('');
  const [showCreateForm, setShowCreateForm] = useState(false);

  // Error recovery state
  const [unmountedLibrary, setUnmountedLibrary] = useState<UnmountedLibrary | null>(null);

  // Settings panel state
  const [showSettings, setShowSettings] = useState(false);

  // Load settings and auto-open library on mount
  useEffect(() => {
    loadAppSettings();
  }, []);

  async function loadAppSettings() {
    setIsLoading(true);
    setError(null);

    try {
      const appSettings = await getAppSettings();
      setSettings(appSettings);

      // Personal mode: auto-open last library if set
      if (appSettings.mode === 'personal' && appSettings.lastLibraryPath) {
        const isValid = await validateLibraryPath(appSettings.lastLibraryPath);

        if (isValid) {
          try {
            const lib = await openLibrary(appSettings.lastLibraryPath);
            clearThumbnailCache();
            setLibrary(lib);
          } catch (err) {
            // Library exists but failed to open (corrupted?)
            console.error('Failed to auto-open library:', err);
            setError(
              typeof err === 'string'
                ? err
                : err instanceof Error
                ? err.message
                : 'Failed to open library'
            );
          }
        } else {
          // Library path doesn't exist (unmounted drive or deleted)
          const recentLib = appSettings.recentLibraries.find(
            (r) => r.path === appSettings.lastLibraryPath
          );
          setUnmountedLibrary({
            path: appSettings.lastLibraryPath,
            name: recentLib?.name || 'Unknown Library',
          });
        }
      }
    } catch (err) {
      // Settings file corrupted - reset to defaults
      console.error('Failed to load settings, using defaults:', err);
      setSettings(DEFAULT_SETTINGS);
    } finally {
      setIsLoading(false);
    }
  }

  // Retry opening unmounted library
  const handleRetryUnmounted = useCallback(async () => {
    if (!unmountedLibrary) return;

    setIsLoading(true);
    setError(null);

    try {
      const isValid = await validateLibraryPath(unmountedLibrary.path);

      if (isValid) {
        const lib = await openLibrary(unmountedLibrary.path);
        clearThumbnailCache();
        setLibrary(lib);
        setUnmountedLibrary(null);
      } else {
        setError('Library is still not available. Please check the drive is connected.');
      }
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to open library'
      );
    } finally {
      setIsLoading(false);
    }
  }, [unmountedLibrary]);

  // Remove unmounted library from recent list
  const handleRemoveUnmounted = useCallback(async () => {
    if (!unmountedLibrary) return;

    try {
      await removeRecentLibrary(unmountedLibrary.path);
      setUnmountedLibrary(null);
      // Reload settings to get updated recent libraries
      const appSettings = await getAppSettings();
      setSettings(appSettings);
    } catch (err) {
      console.error('Failed to remove library from recent list:', err);
    }
  }, [unmountedLibrary]);

  // Open existing library via native folder picker
  const handleOpenLibrary = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    setUnmountedLibrary(null);

    try {
      // Open native folder picker dialog
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Dad Cam Library',
      });

      if (!selected) {
        // User cancelled
        setIsLoading(false);
        return;
      }

      const lib = await openLibrary(selected as string);
      clearThumbnailCache();
      setLibrary(lib);
      setLibraryPath(selected as string);

      // Add to recent libraries (saves settings)
      await addRecentLibrary(selected as string, lib.name, lib.clipCount);

      // Update local settings state
      const updatedSettings = await getAppSettings();
      setSettings(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to open library'
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Browse for folder (used by create form)
  const handleBrowseFolder = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder for New Library',
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
      setError('Please select a folder and enter a library name');
      return;
    }

    setIsLoading(true);
    setError(null);
    setUnmountedLibrary(null);

    try {
      const lib = await createLibrary(libraryPath.trim(), newLibraryName.trim());
      clearThumbnailCache();
      setLibrary(lib);
      setShowCreateForm(false);

      // Add to recent libraries
      await addRecentLibrary(libraryPath.trim(), lib.name, lib.clipCount);

      // Update local settings state
      const updatedSettings = await getAppSettings();
      setSettings(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to create library'
      );
    } finally {
      setIsLoading(false);
    }
  }, [libraryPath, newLibraryName]);

  // Close library
  const handleCloseLibrary = useCallback(async () => {
    try {
      await closeLibrary();
      clearThumbnailCache();
      setLibrary(null);
      setLibraryPath('');
      setError(null);
    } catch (err) {
      console.error('Failed to close library:', err);
    }
  }, []);

  // Loading screen
  if (isLoading && !library && !unmountedLibrary) {
    return (
      <div className="app-welcome">
        <div className="welcome-container">
          <h1 className="welcome-title">dad cam</h1>
          <p className="welcome-subtitle">loading...</p>
        </div>
      </div>
    );
  }

  // Library is open: show library view with MainLayout
  if (library) {
    return (
      <ErrorBoundary>
        <LibraryView
          library={library}
          onClose={handleCloseLibrary}
          mode={settings?.mode}
          settings={settings}
          onSettingsChange={setSettings}
        />
      </ErrorBoundary>
    );
  }

  // Unmounted library error state
  if (unmountedLibrary) {
    return (
      <div className="app-welcome">
        <div className="welcome-container">
          <h1 className="welcome-title">Library Not Available</h1>
          <p className="welcome-subtitle">
            "{unmountedLibrary.name}" is on a drive that's not currently connected.
          </p>
          <p className="library-path">{unmountedLibrary.path}</p>

          {error && <div className="error-message">{error}</div>}

          <div className="button-group">
            <button
              className="primary-button"
              onClick={handleRetryUnmounted}
              disabled={isLoading}
            >
              {isLoading ? 'Checking...' : 'Try Again'}
            </button>
            <button
              className="secondary-button"
              onClick={() => {
                setUnmountedLibrary(null);
                setError(null);
              }}
              disabled={isLoading}
            >
              Open Different Library
            </button>
          </div>

          <div className="checkbox-group">
            <label>
              <input
                type="checkbox"
                onChange={(e) => {
                  if (e.target.checked) {
                    handleRemoveUnmounted();
                  }
                }}
              />
              Remove from recent libraries
            </label>
          </div>
        </div>
      </div>
    );
  }

  // Pro mode: show Library Dashboard
  if (settings?.mode === 'pro') {
    return (
      <ErrorBoundary>
        <LibraryDashboard
          settings={settings}
          onLibrarySelect={setLibrary}
          onSettingsChange={setSettings}
          onOpenSettings={() => setShowSettings(true)}
        />
        {/* Settings panel */}
        {showSettings && (
          <SettingsPanel
            settings={settings}
            onSettingsChange={setSettings}
            onClose={() => setShowSettings(false)}
          />
        )}
      </ErrorBoundary>
    );
  }

  // Personal mode: Welcome/open screen
  return (
    <div className="app-welcome">
      <div className="welcome-container">
        <h1 className="welcome-title">dad cam</h1>
        <p className="welcome-subtitle">video library for dad cam footage</p>

        {error && <div className="error-message">{error}</div>}

        {!showCreateForm ? (
          <div className="open-form">
            <div className="button-group">
              <button
                className="primary-button"
                onClick={handleOpenLibrary}
                disabled={isLoading}
              >
                {isLoading ? 'Opening...' : 'Open Library'}
              </button>
              <button
                className="secondary-button"
                onClick={() => setShowCreateForm(true)}
                disabled={isLoading}
              >
                Create New Library
              </button>
            </div>

            {/* Recent libraries list */}
            {settings && settings.recentLibraries.length > 0 && (
              <div className="recent-libraries">
                <h3 className="recent-title">Recent Libraries</h3>
                <ul className="recent-list">
                  {settings.recentLibraries.map((lib) => (
                    <li key={lib.path} className="recent-item">
                      <button
                        className="recent-button"
                        onClick={async () => {
                          setIsLoading(true);
                          setError(null);
                          try {
                            const isValid = await validateLibraryPath(lib.path);
                            if (isValid) {
                              const openedLib = await openLibrary(lib.path);
                              clearThumbnailCache();
                              setLibrary(openedLib);
                              await addRecentLibrary(lib.path, openedLib.name, openedLib.clipCount);
                            } else {
                              setUnmountedLibrary({ path: lib.path, name: lib.name });
                            }
                          } catch (err) {
                            setError(
                              typeof err === 'string'
                                ? err
                                : err instanceof Error
                                ? err.message
                                : 'Failed to open library'
                            );
                          } finally {
                            setIsLoading(false);
                          }
                        }}
                        disabled={isLoading}
                      >
                        <span className="recent-name">{lib.name}</span>
                        <span className="recent-meta">
                          {lib.clipCount} clips
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        ) : (
          <div className="create-form">
            <div className="input-group">
              <label htmlFor="new-library-path">Library Location</label>
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
              <label htmlFor="library-name">Library Name</label>
              <input
                id="library-name"
                type="text"
                placeholder="My Video Library"
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
                {isLoading ? 'Creating...' : 'Create Library'}
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
        )}

        <div className="help-text">
          <p>Select an existing Dad Cam library folder or create a new one.</p>
          <p>
            Use the CLI to ingest footage: <code>dadcam ingest /path/to/source</code>
          </p>
        </div>

        {/* Mode indicator - click to open settings */}
        {settings && (
          <button
            className="mode-indicator"
            onClick={() => setShowSettings(true)}
            style={{ background: 'none', border: 'none', cursor: 'pointer' }}
          >
            Mode: {settings.mode === 'personal' ? 'Personal' : 'Pro'} (click to change)
          </button>
        )}
      </div>

      {/* Settings button (gear icon) */}
      <button
        className="settings-button"
        onClick={() => setShowSettings(true)}
        aria-label="Open settings"
      >
        <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5">
          <circle cx="10" cy="10" r="3" />
          <path d="M10 1v2M10 17v2M1 10h2M17 10h2M3.5 3.5l1.4 1.4M15.1 15.1l1.4 1.4M3.5 16.5l1.4-1.4M15.1 4.9l1.4-1.4" />
        </svg>
      </button>

      {/* Settings panel */}
      {showSettings && settings && (
        <SettingsPanel
          settings={settings}
          onSettingsChange={setSettings}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  );
}

export default App;
