// Dad Cam - Left Navigation Bar
// Braun Design Language v1.0.0 - Sidebar navigation

import type { LibraryInfo } from '../types/clips';
import type { AppMode } from '../types/settings';
import { LibrarySection } from './nav/LibrarySection';
import { EventsSection } from './nav/EventsSection';
import { DatesSection } from './nav/DatesSection';
import { SettingsSection } from './nav/SettingsSection';

interface LeftNavProps {
  library: LibraryInfo;
  mode: AppMode;
  onOpenSettings: () => void;
  onNavigateToEvent?: (eventId: number) => void;
  onNavigateToDate?: (date: string) => void;
  /** Currently active date for highlighting in nav */
  activeDate?: string | null;
  /** Increment to trigger dates refresh */
  refreshTrigger?: number;
}

export function LeftNav({
  library,
  mode,
  onOpenSettings,
  onNavigateToEvent,
  onNavigateToDate,
  activeDate,
  refreshTrigger,
}: LeftNavProps) {
  return (
    <nav className="left-nav">
      <LibrarySection library={library} />
      <EventsSection onNavigateToEvent={onNavigateToEvent} />
      <DatesSection onNavigateToDate={onNavigateToDate} activeDate={activeDate} refreshTrigger={refreshTrigger} />
      <SettingsSection mode={mode} onOpenSettings={onOpenSettings} />
    </nav>
  );
}
