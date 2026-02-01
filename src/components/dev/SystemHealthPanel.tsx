// Dad Cam - System Health Panel (Dev Menu sub-component)

import { useState, useEffect } from 'react';
import { getSystemHealth, type SystemHealth } from '../../api/diagnostics';

export function SystemHealthPanel() {
  const [health, setHealth] = useState<SystemHealth | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    refresh();
  }, []);

  const refresh = async () => {
    setLoading(true);
    try {
      const h = await getSystemHealth();
      setHealth(h);
    } catch {
      setHealth(null);
    } finally {
      setLoading(false);
    }
  };

  const totalPending = health ? health.pendingJobs.reduce((sum, [, count]) => sum + count, 0) : 0;

  return (
    <div className="devmenu-form-group">
      <label className="devmenu-label">
        System Health
        <button
          className="devmenu-link-btn"
          onClick={refresh}
          disabled={loading}
          style={{ marginLeft: 12 }}
        >
          {loading ? 'Loading...' : 'Refresh'}
        </button>
      </label>

      {health ? (
        <>
          <div className="health-grid">
            <div className="health-stat">
              <span className="health-stat-label">Pending Jobs</span>
              <span className={`health-stat-value ${totalPending === 0 ? 'is-zero' : ''}`}>
                {totalPending}
              </span>
            </div>
            <div className="health-stat">
              <span className="health-stat-label">Failed (24h)</span>
              <span className={`health-stat-value ${health.failedJobs24h > 0 ? 'is-error' : 'is-zero'}`}>
                {health.failedJobs24h}
              </span>
            </div>
            <div className="health-stat">
              <span className="health-stat-label">Originals</span>
              <span className="health-stat-value">{health.originalsSize}</span>
            </div>
            <div className="health-stat">
              <span className="health-stat-label">Derived</span>
              <span className="health-stat-value">{health.derivedSize}</span>
            </div>
            <div className="health-stat">
              <span className="health-stat-label">Database</span>
              <span className="health-stat-value">{health.dbSize}</span>
            </div>
          </div>

          {health.pendingJobs.length > 0 && (
            <div>
              <label className="devmenu-label" style={{ fontSize: 12 }}>Pending Breakdown</label>
              <ul className="health-pending-list">
                {health.pendingJobs.map(([type, count]) => (
                  <li key={type}>
                    <span>{type}</span>
                    <span>{count}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {health.lastError && (
            <div>
              <label className="devmenu-label" style={{ fontSize: 12 }}>Last Error</label>
              <div className="health-last-error">{health.lastError}</div>
            </div>
          )}
        </>
      ) : (
        <span className="devmenu-hint">Open a library to see health data.</span>
      )}
    </div>
  );
}
