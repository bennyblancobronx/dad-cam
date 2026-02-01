// Dad Cam - Settings View (Full Page)
// Braun Design Language v1.0.0 - Settings nav (200px) + Form content (640px max)

import { useState, useRef, useEffect } from 'react';
import type { AppSettings, AppMode, FeatureFlags } from '../types/settings';
import { setMode, saveAppSettings, getAppSettings } from '../api/settings';
import { getDiagnosticsEnabled, setDiagnosticsEnabled, getLogDirectory, exportLogs } from '../api/diagnostics';
import { APP_VERSION } from '../constants';
import { CamerasView } from './CamerasView';

interface SettingsViewProps {
  settings: AppSettings;
  onSettingsChange: (settings: AppSettings) => void;
  onBack: () => void;
  onOpenDevMenu?: () => void;
}

type SettingsSection = 'general' | 'features' | 'cameras' | 'about';

export function SettingsView({ settings, onSettingsChange, onBack, onOpenDevMenu }: SettingsViewProps) {
  const [activeSection, setActiveSection] = useState<SettingsSection>('general');
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [diagnosticsEnabled, setDiagnosticsState] = useState(false);
  const [logDir, setLogDir] = useState<string | null>(null);
  const [exportResult, setExportResult] = useState<string | null>(null);

  // Easter egg: click version 7 times to open dev menu
  const versionClickCount = useRef(0);
  const versionClickTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleVersionClick = () => {
    versionClickCount.current += 1;

    if (versionClickTimer.current) {
      clearTimeout(versionClickTimer.current);
    }

    if (versionClickCount.current >= 7) {
      versionClickCount.current = 0;
      onOpenDevMenu?.();
      return;
    }

    // Reset count after 2 seconds of inactivity
    versionClickTimer.current = setTimeout(() => {
      versionClickCount.current = 0;
    }, 2000);
  };

  // Load diagnostics preference and log directory on mount
  useEffect(() => {
    getDiagnosticsEnabled().then(setDiagnosticsState).catch(() => {});
    getLogDirectory().then(setLogDir).catch(() => {});
  }, []);

  const handleDiagnosticsToggle = async (enabled: boolean) => {
    setIsSaving(true);
    setError(null);
    try {
      await setDiagnosticsEnabled(enabled);
      setDiagnosticsState(enabled);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setIsSaving(false);
    }
  };

  const handleExportLogs = async () => {
    setExportResult(null);
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const dir = await open({ directory: true, title: 'Choose folder for log export' });
      if (!dir) return;
      const count = await exportLogs(dir as string);
      setExportResult(`Exported ${count} log file${count !== 1 ? 's' : ''}`);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Export failed');
    }
  };

  const handleThemeChange = async (theme: 'light' | 'dark') => {
    if (theme === settings.theme) return;

    setIsSaving(true);
    setError(null);

    try {
      const updatedSettings: AppSettings = { ...settings, theme };
      await saveAppSettings(updatedSettings);
      onSettingsChange(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to save settings'
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleFeatureFlagChange = async (flag: keyof FeatureFlags, value: boolean) => {
    setIsSaving(true);
    setError(null);

    try {
      const updatedSettings: AppSettings = {
        ...settings,
        featureFlags: {
          ...settings.featureFlags,
          [flag]: value,
        },
      };
      await saveAppSettings(updatedSettings);
      // If disabling cameras while viewing cameras section, redirect to features
      if (flag === 'camerasTab' && !value && activeSection === 'cameras') {
        setActiveSection('features');
      }
      onSettingsChange(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to save settings'
      );
    } finally {
      setIsSaving(false);
    }
  };

  const handleModeChange = async (newMode: AppMode) => {
    if (newMode === settings.mode) return;

    setIsSaving(true);
    setError(null);

    try {
      await setMode(newMode);
      // Re-fetch settings from backend to stay in sync (set_mode saves mode + flags)
      const updatedSettings = await getAppSettings();
      // If switching to Simple while on an advanced-only tab, go back to general
      if (newMode === 'simple' && (activeSection === 'features' || activeSection === 'cameras')) {
        setActiveSection('general');
      }
      onSettingsChange(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to save settings'
      );
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="settings-view">
      {/* Header with back button */}
      <header className="settings-view-header">
        <button onClick={onBack} className="settings-back-btn" title="Go back">
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 15l-5-5 5-5" />
          </svg>
          Back
        </button>
        <h1 className="settings-view-title">Settings</h1>
      </header>

      <div className="settings-view-body">
        {/* Settings navigation */}
        <nav className="settings-nav">
          <button
            className={`settings-nav-item ${activeSection === 'general' ? 'is-active' : ''}`}
            onClick={() => setActiveSection('general')}
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="10" cy="10" r="3" />
              <path d="M10 2v2M10 16v2M2 10h2M16 10h2M4.2 4.2l1.4 1.4M14.4 14.4l1.4 1.4M4.2 15.8l1.4-1.4M14.4 5.6l1.4-1.4" />
            </svg>
            General
          </button>
          {settings.mode === 'advanced' && (
            <button
              className={`settings-nav-item ${activeSection === 'features' ? 'is-active' : ''}`}
              onClick={() => setActiveSection('features')}
            >
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M4 6h12M4 10h8M4 14h10" />
              </svg>
              Features
            </button>
          )}
          {settings.mode === 'advanced' && settings.featureFlags.camerasTab && (
            <button
              className={`settings-nav-item ${activeSection === 'cameras' ? 'is-active' : ''}`}
              onClick={() => setActiveSection('cameras')}
            >
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="2" y="5" width="16" height="10" rx="2" />
                <circle cx="10" cy="10" r="3" />
                <path d="M14 5V3h-3" />
              </svg>
              Cameras
            </button>
          )}
          <button
            className={`settings-nav-item ${activeSection === 'about' ? 'is-active' : ''}`}
            onClick={() => setActiveSection('about')}
          >
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <circle cx="10" cy="10" r="8" />
              <path d="M10 9v4M10 6h.01" />
            </svg>
            About
          </button>
        </nav>

        {/* Settings content */}
        <div className="settings-content">
          {error && <div className="error-message">{error}</div>}

          {activeSection === 'general' && (
            <div className="settings-section">
              <h2 className="settings-section-title">App Mode</h2>
              <p className="settings-section-description">
                Choose how you want to use Dad Cam
              </p>

              <div className="mode-options">
                <label className={`mode-option ${settings.mode === 'simple' ? 'is-selected' : ''}`}>
                  <input
                    type="radio"
                    name="mode"
                    value="simple"
                    checked={settings.mode === 'simple'}
                    onChange={() => handleModeChange('simple')}
                    disabled={isSaving}
                  />
                  <div className="mode-option-content">
                    <span className="mode-option-title">Simple</span>
                    <span className="mode-option-description">
                      One project, automatic camera matching, no setup needed.
                    </span>
                  </div>
                </label>

                <label className={`mode-option ${settings.mode === 'advanced' ? 'is-selected' : ''}`}>
                  <input
                    type="radio"
                    name="mode"
                    value="advanced"
                    checked={settings.mode === 'advanced'}
                    onChange={() => handleModeChange('advanced')}
                    disabled={isSaving}
                  />
                  <div className="mode-option-content">
                    <span className="mode-option-title">Advanced</span>
                    <span className="mode-option-description">
                      Multiple projects, camera registration, feature toggles.
                    </span>
                  </div>
                </label>
              </div>

              {/* Theme toggle -- Advanced mode only */}
              {settings.mode === 'advanced' && (
                <>
                  <h2 className="settings-section-title" style={{ marginTop: 'var(--space-lg)' }}>Theme</h2>
                  <p className="settings-section-description">
                    Choose your preferred color scheme
                  </p>
                  <div className="mode-options">
                    <label className={`mode-option ${settings.theme === 'light' ? 'is-selected' : ''}`}>
                      <input
                        type="radio"
                        name="theme"
                        value="light"
                        checked={settings.theme === 'light'}
                        onChange={() => handleThemeChange('light')}
                        disabled={isSaving}
                      />
                      <div className="mode-option-content">
                        <span className="mode-option-title">Light</span>
                        <span className="mode-option-description">Default light interface</span>
                      </div>
                    </label>
                    <label className={`mode-option ${settings.theme === 'dark' ? 'is-selected' : ''}`}>
                      <input
                        type="radio"
                        name="theme"
                        value="dark"
                        checked={settings.theme === 'dark'}
                        onChange={() => handleThemeChange('dark')}
                        disabled={isSaving}
                      />
                      <div className="mode-option-content">
                        <span className="mode-option-title">Dark</span>
                        <span className="mode-option-description">Reduced brightness interface</span>
                      </div>
                    </label>
                  </div>
                </>
              )}

              {isSaving && (
                <p className="settings-saving-indicator">Saving...</p>
              )}
            </div>
          )}

          {activeSection === 'features' && settings.mode === 'advanced' && (
            <div className="settings-section">
              <h2 className="settings-section-title">Feature Toggles</h2>
              <p className="settings-section-description">
                Enable or disable features. Changes take effect immediately.
              </p>

              <div className="feature-toggles">
                <label className="feature-toggle-row">
                  <div className="feature-toggle-info">
                    <span className="feature-toggle-label">Screen Grabs</span>
                    <span className="feature-toggle-description">Export still frames from video clips</span>
                  </div>
                  <input
                    type="checkbox"
                    className="feature-toggle-switch"
                    checked={settings.featureFlags.screenGrabs}
                    onChange={(e) => handleFeatureFlagChange('screenGrabs', e.target.checked)}
                    disabled={isSaving}
                  />
                </label>

                <label className="feature-toggle-row">
                  <div className="feature-toggle-info">
                    <span className="feature-toggle-label">Face Detection</span>
                    <span className="feature-toggle-description">Detect faces during scoring analysis</span>
                  </div>
                  <input
                    type="checkbox"
                    className="feature-toggle-switch"
                    checked={settings.featureFlags.faceDetection}
                    onChange={(e) => handleFeatureFlagChange('faceDetection', e.target.checked)}
                    disabled={isSaving}
                  />
                </label>

                <label className="feature-toggle-row">
                  <div className="feature-toggle-info">
                    <span className="feature-toggle-label">Best Clips</span>
                    <span className="feature-toggle-description">Automatically identify top clips by score</span>
                  </div>
                  <input
                    type="checkbox"
                    className="feature-toggle-switch"
                    checked={settings.featureFlags.bestClips}
                    onChange={(e) => handleFeatureFlagChange('bestClips', e.target.checked)}
                    disabled={isSaving}
                  />
                </label>

                <label className="feature-toggle-row">
                  <div className="feature-toggle-info">
                    <span className="feature-toggle-label">Cameras</span>
                    <span className="feature-toggle-description">Manage camera profiles and registered devices in Settings</span>
                  </div>
                  <input
                    type="checkbox"
                    className="feature-toggle-switch"
                    checked={settings.featureFlags.camerasTab}
                    onChange={(e) => handleFeatureFlagChange('camerasTab', e.target.checked)}
                    disabled={isSaving}
                  />
                </label>
              </div>

              {isSaving && (
                <p className="settings-saving-indicator">Saving...</p>
              )}
            </div>
          )}

          {activeSection === 'cameras' && settings.mode === 'advanced' && (
            <CamerasView />
          )}

          {activeSection === 'about' && (
            <div className="settings-section">
              <h2 className="settings-section-title">About Dad Cam</h2>
              <div className="settings-about-content">
                <p
                  className="settings-version"
                  onClick={handleVersionClick}
                  style={{ cursor: 'default', userSelect: 'none' }}
                >
                  Version {APP_VERSION}
                </p>
                <p className="settings-tagline">Video library for dad cam footage</p>
                <div className="settings-about-details">
                  <p>A modern video library for importing, organizing, viewing, and auto-editing footage from old-school digital cameras.</p>
                </div>
              </div>

              <h2 className="settings-section-title" style={{ marginTop: 'var(--space-lg)' }}>Diagnostics</h2>
              <div className="feature-toggles">
                <label className="feature-toggle-row">
                  <div className="feature-toggle-info">
                    <span className="feature-toggle-label">Send anonymous crash reports</span>
                    <span className="feature-toggle-description">
                      Help improve Dad Cam by sending anonymous crash data when something goes wrong. No personal data or file paths are included.
                    </span>
                  </div>
                  <input
                    type="checkbox"
                    className="feature-toggle-switch"
                    checked={diagnosticsEnabled}
                    onChange={(e) => handleDiagnosticsToggle(e.target.checked)}
                    disabled={isSaving}
                  />
                </label>
              </div>

              <h2 className="settings-section-title" style={{ marginTop: 'var(--space-lg)' }}>Log Files</h2>
              <p className="settings-section-description">
                Local logs are always saved for troubleshooting, even when crash reporting is off.
                {logDir && <><br /><span style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{logDir}</span></>}
              </p>
              <button
                className="primary-button"
                onClick={handleExportLogs}
                disabled={isSaving}
                style={{ marginTop: 'var(--space-sm)' }}
              >
                Export log files
              </button>
              {exportResult && (
                <p className="settings-section-description" style={{ marginTop: 'var(--space-xs)' }}>
                  {exportResult}
                </p>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
