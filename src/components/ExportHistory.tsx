// Dad Cam - Export History List
// Extracted from ExportDialog for reuse

import type { ExportHistoryEntry } from '../types/export';

interface ExportHistoryProps {
  history: ExportHistoryEntry[];
}

export function ExportHistory({ history }: ExportHistoryProps) {
  if (history.length === 0) return null;

  return (
    <div style={{ marginTop: '24px', borderTop: '1px solid var(--color-border)', paddingTop: '16px' }}>
      <label className="form-label">RECENT EXPORTS</label>
      <div className="export-history-list">
        {history.map((entry) => (
          <div key={entry.id} className="export-history-item">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span style={{ fontSize: '13px', fontWeight: 500 }}>
                {entry.outputPath.split('/').pop() || entry.outputPath}
              </span>
              <span className={`export-status export-status-${entry.status}`}>
                {entry.status}
              </span>
            </div>
            <div style={{ fontSize: '12px', color: 'var(--color-text-secondary)', marginTop: '2px' }}>
              {entry.clipCount ? `${entry.clipCount} clips` : ''}
              {entry.fileSizeBytes ? ` -- ${formatFileSize(entry.fileSizeBytes)}` : ''}
              {entry.resolution ? ` -- ${entry.resolution}` : ''}
              {entry.isWatermarked ? ' -- Trial' : ''}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}
