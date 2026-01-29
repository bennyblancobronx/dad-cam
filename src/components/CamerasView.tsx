// Dad Cam - Cameras View
// Full cameras tab content for Advanced mode

import { useState, useEffect, useCallback } from 'react';
import type { CameraProfile, CameraDevice } from '../types/cameras';
import { listCameraProfiles, listCameraDevices, registerCameraDevice } from '../api/cameras';

export function CamerasView() {
  const [profiles, setProfiles] = useState<CameraProfile[]>([]);
  const [devices, setDevices] = useState<CameraDevice[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  // Registration form
  const [showRegister, setShowRegister] = useState(false);
  const [regProfileId, setRegProfileId] = useState<string>('');
  const [regSerial, setRegSerial] = useState('');
  const [regLabel, setRegLabel] = useState('');
  const [regNotes, setRegNotes] = useState('');

  const loadData = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [p, d] = await Promise.all([listCameraProfiles(), listCameraDevices()]);
      setProfiles(p);
      setDevices(d);
    } catch (err) {
      setError(typeof err === 'string' ? err : 'Failed to load camera data');
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => { loadData(); }, [loadData]);

  const handleRegister = async () => {
    try {
      await registerCameraDevice({
        profileId: regProfileId ? parseInt(regProfileId, 10) : undefined,
        serialNumber: regSerial.trim() || undefined,
        fleetLabel: regLabel.trim() || undefined,
        rentalNotes: regNotes.trim() || undefined,
        captureUsb: true,
      });
      setMessage('Device registered');
      setShowRegister(false);
      setRegProfileId('');
      setRegSerial('');
      setRegLabel('');
      setRegNotes('');
      setTimeout(() => setMessage(null), 3000);
      await loadData();
    } catch (err) {
      setError(typeof err === 'string' ? err : 'Failed to register device');
    }
  };

  const getProfileName = (profileId: number | null): string => {
    if (!profileId) return 'Unknown';
    const profile = profiles.find(p => p.id === profileId);
    return profile ? profile.name : `Profile #${profileId}`;
  };

  if (isLoading) {
    return (
      <div className="settings-section">
        <h2 className="settings-section-title">Cameras</h2>
        <p className="settings-section-description">Loading camera data...</p>
      </div>
    );
  }

  return (
    <div className="cameras-view">
      <div className="settings-section">
        <h2 className="settings-section-title">Cameras</h2>
        <p className="settings-section-description">
          Registered devices and matched camera profiles.
        </p>

        {error && <div className="error-message">{error}</div>}
        {message && <div className="devmenu-message">{message}</div>}

        {/* Registered Devices */}
        <div style={{ marginTop: 16 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <h3 className="settings-section-title" style={{ fontSize: 14, margin: 0 }}>
              Registered Devices ({devices.length})
            </h3>
            <button className="secondary-button" onClick={() => setShowRegister(!showRegister)}>
              {showRegister ? 'Cancel' : 'Register Device'}
            </button>
          </div>

          {showRegister && (
            <div style={{ marginTop: 12, padding: 12, border: '1px solid var(--color-border)', borderRadius: 4 }}>
              <div className="form-group">
                <label className="form-label">CAMERA PROFILE</label>
                <select
                  className="form-select"
                  value={regProfileId}
                  onChange={(e) => setRegProfileId(e.target.value)}
                >
                  <option value="">-- None (generic) --</option>
                  {profiles.map((p) => (
                    <option key={p.id} value={String(p.id)}>{p.name}</option>
                  ))}
                </select>
              </div>
              <div className="form-group">
                <label className="form-label">SERIAL NUMBER (optional)</label>
                <input className="form-input" value={regSerial} onChange={(e) => setRegSerial(e.target.value)} placeholder="Camera serial number" />
              </div>
              <div className="form-group">
                <label className="form-label">LABEL (optional)</label>
                <input className="form-input" value={regLabel} onChange={(e) => setRegLabel(e.target.value)} placeholder="e.g. Dad's camcorder" />
              </div>
              <div className="form-group">
                <label className="form-label">NOTES (optional)</label>
                <input className="form-input" value={regNotes} onChange={(e) => setRegNotes(e.target.value)} placeholder="Optional notes" />
              </div>
              <button className="primary-button" onClick={handleRegister} style={{ marginTop: 8 }}>
                Register
              </button>
            </div>
          )}

          {devices.length === 0 && !showRegister && (
            <p className="settings-section-description" style={{ marginTop: 8 }}>
              No devices registered. Register a camera to improve metadata matching.
            </p>
          )}

          {devices.map((d) => (
            <div key={d.id} style={{ padding: '8px 0', borderBottom: '1px solid var(--color-border)' }}>
              <div style={{ fontWeight: 500, fontSize: 14 }}>
                {d.fleetLabel || getProfileName(d.profileId) || `Device #${d.id}`}
              </div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginTop: 2 }}>
                {d.profileId ? getProfileName(d.profileId) : 'No profile'}
                {d.serialNumber ? ` -- S/N: ${d.serialNumber}` : ''}
              </div>
            </div>
          ))}
        </div>

        {/* Camera Profiles */}
        <div style={{ marginTop: 24 }}>
          <h3 className="settings-section-title" style={{ fontSize: 14 }}>
            Camera Profiles ({profiles.length})
          </h3>

          {profiles.length === 0 && (
            <p className="settings-section-description">
              No camera profiles loaded from the bundled database.
            </p>
          )}

          {profiles.map((p) => (
            <div key={p.id} style={{ padding: '8px 0', borderBottom: '1px solid var(--color-border)' }}>
              <div style={{ fontWeight: 500, fontSize: 14 }}>{p.name}</div>
              <div style={{ fontSize: 12, color: 'var(--color-text-secondary)', marginTop: 2 }}>
                {p.matchRules.make?.join(', ') || ''} {p.matchRules.model?.join(', ') || ''}
                {' -- v' + p.version}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
