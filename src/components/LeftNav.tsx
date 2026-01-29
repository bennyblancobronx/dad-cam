// Dad Cam - Left Navigation Bar
// Braun Design Language v1.0.0 - Sidebar navigation

import { useState, useEffect } from 'react';
import type { LibraryInfo } from '../types/clips';
import type { AppMode, FeatureFlags } from '../types/settings';
import type { CameraProfile, CameraDevice } from '../types/cameras';
import { listCameraProfiles, listCameraDevices } from '../api/cameras';
import { LibrarySection } from './nav/LibrarySection';
import { EventsSection } from './nav/EventsSection';
import { DatesSection } from './nav/DatesSection';
import { SettingsSection } from './nav/SettingsSection';

interface LeftNavProps {
  library: LibraryInfo;
  onNavigateToSettings: () => void;
  onNavigateToEvent?: (eventId: number) => void;
  onNavigateToDate?: (date: string) => void;
  onNavigateToFavorites?: () => void;
  /** Currently active date for highlighting in nav */
  activeDate?: string | null;
  /** Whether favorites view is currently active */
  isFavoritesActive?: boolean;
  /** Increment to trigger dates refresh */
  refreshTrigger?: number;
  /** App mode for gating features */
  mode?: AppMode;
  /** Feature flags for gating cameras tab */
  featureFlags?: FeatureFlags;
}

export function LeftNav({
  library,
  onNavigateToSettings,
  onNavigateToEvent,
  onNavigateToDate,
  onNavigateToFavorites,
  activeDate,
  isFavoritesActive,
  refreshTrigger,
  mode,
  featureFlags,
}: LeftNavProps) {
  const showCamerasTab = mode === 'advanced' && featureFlags?.camerasTab === true;

  return (
    <nav className="left-nav">
      <LibrarySection
        library={library}
        onNavigateToFavorites={onNavigateToFavorites}
        isFavoritesActive={isFavoritesActive}
      />
      <EventsSection onNavigateToEvent={onNavigateToEvent} />
      <DatesSection onNavigateToDate={onNavigateToDate} activeDate={activeDate} refreshTrigger={refreshTrigger} />
      {showCamerasTab && <CamerasSection />}
      <SettingsSection onNavigateToSettings={onNavigateToSettings} />
    </nav>
  );
}

/** Cameras section -- shows registered devices and matched profiles */
function CamerasSection() {
  const [profiles, setProfiles] = useState<CameraProfile[]>([]);
  const [devices, setDevices] = useState<CameraDevice[]>([]);
  const [isExpanded, setIsExpanded] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setIsLoading(true);
      try {
        const [p, d] = await Promise.all([listCameraProfiles(), listCameraDevices()]);
        if (!cancelled) {
          setProfiles(p);
          setDevices(d);
        }
      } catch (err) {
        console.error('Failed to load cameras:', err);
      } finally {
        if (!cancelled) setIsLoading(false);
      }
    }
    load();
    return () => { cancelled = true; };
  }, []);

  return (
    <div className="nav-section">
      <button
        className="nav-section-header nav-section-toggle"
        onClick={() => setIsExpanded(!isExpanded)}
        title="Cameras"
      >
        <svg className="nav-section-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
          <rect x="2" y="5" width="16" height="10" rx="2" />
          <circle cx="10" cy="10" r="3" />
          <path d="M14 5V3h-3" />
        </svg>
        <h3 className="nav-section-title">Cameras</h3>
        <svg
          className={`nav-section-chevron${isExpanded ? ' is-expanded' : ''}`}
          width="12" height="12" viewBox="0 0 12 12"
          fill="none" stroke="currentColor" strokeWidth="1.5"
        >
          <path d="M4 5l2 2 2-2" />
        </svg>
      </button>
      {isExpanded && (
        <div className="nav-cameras-list">
          {isLoading && (
            <div className="nav-placeholder">
              <span className="nav-placeholder-text">Loading...</span>
            </div>
          )}
          {!isLoading && devices.length === 0 && profiles.length === 0 && (
            <div className="nav-placeholder">
              <span className="nav-placeholder-text">No cameras registered</span>
            </div>
          )}
          {devices.length > 0 && (
            <>
              <div className="nav-cameras-group-label">Registered Devices</div>
              {devices.map((d) => (
                <div key={d.id} className="nav-cameras-item">
                  <span className="nav-cameras-item-label">{d.fleetLabel || `Device ${d.id}`}</span>
                  {d.serialNumber && (
                    <span className="nav-cameras-item-meta">{d.serialNumber}</span>
                  )}
                </div>
              ))}
            </>
          )}
          {profiles.length > 0 && (
            <>
              <div className="nav-cameras-group-label">Matched Profiles</div>
              {profiles.map((p) => (
                <div key={p.id} className="nav-cameras-item">
                  <span className="nav-cameras-item-label">{p.name}</span>
                </div>
              ))}
            </>
          )}
          {!isLoading && (devices.length > 0 || profiles.length > 0) && (
            <>
              <div className="nav-cameras-group-label">Unknown</div>
              <div className="nav-cameras-item">
                <span className="nav-cameras-item-label nav-cameras-item-unknown">Generic fallback</span>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
