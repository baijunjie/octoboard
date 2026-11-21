// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { byAgent, CONSOLE_SCOPE, deletedEvent, entryOf, placeScope, PROJECT, PROJECT_SCOPE, savedEvent } from "./memoryBuilders";
import type { MemoryChangeEvent } from "../store";
import { afterQueuedWork } from "../test/clock";
import { consoleOf, projectOf, snapshotState } from "./builders";
import { createFixtureDaemon } from "./fixtureDaemon";
import type { MemoryFixture } from "./fixture";

const daemonOf = (memory?: MemoryFixture, projects = [PROJECT]) => {
  const daemon = createFixtureDaemon({ state: snapshotState({ consoles: [consoleOf("c-1", "Main")], projects, sessions: [] }), memory });
  const heard: MemoryChangeEvent[] = [];
  daemon.onMemoryChange((event) => heard.push(event));
  return { daemon, heard };
};
/** A request that creates an entry (neither `memory` nor `version`), or updates `memory` at `version`, in console `c-1`'s memory
 * or, with `project`, in the memory of that project's place. */
const save = (target: { memory?: string; version?: number; project?: string } = {}, summary = " About it ", local?: boolean) => {
  const { project, ...rest } = target;
  return { type: "save_memory" as const, ...(project === undefined ? { console: "c-1" } : { project }), summary, content: "Text", ...(local === undefined ? {} : { local }), ...rest };
};
const savedEntry = (event: MemoryChangeEvent | undefined) => (event as Extract<MemoryChangeEvent, { type: "memory_saved" }>).entry;
const orphanBroadcasts = (heard: MemoryChangeEvent[]) => heard.filter((event) => event.type === "orphaned_memory_list");

