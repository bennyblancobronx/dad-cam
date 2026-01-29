// Dad Cam - Phase 5 Library View with MainLayout
import { useState, useEffect, useCallback, useRef } from 'react';
import type { ClipView, LibraryInfo, FilterType, SortField, SortOrder } from '../types/clips';
import type { AppSettings } from '../types/settings';
import type { EventClipView } from '../types/events';
import { getClipsFiltered, toggleTag, getLibraryRoot } from '../api/clips';
import { setLibraryRoot } from '../utils/paths';
import { clearThumbnailCache } from '../utils/thumbnailCache';
import { MainLayout } from './MainLayout';
import { ClipGrid } from './ClipGrid';
import { FilterBar } from './FilterBar';
import { VideoPlayer } from './VideoPlayer';
import { WelcomeDashboard } from './WelcomeDashboard';
import { SettingsView } from './SettingsView';
import { EventView } from './EventView';
import { DateView } from './DateView';
import { AddToEventModal } from './modals/AddToEventModal';
import { ExportDialog } from './ExportDialog';
import { ImportDialog } from './ImportDialog';
import { isFeatureAllowed } from '../api/licensing';

/**
 * Convert EventClipView to ClipView format for VideoPlayer.
 * EventClipView comes from DateView/EventView, ClipView is the main clip type.
 */
function eventClipToClipView(eventClip: EventClipView): ClipView {
  return {
    id: eventClip.id,
    title: eventClip.title,
    mediaType: 'video', // Default to video for event clips
    durationMs: eventClip.durationMs,
    width: eventClip.width,
    height: eventClip.height,
    recordedAt: eventClip.recordedAt,
    thumbPath: eventClip.thumbnailPath, // Map thumbnailPath -> thumbPath
    proxyPath: eventClip.proxyPath,
    spritePath: null, // EventClipView doesn't have sprite
    isFavorite: false, // Unknown from EventClipView
    isBad: false, // Unknown from EventClipView
  };
}

/** Current view within the library */
type LibrarySubView = 'welcome' | 'clips' | 'stills' | 'event' | 'date' | 'settings';

interface LibraryViewProps {
  library: LibraryInfo;
  onClose: () => void;
  /** App mode - shows "Back to Projects" in Advanced mode */
  mode?: 'simple' | 'advanced';
  /** App settings for settings panel */
  settings?: AppSettings | null;
  /** Callback when settings change */
  onSettingsChange?: (settings: AppSettings) => void;
}

const PAGE_SIZE = 50;

