// Dad Cam - Left Nav Settings Section
// Braun Design Language v1.0.0 - Nav link to settings page

interface SettingsSectionProps {
  onNavigateToSettings: () => void;
}

export function SettingsSection({ onNavigateToSettings }: SettingsSectionProps) {
  return (
    <div className="nav-section nav-section-settings">
      <button
        className="nav-settings-link"
        onClick={onNavigateToSettings}
        title="Open settings"
      >
        <svg className="nav-section-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="10" cy="10" r="3" />
          <path d="M10 2v2M10 16v2M2 10h2M16 10h2M4.2 4.2l1.4 1.4M14.4 14.4l1.4 1.4M4.2 15.8l1.4-1.4M14.4 5.6l1.4-1.4" />
        </svg>
        <span className="nav-settings-label">Settings</span>
        <svg className="nav-settings-chevron" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M6 4l4 4-4 4" />
        </svg>
      </button>
    </div>
  );
}
