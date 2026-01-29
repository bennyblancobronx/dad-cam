// Dad Cam - First Run Wizard
// Shown once on first launch. Sets mode and default project path.

import { useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import type { AppSettings, AppMode } from '../types/settings';
import { saveAppSettings } from '../api/settings';
import { createLibrary } from '../api/clips';
import {
  DEFAULT_FEATURE_FLAGS_SIMPLE,
  DEFAULT_FEATURE_FLAGS_ADVANCED,
} from '../types/settings';

interface FirstRunWizardProps {
  settings: AppSettings;
  onComplete: (settings: AppSettings) => void;
}

type WizardStep = 'mode' | 'project';

export function FirstRunWizard({ settings, onComplete }: FirstRunWizardProps) {
  const [step, setStep] = useState<WizardStep>('mode');
  const [selectedMode, setSelectedMode] = useState<AppMode>('simple');
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Step 1 -> Step 2 transition
  const handleModeNext = () => {
    if (selectedMode === 'advanced') {
      // Advanced: finish wizard, normal routing shows dashboard
      handleFinish(null);
    } else {
      // Simple: go to project folder picker step
      setStep('project');
    }
  };

  // Open native folder picker
  const handleBrowseFolder = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder for Default Project',
      });
      if (selected) {
        setProjectPath(selected as string);
      }
    } catch (err) {
      console.error('Failed to open folder picker:', err);
    }
  };

  // Complete wizard: save settings, optionally create project
  const handleFinish = async (path: string | null) => {
    setIsSaving(true);
    setError(null);

    try {
      const updatedSettings: AppSettings = {
        ...settings,
        mode: selectedMode,
        firstRunCompleted: true,
        featureFlags: selectedMode === 'simple'
          ? DEFAULT_FEATURE_FLAGS_SIMPLE
          : DEFAULT_FEATURE_FLAGS_ADVANCED,
        defaultProjectPath: path,
      };

      // Simple mode with a chosen path: create the project
      if (selectedMode === 'simple' && path) {
        try {
          await createLibrary(path, 'Default Project');
        } catch (err) {
          // If creation fails (e.g. project already exists there), still
          // save settings with the path -- user can open it normally.
          console.error('Failed to create default project:', err);
        }
      }

      await saveAppSettings(updatedSettings);
      onComplete(updatedSettings);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Failed to save settings'
      );
      setIsSaving(false);
    }
  };

  // Step 1: Mode selection
  if (step === 'mode') {
    return (
      <div className="app-welcome">
        <div className="welcome-container">
          <h1 className="welcome-title">dad cam</h1>
          <p className="welcome-subtitle">welcome -- choose how you want to use the app</p>

          {error && <div className="error-message">{error}</div>}

          <div className="wizard-mode-options">
            <button
              className={`wizard-mode-card ${selectedMode === 'simple' ? 'is-selected' : ''}`}
              onClick={() => setSelectedMode('simple')}
              disabled={isSaving}
            >
              <span className="wizard-mode-title">Simple</span>
              <span className="wizard-mode-description">
                One project, automatic camera matching, no setup needed.
                Best for home use.
              </span>
            </button>

            <button
              className={`wizard-mode-card ${selectedMode === 'advanced' ? 'is-selected' : ''}`}
              onClick={() => setSelectedMode('advanced')}
              disabled={isSaving}
            >
              <span className="wizard-mode-title">Advanced</span>
              <span className="wizard-mode-description">
                Multiple projects, camera registration, feature toggles.
                For professionals and power users.
              </span>
            </button>
          </div>

          <button
            className="primary-button"
            onClick={handleModeNext}
            disabled={isSaving}
            style={{ marginTop: '24px' }}
          >
            {isSaving ? 'Saving...' : selectedMode === 'advanced' ? 'Get Started' : 'Next'}
          </button>

          <p className="wizard-hint">
            You can change this later in Settings.
          </p>
        </div>
      </div>
    );
  }

  // Step 2 (Simple only): Pick default project folder
  return (
    <div className="app-welcome">
      <div className="welcome-container">
        <h1 className="welcome-title">dad cam</h1>
        <p className="welcome-subtitle">pick a folder for your project</p>

        {error && <div className="error-message">{error}</div>}

        <div className="wizard-project-setup">
          <p className="wizard-project-hint">
            Choose where to store your video project. A "Default Project" will
            be created at this location.
          </p>

          <div className="input-group">
            <div className="input-with-button">
              <input
                type="text"
                placeholder="Select a folder..."
                value={projectPath || ''}
                readOnly
                disabled={isSaving}
              />
              <button
                className="browse-button"
                onClick={handleBrowseFolder}
                disabled={isSaving}
                type="button"
              >
                Browse
              </button>
            </div>
          </div>
        </div>

        <div className="button-group" style={{ marginTop: '24px' }}>
          <button
            className="primary-button"
            onClick={() => handleFinish(projectPath)}
            disabled={isSaving || !projectPath}
          >
            {isSaving ? 'Creating...' : 'Create Project'}
          </button>
          <button
            className="secondary-button"
            onClick={() => setStep('mode')}
            disabled={isSaving}
          >
            Back
          </button>
        </div>

        <button
          className="wizard-skip-link"
          onClick={() => handleFinish(null)}
          disabled={isSaving}
          style={{
            marginTop: '16px',
            background: 'none',
            border: 'none',
            color: 'var(--color-text-muted, #888)',
            cursor: 'pointer',
            textDecoration: 'underline',
            fontSize: '13px',
          }}
        >
          Skip -- I will set this up later
        </button>
      </div>
    </div>
  );
}
