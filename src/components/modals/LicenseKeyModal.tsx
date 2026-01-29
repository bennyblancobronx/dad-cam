// Dad Cam - License Key Entry Modal
// Text input + validate button with success/error feedback

import { useState, useCallback } from 'react';
import type { LicenseState } from '../../types/licensing';
import { activateLicense } from '../../api/licensing';

interface LicenseKeyModalProps {
  onClose: () => void;
  onLicenseChange: (state: LicenseState) => void;
}

export function LicenseKeyModal({ onClose, onLicenseChange }: LicenseKeyModalProps) {
  const [key, setKey] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [isActivating, setIsActivating] = useState(false);
  const [success, setSuccess] = useState(false);

  const handleActivate = useCallback(async () => {
    if (!key.trim()) {
      setError('Please enter a license key');
      return;
    }

    setIsActivating(true);
    setError(null);

    try {
      const state = await activateLicense(key.trim());
      setSuccess(true);
      onLicenseChange(state);
      // Close after brief delay so user sees success
      setTimeout(() => onClose(), 1200);
    } catch (err) {
      setError(
        typeof err === 'string' ? err : err instanceof Error ? err.message : 'Invalid license key'
      );
    } finally {
      setIsActivating(false);
    }
  }, [key, onClose, onLicenseChange]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && !isActivating) {
        handleActivate();
      }
      if (e.key === 'Escape' && !isActivating) {
        onClose();
      }
    },
    [handleActivate, isActivating, onClose]
  );

  return (
    <div className="modal-overlay" onClick={(e) => {
      if (e.target === e.currentTarget && !isActivating) onClose();
    }}>
      <div className="modal-content license-key-modal" onKeyDown={handleKeyDown}>
        <div className="modal-header">
          <h2 className="modal-title">Activate License</h2>
          <button
            className="modal-close"
            onClick={onClose}
            disabled={isActivating}
            aria-label="Close"
          >
            x
          </button>
        </div>

        <div className="modal-body">
          {success ? (
            <div className="license-success">
              <p>License activated successfully.</p>
            </div>
          ) : (
            <>
              <div className="input-group">
                <label htmlFor="license-key-input">License Key</label>
                <input
                  id="license-key-input"
                  type="text"
                  placeholder="DCAM-P-..."
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  disabled={isActivating}
                  autoFocus
                  spellCheck={false}
                  autoComplete="off"
                />
              </div>

              {error && <div className="error-message">{error}</div>}

              <div className="modal-actions">
                <button
                  className="primary-button"
                  onClick={handleActivate}
                  disabled={isActivating || !key.trim()}
                >
                  {isActivating ? 'Activating...' : 'Activate'}
                </button>
                <button
                  className="secondary-button"
                  onClick={onClose}
                  disabled={isActivating}
                >
                  Cancel
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
