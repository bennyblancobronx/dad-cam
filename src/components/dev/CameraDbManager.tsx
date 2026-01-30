// Dad Cam - Camera DB Manager (Dev Menu sub-component)

import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import type { CameraProfile, CameraDevice } from '../../types/cameras';
import { parseMatchRules } from '../../types/cameras';
import {
  listCameraProfiles,
  listCameraDevices,
  importCameraDb,
  exportCameraDb,
} from '../../api/cameras';

interface StagedProfile {
  id: number;
  sourceType: string;
  sourceRef: string;
  name: string;
  matchRules: string;
  transformRules: string;
  createdAt: string;
}

interface CameraDbManagerProps {
  showMessage: (msg: string) => void;
  showError: (msg: string) => void;
  copyToClipboard: (text: string) => void;
}

export function CameraDbManager({ showMessage, showError, copyToClipboard: _copyToClipboard }: CameraDbManagerProps) {
  const [profiles, setProfiles] = useState<CameraProfile[]>([]);
  const [devices, setDevices] = useState<CameraDevice[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  // Staging state (spec 3.6 -- authoring writes to staging before publish)
  const [staged, setStaged] = useState<StagedProfile[]>([]);
  const [stageName, setStageName] = useState('');
  const [stageMatchRules, setStageMatchRules] = useState('{}');
  const [stageTransformRules, setStageTransformRules] = useState('{}');
  const [stageSourceType, setStageSourceType] = useState<'new' | 'user'>('new');
  const [stageSourceRef, setStageSourceRef] = useState('');
  const [validationErrors, setValidationErrors] = useState<Array<[number, string]>>([]);

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

  const loadStaged = useCallback(async () => {
    try {
      const list = await invoke<StagedProfile[]>('list_staged_profiles');
      setStaged(list);
    } catch {
      // staging not available (no dev license or DB issue) -- ignore
    }
  }, []);

  useEffect(() => { loadData(); loadStaged(); }, [loadData, loadStaged]);

  const handleStageProfile = async () => {
    if (!stageName.trim()) { showError('Profile name is required'); return; }
    try {
      await invoke('stage_profile_edit', {
        sourceType: stageSourceType,
        sourceRef: stageSourceRef,
        name: stageName,
        matchRules: stageMatchRules,
        transformRules: stageTransformRules,
      });
      showMessage('Profile staged for validation');
      setStageName('');
      setStageMatchRules('{}');
      setStageTransformRules('{}');
      setStageSourceRef('');
      setValidationErrors([]);
      loadStaged();
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Failed to stage profile');
    }
  };

  const handleValidateStaged = async () => {
    try {
      const errors = await invoke<Array<[number, string]>>('validate_staged_profiles');
      setValidationErrors(errors);
      if (errors.length === 0) {
        showMessage('All staged profiles passed validation');
      } else {
        showError(`${errors.length} validation error(s) found`);
      }
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Validation failed');
    }
  };

  const handlePublishStaged = async () => {
    try {
      const count = await invoke<number>('publish_staged_profiles');
      showMessage(`Published ${count} profile(s)`);
      setValidationErrors([]);
      loadStaged();
      loadData();
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Publish failed');
    }
  };

  const handleDiscardStaged = async () => {
    try {
      const count = await invoke<number>('discard_staged_profiles');
      showMessage(`Discarded ${count} staged profile(s)`);
      setValidationErrors([]);
      loadStaged();
    } catch (err) {
      showError(typeof err === 'string' ? err : 'Discard failed');
    }
  };

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
      showMessage(`Exported ${result.bundledProfilesCount} bundled + ${result.userProfilesCount} user profiles, ${result.devicesCount} devices`);
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
        <label className="devmenu-label">Profiles ({profiles.length})</label>
        {profiles.length === 0 ? (
          <div className="devmenu-hint">No profiles loaded.</div>
        ) : (
          <div className="devmenu-table-scroll">
            <table className="devmenu-table">
              <thead>
                <tr>
                  <th>Type</th>
                  <th>Ref</th>
                  <th>Name</th>
                  <th>Make</th>
                  <th>Ver</th>
                </tr>
              </thead>
              <tbody>
                {profiles.map((p) => {
                  const rules = parseMatchRules(p.matchRules);
                  return (
                    <tr key={`${p.profileType}:${p.profileRef}`}>
                      <td>{p.profileType}</td>
                      <td className="devmenu-mono">{p.profileRef.slice(0, 12)}</td>
                      <td>{p.name}</td>
                      <td>{rules.make?.join(', ') || '--'}</td>
                      <td>{p.version}</td>
                    </tr>
                  );
                })}
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
                  <th>UUID</th>
                  <th>Label</th>
                  <th>Serial</th>
                  <th>Profile</th>
                  <th>USB</th>
                  <th>Created</th>
                </tr>
              </thead>
              <tbody>
                {devices.map((d) => {
                  const profile = profiles.find((p) => p.profileType === d.profileType && p.profileRef === d.profileRef);
                  return (
                    <tr key={d.uuid}>
                      <td className="devmenu-mono">{d.uuid.slice(0, 8)}</td>
                      <td>{d.fleetLabel || '--'}</td>
                      <td className="devmenu-mono">{d.serialNumber || '--'}</td>
                      <td>{profile?.name || (d.profileType !== 'none' ? `${d.profileType}:${d.profileRef.slice(0, 8)}` : '--')}</td>
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

      {/* Profile Staging / Authoring (spec 3.6) */}
      <div className="devmenu-form-group" style={{ marginTop: 32 }}>
        <label className="devmenu-label">Profile Authoring (staging)</label>
        <p className="devmenu-hint" style={{ marginBottom: 12 }}>
          Stage profile edits, validate, then publish. Changes only apply after validation passes.
        </p>

        {/* Stage a new profile */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 16 }}>
          <div className="devmenu-inline">
            <label style={{ display: 'flex', alignItems: 'center', gap: 4, cursor: 'pointer' }}>
              <input
                type="radio"
                name="stageType"
                value="new"
                checked={stageSourceType === 'new'}
                onChange={() => { setStageSourceType('new'); setStageSourceRef(''); }}
              />
              New profile
            </label>
            <label style={{ display: 'flex', alignItems: 'center', gap: 4, cursor: 'pointer' }}>
              <input
                type="radio"
                name="stageType"
                value="user"
                checked={stageSourceType === 'user'}
                onChange={() => setStageSourceType('user')}
              />
              Edit existing (by UUID)
            </label>
          </div>
          {stageSourceType === 'user' && (
            <input
              type="text"
              className="devmenu-input"
              value={stageSourceRef}
              onChange={(e) => setStageSourceRef(e.target.value)}
              placeholder="User profile UUID"
            />
          )}
          <input
            type="text"
            className="devmenu-input"
            value={stageName}
            onChange={(e) => setStageName(e.target.value)}
            placeholder="Profile name"
          />
          <textarea
            className="devmenu-textarea"
            value={stageMatchRules}
            onChange={(e) => setStageMatchRules(e.target.value)}
            placeholder='match_rules JSON, e.g. {"make":["Sony"]}'
            rows={2}
          />
          <textarea
            className="devmenu-textarea"
            value={stageTransformRules}
            onChange={(e) => setStageTransformRules(e.target.value)}
            placeholder='transform_rules JSON, e.g. {"deinterlace":true}'
            rows={2}
          />
          <button
            className="secondary-button"
            onClick={handleStageProfile}
            disabled={!stageName.trim()}
          >
            Stage Profile
          </button>
        </div>

        {/* Staged profiles list */}
        {staged.length > 0 && (
          <>
            <label className="devmenu-label">Staged ({staged.length})</label>
            <div className="devmenu-table-scroll">
              <table className="devmenu-table">
                <thead>
                  <tr>
                    <th>ID</th>
                    <th>Type</th>
                    <th>Name</th>
                    <th>Match Rules</th>
                  </tr>
                </thead>
                <tbody>
                  {staged.map((s) => (
                    <tr key={s.id}>
                      <td>{s.id}</td>
                      <td>{s.sourceType}</td>
                      <td>{s.name}</td>
                      <td className="devmenu-mono">{s.matchRules.slice(0, 40)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Validation errors */}
            {validationErrors.length > 0 && (
              <div className="error-message" style={{ marginTop: 8 }}>
                {validationErrors.map(([id, msg]) => (
                  <div key={`${id}-${msg}`}>#{id}: {msg}</div>
                ))}
              </div>
            )}

            <div className="devmenu-actions" style={{ marginTop: 8 }}>
              <button className="secondary-button" onClick={handleValidateStaged}>
                Validate
              </button>
              <button className="primary-button" onClick={handlePublishStaged}>
                Publish
              </button>
              <button className="secondary-button" onClick={handleDiscardStaged}>
                Discard All
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
