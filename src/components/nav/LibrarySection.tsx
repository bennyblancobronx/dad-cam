// Dad Cam - Left Nav Library Section
// Shows current library info

import type { LibraryInfo } from '../../types/clips';

interface LibrarySectionProps {
  library: LibraryInfo;
}

export function LibrarySection({ library }: LibrarySectionProps) {
  return (
    <div className="nav-section">
      <div className="nav-section-header">
        <svg className="nav-section-icon" width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M3 5a2 2 0 012-2h10a2 2 0 012 2v10a2 2 0 01-2 2H5a2 2 0 01-2-2V5z" />
          <path d="M7 3v14M3 8h4" />
        </svg>
        <h3 className="nav-section-title">Library</h3>
      </div>
      <div className="nav-library-info">
        <div className="nav-library-name">{library.name}</div>
        <div className="nav-library-meta">{library.clipCount} clips</div>
      </div>
    </div>
  );
}
