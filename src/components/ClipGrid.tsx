// Dad Cam - Phase 3 Virtualized Clip Grid
import { useRef, useCallback, useEffect, useState } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import type { ClipView } from '../types/clips';
import { toAssetUrl } from '../utils/paths';
import { preloadThumbnails } from '../utils/thumbnailCache';
import { ClipThumbnail } from './ClipThumbnail';

interface ClipGridProps {
  clips: ClipView[];
  totalClips: number;
  onLoadMore: () => void;
  onClipClick: (clip: ClipView) => void;
  onTagToggle: (clipId: number, tag: 'favorite' | 'bad') => void;
  isLoading: boolean;
  columnCount?: number;
  itemHeight?: number;
  gap?: number;
  /** Selection mode props */
  selectionMode?: boolean;
  selectedClipIds?: Set<number>;
  onSelectionChange?: (clipId: number) => void;
  /** Range selection callback for Shift+click */
  onRangeSelect?: (startId: number, endId: number) => void;
}

export function ClipGrid({
  clips,
  totalClips,
  onLoadMore,
  onClipClick,
  onTagToggle,
  isLoading,
  columnCount: defaultColumnCount = 4,
  itemHeight = 200,
  gap = 16,
  selectionMode = false,
  selectedClipIds = new Set(),
  onSelectionChange,
  onRangeSelect,
}: ClipGridProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);
  const [lastSelectedIndex, setLastSelectedIndex] = useState<number | null>(null);

  // Calculate responsive column count
  useEffect(() => {
    const updateWidth = () => {
      if (parentRef.current) {
        const width = parentRef.current.offsetWidth;
        setContainerWidth(width);
      }
    };
    updateWidth();
    window.addEventListener('resize', updateWidth);
    return () => window.removeEventListener('resize', updateWidth);
  }, []);

  // Calculate actual column count based on width
  const columnCount = containerWidth > 0
    ? Math.max(1, Math.min(defaultColumnCount, Math.floor((containerWidth - gap) / (200 + gap))))
    : defaultColumnCount;

  // Calculate items per row based on container width
  const itemWidth = containerWidth > 0
    ? (containerWidth - gap * (columnCount + 1)) / columnCount
    : 200;
  const rowCount = Math.ceil(clips.length / columnCount);

  // Create virtualizer for rows
  const rowVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => itemHeight + gap,
    overscan: 3, // Render 3 extra rows above/below viewport
  });

  // Load more when approaching end
  const virtualItems = rowVirtualizer.getVirtualItems();
  const lastItem = virtualItems[virtualItems.length - 1];

  useEffect(() => {
    if (!lastItem) return;

    const lastRowIndex = lastItem.index;

    // Load more when within 5 rows of the end
    if (lastRowIndex >= rowCount - 5 && clips.length < totalClips && !isLoading) {
      onLoadMore();
    }
  }, [lastItem, clips.length, totalClips, rowCount, isLoading, onLoadMore]);

  // Preload thumbnails for visible + nearby items
  useEffect(() => {
    if (virtualItems.length === 0) return;

    const firstRow = virtualItems[0].index;
    const lastRow = virtualItems[virtualItems.length - 1].index;

    // Include 2 rows buffer
    const startIdx = Math.max(0, (firstRow - 2) * columnCount);
    const endIdx = Math.min(clips.length, (lastRow + 3) * columnCount);

    const urlsToPreload = clips
      .slice(startIdx, endIdx)
      .map(clip => toAssetUrl(clip.thumbPath))
      .filter((url): url is string => url !== null);

    preloadThumbnails(urlsToPreload);
  }, [virtualItems, clips, columnCount]);

  const handleTagToggle = useCallback((clipId: number, tag: 'favorite' | 'bad') => {
    onTagToggle(clipId, tag);
  }, [onTagToggle]);

  // Show skeleton loading on initial load
  if (clips.length === 0 && isLoading) {
    return (
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: `repeat(${defaultColumnCount}, 1fr)`,
          gap: `${gap}px`,
          padding: `${gap}px`,
        }}
      >
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} style={{ display: 'flex', flexDirection: 'column' }}>
            <div className="skeleton skeleton-card" />
            <div className="skeleton skeleton-text" style={{ width: '80%' }} />
            <div className="skeleton skeleton-text-sm" />
          </div>
        ))}
      </div>
    );
  }

  if (clips.length === 0 && !isLoading) {
    return (
      <div className="loading-indicator">
        No clips found
      </div>
    );
  }

  return (
    <div
      ref={parentRef}
      className="clip-grid-container"
      style={{
        height: '100%',
        overflow: 'auto',
        contain: 'strict',
      }}
    >
      <div
        style={{
          height: `${rowVirtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualItems.map(virtualRow => {
          const rowIndex = virtualRow.index;
          const startIndex = rowIndex * columnCount;
          const rowClips = clips.slice(startIndex, startIndex + columnCount);

          return (
            <div
              key={virtualRow.key}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
                display: 'flex',
                gap: `${gap}px`,
                padding: `0 ${gap}px`,
              }}
            >
              {rowClips.map((clip) => (
                <ClipThumbnail
                  key={clip.id}
                  clip={clip}
                  width={itemWidth}
                  height={itemHeight}
                  onClick={(e) => {
                    if (selectionMode) {
                      const currentIndex = clips.findIndex(c => c.id === clip.id);

                      // Shift+click for range selection
                      if (e.shiftKey && lastSelectedIndex !== null && onRangeSelect) {
                        const start = Math.min(lastSelectedIndex, currentIndex);
                        const end = Math.max(lastSelectedIndex, currentIndex);
                        const startId = clips[start].id;
                        const endId = clips[end].id;
                        onRangeSelect(startId, endId);
                      } else if (onSelectionChange) {
                        onSelectionChange(clip.id);
                        setLastSelectedIndex(currentIndex);
                      }
                    } else {
                      onClipClick(clip);
                    }
                  }}
                  onFavoriteToggle={() => handleTagToggle(clip.id, 'favorite')}
                  onBadToggle={() => handleTagToggle(clip.id, 'bad')}
                  selectionMode={selectionMode}
                  isSelected={selectedClipIds.has(clip.id)}
                />
              ))}
            </div>
          );
        })}
      </div>

      {isLoading && (
        <div className="loading-indicator">
          Loading more clips...
        </div>
      )}
    </div>
  );
}
