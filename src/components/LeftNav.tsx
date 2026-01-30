// Dad Cam - Left Navigation Bar
// Braun Design Language v1.0.0 - Sidebar navigation

import type { LibraryInfo } from '../types/clips';
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
}: LeftNavProps) {
  return (
    <nav className="left-nav">
      <LibrarySection
        library={library}
        onNavigateToFavorites={onNavigateToFavorites}
        isFavoritesActive={isFavoritesActive}
      />
      <EventsSection onNavigateToEvent={onNavigateToEvent} />
      <DatesSection onNavigateToDate={onNavigateToDate} activeDate={activeDate} refreshTrigger={refreshTrigger} />
      <SettingsSection onNavigateToSettings={onNavigateToSettings} />
    </nav>
  );
}
