// Dad Cam - Events API (Phase 6)
// Calls to Tauri backend for event management

import { invoke } from '@tauri-apps/api/core';
import type {
  EventView,
  EventClipsResponse,
  DateGroup,
  EventType,
} from '../types/events';

/** Create a new event */
export async function createEvent(
  name: string,
  eventType: EventType,
  options?: {
    description?: string;
    dateStart?: string;
    dateEnd?: string;
    color?: string;
    icon?: string;
  }
): Promise<EventView> {
  return await invoke<EventView>('create_event', {
    name,
    eventType,
    description: options?.description ?? null,
    dateStart: options?.dateStart ?? null,
    dateEnd: options?.dateEnd ?? null,
    color: options?.color ?? null,
    icon: options?.icon ?? null,
  });
}

/** Get all events for current library */
export async function getEvents(): Promise<EventView[]> {
  return await invoke<EventView[]>('get_events');
}

/** Get a single event by ID */
export async function getEvent(eventId: number): Promise<EventView> {
  return await invoke<EventView>('get_event', { eventId });
}

/** Update an event */
export async function updateEvent(
  eventId: number,
  updates: {
    name?: string;
    description?: string;
    dateStart?: string;
    dateEnd?: string;
    color?: string;
    icon?: string;
  }
): Promise<EventView> {
  return await invoke<EventView>('update_event', {
    eventId,
    name: updates.name ?? null,
    description: updates.description ?? null,
    dateStart: updates.dateStart ?? null,
    dateEnd: updates.dateEnd ?? null,
    color: updates.color ?? null,
    icon: updates.icon ?? null,
  });
}

/** Delete an event */
export async function deleteEvent(eventId: number): Promise<void> {
  await invoke('delete_event', { eventId });
}

/** Add clips to an event */
export async function addClipsToEvent(
  eventId: number,
  clipIds: number[]
): Promise<void> {
  await invoke('add_clips_to_event', { eventId, clipIds });
}

/** Remove clips from an event */
export async function removeClipsFromEvent(
  eventId: number,
  clipIds: number[]
): Promise<void> {
  await invoke('remove_clips_from_event', { eventId, clipIds });
}

/** Get clips for an event with pagination */
export async function getEventClips(
  eventId: number,
  offset: number = 0,
  limit: number = 50
): Promise<EventClipsResponse> {
  return await invoke<EventClipsResponse>('get_event_clips', {
    eventId,
    offset,
    limit,
  });
}

/** Get clips grouped by date for navigation */
export async function getClipsGroupedByDate(): Promise<DateGroup[]> {
  return await invoke<DateGroup[]>('get_clips_grouped_by_date');
}

/** Get clips for a specific date */
export async function getClipsByDate(
  date: string,
  offset: number = 0,
  limit: number = 50
): Promise<EventClipsResponse> {
  return await invoke<EventClipsResponse>('get_clips_by_date', {
    date,
    offset,
    limit,
  });
}
