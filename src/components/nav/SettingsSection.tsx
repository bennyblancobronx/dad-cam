// Dad Cam - Left Nav Settings Section
// Shows mode info and settings access

import type { AppMode } from '../../types/settings';

interface SettingsSectionProps {
  mode: AppMode;
  onOpenSettings: () => void;
}

export function SettingsSection({ mode, onOpenSettings }: SettingsSectionProps) {
  return (
    <div className="nav-section nav-section-settings">
      <div className="nav-section-header">
        <svg className="nav-section-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="10" cy="10" r="3" />
          <path d="M10 2v2M10 16v2M2 10h2M16 10h2M4.2 4.2l1.4 1.4M14.4 14.4l1.4 1.4M4.2 15.8l1.4-1.4M14.4 5.6l1.4-1.4" />
        </svg>
        <h3 className="nav-section-title">Settings</h3>
      </div>
      <div className="nav-settings-content">
        <div className="nav-settings-mode">
          Mode: {mode === 'personal' ? 'Personal' : 'Pro'}
        </div>
        <button
          className="nav-settings-button"
          onClick={onOpenSettings}
        >
          Switch to {mode === 'personal' ? 'Pro' : 'Personal'}
        </button>
      </div>
    </div>
  );
}
