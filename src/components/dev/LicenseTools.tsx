// Dad Cam - License Tools (Dev Menu sub-component)

import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { LicenseState } from '../../types/licensing';
import { getLicenseState, activateLicense, deactivateLicense } from '../../api/licensing';

interface LicenseToolsProps {
  showMessage: (msg: string) => void;
  showError: (msg: string) => void;
  copyToClipboard: (text: string) => void;
}

export function LicenseTools({ showMessage, showError, copyToClipboard }: LicenseToolsProps) {
  const [licenseState, setLicenseState] = useState<LicenseState | null>(null);
  const [rentalKeys, setRentalKeys] = useState<string[]>([]);
  const [rentalCount, setRentalCount] = useState(5);
  const [keyInput, setKeyInput] = useState('');
  const [isActivating, setIsActivating] = useState(false);

  const loadLicense = useCallback(async () => {
    try {
      const state = await getLicenseState();
      setLicenseState(state);
    } catch (_err) {
      showError('Failed to load license state');
    }
  }, [showError]);

  // Load license on mount
  useEffect(() => { loadLicense(); }, [loadLicense]);

  const handleActivateKey = async () => {
    const key = keyInput.trim();
    if (!key) return;

    setIsActivating(true);
    try {
      const state = await activateLicense(key);
      setLicenseState(state);
      setKeyInput('');
      showMessage(`License activated: ${state.licenseType}`);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Invalid license key');
    } finally {
      setIsActivating(false);
    }
  };

  const handleGenerateRentalKeys = async () => {
    try {
      const keys = await invoke<string[]>('generate_rental_keys', { count: rentalCount });
      setRentalKeys(keys);
      showMessage(`Generated ${keys.length} rental keys`);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to generate keys');
    }
  };

  const handleClearLicense = async () => {
    try {
      const state = await deactivateLicense();
      setLicenseState(state);
      showMessage('License cleared -- reverted to trial');
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to clear license');
    }
  };

  return (
    <div className="settings-section">
      <h2 className="settings-section-title">License Tools</h2>
      <p className="settings-section-description">
        View and manage licensing state for testing.
      </p>

      {licenseState && (
        <div className="devmenu-license-info">
          <div className="devmenu-info-row">
            <span className="devmenu-info-label">Type:</span>
            <span className="devmenu-info-value">{licenseState.licenseType}</span>
          </div>
          <div className="devmenu-info-row">
            <span className="devmenu-info-label">Active:</span>
            <span className="devmenu-info-value">{licenseState.isActive ? 'Yes' : 'No'}</span>
          </div>
          {licenseState.trialDaysRemaining !== null && (
            <div className="devmenu-info-row">
              <span className="devmenu-info-label">Trial Days Left:</span>
              <span className="devmenu-info-value">{licenseState.trialDaysRemaining}</span>
            </div>
          )}
          {licenseState.keyHash && (
            <div className="devmenu-info-row">
              <span className="devmenu-info-label">Key Hash:</span>
              <span className="devmenu-info-value devmenu-mono">{licenseState.keyHash}</span>
            </div>
          )}
        </div>
      )}

      {/* Activate Key (inline) */}
      <div className="devmenu-form-group" style={{ marginTop: 16 }}>
        <label className="devmenu-label">Activate License Key</label>
        <div className="devmenu-inline">
          <input
            type="text"
            className="devmenu-input"
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleActivateKey()}
            placeholder="DCAM-P-xxxx or DCAM-R-xxxx or DCAM-D-xxxx"
            disabled={isActivating}
          />
          <button
            className="primary-button"
            onClick={handleActivateKey}
            disabled={isActivating || !keyInput.trim()}
          >
            {isActivating ? 'Validating...' : 'Activate'}
          </button>
        </div>
        <span className="devmenu-hint">Enter a purchased, rental, or dev license key</span>
      </div>

      <div className="devmenu-actions" style={{ marginTop: 16 }}>
        <button className="secondary-button" onClick={loadLicense}>
          Refresh State
        </button>
        <button className="secondary-button" onClick={handleClearLicense}>
          Clear License (Reset to Trial)
        </button>
      </div>

      <div className="devmenu-form-group" style={{ marginTop: 24 }}>
        <label className="devmenu-label">Generate Rental Keys</label>
        <div className="devmenu-inline">
          <input
            type="number"
            className="devmenu-input devmenu-input-sm"
            value={rentalCount}
            onChange={(e) => setRentalCount(Math.min(100, Math.max(1, parseInt(e.target.value) || 1)))}
            min={1}
            max={100}
          />
          <button className="secondary-button" onClick={handleGenerateRentalKeys}>
            Generate
          </button>
        </div>

        {rentalKeys.length > 0 && (
          <div className="devmenu-keys">
            <div className="devmenu-keys-header">
              <span>{rentalKeys.length} keys generated</span>
              <button
                className="devmenu-link-btn"
                onClick={() => copyToClipboard(rentalKeys.join('\n'))}
              >
                Copy All
              </button>
            </div>
            <pre className="devmenu-pre">{rentalKeys.join('\n')}</pre>
          </div>
        )}
      </div>
    </div>
  );
}