describe("the fixture daemon's console memory", () => {
  it("lists the entries without their content and scope, the most recently updated first, and reads one in full", async () => {
    const { daemon } = daemonOf({ entries: [entryOf("old00000", { updated_at: 1_000 }), entryOf("new00000", { updated_at: 5_000, local: true })] });
    const listing = await daemon.request({ type: "list_memories", console: "c-1" });
    expect(listing.scope).toEqual(CONSOLE_SCOPE);
    expect(listing.memories.map((line) => line.id)).toEqual(["new00000", "old00000"]);
    expect(listing.memories[0]).toMatchObject({ local: true });
    expect(listing.memories[0]).not.toHaveProperty("content");
    expect(listing.memories[0]).not.toHaveProperty("scope");
    expect((await daemon.request({ type: "read_memory", console: "c-1", memory: "old00000" })).entry.content).toBe("Content of old00000");
    await expect(daemon.request({ type: "read_memory", console: "c-1", memory: "none0000" })).rejects.toMatchObject({ code: "unknown_memory", params: { memory: "none0000" } });
    await expect(daemon.request({ type: "list_memories", console: "missing" })).rejects.toMatchObject({ code: "unknown_console" });
  });

  it("holds a listing back or refuses it as the scenario says", async () => {
    const error = { code: "store_write_failed", params: { cause: "other", detail: "disk I/O error" }, message: "disk I/O error" };
    await expect(daemonOf({ listing: error }).daemon.request({ type: "list_memories", console: "c-1" })).rejects.toMatchObject({ code: "store_write_failed" });
    const settled = vi.fn();
    void daemonOf({ listing: "pending" }).daemon.request({ type: "list_memories", console: "c-1" }).then(settled, settled);
    await afterQueuedWork();
    expect(settled).not.toHaveBeenCalled();
  });

  it("creates an entry as the user under an id it assigns, announcing it, and updates one at its version", async () => {
    const { daemon, heard } = daemonOf({ entries: [entryOf("kept0000", { version: 3 })] });
    expect(await daemon.request(save())).toEqual({ type: "ack", id: null });
    expect(savedEntry(heard[0])).toMatchObject({ scope: CONSOLE_SCOPE, summary: "About it", local: false, version: 1, updated_by: { kind: "user" } });
    const created = savedEntry(heard[0]);
    expect(created.id).toMatch(/^[a-z0-9]{8}$/);
    // The same summary is no clash: entries are told apart by id.
    await daemon.request(save());
    expect(savedEntry(heard[1]).id).not.toBe(created.id);
    await daemon.request(save({ memory: "kept0000", version: 3 }, "Changed"));
    expect(savedEntry(heard[2])).toMatchObject({ id: "kept0000", summary: "Changed", version: 4, created_at: 1_000 });
    expect((await daemon.request({ type: "list_memories", console: "c-1" })).memories.find((line) => line.id === "kept0000")?.version).toBe(4);
    expect(heard).toHaveLength(3);
  });

  it("marks a console's entry local when asked and keeps the mark over an update that leaves it out", async () => {
    const { daemon, heard } = daemonOf({ entries: [entryOf("kept0000", { local: true })] });
    await daemon.request(save({}, "Mine", true));
    expect(savedEntry(heard[0]).local).toBe(true);
    await daemon.request(save({ memory: "kept0000", version: 1 }, "Edited"));
    expect(savedEntry(heard[1])).toMatchObject({ id: "kept0000", local: true });
    await daemon.request(save({ memory: "kept0000", version: 2 }, "Edited", false));
    expect(savedEntry(heard[2]).local).toBe(false);
  });

  it("refuses what the daemon refuses", async () => {
    const { daemon, heard } = daemonOf({ entries: [entryOf("kept0000", { summary: "The kept one", version: 4 })] });
    await expect(daemon.request(save({ memory: "kept0000" }))).rejects.toMatchObject({ code: "field_required", params: { field: "version" } });
    await expect(daemon.request(save({ version: 4 }))).rejects.toMatchObject({ code: "field_required", params: { field: "memory" } });
    await expect(daemon.request(save({ memory: "kept0000", version: 3 }))).rejects.toMatchObject({ code: "memory_changed", params: { memory: "kept0000", summary: "The kept one" } });
    await expect(daemon.request(save({ memory: "none0000", version: 1 }))).rejects.toMatchObject({ code: "unknown_memory", params: { memory: "none0000" } });
    expect(heard).toEqual([]);
  });

  it("refuses a request that names both a console and a project, or neither", async () => {
    const { daemon } = daemonOf();
    await expect(daemon.request({ type: "list_memories", console: "c-1", project: PROJECT.id })).rejects.toMatchObject({ code: "unreadable_request" });
    await expect(daemon.request({ type: "list_memories" })).rejects.toMatchObject({ code: "field_required", params: { field: "console" } });
    await expect(daemon.request({ type: "list_memories", project: "missing" })).rejects.toMatchObject({ code: "unknown_project" });
  });

  it("refuses a new entry in a memory holding 200", async () => {
    const entries = Array.from({ length: 200 }, (_, index) => entryOf(`n${String(index).padStart(7, "0")}`));
    await expect(daemonOf({ entries }).daemon.request(save())).rejects.toMatchObject({ code: "memory_full", params: { limit: "200" } });
  });

  it("checks a save against, and reads, the entry as the daemon holds it now when the window never heard of the change", async () => {
    const { daemon, heard } = daemonOf({ entries: [entryOf("a0000000")], unheard: [entryOf("a0000000", { summary: "Newer text", version: 2 })] });
    expect((await daemon.request({ type: "list_memories", console: "c-1" })).memories[0].version).toBe(1);
    await expect(daemon.request(save({ memory: "a0000000", version: 1 }))).rejects.toMatchObject({ code: "memory_changed", params: { memory: "a0000000", summary: "Newer text" } });
    // Reading the entry again finds what the daemon holds now.
    expect((await daemon.request({ type: "read_memory", console: "c-1", memory: "a0000000" })).entry).toMatchObject({ summary: "Newer text", version: 2 });
    expect(heard).toEqual([]);
  });

  it("announces deletions, and nothing for a memory with no entries", async () => {
    const { daemon, heard } = daemonOf({ entries: [entryOf("a0000000"), entryOf("b0000000")] });
    await daemon.request({ type: "delete_memory", console: "c-1", memory: "a0000000" });
    await expect(daemon.request({ type: "delete_memory", console: "c-1", memory: "a0000000" })).rejects.toMatchObject({ code: "unknown_memory", params: { memory: "a0000000" } });
    await daemon.request({ type: "delete_memories", console: "c-1" });
    await daemon.request({ type: "delete_memories", console: "c-1" });
    expect(heard).toEqual([deletedEvent(["a0000000"]), deletedEvent(["b0000000"])]);
  });

  it("applies a change a session made to what it holds and delivers it", async () => {
    const { daemon, heard } = daemonOf();
    const entry = entryOf("sess0000", { updated_by: byAgent("codex") });
    daemon.deliverMemoryChange(savedEvent(entry));
    expect(heard).toEqual([savedEvent(entry)]);
    expect((await daemon.request({ type: "read_memory", console: "c-1", memory: "sess0000" })).entry).toEqual(entry);
  });
});

describe("the fixture daemon's project memory", () => {
  const placeEntry = (id: string, extra = {}) => entryOf(id, { scope: PROJECT_SCOPE, ...extra });

  it("reads a project's memory from the place the project is at, shared with any project there", async () => {
    const sibling = { ...projectOf("p-2", "c-1", "Copy"), host_id: PROJECT.host_id, path: PROJECT.path };
    const { daemon } = daemonOf({ entries: [placeEntry("place000"), entryOf("cons0000")] }, [PROJECT, sibling]);
    for (const project of [PROJECT.id, sibling.id]) {
      const listing = await daemon.request({ type: "list_memories", project });
      expect(listing.scope).toEqual(PROJECT_SCOPE);
      expect(listing.memories.map((line) => line.id)).toEqual(["place000"]);
    }
    expect((await daemon.request({ type: "read_memory", project: PROJECT.id, memory: "place000" })).entry.scope).toEqual(PROJECT_SCOPE);
    await expect(daemon.request({ type: "read_memory", project: PROJECT.id, memory: "cons0000" })).rejects.toMatchObject({ code: "unknown_memory" });
  });

  it("saves an entry of a place, never local, and announces it with the place's scope", async () => {
    const { daemon, heard } = daemonOf();
    await daemon.request(save({ project: PROJECT.id }, "A fact", true));
    expect(savedEntry(heard[0])).toMatchObject({ scope: PROJECT_SCOPE, summary: "A fact", local: false });
    // A project is at the place, so nothing is left behind.
    expect(orphanBroadcasts(heard)).toEqual([]);
    await daemon.request({ type: "delete_memories", project: PROJECT.id });
    expect(heard[1]).toEqual(deletedEvent([savedEntry(heard[0]).id], PROJECT_SCOPE));
  });
});

