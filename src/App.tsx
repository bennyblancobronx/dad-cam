// Dad Cam - Main App with Settings Persistence
import { useState, useCallback, useEffect } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import type { LibraryInfo } from './types/clips';
import type { AppSettings } from './types/settings';
import type { LicenseState } from './types/licensing';
import { DEFAULT_SETTINGS } from './types/settings';
import { openLibrary, closeLibrary, createLibrary } from './api/clips';
import {
  getAppSettings,
  addRecentLibrary,
  validateLibraryPath,
  removeRecentLibrary,
} from './api/settings';
import { getLicenseState } from './api/licensing';
import { clearThumbnailCache } from './utils/thumbnailCache';
import { LibraryView } from './components/LibraryView';
import { LibraryDashboard } from './components/LibraryDashboard';
import { SettingsView } from './components/SettingsView';
import { FirstRunWizard } from './components/FirstRunWizard';
import { TrialBanner } from './components/TrialBanner';
import { DevMenu } from './components/DevMenu';
import { ErrorBoundary } from './components/ErrorBoundary';
import { ToastNotification } from './components/ToastNotification';
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
  const [license, setLicense] = useState<LicenseState | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Form state
  const [libraryPath, setLibraryPath] = useState('');
  const [newLibraryName, setNewLibraryName] = useState('');
  const [showCreateForm, setShowCreateForm] = useState(false);

  // Error recovery state
  const [unmountedLibrary, setUnmountedLibrary] = useState<UnmountedLibrary | null>(null);

  // App-level view state for Advanced mode settings
  const [showAppSettings, setShowAppSettings] = useState(false);

  // Dev menu state
  const [showDevMenu, setShowDevMenu] = useState(false);

  // Load settings and auto-open library on mount
  useEffect(() => {
    loadAppSettings();
  }, []);

  // Keyboard shortcut: Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win/Linux) for Dev Menu
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === 'D') {
        e.preventDefault();
        setShowDevMenu((prev) => !prev);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // Apply theme class when settings change
  useEffect(() => {
    if (settings) {
      document.documentElement.classList.toggle('dark-mode', settings.theme === 'dark');
    }
  }, [settings?.theme]);

  async function loadAppSettings() {
    setIsLoading(true);
    setError(null);

    try {
      let appSettings = await getAppSettings();
      setSettings(appSettings);

      // Load license state and sync cache in settings if stale
      try {
        const licenseState = await getLicenseState();
        setLicense(licenseState);

        // Sync licenseStateCache in settings if it differs from live state
        const cache = appSettings.licenseStateCache;
        if (
          !cache ||
          cache.licenseType !== licenseState.licenseType ||
          cache.isActive !== licenseState.isActive ||
          cache.daysRemaining !== licenseState.trialDaysRemaining
        ) {
          const { saveAppSettings } = await import('./api/settings');
          const synced: AppSettings = {
            ...appSettings,
            licenseStateCache: {
              licenseType: licenseState.licenseType,
              isActive: licenseState.isActive,
              daysRemaining: licenseState.trialDaysRemaining,
            },
          };
          await saveAppSettings(synced);
          appSettings = synced;
          setSettings(synced);
        }
      } catch (err) {
        console.error('Failed to load license state:', err);
      }

      // Simple mode: auto-open last project if set
      if (appSettings.mode === 'simple' && appSettings.defaultProjectPath) {
        const isValid = await validateLibraryPath(appSettings.defaultProjectPath);

        if (isValid) {
          try {
            const lib = await openLibrary(appSettings.defaultProjectPath);
            clearThumbnailCache();
            setLibrary(lib);
          } catch (err) {
            console.error('Failed to auto-open library:', err);
            setError(
              typeof err === 'string'
                ? err
                : err instanceof Error
                ? err.message
                : 'Failed to open project'
            );
          }
        } else {
          const recentProject = appSettings.recentProjects.find(
            (r) => r.path === appSettings.defaultProjectPath
          );
          setUnmountedLibrary({
            path: appSettings.defaultProjectPath,
            name: recentProject?.name || 'Unknown Project',
          });
        }
      }
    } catch (err) {
      console.error('Failed to load settings, using defaults:', err);
      setSettings(DEFAULT_SETTINGS);
    } finally {
      setIsLoading(false);
    }
  }

  // Handle wizard completion
  const handleWizardComplete = useCallback((updatedSettings: AppSettings) => {
    setSettings(updatedSettings);
  }, []);

  // Handle license state change (activation/deactivation) -- update both state and settings cache
  const handleLicenseChange = useCallback(async (newLicense: LicenseState) => {
    setLicense(newLicense);

    // Sync licenseStateCache in settings
    if (settings) {
      const updated: AppSettings = {
        ...settings,
        licenseStateCache: {
          licenseType: newLicense.licenseType,
          isActive: newLicense.isActive,
          daysRemaining: newLicense.trialDaysRemaining,
        },
      };
      try {
        const { saveAppSettings } = await import('./api/settings');
        await saveAppSettings(updated);
        setSettings(updated);
      } catch (err) {
        console.error('Failed to update license cache in settings:', err);
      }
    }
  }, [settings]);

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
        setError('Project is still not available. Please check the drive is connected.');
      }
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to open project'
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
      setLibrary(lib);
      setLibraryPath(selected as string);

      await addRecentLibrary(selected as string, lib.name, lib.clipCount);
      const updatedSettings = await getAppSettings();
      setSettings(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to open project'
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
    setUnmountedLibrary(null);

    try {
      const lib = await createLibrary(libraryPath.trim(), newLibraryName.trim());
      clearThumbnailCache();
      setLibrary(lib);
      setShowCreateForm(false);

      await addRecentLibrary(libraryPath.trim(), lib.name, lib.clipCount);
      const updatedSettings = await getAppSettings();
      setSettings(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to create project'
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

  // Dev Menu overlay (renders above everything when active)
  if (showDevMenu && settings) {
    return (
      <DevMenu
        settings={settings}
        onSettingsChange={setSettings}
        onClose={() => setShowDevMenu(false)}
      />
    );
  }

  // First-run wizard gate (before anything else after loading)
  if (settings && !settings.firstRunCompleted) {
    return (
      <FirstRunWizard
        settings={settings}
        onComplete={handleWizardComplete}
      />
    );
  }

  // Library is open: show library view with MainLayout
  if (library) {
    return (
      <ErrorBoundary>
        <ToastNotification />
        {license && <TrialBanner licenseState={license} onLicenseChange={handleLicenseChange} />}
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
        {license && <TrialBanner licenseState={license} onLicenseChange={handleLicenseChange} />}
        <div className="welcome-container">
          <h1 className="welcome-title">Project Not Available</h1>
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
              Open Different Project
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
              Remove from recent projects
            </label>
          </div>
        </div>
      </div>
    );
  }

  // Advanced mode: show Library Dashboard or Settings
  if (settings?.mode === 'advanced') {
    if (showAppSettings) {
      return (
        <ErrorBoundary>
          {license && <TrialBanner licenseState={license} onLicenseChange={handleLicenseChange} />}
          <SettingsView
            settings={settings}
            onSettingsChange={setSettings}
            onBack={() => setShowAppSettings(false)}
            onOpenDevMenu={() => { setShowAppSettings(false); setShowDevMenu(true); }}
          />
        </ErrorBoundary>
      );
    }

    return (
      <ErrorBoundary>
        <ToastNotification />
        {license && <TrialBanner licenseState={license} onLicenseChange={handleLicenseChange} />}
        <LibraryDashboard
          onLibrarySelect={setLibrary}
          onSettingsChange={setSettings}
          onNavigateToSettings={() => setShowAppSettings(true)}
        />
      </ErrorBoundary>
    );
  }

  // Simple mode: Welcome/open screen
  return (
    <div className="app-welcome">
      <ToastNotification />
      {license && <TrialBanner licenseState={license} onLicenseChange={handleLicenseChange} />}
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
                {isLoading ? 'Opening...' : 'Open Project'}
              </button>
              <button
                className="secondary-button"
                onClick={() => setShowCreateForm(true)}
                disabled={isLoading}
              >
                Create New Project
              </button>
            </div>

            {/* Recent projects list */}
            {settings && settings.recentProjects.length > 0 && (
              <div className="recent-libraries">
                <h3 className="recent-title">Recent Projects</h3>
                <ul className="recent-list">
                  {settings.recentProjects.map((proj) => (
                    <li key={proj.path} className="recent-item">
                      <button
                        className="recent-button"
                        onClick={async () => {
                          setIsLoading(true);
                          setError(null);
                          try {
                            const isValid = await validateLibraryPath(proj.path);
                            if (isValid) {
                              const openedLib = await openLibrary(proj.path);
                              clearThumbnailCache();
                              setLibrary(openedLib);
                              await addRecentLibrary(proj.path, openedLib.name, openedLib.clipCount);
                            } else {
                              setUnmountedLibrary({ path: proj.path, name: proj.name });
                            }
                          } catch (err) {
                            setError(
                              typeof err === 'string'
                                ? err
                                : err instanceof Error
                                ? err.message
                                : 'Failed to open project'
                            );
                          } finally {
                            setIsLoading(false);
                          }
                        }}
                        disabled={isLoading}
                      >
                        <span className="recent-name">{proj.name}</span>
                        <span className="recent-meta">
                          {proj.clipCount} clips
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
              <label htmlFor="new-library-path">Project Location</label>
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
              <label htmlFor="library-name">Project Name</label>
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
        )}

        <div className="help-text">
          <p>Select an existing Dad Cam project folder or create a new one.</p>
          <p>
            Use the CLI to ingest footage: <code>dadcam ingest /path/to/source</code>
          </p>
        </div>

        {/* Mode indicator - click to open settings */}
        {settings && (
          <button
            className="mode-indicator"
            onClick={() => setShowAppSettings(true)}
            style={{ background: 'none', border: 'none', cursor: 'pointer' }}
          >
            Mode: {settings.mode === 'simple' ? 'Simple' : 'Advanced'} (click to change)
          </button>
        )}
      </div>

      {/* Settings button (gear icon) */}
      <button
        className="settings-button"
        onClick={() => setShowAppSettings(true)}
        aria-label="Open settings"
      >
        <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5">
          <circle cx="10" cy="10" r="3" />
          <path d="M10 1v2M10 17v2M1 10h2M17 10h2M3.5 3.5l1.4 1.4M15.1 15.1l1.4 1.4M3.5 16.5l1.4-1.4M15.1 4.9l1.4-1.4" />
        </svg>
      </button>

      {/* Settings view (full page overlay for Simple mode welcome screen) */}
      {showAppSettings && settings && (
        <div className="settings-overlay">
          <SettingsView
            settings={settings}
            onSettingsChange={setSettings}
            onBack={() => setShowAppSettings(false)}
            onOpenDevMenu={() => { setShowAppSettings(false); setShowDevMenu(true); }}
          />
        </div>
      )}
    </div>
  );
}

export default App;
