// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { providerLabel } from "../agents/providers";
import { translatorFor } from "../i18n/catalog";
import { joinValues } from "../i18n/joinPhrases";
import { fakeTimers, runPendingTimers } from "../test/clock";
import type { Agent, ClientRequestBody, Event, OrphanedMemory, ProviderInfo } from "../protocol";
import { served as claude } from "../providers/claude.fixture";
import { served as codex } from "../providers/codex.fixture";
import { served as pi } from "../providers/pi.fixture";
import { served as testAgent } from "../providers/testAgent.fixture";
import { DEFAULT_SETTINGS, createMemoryListeners } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { MemorySection } from "./MemorySection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => fakeTimers());

const NOW = Date.now();
const leftBehind = (path: string, extra: Partial<OrphanedMemory> = {}): OrphanedMemory => ({ host_id: "local", path, entries: 3, updated_at: NOW - 2 * 3600_000, ...extra });

function mount(
  places: OrphanedMemory[],
  {
    hosts = [{ id: "local", name: "This computer", kind: "local" as const, ssh_config: null }],
    list,
    providers,
    added,
  }: { hosts?: { id: string; name: string; kind: "local"; ssh_config: null }[]; list?: () => Promise<Event>; providers?: ProviderInfo[]; added?: Agent[] } = {},
) {
  const listeners = createMemoryListeners();
  const request = vi.fn((body: ClientRequestBody): Promise<Event> =>
    body.type === "list_orphaned_memories" && list ? list() : Promise.resolve(body.type === "list_orphaned_memories" ? { type: "orphaned_memory_list", id: null, places } : { type: "ack", id: null }));
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1, hosts: new Map(hosts.map((host) => [host.id, host])), ...(providers && { providers }), ...(added && { settings: { ...DEFAULT_SETTINGS, added_agents: added } }) }, request: request as never, onMemoryChange: listeners.subscribe });
  const container = mountHost(<MemorySection />, { daemon });
  return { container, request, daemon, broadcast: (next: OrphanedMemory[]) => act(() => listeners.deliver({ type: "orphaned_memory_list", id: null, places: next })) };
}

const settle = async () => {
  await act(async () => {});
  runPendingTimers();
};
const click = (element: Element | null | undefined) => act(async () => void (element as HTMLElement | undefined)?.click());
const deleteButtons = () => [...document.querySelectorAll<HTMLButtonElement>('button[aria-label^="Delete memory kept for"]')];
/** The button showing `text`, by its visible content alone: an unavailable button also holds its reason, hidden. */
const button = (root: ParentNode, text: string) =>
  [...root.querySelectorAll<HTMLButtonElement>("button")].find((element) => (element.querySelector(".app-button__content") ?? element).textContent === text);
const manageButton = (container: HTMLElement) => button(container, "Manage")!;
const row = (container: HTMLElement) => container.querySelector<HTMLElement>('[data-settings-search-id="memory.leftBehind"]')!;
/** The list dialog, opened from the row's Manage button. */
const openManage = async (container: HTMLElement) => {
  await click(manageButton(container));
  return document.querySelector<HTMLElement>('[role="dialog"]')!;
};

it("shows two rows, the switch and then the memory of removed projects, with no group heading", async () => {
  const { container } = mount([leftBehind("/work/old")]);
  await settle();
  const order = [...container.querySelectorAll<HTMLElement>("[data-settings-search-id]")].map((element) => element.dataset.settingsSearchId);
  expect(order).toEqual(["memory_enabled", "memory.leftBehind"]);
  expect(container.querySelector("h3, [data-settings-search-group]")).toBeNull();
  expect(row(container).textContent).toContain("Memory of removed projects");
  expect(row(container).textContent).toContain("adding a project at the same folder picks it up again");
});

it("counts the folders left behind beside an available Manage button", async () => {
  const { container } = mount([leftBehind("/work/old"), leftBehind("/srv/other", { host_id: "gone-host" })]);
  await settle();
  expect(row(container).textContent).toContain("2 folders");
  expect(manageButton(container).getAttribute("aria-disabled")).toBeNull();
  // The places themselves are only in the dialog.
  expect(container.textContent).not.toContain("/work/old");
});

