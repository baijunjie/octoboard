// What the fixture daemon answers for the memories: a console's and the memory of a place (a host and a path, which a
// project shows), the listing and reads, a user's saves and deletions, the places whose memory no project is at any more,
// and the changes a session makes, which every client hears of from `memory_saved`, `memories_deleted` and
// `orphaned_memory_list`.
import type { MemoryEntry, MemoryIndexEntry, MemoryScope, OrphanedMemory } from "../protocol";
import type { MemoryChangeEvent } from "../store";
import { neverReplied, type FixtureContext, type Handlers } from "./fixtureContext";

/** The most entries one memory holds (`MEMORY_MAX_ENTRIES` of the daemon's protocol). */
const MEMORY_MAX_ENTRIES = 200;

/** The index line of `entry`: the entry without its content and scope. */
function indexLineOf(entry: MemoryEntry): MemoryIndexEntry {
  const { content: _content, scope: _scope, ...line } = entry;
  return line;
}

/** What names a scope in a map. A scope of a kind this fixture does not hold has no key: the fixture never builds one. */
function scopeKey(scope: MemoryScope): string {
  const kind: string = scope.kind;
  if (kind === "console" && "console_id" in scope) return `console:${scope.console_id}`;
  if (kind === "place" && "host_id" in scope) return `place:${scope.host_id}\u0000${scope.path}`;
  throw new Error(`fixtureMemory: a memory of kind ${kind} is not held`);
}

/** The characters of an entry's id, which the daemon gives (`ID_ALPHABET` of `store/memory.rs`). */
const ID_ALPHABET = "abcdefghijklmnopqrstuvwxyz0123456789";
/** How many characters an entry's id has (`MEMORY_ID_LENGTH` of the daemon's protocol). */
const MEMORY_ID_LENGTH = 8;

/** A new id of the daemon's alphabet and length, never one the memory holds. */
function newMemoryId(taken: ReadonlySet<string>): string {
  for (;;) {
    const id = Array.from({ length: MEMORY_ID_LENGTH }, () => ID_ALPHABET[Math.floor(Math.random() * ID_ALPHABET.length)]).join("");
    if (!taken.has(id)) return id;
  }
}

/** What `delete_project` asks of the memory: refusing before anything is done, and the memory's part once the project is
 * gone. */
export interface ProjectMemoryHooks {
  /** Refuses with `memory_place_in_use` a removal that deletes the place's memory while another project is at the place. */
  refuseRemoval: (project: string, deleteMemory: boolean | undefined) => void;
  /** Deletes the place's memory when the removal asks for it, and tells every client what is left behind. */
  removed: (project: string, deleteMemory: boolean | undefined) => void;
}

/** The replies about the memories, `hear`, which a scenario's step calls for a change a session made (it is applied to
 * what the daemon holds and delivered to `deliver`, the window's listeners, as the connection delivers the daemon's
 * broadcasts; a save or deletion the user makes is delivered the same way before it is acknowledged), and the hooks of
 * `delete_project`.
 *
 * A request names exactly one of a console and a project; a project names the memory of the place it is at, which the
 * fixture reads from the project's record. The daemon holds a save to the rules of an entry (the form checks the same
 * before it asks), so only the refusals that depend on what is held or on the request's shape are repeated here:
 * `field_required` for one of `memory` and `version` without the other, an update of another version than the current
 * one, of an entry that is not there, and a new entry in a memory that holds 200. A save that names neither creates an
 * entry, whose id the daemon gives; it is answered `ack` and the entry arrives in `memory_saved` alone. The write is
 * stamped as the user's, and `local` is kept off a place's entries. */
