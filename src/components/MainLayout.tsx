// Dad Cam - Main Layout with Left Navigation
// Braun Design Language v1.0.0 - App shell with sidebar

import type { ReactNode } from 'react';
import type { LibraryInfo } from '../types/clips';
import { LeftNav } from './LeftNav';

interface MainLayoutProps {
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
  /** Header content (back button, title, actions) */
  header?: ReactNode;
  /** Main content area */
  children: ReactNode;
}

export function MainLayout({
  library,
  onNavigateToSettings,
  onNavigateToEvent,
  onNavigateToDate,
  onNavigateToFavorites,
  activeDate,
  isFavoritesActive,
  refreshTrigger,
  header,
  children,
}: MainLayoutProps) {
  return (
    <div className="main-layout">
      <LeftNav
        library={library}
        onNavigateToSettings={onNavigateToSettings}
        onNavigateToEvent={onNavigateToEvent}
        onNavigateToDate={onNavigateToDate}
        onNavigateToFavorites={onNavigateToFavorites}
        activeDate={activeDate}
        isFavoritesActive={isFavoritesActive}
        refreshTrigger={refreshTrigger}
      />
      <div className="main-content">
        {header && <header className="main-header">{header}</header>}
        <div className="main-content-area">{children}</div>
      </div>
    </div>
  );
}