it("makes Manage unavailable, saying why, when nothing is left behind", async () => {
  const { container } = mount([]);
  await settle();
  // No folder count: the row's only tabular-numbers element is the count.
  expect(row(container).querySelector(".tabular-nums")).toBeNull();
  const manage = manageButton(container);
  expect(manage.getAttribute("aria-disabled")).toBe("true");
  const reason = manage.getAttribute("aria-describedby")!.split(" ").map((id) => document.getElementById(id)?.textContent).join(" ");
  expect(reason).toContain("No memory left behind");
  await click(manage);
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("lists each place's memory left behind in the dialog with its host, directory, entry count and last update", async () => {
  const { container } = mount([leftBehind("/work/old"), leftBehind("/srv/other", { host_id: "gone-host", entries: 1 })]);
  await settle();
  const dialog = await openManage(container);
  expect(dialog.textContent).toContain("Memory of removed projects");
  const items = [...dialog.querySelectorAll("li")];
  expect(items).toHaveLength(2);
  expect(items[0]!.textContent).toContain("/work/old");
  expect(items[0]!.textContent).toContain("This computer");
  expect(items[0]!.textContent).toContain("3 entries");
  expect(items[0]!.textContent).toContain("2 hours ago");
  // A host the window does not list is named neutrally, never by its id.
  expect(items[1]!.textContent).toContain("Unknown machine");
  expect(items[1]!.textContent).not.toContain("gone-host");
  expect(items[1]!.textContent).toContain("1 entry");
  expect(deleteButtons()).toHaveLength(2);
  await click(button(dialog, "Close"));
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("asks before deleting, then deletes the place named by host and path", async () => {
  const { container, request } = mount([leftBehind("/work/old")]);
  await settle();
  await openManage(container);
  await click(deleteButtons()[0]);
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "delete_orphaned_memory" }));
  const confirm = document.querySelector('[role="alertdialog"]') ?? [...document.querySelectorAll('[role="dialog"]')].at(-1)!;
  expect(confirm.textContent).toContain("/work/old");
  expect(confirm.textContent).toContain("It holds 3 entries");
  await click(button(confirm, "Delete"));
  expect(request).toHaveBeenCalledWith({ type: "delete_orphaned_memory", host_id: "local", path: "/work/old" });
});

it("deletes nothing when the confirmation is cancelled", async () => {
  const { container, request } = mount([leftBehind("/work/old")]);
  await settle();
  await openManage(container);
  await click(deleteButtons()[0]);
  const confirm = document.querySelector('[role="alertdialog"]') ?? [...document.querySelectorAll('[role="dialog"]')].at(-1)!;
  await click(button(confirm, "Cancel"));
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "delete_orphaned_memory" }));
});

it("follows the broadcast in the row and the open dialog, saying so when the last place goes", async () => {
  const { container, broadcast } = mount([leftBehind("/work/old")]);
  await settle();
  expect(row(container).textContent).toContain("1 folder");
  const dialog = await openManage(container);
  broadcast([leftBehind("/work/old"), leftBehind("/work/newer")]);
  expect(row(container).textContent).toContain("2 folders");
  expect(dialog.textContent).toContain("/work/newer");
  broadcast([]);
  expect(dialog.querySelector("li")).toBeNull();
  expect(dialog.textContent).toContain("No memory left behind");
  expect(manageButton(container).getAttribute("aria-disabled")).toBe("true");
  expect(button(dialog, "Delete all")!.disabled).toBe(true);
});

/** The confirmation over the list dialog. */
const confirmation = () => document.querySelector<HTMLElement>('[role="alertdialog"]') ?? [...document.querySelectorAll<HTMLElement>('[role="dialog"]')].at(-1)!;
/** Types the word a type-to-confirm asks for into its field. */
const typeWord = (root: HTMLElement, word: string) =>
  act(() => {
    const input = root.querySelector<HTMLInputElement>("input")!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, word);
    input.dispatchEvent(new globalThis.Event("input", { bubbles: true }));
  });

it("puts Delete all before Close in the dialog's footer", async () => {
  const { container } = mount([leftBehind("/work/old")]);
  await settle();
  const dialog = await openManage(container);
  const footer = [...dialog.querySelectorAll("button")].map((element) => element.querySelector(".app-button__content")?.textContent).filter((text) => text === "Delete all" || text === "Close");
  expect(footer).toEqual(["Delete all", "Close"]);
  expect(button(dialog, "Delete all")!.disabled).toBe(false);
});

