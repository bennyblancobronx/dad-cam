// Dad Cam - Dev Menu
// Access: Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win/Linux)
// Or: Settings > About > click version 7 times

import { useState } from 'react';
import type { AppSettings } from '../types/settings';
import { APP_VERSION } from '../constants';
import { FormulasEditor } from './dev/FormulasEditor';
import { CameraDbManager } from './dev/CameraDbManager';
import { LicenseTools } from './dev/LicenseTools';
import { DebugTools } from './dev/DebugTools';

interface DevMenuProps {
  settings: AppSettings;
  onSettingsChange: (settings: AppSettings) => void;
  onClose: () => void;
}

type DevSection = 'formulas' | 'cameras' | 'license' | 'debug';

export function DevMenu({ settings, onSettingsChange, onClose }: DevMenuProps) {
  const [activeSection, setActiveSection] = useState<DevSection>('formulas');
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const showMessage = (msg: string) => {
    setMessage(msg);
    setError(null);
    setTimeout(() => setMessage(null), 3000);
  };

  const showError = (msg: string) => {
    setError(msg);
    setMessage(null);
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text).then(
      () => showMessage('Copied to clipboard'),
      () => showError('Failed to copy')
    );
  };

  return (
    <div className="devmenu-overlay">
      <div className="devmenu">
        {/* Header */}
        <header className="devmenu-header">
          <div className="devmenu-header-left">
            <button onClick={onClose} className="settings-back-btn" title="Close Dev Menu">
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M15 5L5 15M5 5l10 10" />
              </svg>
              Close
            </button>
            <h1 className="settings-view-title">Dev Menu</h1>
            <span className="devmenu-version">v{APP_VERSION}</span>
          </div>
        </header>

        <div className="settings-view-body">
          {/* Navigation */}
          <nav className="settings-nav">
            <button
              className={`settings-nav-item ${activeSection === 'formulas' ? 'is-active' : ''}`}
              onClick={() => setActiveSection('formulas')}
            >
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M4 6h12M4 10h8M4 14h10" />
              </svg>
              Formulas
            </button>
            <button
              className={`settings-nav-item ${activeSection === 'cameras' ? 'is-active' : ''}`}
              onClick={() => setActiveSection('cameras')}
            >
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="3" y="4" width="14" height="12" rx="2" />
                <circle cx="10" cy="10" r="3" />
              </svg>
              Camera DB
            </button>
            <button
              className={`settings-nav-item ${activeSection === 'license' ? 'is-active' : ''}`}
              onClick={() => setActiveSection('license')}
            >
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
                <rect x="3" y="5" width="14" height="10" rx="1" />
                <path d="M3 9h14" />
              </svg>
              License
            </button>
            <button
              className={`settings-nav-item ${activeSection === 'debug' ? 'is-active' : ''}`}
              onClick={() => setActiveSection('debug')}
            >
              <svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M6 4v3M14 4v3M5 8l3 3-3 3M10 16h5" />
              </svg>
              Debug
            </button>
          </nav>

          {/* Content */}
          <div className="settings-content">
            {message && <div className="devmenu-message">{message}</div>}
            {error && <div className="error-message">{error}</div>}

            {activeSection === 'formulas' && (
              <FormulasEditor
                settings={settings}
                onSettingsChange={onSettingsChange}
                showMessage={showMessage}
                showError={showError}
              />
            )}

            {activeSection === 'cameras' && (
              <CameraDbManager
                showMessage={showMessage}
                showError={showError}
                copyToClipboard={copyToClipboard}
              />
            )}

            {activeSection === 'license' && (
              <LicenseTools
                showMessage={showMessage}
                showError={showError}
                copyToClipboard={copyToClipboard}
              />
            )}

            {activeSection === 'debug' && (
              <DebugTools
                showMessage={showMessage}
                showError={showError}
              />
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
