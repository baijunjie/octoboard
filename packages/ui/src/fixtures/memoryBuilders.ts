// The memory entries, scopes and changes the memory tests and the gallery's memory scenarios are built from; no
// rendering, so a pure test may import them.
import { projectOf } from "./builders";
import type { MemoryEntry, MemoryScope } from "../protocol";
import type { EntryChangeEvent } from "../dialogs/memory/memoryState";

/** The scope of console `c-1`'s memory. */
export const CONSOLE_SCOPE: MemoryScope = { kind: "console", console_id: "c-1" };

/** The scope of the memory of the place at `host_id` and `path`, which a project there shows. */
export const placeScope = (host_id: string, path: string): MemoryScope => ({ kind: "place", host_id, path });

/** An entry of console `c-1`'s memory (or another scope given in `extra`) with the id `id`, written by the user at version 1 unless `extra` says otherwise. */
export function entryOf(id: string, extra: Partial<MemoryEntry> = {}): MemoryEntry {
  return {
    scope: CONSOLE_SCOPE,
    id,
    summary: `About ${id}`,
    local: false,
    content: `Content of ${id}`,
    created_at: 1_000,
    updated_at: 2_000,
    updated_by: { kind: "user" },
    version: 1,
    ...extra,
  };
}

export const savedEvent = (entry: MemoryEntry): EntryChangeEvent => ({ type: "memory_saved", entry });
export const deletedEvent = (memories: string[], scope: MemoryScope = CONSOLE_SCOPE): EntryChangeEvent => ({ type: "memories_deleted", scope, memories });

/** The project the project memory tests open, at host `local` and path `/work/app`. */
export const PROJECT = { ...projectOf("p-1", "c-1", "App"), host_id: "local", path: "/work/app" };
/** The scope of the memory the project shows. */
export const PROJECT_SCOPE: MemoryScope = { kind: "place", host_id: "local", path: "/work/app" };