it("asks before deleting all, naming how many folders and entries, then deletes them in one request", async () => {
  const { container, request } = mount([leftBehind("/work/old"), leftBehind("/srv/other", { entries: 1 })]);
  await settle();
  const dialog = await openManage(container);
  await click(button(dialog, "Delete all"));
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "delete_orphaned_memories" }));
  const confirm = confirmation();
  expect(confirm.textContent).toContain("Delete all memory");
  expect(confirm.textContent).toContain("The memory kept for 2 folders will be deleted.");
  expect(confirm.textContent).toContain("It holds 4 entries in all. This cannot be undone.");
  const confirmButton = button(confirm, "Delete all")!;
  // The heaviest loss asks for the word first.
  expect(confirmButton.disabled).toBe(true);
  await typeWord(confirm, "DELETE");
  await click(button(confirm, "Delete all"));
  await settle();
  expect(request).toHaveBeenCalledWith({ type: "delete_orphaned_memories" });
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "delete_orphaned_memory" }));
});

it("deletes nothing when the confirmation to delete all is cancelled", async () => {
  const { container, request } = mount([leftBehind("/work/old")]);
  await settle();
  const dialog = await openManage(container);
  await click(button(dialog, "Delete all"));
  await click(button(confirmation(), "Cancel"));
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "delete_orphaned_memories" }));
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
});

it("lists again after a snapshot", async () => {
  const { container, broadcast, daemon, request } = mount([leftBehind("/work/old")]);
  await settle();
  broadcast([]);
  const listings = () => request.mock.calls.filter((call) => call[0].type === "list_orphaned_memories").length;
  expect(listings()).toBe(1);
  act(() => daemon.store.setState({ snapshotEpoch: 2 }));
  await settle();
  expect(listings()).toBe(2);
  expect(row(container).textContent).toContain("1 folder");
});

it("keeps the memory of removed projects whether the switch is on or off", async () => {
  const { container, daemon } = mount([leftBehind("/work/old")]);
  await settle();
  const sw = container.querySelector<HTMLInputElement>('[data-settings-search-id="memory_enabled"] input');
  expect(sw!.checked).toBe(false);
  expect(container.textContent).toContain("Let sessions share");
  expect(row(container).textContent).toContain("1 folder");
  act(() => daemon.store.setState((state) => ({ settings: { ...state.settings, memory_enabled: true } })));
  expect(row(container).textContent).toContain("1 folder");
});

it("turns memory on through the settings write", async () => {
  const { container, request } = mount([]);
  await settle();
  await click(container.querySelector('[data-settings-search-id="memory_enabled"] input'));
  expect(request).toHaveBeenCalledWith(expect.objectContaining({ type: "update_settings", memory_enabled: true }));
});

it("says why Manage is unavailable when the places could not be listed, and lists them on Try again", async () => {
  let failing = true;
  const { container } = mount([leftBehind("/work/old")], {
    list: () => (failing ? Promise.reject(new Error("connection lost")) : Promise.resolve({ type: "orphaned_memory_list", id: null, places: [leftBehind("/work/old")] })),
  });
  await settle();
  const manage = manageButton(container);
  expect(manage.getAttribute("aria-disabled")).toBe("true");
  const reason = manage.getAttribute("aria-describedby")!.split(" ").map((id) => document.getElementById(id)?.textContent).join(" ");
  expect(reason).toContain("Could not list the memory left behind");
  // The mark before the button opens the failure, with Try again.
  await click(row(container).querySelector('button[aria-label="Could not list the memory left behind"]'));
  failing = false;
  await click([...document.querySelectorAll<HTMLButtonElement>("button")].find((element) => element.textContent === "Try again"));
  await settle();
  expect(row(container).textContent).toContain("1 folder");
  expect(manageButton(container).getAttribute("aria-disabled")).toBeNull();
});

it("says a place updated within the last minute was updated just now", async () => {
  const { container } = mount([leftBehind("/work/old", { updated_at: Date.now() })]);
  await settle();
  const dialog = await openManage(container);
  expect(dialog.querySelector("li")!.textContent).toContain("updated just now");
});

it("names a machine the daemon lists as a reference in the delete confirmation, and an unknown one in plain words", async () => {
  const { container } = mount([leftBehind("/work/old"), leftBehind("/srv/other", { host_id: "gone-host", updated_at: NOW - 3 * 3600_000 })]);
  await settle();
  await openManage(container);
  const confirmFor = async (index: number) => {
    await click(deleteButtons()[index]);
    return document.querySelector('[role="alertdialog"]') ?? [...document.querySelectorAll('[role="dialog"]')].at(-1)!;
  };
  const known = await confirmFor(0);
  expect([...known.querySelectorAll("[data-inline-reference]")].map((element) => element.textContent)).toContain("This computer");
  await click(button(known, "Cancel"));
  const unknown = await confirmFor(1);
  expect(unknown.textContent).toContain("Unknown machine");
  expect([...unknown.querySelectorAll("[data-inline-reference]")].map((element) => element.textContent)).not.toContain("Unknown machine");
});