export function createMemoryReplies(
  { scenario, store, fail }: FixtureContext,
  deliver: (event: MemoryChangeEvent) => void,
): { replies: Handlers; hear: (event: MemoryChangeEvent) => void; projects: ProjectMemoryHooks } {
  const staged = scenario.memory;
  /** What the daemon holds, by scope and then id. */
  const held = new Map<string, Map<string, MemoryEntry>>();
  const entriesOf = (scope: MemoryScope): Map<string, MemoryEntry> => {
    const key = scopeKey(scope);
    let entries = held.get(key);
    if (!entries) held.set(key, (entries = new Map()));
    return entries;
  };
  for (const entry of staged?.entries ?? []) entriesOf(entry.scope).set(entry.id, entry);
  /** The entries as the daemon has them now: those the window did not hear of take the place of what it was told. */
  const current = (scope: MemoryScope, id: string): MemoryEntry | undefined =>
    staged?.unheard?.find((entry) => scopeKey(entry.scope) === scopeKey(scope) && entry.id === id) ?? entriesOf(scope).get(id);
  /** The ids a memory holds or would hold had it heard of every change, which a new entry does not take. */
  const used = (scope: MemoryScope): Set<string> =>
    new Set([...entriesOf(scope).keys(), ...(staged?.unheard ?? []).filter((entry) => scopeKey(entry.scope) === scopeKey(scope)).map((entry) => entry.id)]);
  const unknownEntry = (memory: string): never => fail({ code: "unknown_memory", params: { memory }, message: `the memory has no entry ${memory}` });
  /** Whether a save was refused for a changed entry: from then on reads find the entries the window did not hear of. */
  let changedRefused = false;
  const requiredField = (field: "memory" | "version" | "console"): never => fail({ code: "field_required", params: { field }, message: `\`${field}\` is required` });

  /** The memory a request names: a console's, or the place of a project. */
  const scopeOf = (body: { console?: string; project?: string }): MemoryScope => {
    if (body.console !== undefined && body.project !== undefined) {
      return fail({ code: "unreadable_request", params: { detail: "exactly one of `console` and `project` names a memory" }, message: "exactly one of `console` and `project` names a memory" });
    }
    if (body.console !== undefined) {
      if (!store.getState().consoles.has(body.console)) fail({ code: "unknown_console", params: { console: body.console }, message: `unknown console ${body.console}` });
      return { kind: "console", console_id: body.console };
    }
    if (body.project === undefined) return requiredField("console");
    const project = store.getState().projects.get(body.project);
    if (!project) return fail({ code: "unknown_project", params: { project: body.project }, message: `unknown project ${body.project}` });
    return { kind: "place", host_id: project.host_id, path: project.path };
  };

  /** The places holding memory that no project is at, the project `except` taken as gone. */
  const orphans = (except?: string): OrphanedMemory[] => {
    const projects = [...store.getState().projects.values()].filter((project) => project.id !== except);
    const places: OrphanedMemory[] = [];
    for (const entries of held.values()) {
      const [first] = entries.values();
      if (!first || first.scope.kind !== "place") continue;
      const { host_id, path } = first.scope;
      if (projects.some((project) => project.host_id === host_id && project.path === path)) continue;
      places.push({ host_id, path, entries: entries.size, updated_at: Math.max(...[...entries.values()].map((entry) => entry.updated_at)) });
    }
    // As the daemon orders them: `MAX(updated_at) DESC, host_id, path`, the text compared by code unit.
    const byCodeUnit = (x: string, y: string) => (x < y ? -1 : x > y ? 1 : 0);
    return places.sort((a, b) => b.updated_at - a.updated_at || byCodeUnit(a.host_id, b.host_id) || byCodeUnit(a.path, b.path));
  };
  const announceOrphans = (except?: string): void => deliver({ type: "orphaned_memory_list", id: null, places: orphans(except) });

  const apply = (event: MemoryChangeEvent): void => {
    if (event.type === "memory_saved") entriesOf(event.entry.scope).set(event.entry.id, event.entry);
    else if (event.type === "memories_deleted") for (const id of event.memories) entriesOf(event.scope).delete(id);
    deliver(event);
    // A place's memory that changes while no project is at it changes what is left behind.
    const scope = event.type === "memory_saved" ? event.entry.scope : event.type === "memories_deleted" ? event.scope : undefined;
    if (scope?.kind === "place" && !projectAt(scope.host_id, scope.path)) announceOrphans();
  };

  /** The project at `host_id` and `path` other than `except`, if any. */
  const projectAt = (host_id: string, path: string, except?: string) =>
    [...store.getState().projects.values()].find((project) => project.id !== except && project.host_id === host_id && project.path === path);
  const placeInUse = (project: string): never => fail({ code: "memory_place_in_use", params: { project }, message: `the project ${project} is at the place and still reads its memory` });
  /** Deletes every entry of the place and announces it, the entries of an empty one being nothing to announce. */
  const clear = (scope: MemoryScope): void => {
    const memories = [...entriesOf(scope).keys()];
    if (memories.length > 0) apply({ type: "memories_deleted", scope, memories });
  };

  const replies: Handlers = {
    list_memories: (body) => {
      const scope = scopeOf(body);
      if (staged?.listing === "pending") return neverReplied();
      if (staged?.listing) return fail(staged.listing);
      const memories = [...entriesOf(scope).values()].map(indexLineOf).sort((a, b) => b.updated_at - a.updated_at || a.id.localeCompare(b.id));
      return { type: "memory_list", id: null, scope, memories };
    },
    read_memory: (body) => {
      const scope = scopeOf(body);
      const entry = changedRefused ? current(scope, body.memory) : entriesOf(scope).get(body.memory);
      return entry ? { type: "memory_content", id: null, entry } : unknownEntry(body.memory);
    },
    save_memory: (body) => {
      const scope = scopeOf(body);
      if (body.memory !== undefined && body.version === undefined) return requiredField("version");
      if (body.memory === undefined && body.version !== undefined) return requiredField("memory");
      const existing = body.memory === undefined ? undefined : current(scope, body.memory);
      if (body.memory !== undefined && !existing) return unknownEntry(body.memory);
      if (body.memory === undefined && entriesOf(scope).size >= MEMORY_MAX_ENTRIES) return fail({ code: "memory_full", params: { limit: String(MEMORY_MAX_ENTRIES) }, message: `the memory holds ${MEMORY_MAX_ENTRIES} entries already` });
      if (existing && existing.version !== body.version) {
        changedRefused = true;
        return fail({ code: "memory_changed", params: { memory: existing.id, summary: existing.summary }, message: `the memory entry ${existing.id} (${existing.summary}) was saved by someone else since it was read` });
      }
      const now = Date.now();
      apply({
        type: "memory_saved",
        entry: {
          scope,
          id: existing?.id ?? newMemoryId(used(scope)),
          summary: body.summary.trim(),
          content: body.content,
          local: scope.kind === "console" ? body.local ?? existing?.local ?? false : false,
          created_at: existing?.created_at ?? now,
          updated_at: now,
          updated_by: { kind: "user" },
          version: (existing?.version ?? 0) + 1,
        },
      });
      return { type: "ack", id: null };
    },
    delete_memory: (body) => {
      const scope = scopeOf(body);
      if (!entriesOf(scope).has(body.memory)) return unknownEntry(body.memory);
      apply({ type: "memories_deleted", scope, memories: [body.memory] });
      return { type: "ack", id: null };
    },
    delete_memories: (body) => {
      clear(scopeOf(body));
      return { type: "ack", id: null };
    },
    list_orphaned_memories: () => {
      if (staged?.orphanListing === "pending") return neverReplied();
      if (staged?.orphanListing) return fail(staged.orphanListing);
      return { type: "orphaned_memory_list", id: null, places: orphans() };
    },
    delete_orphaned_memory: (body) => {
      const using = projectAt(body.host_id, body.path);
      if (using) return placeInUse(using.id);
      clear({ kind: "place", host_id: body.host_id, path: body.path });
      return { type: "ack", id: null };
    },
    delete_orphaned_memories: () => {
      // Each place's deletion is told on its own, and what is left behind once, after all of them, as the daemon does.
      for (const { host_id, path } of orphans()) {
        const scope: MemoryScope = { kind: "place", host_id, path };
        const entries = entriesOf(scope);
        const memories = [...entries.keys()];
        entries.clear();
        deliver({ type: "memories_deleted", scope, memories });
      }
      announceOrphans();
      return { type: "ack", id: null };
    },
  };

  const projects: ProjectMemoryHooks = {
    refuseRemoval: (project, deleteMemory) => {
      const removing = store.getState().projects.get(project);
      if (!deleteMemory || !removing) return;
      const other = projectAt(removing.host_id, removing.path, project);
      if (other) placeInUse(other.id);
    },
    removed: (project, deleteMemory) => {
      const removed = store.getState().projects.get(project);
      if (!removed) return;
      const scope: MemoryScope = { kind: "place", host_id: removed.host_id, path: removed.path };
      // Another project at the place keeps the memory in use, so nothing is deleted; what is left behind is announced
      // on every removal, as the daemon does.
      if (deleteMemory && !projectAt(removed.host_id, removed.path, project)) clear(scope);
      announceOrphans(project);
    },
  };
  return { replies, hear: apply, projects };
}
