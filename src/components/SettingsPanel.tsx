// Dad Cam - Settings Panel Component
// Mode toggle and app settings

import { useState } from 'react';
import type { AppSettings, AppMode } from '../types/settings';
import { setMode } from '../api/settings';
import { APP_VERSION } from '../constants';

interface SettingsPanelProps {
  settings: AppSettings;
  onSettingsChange: (settings: AppSettings) => void;
  onClose: () => void;
}

export function SettingsPanel({ settings, onSettingsChange, onClose }: SettingsPanelProps) {
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
    <div className="settings-backdrop" onClick={onClose}>
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <h2 className="settings-title">Settings</h2>
          <button className="settings-close" onClick={onClose} aria-label="Close settings" title="Close settings">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M15 5L5 15M5 5l10 10" />
            </svg>
          </button>
        </div>

        <div className="settings-body">
          {error && <div className="error-message">{error}</div>}

          <div className="settings-section">
            <h3 className="settings-section-title">App Mode</h3>
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
          </div>

          <div className="settings-section">
            <h3 className="settings-section-title">About</h3>
            <div className="settings-about">
              <p>Dad Cam v{APP_VERSION}</p>
              <p className="settings-about-muted">Video library for dad cam footage</p>
            </div>
          </div>
        </div>

        <div className="settings-footer">
          <button className="primary-button" onClick={onClose} disabled={isSaving}>
            {isSaving ? 'Saving...' : 'Done'}
          </button>
        </div>
      </div>
    </div>
  );
}
