// Dad Cam - Phase 3 TypeScript Types

// Clip data from backend
export interface ClipView {
  id: number;
  title: string;
  mediaType: 'video' | 'audio' | 'image';
  durationMs: number | null;
  width: number | null;
  height: number | null;
  recordedAt: string | null;
  thumbPath: string | null;
  proxyPath: string | null;
  spritePath: string | null;
  isFavorite: boolean;
  isBad: boolean;
}

// Query parameters for fetching clips
export interface ClipQuery {
  offset: number;
  limit: number;
  filter?: 'all' | 'favorites' | 'bad' | 'unreviewed';
  search?: string;
  dateFrom?: string;
  dateTo?: string;
  sortBy?: 'recorded_at' | 'title' | 'created_at';
  sortOrder?: 'asc' | 'desc';
}

// Paginated response
export interface ClipListResponse {
  clips: ClipView[];
  total: number;
  offset: number;
  limit: number;
}

// Library info
export interface LibraryInfo {
  id: number;
  rootPath: string;
  name: string;
  ingestMode: string;
  createdAt: string;
  clipCount: number;
}

// Sprite sheet metadata (from JSON file)
export interface SpriteMetadata {
  fps: number;
  tile_width: number;
  tile_height: number;
  frame_count: number;
  columns: number;
  rows: number;
  interval_ms: number;
  page_index?: number;
  page_count?: number;
}

// Filter options for the filter bar
export type FilterType = 'all' | 'favorites' | 'bad' | 'unreviewed';
export type SortField = 'recorded_at' | 'title' | 'created_at';
export type SortOrder = 'asc' | 'desc';