export function LibraryView({
  library,
  onClose,
  mode = 'simple',
  settings,
  onSettingsChange,
}: LibraryViewProps) {
  // View state - Simple mode starts on Welcome Dashboard
  const [currentView, setCurrentView] = useState<LibrarySubView>(
    mode === 'simple' ? 'welcome' : 'clips'
  );

  // Previous view for back navigation from settings
  const [previousView, setPreviousView] = useState<LibrarySubView>('welcome');

  // Clips state
  const [clips, setClips] = useState<ClipView[]>([]);
  const [totalClips, setTotalClips] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Filter state
  const [filter, setFilter] = useState<FilterType>('all');
  const [search, setSearch] = useState('');
  const [sortBy, setSortBy] = useState<SortField>('recorded_at');
  const [sortOrder, setSortOrder] = useState<SortOrder>('desc');
  const [dateFrom, setDateFrom] = useState<string | undefined>();
  const [dateTo, setDateTo] = useState<string | undefined>();

  // Player state
  const [selectedClip, setSelectedClip] = useState<ClipView | null>(null);

  // Import dialog state
  const [showImportDialog, setShowImportDialog] = useState(false);
  const [importStatus, setImportStatus] = useState<string | null>(null);

  // Stills mode state - when user navigates from Welcome for stills
  const [isStillsMode, setIsStillsMode] = useState(false);

  // Event view state
  const [selectedEventId, setSelectedEventId] = useState<number | null>(null);

  // Date view state
  const [selectedDate, setSelectedDate] = useState<string | null>(null);

  // Clip selection state (for adding to events)
  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedClipIds, setSelectedClipIds] = useState<Set<number>>(new Set());
  const [showAddToEventModal, setShowAddToEventModal] = useState(false);

  // Export dialog state
  const [showExportDialog, setShowExportDialog] = useState(false);

  // Request cancellation ref
  const abortControllerRef = useRef<AbortController | null>(null);

  // Refresh trigger for dates section (increment after import)
  const [refreshTrigger, setRefreshTrigger] = useState(0);

  // Initialize library root path
  useEffect(() => {
    async function init() {
      try {
        const root = await getLibraryRoot();
        setLibraryRoot(root);
        clearThumbnailCache();
      } catch (err) {
        console.error('Failed to get library root:', err);
      }
    }
    init();
  }, [library]);

  // Load clips
  const loadClips = useCallback(async (reset: boolean = false) => {
    if (isLoading && !reset) return;

    // Cancel previous request
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    abortControllerRef.current = new AbortController();

    setIsLoading(true);
    setError(null);

    try {
      const offset = reset ? 0 : clips.length;
      const response = await getClipsFiltered({
        offset,
        limit: PAGE_SIZE,
        filter: filter === 'all' ? undefined : filter,
        search: search || undefined,
        dateFrom,
        dateTo,
        sortBy,
        sortOrder,
      });

      if (reset) {
        setClips(response.clips);
      } else {
        setClips(prev => [...prev, ...response.clips]);
      }
      setTotalClips(response.total);
    } catch (err) {
      // Ignore abort errors
      if (err instanceof Error && err.name === 'AbortError') {
        return;
      }
      setError(err instanceof Error ? err.message : 'Failed to load clips');
      console.error('Failed to load clips:', err);
    } finally {
      setIsLoading(false);
    }
  }, [clips.length, filter, search, dateFrom, dateTo, sortBy, sortOrder, isLoading]);

  // Initial load and reload on filter/sort change
  useEffect(() => {
    loadClips(true);
  }, [filter, search, dateFrom, dateTo, sortBy, sortOrder]);

  // Handle filter change
  const handleFilterChange = useCallback((newFilter: FilterType) => {
    setFilter(newFilter);
  }, []);

  // Handle search change
  const handleSearchChange = useCallback((newSearch: string) => {
    setSearch(newSearch);
  }, []);

  // Handle sort change
  const handleSortChange = useCallback((newSortBy: SortField, newSortOrder: SortOrder) => {
    setSortBy(newSortBy);
    setSortOrder(newSortOrder);
  }, []);

  // Handle date range change
  const handleDateRangeChange = useCallback((newDateFrom?: string, newDateTo?: string) => {
    setDateFrom(newDateFrom);
    setDateTo(newDateTo);
  }, []);

  // Handle clip click - open player
  const handleClipClick = useCallback((clip: ClipView) => {
    setSelectedClip(clip);
  }, []);

  // Handle tag toggle with optimistic update
  const handleTagToggle = useCallback(async (clipId: number, tag: 'favorite' | 'bad') => {
    const clipIndex = clips.findIndex(c => c.id === clipId);
    if (clipIndex === -1) return;

    const clip = clips[clipIndex];
    const tagField = tag === 'favorite' ? 'isFavorite' : 'isBad';
    const currentValue = clip[tagField];

    // Optimistic update
    setClips(prev => {
      const updated = [...prev];
      updated[clipIndex] = { ...clip, [tagField]: !currentValue };
      return updated;
    });

    // Also update selected clip if it's the same
    if (selectedClip?.id === clipId) {
      setSelectedClip(prev => prev ? { ...prev, [tagField]: !currentValue } : null);
    }

    try {
      await toggleTag(clipId, tag);
    } catch (err) {
      console.error('Failed to toggle tag:', err);
      // Revert on error
      setClips(prev => {
        const updated = [...prev];
        updated[clipIndex] = clip;
        return updated;
      });
      if (selectedClip?.id === clipId) {
        setSelectedClip(clip);
      }
    }
  }, [clips, selectedClip]);

  // Navigate to previous/next clip in player
  const handlePreviousClip = useCallback(() => {
    if (!selectedClip) return;
    const currentIndex = clips.findIndex(c => c.id === selectedClip.id);
    if (currentIndex > 0) {
      setSelectedClip(clips[currentIndex - 1]);
    }
  }, [clips, selectedClip]);

  const handleNextClip = useCallback(() => {
    if (!selectedClip) return;
    const currentIndex = clips.findIndex(c => c.id === selectedClip.id);
    if (currentIndex < clips.length - 1) {
      setSelectedClip(clips[currentIndex + 1]);
    }
  }, [clips, selectedClip]);

  // Get current clip index for navigation
  const currentClipIndex = selectedClip ? clips.findIndex(c => c.id === selectedClip.id) : -1;

  // Navigate to clips view
  const handleNavigateToClips = useCallback(() => {
    setIsStillsMode(false);
    setCurrentView('clips');
    // Load clips if not already loaded
    if (clips.length === 0) {
      loadClips(true);
    }
  }, [clips.length, loadClips]);

  // Navigate to stills (clips view with stills intent)
  const handleNavigateToStills = useCallback(() => {
    setIsStillsMode(true);
    setCurrentView('stills');
    // Load clips if not already loaded
    if (clips.length === 0) {
      loadClips(true);
    }
  }, [clips.length, loadClips]);

  // Navigate back to welcome
  const handleNavigateToWelcome = useCallback(() => {
    setIsStillsMode(false);
    setCurrentView('welcome');
  }, []);

  // Navigate to event view
  const handleNavigateToEvent = useCallback((eventId: number) => {
    setSelectedEventId(eventId);
    setCurrentView('event');
  }, []);

  // Navigate back from event view
  const handleBackFromEvent = useCallback(() => {
    setSelectedEventId(null);
    setCurrentView('clips');
    // Load clips if not already loaded
    if (clips.length === 0) {
      loadClips(true);
    }
  }, [clips.length, loadClips]);

  // Navigate to date view
  const handleNavigateToDate = useCallback((date: string) => {
    setSelectedDate(date);
    setCurrentView('date');
  }, []);

  // Navigate back from date view
  const handleBackFromDate = useCallback(() => {
    setSelectedDate(null);
    setCurrentView('clips');
    // Load clips if not already loaded
    if (clips.length === 0) {
      loadClips(true);
    }
  }, [clips.length, loadClips]);

  // Navigate to favorites (clips view with favorites filter)
  const handleNavigateToFavorites = useCallback(() => {
    setFilter('favorites');
    setIsStillsMode(false);
    setCurrentView('clips');
  }, []);

  // Navigate to settings
  const handleNavigateToSettings = useCallback(() => {
    setPreviousView(currentView);
    setCurrentView('settings');
  }, [currentView]);

  // Navigate back from settings
  const handleBackFromSettings = useCallback(() => {
    setCurrentView(previousView);
  }, [previousView]);

  // Toggle clip selection
  const handleClipSelectionChange = useCallback((clipId: number) => {
    setSelectedClipIds((prev) => {
      const next = new Set(prev);
      if (next.has(clipId)) {
        next.delete(clipId);
      } else {
        next.add(clipId);
      }
      return next;
    });
  }, []);

  // Enter selection mode
  const handleEnterSelectionMode = useCallback(() => {
    setSelectionMode(true);
    setSelectedClipIds(new Set());
  }, []);

  // Exit selection mode
  const handleExitSelectionMode = useCallback(() => {
    setSelectionMode(false);
    setSelectedClipIds(new Set());
  }, []);

  // Handle clips added to event
  const handleClipsAddedToEvent = useCallback(() => {
    setSelectionMode(false);
    setSelectedClipIds(new Set());
  }, []);

  // Handle import complete - reload clips and refresh dates
  const handleImportComplete = useCallback(() => {
    loadClips(true);
    setRefreshTrigger(prev => prev + 1);
    setImportStatus('Import complete');
    setTimeout(() => setImportStatus(null), 5000);
  }, [loadClips]);

  // Build header content based on current view
  const headerContent = (
    <>
      <div className="main-header-left">
        {/* Back/Close button */}
        {currentView === 'welcome' ? (
          // Welcome view: Advanced mode shows back to projects, Simple shows close
          mode === 'advanced' ? (
            <button onClick={onClose} className="back-to-libraries-btn">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M10 12L6 8l4-4" />
              </svg>
              Projects
            </button>
          ) : (
            <button onClick={onClose} className="secondary-button" style={{ padding: '6px 12px' }}>
              Close Project
            </button>
          )
        ) : (
          // Clips/Stills view: Simple shows back to welcome, Advanced shows back to libraries
          mode === 'simple' ? (
            <button onClick={handleNavigateToWelcome} className="back-to-libraries-btn">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M10 12L6 8l4-4" />
              </svg>
              Back
            </button>
          ) : (
            <button onClick={onClose} className="back-to-libraries-btn">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
                <path d="M10 12L6 8l4-4" />
              </svg>
              Projects
            </button>
          )
        )}
        {/* Title - only shown in clips view */}
        {currentView !== 'welcome' && (
          <h1 className="main-header-title">
            {isStillsMode ? 'Select a clip for stills export' : library.name}
          </h1>
        )}
      </div>
      <div className="main-header-right">
        {/* Import status */}
        {importStatus && (
          <span style={{ color: importStatus.toLowerCase().includes('expired') ? 'var(--color-error)' : 'var(--color-success)', fontSize: '14px' }}>{importStatus}</span>
        )}
        {/* Selection mode actions */}
        {selectionMode && currentView === 'clips' && (
          <>
            <span style={{ fontSize: '14px', color: 'var(--color-text-secondary)' }}>
              {selectedClipIds.size} selected
            </span>
            <button
              onClick={() => setShowAddToEventModal(true)}
              disabled={selectedClipIds.size === 0}
              className="secondary-button"
              style={{ padding: '6px 12px' }}
            >
              Add to Event
            </button>
            <button
              onClick={handleExitSelectionMode}
              className="secondary-button"
              style={{ padding: '6px 12px' }}
            >
              Cancel
            </button>
          </>
        )}
        {/* Normal mode actions */}
        {!selectionMode && currentView === 'clips' && !isStillsMode && (
          <>
            <button
              onClick={handleEnterSelectionMode}
              className="secondary-button"
              style={{ padding: '6px 12px' }}
            >
              Select Clips
            </button>
            <button
              onClick={() => setShowExportDialog(true)}
              className="secondary-button"
              style={{ padding: '6px 12px' }}
            >
              VHS Export
            </button>
            <button
              onClick={async () => {
                try {
                  const allowed = await isFeatureAllowed('import');
                  if (!allowed) {
                    setImportStatus('Trial expired. Please activate a license to import footage.');
                    return;
                  }
                } catch {
                  // If check fails, let backend handle it
                }
                setShowImportDialog(true);
              }}
              className="secondary-button"
              style={{ padding: '6px 12px' }}
            >
              Import Footage
            </button>
          </>
        )}
        {/* Stills mode hint */}
        {isStillsMode && (
          <span style={{ color: 'var(--color-info)', fontSize: '14px' }}>
            Click a clip, then press S to export still frame
          </span>
        )}
      </div>
    </>
  );

  // Render content based on current view
  const renderContent = () => {
    if (currentView === 'welcome') {
      return (
        <WelcomeDashboard
          library={library}
          onNavigateToClips={handleNavigateToClips}
          onNavigateToStills={handleNavigateToStills}
          featureFlags={settings?.featureFlags}
        />
      );
    }

    if (currentView === 'event' && selectedEventId !== null) {
      return (
        <EventView
          eventId={selectedEventId}
          onBack={handleBackFromEvent}
          onClipSelect={(eventClip) => {
            // Convert EventClipView to ClipView and open player
            setSelectedClip(eventClipToClipView(eventClip));
          }}
        />
      );
    }

    if (currentView === 'date' && selectedDate !== null) {
      return (
        <DateView
          date={selectedDate}
          onBack={handleBackFromDate}
          onClipSelect={(eventClip) => {
            // Convert EventClipView to ClipView and open player
            setSelectedClip(eventClipToClipView(eventClip));
          }}
        />
      );
    }

    // Clips/Stills view
    return (
      <>
        {/* Filter Bar */}
        <FilterBar
          filter={filter}
          onFilterChange={handleFilterChange}
          search={search}
          onSearchChange={handleSearchChange}
          sortBy={sortBy}
          sortOrder={sortOrder}
          onSortChange={handleSortChange}
          dateFrom={dateFrom}
          dateTo={dateTo}
          onDateRangeChange={handleDateRangeChange}
          totalClips={totalClips}
          displayedClips={clips.length}
        />

        {/* Error display */}
        {error && (
          <div className="error-message" style={{ margin: '0 var(--space-4)' }}>
            {error}
          </div>
        )}

        {/* Clip Grid */}
        <div style={{ flex: 1, overflow: 'hidden' }}>
          <ClipGrid
            clips={clips}
            totalClips={totalClips}
            onLoadMore={() => loadClips(false)}
            onClipClick={handleClipClick}
            onTagToggle={handleTagToggle}
            isLoading={isLoading}
            selectionMode={selectionMode}
            selectedClipIds={selectedClipIds}
            onSelectionChange={handleClipSelectionChange}
          />
        </div>

        {/* Add to Event Modal */}
        {showAddToEventModal && (
          <AddToEventModal
            clipIds={Array.from(selectedClipIds)}
            onClose={() => setShowAddToEventModal(false)}
            onAdded={handleClipsAddedToEvent}
          />
        )}

        {/* VHS Export Dialog */}
        {showExportDialog && (
          <ExportDialog
            libraryPath={library.rootPath}
            mode={mode}
            onClose={() => setShowExportDialog(false)}
          />
        )}

        {/* Import Dialog */}
        {showImportDialog && (
          <ImportDialog
            libraryPath={library.rootPath}
            showCameraBreakdown={mode === 'advanced'}
            onClose={() => setShowImportDialog(false)}
            onComplete={handleImportComplete}
          />
        )}
      </>
    );
  };

  // Check if clip navigation is available (only in main clips view)
  const canNavigateClips = currentView === 'clips' || currentView === 'stills';
  const hasPrevious = canNavigateClips && currentClipIndex > 0;
  const hasNext = canNavigateClips && currentClipIndex < clips.length - 1;

  // Settings view renders without MainLayout wrapper
  if (currentView === 'settings' && settings && onSettingsChange) {
    return (
      <>
        <SettingsView
          settings={settings}
          onSettingsChange={onSettingsChange}
          onBack={handleBackFromSettings}
        />
        {/* Video Player Modal - available even in settings */}
        {selectedClip && (
          <VideoPlayer
            clip={selectedClip}
            onClose={() => setSelectedClip(null)}
            onPrevious={hasPrevious ? handlePreviousClip : undefined}
            onNext={hasNext ? handleNextClip : undefined}
            onFavoriteToggle={() => handleTagToggle(selectedClip.id, 'favorite')}
            onBadToggle={() => handleTagToggle(selectedClip.id, 'bad')}
          />
        )}
      </>
    );
  }

  return (
    <>
      <MainLayout
        library={library}
        onNavigateToSettings={handleNavigateToSettings}
        onNavigateToEvent={handleNavigateToEvent}
        onNavigateToDate={handleNavigateToDate}
        onNavigateToFavorites={handleNavigateToFavorites}
        activeDate={selectedDate}
        isFavoritesActive={currentView === 'clips' && filter === 'favorites'}
        refreshTrigger={refreshTrigger}
        mode={mode}
        featureFlags={settings?.featureFlags}
        header={headerContent}
      >
        {renderContent()}
      </MainLayout>

      {/* Video Player Modal - renders for all views */}
      {selectedClip && (
        <VideoPlayer
          clip={selectedClip}
          onClose={() => setSelectedClip(null)}
          onPrevious={hasPrevious ? handlePreviousClip : undefined}
          onNext={hasNext ? handleNextClip : undefined}
          onFavoriteToggle={() => handleTagToggle(selectedClip.id, 'favorite')}
          onBadToggle={() => handleTagToggle(selectedClip.id, 'bad')}
        />
      )}
    </>
  );
}
