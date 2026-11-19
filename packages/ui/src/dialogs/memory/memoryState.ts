import type { MemoryEntry, MemoryIndexEntry } from "../../protocol";
import type { MemoryChangeEvent } from "../../store";

/** The changes of one memory the daemon broadcasts. */
export type EntryChangeEvent = Exclude<MemoryChangeEvent, { type: "orphaned_memory_list" }>;

/** What the window holds of one memory (a console's or a project place's): the index, the most recently updated first, and the entries whose
 * content it has read or been sent (an entry is read when the user opens it; one saved while the window listens arrives
 * whole). */
export interface HeldMemory {
  index: readonly MemoryIndexEntry[];
  contents: ReadonlyMap<string, MemoryEntry>;
}

export const EMPTY_MEMORY: HeldMemory = { index: [], contents: new Map() };

/** The index line of `entry`. */
export function indexLine(entry: MemoryEntry): MemoryIndexEntry {
  return {
    id: entry.id,
    summary: entry.summary,
    local: entry.local,
    created_at: entry.created_at,
    updated_at: entry.updated_at,
    updated_by: entry.updated_by,
    version: entry.version,
  };
}

/** The most recently updated first, as the daemon lists them. */
function recentFirst(index: readonly MemoryIndexEntry[]): MemoryIndexEntry[] {
  return [...index].sort((a, b) => b.updated_at - a.updated_at || a.id.localeCompare(b.id));
}

/** `memory` with `entry` created or replaced, content and all. */
export function withSaved(memory: HeldMemory, entry: MemoryEntry): HeldMemory {
  const index = recentFirst([...memory.index.filter((line) => line.id !== entry.id), indexLine(entry)]);
  return { index, contents: new Map(memory.contents).set(entry.id, entry) };
}

/** `memory` without the entries `ids` name. */
export function withDeleted(memory: HeldMemory, ids: readonly string[]): HeldMemory {
  const gone = new Set(ids);
  const contents = new Map(memory.contents);
  for (const id of gone) contents.delete(id);
  return { index: memory.index.filter((line) => !gone.has(line.id)), contents };
}

/** `memory` with the content `entry` a read answered, unless something newer is held (a save broadcast before the
 * read's reply arrived) or the entry is gone (a deletion broadcast first). */
export function withRead(memory: HeldMemory, entry: MemoryEntry): HeldMemory {
  const line = memory.index.find((candidate) => candidate.id === entry.id);
  if (!line || line.version > entry.version) return memory;
  if (line.version === entry.version) return { ...memory, contents: new Map(memory.contents).set(entry.id, entry) };
  return withSaved(memory, entry);
}

/** `memory` after one change the daemon broadcast. */
export function withChange(memory: HeldMemory, event: EntryChangeEvent): HeldMemory {
  return event.type === "memory_saved" ? withSaved(memory, event.entry) : withDeleted(memory, event.memories);
}

/** `memory` as a `list_memories` reply (`listed`) has it, with the changes that were broadcast while it was out
 * (`during`, for this memory, in the order they came) put over it. A broadcast can be older than the reply, which the
 * daemon may have computed after it was made: a save older than the listed entry is dropped, so the list does not step
 * back. The content held of an entry the list still has is kept, though it may be of an older version
 * (`staleContents`), and the content of one it lacks is dropped. */
export function withListing(memory: HeldMemory, listed: readonly MemoryIndexEntry[], during: readonly EntryChangeEvent[]): HeldMemory {
  const contents = new Map<string, MemoryEntry>();
  for (const line of listed) {
    const held = memory.contents.get(line.id);
    if (held) contents.set(line.id, held);
  }
  let next: HeldMemory = { index: listed, contents };
  for (const event of during) {
    if (event.type === "memory_saved") {
      const line = next.index.find((candidate) => candidate.id === event.entry.id);
      if (line && line.version > event.entry.version) continue;
    }
    next = withChange(next, event);
  }
  return next;
}

/** The ids whose content `memory` holds at a version other than the one its index has: those a listing found
 * changed, which are read again. */
export function staleContents(memory: HeldMemory): string[] {
  return memory.index.filter((line) => {
    const held = memory.contents.get(line.id);
    return held !== undefined && held.version !== line.version;
  }).map((line) => line.id);
}