describe("the fixture daemon's memory left behind by removed projects", () => {
  const away = placeScope("local", "/work/gone");
  const unknownHost = placeScope("ssh-old", "/srv/old");
  const entries = [entryOf("a0000000", { scope: away, updated_at: 5_000 }), entryOf("b0000000", { scope: away, updated_at: 3_000 }), entryOf("c0000000", { scope: unknownHost, updated_at: 9_000 }), entryOf("d0000000", { scope: PROJECT_SCOPE })];

  it("lists the places no project is at, the most recently updated first, with their entry counts", async () => {
    const { daemon } = daemonOf({ entries });
    expect((await daemon.request({ type: "list_orphaned_memories" })).places).toEqual([
      { host_id: "ssh-old", path: "/srv/old", entries: 1, updated_at: 9_000 },
      { host_id: "local", path: "/work/gone", entries: 2, updated_at: 5_000 },
    ]);
  });

  it("deletes a place's memory, announcing the entries and what is left, and refuses a place a project is at", async () => {
    const { daemon, heard } = daemonOf({ entries });
    await daemon.request({ type: "delete_orphaned_memory", host_id: "local", path: "/work/gone" });
    expect(heard[0]).toMatchObject({ type: "memories_deleted", scope: away });
    expect(heard.at(-1)).toEqual({ type: "orphaned_memory_list", id: null, places: [{ host_id: "ssh-old", path: "/srv/old", entries: 1, updated_at: 9_000 }] });
    await expect(daemon.request({ type: "delete_orphaned_memory", host_id: PROJECT.host_id, path: PROJECT.path })).rejects.toMatchObject({ code: "memory_place_in_use", params: { project: PROJECT.id } });
  });

  it("leaves a removed project's memory behind unless the removal deletes it, announcing what is left", async () => {
    const kept = daemonOf({ entries });
    await kept.daemon.request({ type: "delete_project", project: PROJECT.id });
    expect(kept.heard).toEqual([expect.objectContaining({ type: "orphaned_memory_list", id: null })]);
    expect(orphanBroadcasts(kept.heard).at(-1)).toMatchObject({ places: expect.arrayContaining([{ host_id: "local", path: "/work/app", entries: 1, updated_at: 2_000 }]) });
    const deleted = daemonOf({ entries });
    await deleted.daemon.request({ type: "delete_project", project: PROJECT.id, delete_memory: true });
    expect(deleted.heard[0]).toEqual(deletedEvent(["d0000000"], PROJECT_SCOPE));
    expect(orphanBroadcasts(deleted.heard).at(-1)).toMatchObject({ places: [expect.objectContaining({ path: "/srv/old" }), expect.objectContaining({ path: "/work/gone" })] });
  });

  it("refuses to delete the memory with a project while another project is at the place", async () => {
    const sibling = { ...projectOf("p-2", "c-1", "Copy"), host_id: PROJECT.host_id, path: PROJECT.path };
    const { daemon, heard } = daemonOf({ entries }, [PROJECT, sibling]);
    await expect(daemon.request({ type: "delete_project", project: PROJECT.id, delete_memory: true })).rejects.toMatchObject({ code: "memory_place_in_use", params: { project: sibling.id } });
    expect(heard).toEqual([]);
    // Without the option the project goes and the sibling still reads the memory, so the place is not left behind; what
    // is left behind is announced all the same, as on every removal.
    await daemon.request({ type: "delete_project", project: PROJECT.id });
    expect(heard).toEqual([{ type: "orphaned_memory_list", id: null, places: [expect.objectContaining({ path: "/srv/old" }), expect.objectContaining({ path: "/work/gone" })] }]);
  });
});

it("refuses the listing of the memory left behind with the scenario's error", async () => {
  const { daemon } = daemonOf({ entries: [], orphanListing: { code: "internal_error", params: {}, message: "the store could not be read" } });
  await expect(daemon.request({ type: "list_orphaned_memories" })).rejects.toMatchObject({ code: "internal_error" });
});

it("orders places updated at the same time by host, then path, by code unit, as the daemon does", async () => {
  const at = (host: string, path: string, id: string) => entryOf(id, { scope: placeScope(host, path), updated_at: 7_000 });
  const { daemon } = daemonOf({ entries: [at("b-host", "/a", "a0000000"), at("a-host", "/b", "b0000000"), at("a-host", "/B", "c0000000")] });
  expect((await daemon.request({ type: "list_orphaned_memories" })).places.map((place) => `${place.host_id}:${place.path}`)).toEqual(["a-host:/B", "a-host:/b", "b-host:/a"]);
});
