// Dad Cam - Project Card Component (Advanced Mode)
// Card display for projects in the Project Dashboard grid

import { convertFileSrc } from '@tauri-apps/api/core';
import { RecentProject } from '../types/settings';

interface LibraryCardProps {
  library: RecentProject;
  onSelect: () => void;
  isLoading?: boolean;
  isSelected?: boolean;
}

/** Format relative time for last opened date */
function formatLastOpened(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));

  if (diffDays === 0) return 'Today';
  if (diffDays === 1) return 'Yesterday';
  if (diffDays < 7) return `${diffDays} days ago`;

  const weeks = Math.floor(diffDays / 7);
  if (diffDays < 30) return `${weeks} ${weeks === 1 ? 'week' : 'weeks'} ago`;

  const months = Math.floor(diffDays / 30);
  if (diffDays < 365) return `${months} ${months === 1 ? 'month' : 'months'} ago`;

  const years = Math.floor(diffDays / 365);
  return `${years} ${years === 1 ? 'year' : 'years'} ago`;
}

export function LibraryCard({ library, onSelect, isLoading, isSelected }: LibraryCardProps) {
  return (
    <button
      className={`library-card${isSelected ? ' is-selected' : ''}`}
      onClick={onSelect}
      disabled={isLoading}
      type="button"
    >
      {/* Thumbnail area */}
      <div className="library-card-image">
        {library.thumbnailPath ? (
          <img
            src={convertFileSrc(library.thumbnailPath)}
            alt={library.name}
            loading="lazy"
          />
        ) : (
          <div className="library-card-placeholder">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
              <path d="M3 7a2 2 0 012-2h14a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" />
              <path d="M3 7l9 6 9-6" />
            </svg>
          </div>
        )}
      </div>

      {/* Content area */}
      <div className="library-card-content">
        <div className="library-card-title">{library.name}</div>
        <div className="library-card-meta">
          {library.clipCount} clips
          <span className="library-card-separator">-</span>
          {formatLastOpened(library.lastOpened)}
        </div>
      </div>
    </button>
  );
}
