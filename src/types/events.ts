// Dad Cam - Events Types (Phase 6)

/** Event type constants */
export const EVENT_TYPES = {
  DATE_RANGE: 'date_range',
  CLIP_SELECTION: 'clip_selection',
} as const;

export type EventType = typeof EVENT_TYPES[keyof typeof EVENT_TYPES];

/** Event view with clip count */
export interface EventView {
  id: number;
  libraryId: number;
  name: string;
  description: string | null;
  eventType: EventType;
  dateStart: string | null;
  dateEnd: string | null;
  color: string;
  icon: string;
  clipCount: number;
  createdAt: string;
  updatedAt: string;
}

/** Clip view for event clips list */
export interface EventClipView {
  id: number;
  title: string;
  durationMs: number | null;
  width: number | null;
  height: number | null;
  recordedAt: string | null;
  thumbnailPath: string | null;
  proxyPath: string | null;
  originalPath: string | null;
}

/** Response for paginated event clips */
export interface EventClipsResponse {
  clips: EventClipView[];
  total: number;
  offset: number;
  limit: number;
}

/** Date group for navigation */
export interface DateGroup {
  date: string;
  clipCount: number;
}

/** Helper to check event type */
export function isDateRangeEvent(event: EventView): boolean {
  return event.eventType === EVENT_TYPES.DATE_RANGE;
}

export function isClipSelectionEvent(event: EventView): boolean {
  return event.eventType === EVENT_TYPES.CLIP_SELECTION;
}

/**
 * Parse a date string as local time (avoids UTC timezone shift).
 * Handles both YYYY-MM-DD and full ISO timestamps.
 */
export function parseLocalDate(dateStr: string): Date {
  // Check if it's a date-only string (YYYY-MM-DD)
  if (dateStr.length === 10 && dateStr.match(/^\d{4}-\d{2}-\d{2}$/)) {
    const [year, month, day] = dateStr.split('-').map(Number);
    return new Date(year, month - 1, day);
  }
  // For full timestamps, parse normally
  return new Date(dateStr);
}

/** Format date for display (safe for YYYY-MM-DD strings) */
export function formatEventDate(dateStr: string | null): string {
  if (!dateStr) return '';
  const date = parseLocalDate(dateStr);
  return date.toLocaleDateString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

/** Format clip timestamp for display (handles full timestamps safely) */
export function formatClipDate(dateStr: string | null): string {
  if (!dateStr) return '';
  const date = parseLocalDate(dateStr);
  return date.toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
}

/** Format clip time for display */
export function formatClipTime(dateStr: string | null): string {
  if (!dateStr) return '';
  const date = parseLocalDate(dateStr);
  return date.toLocaleTimeString('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    hour12: true,
  });
}

/** Format date range for display */
export function formatDateRange(start: string | null, end: string | null): string {
  if (!start || !end) return '';
  return `${formatEventDate(start)} - ${formatEventDate(end)}`;
}

/** Check if a year is a leap year */
function isLeapYear(year: number): boolean {
  return (year % 4 === 0 && year % 100 !== 0) || (year % 400 === 0);
}

/** Get the number of days in a month */
function daysInMonth(year: number, month: number): number {
  const days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  if (month === 2 && isLeapYear(year)) return 29;
  return days[month - 1] || 0;
}

/** Validate date string is in YYYY-MM-DD format with proper calendar rules */
export function isValidDateFormat(date: string): boolean {
  if (date.length !== 10) return false;

  const parts = date.split('-');
  if (parts.length !== 3) return false;

  const year = parseInt(parts[0], 10);
  const month = parseInt(parts[1], 10);
  const day = parseInt(parts[2], 10);

  if (isNaN(year) || isNaN(month) || isNaN(day)) return false;
  if (year < 1900 || year > 2100) return false;
  if (month < 1 || month > 12) return false;
  if (day < 1 || day > daysInMonth(year, month)) return false;

  return true;
}
