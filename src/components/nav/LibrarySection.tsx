// Dad Cam - Left Nav Library Section
// Shows current library info + Favorites link

import type { LibraryInfo } from '../../types/clips';

interface LibrarySectionProps {
  library: LibraryInfo;
  onNavigateToFavorites?: () => void;
  isFavoritesActive?: boolean;
}

export function LibrarySection({ library, onNavigateToFavorites, isFavoritesActive }: LibrarySectionProps) {
  return (
    <div className="nav-section">
      <div className="nav-section-header">
        <svg className="nav-section-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M3 5a2 2 0 012-2h10a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2V5z" />
          <path d="M7 3v14M3 8h4" />
        </svg>
        <h3 className="nav-section-title">Project</h3>
      </div>
      <div className="nav-library-info">
        <div className="nav-library-name">{library.name}</div>
        <div className="nav-library-meta">{library.clipCount} clips</div>
      </div>
      {onNavigateToFavorites && (
        <button
          className={`nav-item${isFavoritesActive ? ' is-active' : ''}`}
          onClick={onNavigateToFavorites}
          title="Show favorite clips"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M8 2.5l1.7 3.5 3.8.6-2.7 2.7.6 3.7L8 11.2 4.6 13l.6-3.7L2.5 6.6l3.8-.6L8 2.5z" />
          </svg>
          Favorites
        </button>
      )}
    </div>
  );
}
