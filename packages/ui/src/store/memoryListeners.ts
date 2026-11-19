import type { Event, MemoryScope } from "../protocol";

/** A change to a console's or a project place's memory, broadcast to every client whoever made it, or the set of
 * places whose memory no project is at any more, broadcast (with no `id`) when it changes. */
export type MemoryChangeEvent = Extract<Event, { type: "memory_saved" | "memories_deleted" | "orphaned_memory_list" }>;

/** Whether `event` is a change to a memory: the broadcast of one, never the reply to `list_orphaned_memories`. */
export function isMemoryChangeEvent(event: Event): event is MemoryChangeEvent {
  return event.type === "memory_saved" || event.type === "memories_deleted" || (event.type === "orphaned_memory_list" && event.id === null);
}

/** The memory a saved or deleted entry belongs to; none for the set of orphaned places, which no memory shows. */
export function changeScope(event: MemoryChangeEvent): MemoryScope | undefined {
  if (event.type === "orphaned_memory_list") return undefined;
  return event.type === "memory_saved" ? event.entry.scope : event.scope;
}

/** The listeners of memory changes, in subscription order: `deliver` hands every change to all of them (each
 * keeps the memory it shows), `subscribe` adds one and returns the unsubscribe. Memory is not in the store, so what
 * shows it listens here. */
export function createMemoryListeners() {
  const held = new Set<(event: MemoryChangeEvent) => void>();
  return {
    subscribe(listener: (event: MemoryChangeEvent) => void): () => void {
      held.add(listener);
      return () => void held.delete(listener);
    },
    deliver(event: MemoryChangeEvent): void {
      for (const listener of held) listener(event);
    },
  };
}
