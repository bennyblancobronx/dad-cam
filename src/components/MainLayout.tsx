// Dad Cam - Main Layout with Left Navigation
// Braun Design Language v1.0.0 - App shell with sidebar

import type { ReactNode } from 'react';
import type { LibraryInfo } from '../types/clips';
import type { AppMode } from '../types/settings';
import { LeftNav } from './LeftNav';

interface MainLayoutProps {
  library: LibraryInfo;
  mode: AppMode;
  onOpenSettings: () => void;
  onNavigateToEvent?: (eventId: number) => void;
  onNavigateToDate?: (date: string) => void;
  /** Currently active date for highlighting in nav */
  activeDate?: string | null;
  /** Increment to trigger dates refresh */
  refreshTrigger?: number;
  /** Header content (back button, title, actions) */
  header?: ReactNode;
  /** Main content area */
  children: ReactNode;
}

export function MainLayout({
  library,
  mode,
  onOpenSettings,
  onNavigateToEvent,
  onNavigateToDate,
  activeDate,
  refreshTrigger,
  header,
  children,
}: MainLayoutProps) {
  return (
    <div className="main-layout">
      <LeftNav
        library={library}
        mode={mode}
        onOpenSettings={onOpenSettings}
        onNavigateToEvent={onNavigateToEvent}
        onNavigateToDate={onNavigateToDate}
        activeDate={activeDate}
        refreshTrigger={refreshTrigger}
      />
      <div className="main-content">
        {header && <header className="main-header">{header}</header>}
        <div className="main-content-area">{children}</div>
      </div>
    </div>
  );
}