/** Text with the isolates a row puts around a value it substitutes taken out. */
const withoutIsolates = (text: string) => text.replace(/[\u2068\u2069]/g, "");

/** The switch row's description, as `withoutIsolates` leaves it. */
const switchDescription = (container: HTMLElement) => withoutIsolates(container.querySelector('[data-settings-search-id="memory_enabled"] [data-setting-description]')?.textContent ?? "");

/** The names the switch row's description sets apart as references, in order. */
const namedAgents = (container: HTMLElement) =>
  [...container.querySelectorAll('[data-settings-search-id="memory_enabled"] [data-setting-description] [data-inline-reference]')].map((element) => element.textContent);

const en = translatorFor("en");

/** The English sentence `key` names `ids` in, each by its full name among `providers`. */
const naming = (key: "settings.memory.enabled.nativeMemory.turnedOff" | "settings.memory.enabled.nativeMemory.kept", providers: ProviderInfo[], ids: string[]) =>
  withoutIsolates(en(key, { agents: joinValues("en", ids.map((id) => providerLabel(en, providers, id))) }));

it("names, under the switch's description, the agents whose own memory those sessions turn off and those whose own memory they leave as it is set", async () => {
  // Two whose memory one launch turns off, one whose memory no launch can, and one with no memory of its own.
  const providers: ProviderInfo[] = [
    { ...claude, native_memory: "switchable" },
    { ...codex, native_memory: "switchable" },
    { ...pi, native_memory: "unswitchable" },
    { ...testAgent, native_memory: "absent" },
  ];
  const { container } = mount([], { providers });
  await settle();
  const description = switchDescription(container);
  expect(description).toContain(en("settings.memory.enabled.sharedDescription.off"));
  expect(description).toContain(naming("settings.memory.enabled.nativeMemory.turnedOff", providers, [claude.id, codex.id]));
  expect(description).toContain(naming("settings.memory.enabled.nativeMemory.kept", providers, [pi.id]));
  // Each agent is named as a reference, not as plain text of the sentence, and the one with no memory of its own not at all.
  expect(namedAgents(container)).toEqual([claude.id, codex.id, pi.id].map((id) => providerLabel(en, providers, id)));
});

it("says nothing of the agents' own memory when no served agent keeps one, or keeps one of a kind this build does not know", async () => {
  const providers: ProviderInfo[] = [{ ...claude, native_memory: "absent" }, { ...codex, native_memory: "remembered_elsewhere" }];
  const { container } = mount([], { providers });
  await settle();
  expect(switchDescription(container)).toBe(en("settings.memory.enabled.sharedDescription.off"));
  expect(namedAgents(container)).toEqual([]);
});

it("leaves out the sentence with no agent to name", async () => {
  const providers: ProviderInfo[] = [{ ...pi, native_memory: "unswitchable" }];
  const { container } = mount([], { providers });
  await settle();
  expect(switchDescription(container)).toBe(en("settings.memory.enabled.sharedDescription.off") + naming("settings.memory.enabled.nativeMemory.kept", providers, [pi.id]));
});

it("names only the agents the user has added, in the order Settings lists them", async () => {
  const providers: ProviderInfo[] = [
    { ...claude, native_memory: "switchable" },
    { ...codex, native_memory: "switchable" },
    { ...pi, native_memory: "unswitchable" },
    { ...testAgent, native_memory: "switchable" },
  ];
  // The test agent keeps a memory one launch can turn off too, but is not added; the others are listed out of the registry's order.
  const { container } = mount([], { providers, added: [codex.id, claude.id, pi.id] });
  await settle();
  expect(namedAgents(container)).toEqual([codex.id, claude.id, pi.id].map((id) => providerLabel(en, providers, id)));
  expect(switchDescription(container)).toContain(naming("settings.memory.enabled.nativeMemory.turnedOff", providers, [codex.id, claude.id]));
  expect(switchDescription(container)).not.toContain(providerLabel(en, providers, testAgent.id));
});
