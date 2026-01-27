// Dad Cam - Phase 3 Library View (Main Container)
import { useState, useEffect, useCallback, useRef } from 'react';
import type { ClipView, LibraryInfo, FilterType, SortField, SortOrder } from '../types/clips';
import { getClipsFiltered, toggleTag, getLibraryRoot } from '../api/clips';
import { open } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { setLibraryRoot } from '../utils/paths';
import { clearThumbnailCache } from '../utils/thumbnailCache';
import { ClipGrid } from './ClipGrid';
import { FilterBar } from './FilterBar';
import { VideoPlayer } from './VideoPlayer';

interface LibraryViewProps {
  library: LibraryInfo;
  onClose: () => void;
}

const PAGE_SIZE = 50;

export function LibraryView({ library, onClose }: LibraryViewProps) {
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

  // Import state
  const [isImporting, setIsImporting] = useState(false);
  const [importStatus, setImportStatus] = useState<string | null>(null);

  // Request cancellation ref
  const abortControllerRef = useRef<AbortController | null>(null);

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

  // Handle import footage
  const handleImport = useCallback(async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Folder to Import',
      });

      if (!selected) return;

      setIsImporting(true);
      setImportStatus('Importing...');
      setError(null);

      const result = await invoke<{
        jobId: number;
        totalFiles: number;
        processed: number;
        skipped: number;
        failed: number;
        clipsCreated: number[];
      }>('start_ingest', {
        sourcePath: selected,
        libraryPath: library.rootPath,
      });

      setImportStatus(`Imported ${result.processed} files (${result.skipped} skipped, ${result.failed} failed)`);

      // Reload clips to show new imports
      loadClips(true);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : 'Import failed');
      setImportStatus(null);
    } finally {
      setIsImporting(false);
    }
  }, [library.rootPath, loadClips]);

  return (
    <div
      className="library-view"
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100vh',
        backgroundColor: '#0f0f0f',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          padding: '12px 16px',
          backgroundColor: '#1a1a1a',
          borderBottom: '1px solid #333',
        }}
      >
        <h1 style={{ color: 'white', margin: 0, fontSize: '20px' }}>
          {library.name}
        </h1>
        <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
          {importStatus && (
            <span style={{ color: '#88ff88', fontSize: '14px' }}>{importStatus}</span>
          )}
          <button
            onClick={handleImport}
            disabled={isImporting}
            style={{
              padding: '6px 12px',
              border: '1px solid #446644',
              borderRadius: '4px',
              backgroundColor: isImporting ? '#1a2a1a' : '#2a3a2a',
              color: isImporting ? '#668866' : '#88ff88',
              cursor: isImporting ? 'not-allowed' : 'pointer',
            }}
          >
            {isImporting ? 'Importing...' : 'Import Footage'}
          </button>
          <button
            onClick={onClose}
            style={{
              padding: '6px 12px',
              border: '1px solid #444',
              borderRadius: '4px',
              backgroundColor: '#2a2a2a',
              color: 'white',
              cursor: 'pointer',
            }}
          >
            Close Library
          </button>
        </div>
      </div>

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
        <div style={{
          padding: '12px 16px',
          backgroundColor: '#442222',
          color: '#ff8888',
        }}>
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
        />
      </div>

      {/* Video Player Modal */}
      {selectedClip && (
        <VideoPlayer
          clip={selectedClip}
          onClose={() => setSelectedClip(null)}
          onPrevious={currentClipIndex > 0 ? handlePreviousClip : undefined}
          onNext={currentClipIndex < clips.length - 1 ? handleNextClip : undefined}
          onFavoriteToggle={() => handleTagToggle(selectedClip.id, 'favorite')}
          onBadToggle={() => handleTagToggle(selectedClip.id, 'bad')}
        />
      )}
    </div>
  );
}
