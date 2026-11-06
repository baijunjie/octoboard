import type { Event } from "../protocol";

/** A progress report of a running configuration import. */
export type ImportProgressEvent = Extract<Event, { type: "configuration_import_progress" }>;

/** The listeners of running imports, each for one import: `deliver` hands an event to those that subscribed to the
 * import it names (every window is sent the reports of every import, so the rest are not theirs), `subscribe` adds one
 * and returns the unsubscribe. */
export function createImportProgressListeners() {
  const held = new Set<{ importId: string; listener: (event: ImportProgressEvent) => void }>();
  return {
    subscribe(importId: string, listener: (event: ImportProgressEvent) => void): () => void {
      const entry = { importId, listener };
      held.add(entry);
      return () => void held.delete(entry);
    },
    deliver(event: ImportProgressEvent): void {
      for (const entry of held) if (entry.importId === event.import_id) entry.listener(event);
    },
  };
}
