import type { Event } from "../protocol";

/** An event of a running content search: a batch of matching lines, or the search's end. */
export type ContentSearchEvent = Extract<Event, { type: "project_content_matches" | "project_content_search_ended" }>;

/** Whether `event` belongs to a content search. */
export function isContentSearchEvent(event: Event): event is ContentSearchEvent {
  return event.type === "project_content_matches" || event.type === "project_content_search_ended";
}

/** The listeners of running content searches, each for one search: `deliver` hands an event to those that subscribed
 * to the search it names, and drops one no listener holds (a search the client has moved on from), `subscribe` adds
 * one and returns the unsubscribe. */
export function createContentSearchListeners() {
  const held = new Set<{ search: string; listener: (event: ContentSearchEvent) => void }>();
  return {
    subscribe(search: string, listener: (event: ContentSearchEvent) => void): () => void {
      const entry = { search, listener };
      held.add(entry);
      return () => void held.delete(entry);
    },
    deliver(event: ContentSearchEvent): void {
      for (const entry of held) if (entry.search === event.search) entry.listener(event);
    },
  };
}
