// Dad Cam - Formulas Editor (Dev Menu sub-component)

import { useState, useCallback } from 'react';
import type { AppSettings, DevMenuSettings, ScoreWeights } from '../../types/settings';
import { saveAppSettings } from '../../api/settings';

interface FormulasEditorProps {
  settings: AppSettings;
  onSettingsChange: (settings: AppSettings) => void;
  showMessage: (msg: string) => void;
  showError: (msg: string) => void;
}

export function FormulasEditor({ settings, onSettingsChange, showMessage, showError }: FormulasEditorProps) {
  const [isSaving, setIsSaving] = useState(false);
  const [devMenu, setDevMenu] = useState<DevMenuSettings>({ ...settings.devMenu });

  const saveFormulas = useCallback(async (updated: DevMenuSettings) => {
    setIsSaving(true);
    try {
      const updatedSettings: AppSettings = { ...settings, devMenu: updated };
      await saveAppSettings(updatedSettings);
      onSettingsChange(updatedSettings);
      setDevMenu(updated);
      showMessage('Formulas saved');
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to save');
    } finally {
      setIsSaving(false);
    }
  }, [settings, onSettingsChange, showMessage, showError]);

  const handleWeightChange = (key: keyof ScoreWeights, value: number) => {
    const updated = {
      ...devMenu,
      scoreWeights: { ...devMenu.scoreWeights, [key]: value },
    };
    setDevMenu(updated);
  };

  const resetWeights = () => {
    const updated = {
      ...devMenu,
      scoreWeights: { scene: 0.25, audio: 0.25, sharpness: 0.25, motion: 0.25 },
    };
    setDevMenu(updated);
    saveFormulas(updated);
  };

  return (
    <div className="settings-section">
      <h2 className="settings-section-title">Formulas</h2>
      <p className="settings-section-description">
        Adjust timing, blend, and scoring parameters.
      </p>

      <div className="devmenu-form-group">
        <label className="devmenu-label">Title Start Time (seconds)</label>
        <input
          type="number"
          className="devmenu-input"
          value={devMenu.titleStartSeconds}
          onChange={(e) => setDevMenu({ ...devMenu, titleStartSeconds: parseFloat(e.target.value) || 0 })}
          min={0}
          max={60}
          step={0.5}
        />
        <span className="devmenu-hint">When the opening title text appears in VHS exports</span>
      </div>

      <div className="devmenu-form-group">
        <label className="devmenu-label">J & L Blend Duration (ms)</label>
        <input
          type="number"
          className="devmenu-input"
          value={devMenu.jlBlendMs}
          onChange={(e) => setDevMenu({ ...devMenu, jlBlendMs: parseInt(e.target.value) || 0 })}
          min={0}
          max={5000}
          step={50}
        />
        <span className="devmenu-hint">Audio crossfade duration for J-cuts and L-cuts</span>
      </div>

      <div className="devmenu-form-group">
        <label className="devmenu-label">Score Weights</label>
        <div className="devmenu-weights">
          {(['scene', 'audio', 'sharpness', 'motion'] as const).map((key) => (
            <div key={key} className="devmenu-weight-row">
              <span className="devmenu-weight-label">{key}</span>
              <input
                type="range"
                min={0}
                max={1}
                step={0.05}
                value={devMenu.scoreWeights[key]}
                onChange={(e) => handleWeightChange(key, parseFloat(e.target.value))}
              />
              <span className="devmenu-weight-value">
                {devMenu.scoreWeights[key].toFixed(2)}
              </span>
            </div>
          ))}
        </div>
        <div className="devmenu-weight-actions">
          <button className="secondary-button" onClick={resetWeights} disabled={isSaving}>
            Reset to Equal
          </button>
        </div>
      </div>

      <div className="devmenu-form-group">
        <label className="devmenu-label">Watermark Text (override)</label>
        <input
          type="text"
          className="devmenu-input"
          value={devMenu.watermarkText || ''}
          onChange={(e) => setDevMenu({ ...devMenu, watermarkText: e.target.value || null })}
          placeholder="Leave empty for default"
        />
        <span className="devmenu-hint">Custom watermark for unlicensed exports</span>
      </div>

      <button
        className="primary-button"
        onClick={() => saveFormulas(devMenu)}
        disabled={isSaving}
        style={{ marginTop: 16 }}
      >
        {isSaving ? 'Saving...' : 'Save Formulas'}
      </button>
    </div>
  );
}
