// Dad Cam - Phase 3 Thumbnail LRU Cache
import { LRUCache } from 'lru-cache';

interface CachedImage {
  url: string;
  loaded: boolean;
  error: boolean;
}

// LRU cache with max 500 entries
const cache = new LRUCache<string, CachedImage>({
  max: 500,
});

// Preload queue for background loading
const preloadQueue: string[] = [];
let isPreloading = false;

/**
 * Get a cached image or start loading it
 */
export function getThumbnail(url: string): CachedImage {
  const cached = cache.get(url);
  if (cached) return cached;

  // Create placeholder and start loading
  const placeholder: CachedImage = {
    url,
    loaded: false,
    error: false,
  };
  cache.set(url, placeholder);

  // Load image
  const img = new Image();
  img.onload = () => {
    cache.set(url, { url, loaded: true, error: false });
  };
  img.onerror = () => {
    cache.set(url, { url, loaded: false, error: true });
  };
  img.src = url;

  return placeholder;
}

/**
 * Preload thumbnails in background (for smooth scrolling)
 */
export function preloadThumbnails(urls: string[]): void {
  urls.forEach(url => {
    if (!cache.has(url) && !preloadQueue.includes(url)) {
      preloadQueue.push(url);
    }
  });

  if (!isPreloading) {
    processPreloadQueue();
  }
}

async function processPreloadQueue(): Promise<void> {
  isPreloading = true;

  while (preloadQueue.length > 0) {
    const batch = preloadQueue.splice(0, 10); // Process 10 at a time

    await Promise.all(
      batch.map(url => new Promise<void>(resolve => {
        const img = new Image();
        img.onload = () => {
          cache.set(url, { url, loaded: true, error: false });
          resolve();
        };
        img.onerror = () => {
          cache.set(url, { url, loaded: false, error: true });
          resolve();
        };
        img.src = url;
      }))
    );

    // Small delay to prevent overwhelming the system
    await new Promise(r => setTimeout(r, 50));
  }

  isPreloading = false;
}

/**
 * Clear the cache (e.g., when switching libraries)
 */
export function clearThumbnailCache(): void {
  cache.clear();
  preloadQueue.length = 0;
}

/**
 * Get cache statistics
 */
export function getCacheStats(): { size: number; maxSize: number } {
  return {
    size: cache.size,
    maxSize: cache.max,
  };
}
