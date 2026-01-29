// Dad Cam - Camera DB Manager (Dev Menu sub-component)

import { useState, useEffect, useCallback } from 'react';
import { open, save } from '@tauri-apps/plugin-dialog';
import type { CameraProfile, CameraDevice } from '../../types/cameras';
import {
  listCameraProfiles,
  listCameraDevices,
  importCameraDb,
  exportCameraDb,
} from '../../api/cameras';

interface CameraDbManagerProps {
  showMessage: (msg: string) => void;
  showError: (msg: string) => void;
  copyToClipboard: (text: string) => void;
}

export function CameraDbManager({ showMessage, showError, copyToClipboard: _copyToClipboard }: CameraDbManagerProps) {
  const [profiles, setProfiles] = useState<CameraProfile[]>([]);
  const [devices, setDevices] = useState<CameraDevice[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const loadData = useCallback(async () => {
    setIsLoading(true);
    try {
      const [p, d] = await Promise.all([listCameraProfiles(), listCameraDevices()]);
      setProfiles(p);
      setDevices(d);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to load camera data');
    } finally {
      setIsLoading(false);
    }
  }, [showError]);

  useEffect(() => { loadData(); }, [loadData]);

  const handleImport = async () => {
    try {
      const selected = await open({
        title: 'Import Camera Database',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!selected) return;
      const result = await importCameraDb(selected as string);
      showMessage(`Imported ${result.profilesImported} profiles, ${result.devicesImported} devices`);
      loadData();
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to import');
    }
  };

  const handleExport = async () => {
    try {
      const outputPath = await save({
        title: 'Export Camera Database',
        defaultPath: 'cameras.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!outputPath) return;
      const result = await exportCameraDb(outputPath);
      showMessage(`Exported ${result.profilesCount} profiles, ${result.devicesCount} devices`);
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to export');
    }
  };

  return (
    <div className="settings-section">
      <h2 className="settings-section-title">Camera Database</h2>
      <p className="settings-section-description">
        View and manage the camera profile and device databases.
      </p>

      {/* Actions */}
      <div className="devmenu-actions">
        <button className="secondary-button" onClick={loadData} disabled={isLoading}>
          {isLoading ? 'Loading...' : 'Refresh'}
        </button>
        <button className="secondary-button" onClick={handleImport}>
          Import JSON
        </button>
        <button className="secondary-button" onClick={handleExport}>
          Export JSON
        </button>
      </div>

      {/* Bundled Profiles (read-only list) */}
      <div className="devmenu-form-group" style={{ marginTop: 24 }}>
        <label className="devmenu-label">Bundled Profiles ({profiles.length})</label>
        {profiles.length === 0 ? (
          <div className="devmenu-hint">No profiles loaded.</div>
        ) : (
          <div className="devmenu-table-scroll">
            <table className="devmenu-table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Name</th>
                  <th>Make</th>
                  <th>Model</th>
                  <th>Ver</th>
                </tr>
              </thead>
              <tbody>
                {profiles.map((p) => (
                  <tr key={p.id}>
                    <td>{p.id}</td>
                    <td>{p.name}</td>
                    <td>{p.matchRules.make?.join(', ') || '--'}</td>
                    <td>{p.matchRules.model?.join(', ') || '--'}</td>
                    <td>{p.version}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Custom Devices (list with detail) */}
      <div className="devmenu-form-group" style={{ marginTop: 24 }}>
        <label className="devmenu-label">Custom Devices ({devices.length})</label>
        {devices.length === 0 ? (
          <div className="devmenu-hint">
            No custom devices registered. Use the Cameras tab in Advanced mode to register devices.
          </div>
        ) : (
          <div className="devmenu-table-scroll">
            <table className="devmenu-table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Label</th>
                  <th>Serial</th>
                  <th>Profile</th>
                  <th>USB</th>
                  <th>Created</th>
                </tr>
              </thead>
              <tbody>
                {devices.map((d) => {
                  const profile = profiles.find((p) => p.id === d.profileId);
                  return (
                    <tr key={d.id}>
                      <td>{d.id}</td>
                      <td>{d.fleetLabel || '--'}</td>
                      <td className="devmenu-mono">{d.serialNumber || '--'}</td>
                      <td>{profile?.name || (d.profileId ? `#${d.profileId}` : '--')}</td>
                      <td>{d.usbFingerprints.length > 0 ? `${d.usbFingerprints.length} fp` : '--'}</td>
                      <td>{d.createdAt?.slice(0, 10) || '--'}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div className="devmenu-hint" style={{ marginTop: 16 }}>
        Camera registration is available via the Cameras tab in Advanced mode.
        Import/export operates on the full camera database (profiles + devices).
      </div>
    </div>
  );
}
