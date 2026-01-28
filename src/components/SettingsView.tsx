// Dad Cam - Settings View (Full Page)
// Braun Design Language v1.0.0 - Settings nav (200px) + Form content (640px max)

import { useState } from 'react';
import type { AppSettings, AppMode } from '../types/settings';
import { setMode } from '../api/settings';
import { APP_VERSION } from '../constants';

interface SettingsViewProps {
  settings: AppSettings;
  onSettingsChange: (settings: AppSettings) => void;
  onBack: () => void;
}

type SettingsSection = 'general' | 'about';

export function SettingsView({ settings, onSettingsChange, onBack }: SettingsViewProps) {
  const [activeSection, setActiveSection] = useState<SettingsSection>('general');
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleModeChange = async (newMode: AppMode) => {
    if (newMode === settings.mode) return;

    setIsSaving(true);
    setError(null);

    try {
      await setMode(newMode);
      onSettingsChange({ ...settings, mode: newMode });
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
                <label className={`mode-option ${settings.mode === 'personal' ? 'is-selected' : ''}`}>
                  <input
                    type="radio"
                    name="mode"
                    value="personal"
                    checked={settings.mode === 'personal'}
                    onChange={() => handleModeChange('personal')}
                    disabled={isSaving}
                  />
                  <div className="mode-option-content">
                    <span className="mode-option-title">Personal</span>
                    <span className="mode-option-description">
                      Single library, auto-opens on launch. Best for home use.
                    </span>
                  </div>
                </label>

                <label className={`mode-option ${settings.mode === 'pro' ? 'is-selected' : ''}`}>
                  <input
                    type="radio"
                    name="mode"
                    value="pro"
                    checked={settings.mode === 'pro'}
                    onChange={() => handleModeChange('pro')}
                    disabled={isSaving}
                  />
                  <div className="mode-option-content">
                    <span className="mode-option-title">Pro</span>
                    <span className="mode-option-description">
                      Multi-library management with library dashboard. For professionals.
                    </span>
                  </div>
                </label>
              </div>

              {isSaving && (
                <p className="settings-saving-indicator">Saving...</p>
              )}
            </div>
          )}

          {activeSection === 'about' && (
            <div className="settings-section">
              <h2 className="settings-section-title">About Dad Cam</h2>
              <div className="settings-about-content">
                <p className="settings-version">Version {APP_VERSION}</p>
                <p className="settings-tagline">Video library for dad cam footage</p>
                <div className="settings-about-details">
                  <p>A modern video library for importing, organizing, viewing, and auto-editing footage from old-school digital cameras.</p>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
