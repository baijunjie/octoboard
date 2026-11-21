// @vitest-environment jsdom
import { act } from "react";
import { beforeEach, expect, it } from "vitest";

import { providerLabel } from "../../agents/providers";
import { errorAlerts } from "../../components/alerts/errorAlerts";
import { translatorFor } from "../../i18n/catalog";
import { fakeTimers, runPendingTimers } from "../../test/clock";
import { buttons, click, inputLabelled, type } from "../../test/dom";
import { daemonError } from "../../test/daemonError";
import { TEST_PROVIDERS } from "../../test/testProviders";
import { indexLine } from "./memoryState";
import { deletedEvent, entryOf, renderMemory, savedEvent, settled } from "./memoryTestSupport";

/** Two providers the daemon serves, each with a name of its own, and how the window names one. */
const named = TEST_PROVIDERS[0]!;
const other = TEST_PROVIDERS[1]!;
const nameOf = (id: string) => providerLabel(translatorFor("en"), TEST_PROVIDERS, id);

beforeEach(() => fakeTimers());

const text = () => document.body.textContent ?? "";
const button = (label: string) => buttons().find((candidate) => candidate.textContent === label);
// The relative times keep a ticker armed, so the clock is advanced one round of pending timers at a time, never run out.
const settle = async () => {
  await settled();
  runPendingTimers();
};
/** The row whose summary is exactly `summary` (`entryOf` describes an entry "About <id>"). */
const row = (summary: string) =>
  Array.from(document.querySelectorAll("li")).find((item) => Array.from(item.querySelectorAll("span")).some((span) => span.textContent === summary))!;
const rowPress = (summary: string) => row(summary).querySelector<HTMLButtonElement>("button")!;
async function chooseFromMenu(summary: string, item: string) {
  const trigger = row(summary).querySelector<HTMLButtonElement>('button[aria-label^="Actions for entry"]')!;
  await act(async () => {
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  runPendingTimers();
  await click(Array.from(document.querySelectorAll('[role="menuitem"]')).find((candidate) => candidate.textContent?.startsWith(item)));
  runPendingTimers();
}

it("says the memory is empty, offers adding an entry once, and has nothing to delete", async () => {
  renderMemory();
  await settle();
  expect(text()).toContain("No memory yet");
  expect(text()).not.toContain("of 200");
  expect(buttons().filter((candidate) => candidate.textContent === "Add entry")).toHaveLength(1);
  expect(button("Delete all")!.disabled).toBe(true);
});

it("lists each entry with its summary, the machine-only tag where it has one, and when and by whom it was updated", async () => {
  const now = Date.now();
  renderMemory({
    entries: [
      entryOf("preferred-language", { summary: "Replies in Japanese", local: true, updated_at: now - 3 * 3600_000, updated_by: { kind: "agent", agent: named.id } }),
      entryOf("odd", { updated_by: { kind: "somebody" } as never }),
      entryOf("mine", { updated_by: { kind: "user" } }),
      entryOf("unnamed", { updated_at: now - 3 * 3600_000, updated_by: { kind: "agent" } }),
      entryOf("retired", { updated_at: now - 3 * 3600_000, updated_by: { kind: "agent", agent: "retired-agent" } }),
    ],
  });
  await settle();
  const first = row("Replies in Japanese").textContent ?? "";
  expect(first).toContain("Replies in Japanese");
  expect(first).toContain("This machine only");
  expect(row("About mine").textContent).not.toContain("This machine only");
  // The agent whose session wrote it is named as the app names it, as a reference within the sentence.
  expect(first).toContain(`Updated 3 hours ago by ${nameOf(named.id)}`);
  expect([...row("Replies in Japanese").querySelectorAll('[data-slot="chip"]')].map((chip) => chip.textContent)).toContain(nameOf(named.id));
  expect(row("About mine").textContent).toContain("by you");
  // An agent's entry that names no agent, or one this client cannot name, says an agent wrote it.
  expect(row("About unnamed").textContent).toContain("Updated 3 hours ago by an agent");
  expect(row("About retired").textContent).toContain("Updated 3 hours ago by an agent");
  expect(row("About retired").textContent).not.toContain("retired-agent");
  // A writer this client does not know is left out, not shown raw.
  expect(row("About odd").textContent).toMatch(/Updated .* ago$/);
});

it("shows an entry's content on pressing its row, reading it from the daemon", async () => {
  const { request } = renderMemory({ entries: [entryOf("a", { content: "Line one\nLine two" })] });
  await settle();
  await click(rowPress("About a"));
  await settle();
  expect(request).toHaveBeenCalledWith({ type: "read_memory", console: "c-1", memory: "a" });
  expect(document.querySelector("pre")?.textContent).toBe("Line one\nLine two");
});

it("counts the entries at the start of the footer, before its buttons", async () => {
  renderMemory({ entries: [entryOf("a")] });
  await settle();
  const footer = document.querySelector<HTMLElement>('[role="dialog"] [data-slot="modal-footer"]')!;
  const children = [...footer.children];
  expect(children[0]?.textContent).toBe("1 of 200 entries");
  expect(children.slice(1).map((child) => child.textContent)).toEqual(["Delete all", "Add entry"]);
  // The body no longer carries it.
  expect(document.querySelector('[role="dialog"] [data-slot="modal-body"]')?.textContent).not.toContain("of 200");
});

it("shows an entry a console session saves, and drops one it deletes, without reopening", async () => {
  const { broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  expect(text()).toContain("1 of 200 entries");
  broadcast(savedEvent(entryOf("b", { summary: "Fresh", updated_at: 9_000, updated_by: { kind: "agent" } })));
  expect(row("Fresh").textContent).toContain("Fresh");
  expect(text()).toContain("2 of 200 entries");
  // The most recently updated is first.
  expect(document.querySelector("li")!.textContent).toContain("b");
  broadcast(deletedEvent(["a"]));
  expect(document.body.textContent).not.toContain("About a");
  expect(text()).toContain("1 of 200 entries");
});

it("ignores a change to another console's memory", async () => {
  const { broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  broadcast(savedEvent(entryOf("other", { scope: { kind: "console", console_id: "c-2" } })));
  broadcast(deletedEvent(["a"], { kind: "console", console_id: "c-2" }));
  expect(text()).toContain("About a");
  expect(text()).not.toContain("other");
});

it("shows an entry's summary in full with its machine-only tag, then when it was updated, then its content", async () => {
  const summary = "A summary long enough to need the whole width of a row and then some more, which the view shows in full";
  renderMemory({ entries: [entryOf("a", { summary, local: true })] });
  await settle();
  await click(rowPress(summary));
  await settle();
  const view = Array.from(document.querySelectorAll('[role="dialog"]')).at(-1)!;
  const inView = (predicate: (element: Element) => boolean) => Array.from(view.querySelectorAll("p, span, pre")).find(predicate)!;
  const shown = inView((element) => element.textContent === summary);
  const tag = inView((element) => element.textContent === "This machine only");
  const updated = inView((element) => element.tagName === "P" && /^Updated /.test(element.textContent ?? ""));
  const content = view.querySelector("pre")!;
  const before = (first: Element, second: Element) => Boolean(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING);
  expect([before(shown, tag), before(tag, updated), before(updated, content)]).toEqual([true, true, true]);
  // The content takes the direction of its own text, as the summary does.
  expect(content.getAttribute("dir")).toBe("auto");
});

it("updates an open entry when a console session saves it, and says so when it is deleted", async () => {
  const { broadcast } = renderMemory({ entries: [entryOf("a", { content: "Before" })] });
  await settle();
  await click(rowPress("About a"));
  await settle();
  broadcast(savedEvent(entryOf("a", { content: "After", version: 2 })));
  expect(document.querySelector("pre")?.textContent).toBe("After");
  broadcast(deletedEvent(["a"]));
  expect(document.querySelector("pre")).toBeNull();
  expect(text()).toContain("This entry was deleted while it was open.");
});

it("lists again after every snapshot, and keeps what the listing finds changed", async () => {
  const entries = [entryOf("a")];
  const { store, listings } = renderMemory({ entries });
  await settle();
  expect(listings()).toBe(1);
  entries[0] = entryOf("a", { summary: "Changed while away", version: 2 });
  entries.push(entryOf("b"));
  act(() => store.setState({ snapshotEpoch: 2 }));
  await settle();
  expect(listings()).toBe(2);
  expect(text()).toContain("Changed while away");
  expect(row("About b")).toBeDefined();
});

it("offers Try again when the first listing fails, and lists on retry", async () => {
  let failing = true;
  const { listings } = renderMemory({
    entries: [entryOf("a")],
    respond: (body) => (body.type === "list_memories" && failing ? Promise.reject(daemonError("internal_error", {})) : undefined),
  });
  await settle();
  expect(text()).toContain("Could not load the memory");
  failing = false;
  await click(button("Try again"));
  await settle();
  expect(listings()).toBe(2);
  expect(text()).toContain("About a");
});

it("refuses a new entry past the limit with the reason", async () => {
  renderMemory({ entries: Array.from({ length: 200 }, (_, index) => entryOf(`e${index}`)) });
  await settle();
  const add = buttons().find((candidate) => candidate.textContent?.endsWith("Add entry"))!;
  expect(add.getAttribute("aria-disabled")).toBe("true");
  expect(add.textContent).toContain("The memory holds 200 entries, the most it can. Delete one to add another.");
  expect(text()).toContain("200 of 200 entries");
});

it("adds an entry: validates the fields, then sends it with neither an id nor a version", async () => {
  const { sent, broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  await click(button("Add entry"));
  await settle();
  expect(inputLabelled("Name")).toBeUndefined();
  await click(button("Add"));
  expect(text()).toContain("A summary is one line");
  expect(text()).toContain("Content is required.");
  expect(sent()).toEqual([]);
  // Two entries may share a summary.
  type(inputLabelled("Summary"), "About a");
  type(document.querySelector("textarea")!, "Some content");
  await click(inputLabelled("This machine only"));
  await click(button("Add"));
  expect(sent()).toEqual([
    { type: "save_memory", console: "c-1", summary: "About a", content: "Some content", local: true },
  ]);
  broadcast(savedEvent(entryOf("zzzzzzzz", { summary: "About a", local: true, updated_at: 5_000 })));
  await settle();
  expect(document.querySelectorAll("li")).toHaveLength(2);
});

it("holds content past the byte limit, counted in UTF-8, to the same limit as the daemon", async () => {
  const { sent } = renderMemory();
  await settle();
  await click(button("Add entry"));
  await settle();
  type(inputLabelled("Summary"), "d");
  // 6000 three-byte characters are 18000 bytes, though only 6000 characters.
  type(document.querySelector("textarea")!, "あ".repeat(6000));
  await click(button("Add"));
  expect(text()).toMatch(/The content is 18,000 bytes\. It can be at most 16,384\./);
  expect(sent()).toEqual([]);
});

it("edits an entry by its id, with the version it read", async () => {
  const { sent } = renderMemory({ entries: [entryOf("a", { version: 3, local: true })] });
  await settle();
  await chooseFromMenu("About a", "Edit");
  await settle();
  type(inputLabelled("Summary"), "Better");
  await click(button("Save"));
  expect(sent()).toEqual([
    { type: "save_memory", console: "c-1", memory: "a", summary: "Better", content: "Content of a", local: true, version: 3 },
  ]);
});

it("replaces a field a console session changed while the form is open, keeps the other edit, and saves on the new version", async () => {
  const { sent, broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  await chooseFromMenu("About a", "Edit");
  await settle();
  type(inputLabelled("Summary"), "Mine");
  type(document.querySelector("textarea")!, "My content");
  broadcast(savedEvent(entryOf("a", { content: "Theirs", version: 2 })));
  expect(document.querySelector("textarea")!.value).toBe("Theirs");
  expect(inputLabelled("Summary").value).toBe("Mine");
  await click(button("Save"));
  expect(sent()).toEqual([
    { type: "save_memory", console: "c-1", memory: "a", summary: "Mine", content: "Theirs", local: false, version: 2 },
  ]);
});

it("keeps an edit when the entry's version moves over the value the form read", async () => {
  const { broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  await chooseFromMenu("About a", "Edit");
  await settle();
  type(document.querySelector("textarea")!, "Mine");
  broadcast(savedEvent(entryOf("a", { version: 2 })));
  expect(document.querySelector("textarea")!.value).toBe("Mine");
});

it("raises an alert when the daemon refuses the edit as changed, keeping the form open on the entry as it now stands", async () => {
  const alerts = errorAlerts.getSnapshot().length;
  const { broadcast } = renderMemory({
    entries: [entryOf("a")],
    respond: (body) => (body.type === "save_memory" ? Promise.reject(daemonError("memory_changed", { memory: "a", summary: "About a" })) : undefined),
  });
  await settle();
  await chooseFromMenu("About a", "Edit");
  await settle();
  type(inputLabelled("Summary"), "Mine");
  await click(button("Save"));
  expect(errorAlerts.getSnapshot().length).toBe(alerts + 1);
  expect(document.querySelector("textarea")).not.toBeNull();
  broadcast(savedEvent(entryOf("a", { content: "Theirs", version: 2 })));
  expect(document.querySelector("textarea")!.value).toBe("Theirs");
  expect(inputLabelled("Summary").value).toBe("Mine");
});

it("says an entry was deleted under the form, and cannot save it", async () => {
  const { sent, broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  await chooseFromMenu("About a", "Edit");
  await settle();
  broadcast(deletedEvent(["a"]));
  expect(text()).toContain("This entry was deleted while it was open.");
  await click(button("Save"));
  expect(sent()).toEqual([]);
});

it("deletes one entry after a confirmation", async () => {
  const { sent } = renderMemory({ entries: [entryOf("a"), entryOf("b")] });
  await settle();
  await chooseFromMenu("About a", "Delete");
  await settle();
  expect(text()).toContain("About a");
  expect(text()).toContain("will be deleted from the memory.");
  expect(sent()).toEqual([]);
  await click(buttons().filter((candidate) => candidate.textContent === "Delete").at(-1));
  expect(sent()).toEqual([{ type: "delete_memory", console: "c-1", memory: "a" }]);
});

it("deletes every entry only once the word is typed", async () => {
  const { sent } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  await click(button("Delete all"));
  await settle();
  const confirm = buttons().filter((candidate) => candidate.textContent === "Delete all").at(-1)!;
  await click(confirm);
  expect(sent()).toEqual([]);
  type(document.querySelector<HTMLInputElement>('input[type="text"], input:not([type])')!, "delete");
  await click(buttons().filter((candidate) => candidate.textContent === "Delete all").at(-1));
  expect(sent()).toEqual([{ type: "delete_memories", console: "c-1" }]);
});

it("raises an alert when the daemon refuses a deletion because the entry is gone", async () => {
  const alerts = errorAlerts.getSnapshot().length;
  renderMemory({
    entries: [entryOf("a")],
    respond: (body) => (body.type === "delete_memory" ? Promise.reject(daemonError("unknown_memory", { memory: "a" })) : undefined),
  });
  await settle();
  await chooseFromMenu("About a", "Delete");
  await settle();
  await click(buttons().filter((candidate) => candidate.textContent === "Delete").at(-1));
  expect(errorAlerts.getSnapshot().length).toBe(alerts + 1);
});

/** A listing the test answers when it likes, with `lines` as the index. */
function heldListing(lines: ReturnType<typeof entryOf>[]) {
  let release!: () => void;
  const respond = (body: { type: string; console?: string }) =>
    body.type === "list_memories"
      ? new Promise<never>((resolve) => {
        release = () => resolve({ type: "memory_list", id: null, scope: { kind: "console", console_id: "c-1" }, memories: lines.map(indexLine) } as never);
      })
      : undefined;
  return { respond, release: () => release() };
}

it("shows that the memory is loading, with nothing to add yet, until the first listing arrives", async () => {
  const listing = heldListing([entryOf("a")]);
  renderMemory({ respond: listing.respond });
  await settle();
  expect(text()).toContain("Loading the memory");
  expect(text()).not.toContain("No memory yet");
  const add = button("Add entry")!;
  expect(add.disabled || add.getAttribute("aria-disabled") === "true").toBe(true);
  listing.release();
  await settle();
  expect(text()).not.toContain("Loading the memory");
  expect(text()).toContain("About a");
});

it("keeps a change a console session broadcast while the listing was out, though the listing lacks it", async () => {
  const listing = heldListing([entryOf("a")]);
  const { broadcast } = renderMemory({ respond: listing.respond });
  await settle();
  broadcast(savedEvent(entryOf("b", { summary: "Saved meanwhile", updated_at: 9_000 })));
  listing.release();
  await settle();
  expect(text()).toContain("About a");
  expect(text()).toContain("Saved meanwhile");
});

it("leaves Edit unavailable on an entry that was deleted while it is open", async () => {
  const { broadcast } = renderMemory({ entries: [entryOf("a")] });
  await settle();
  await click(rowPress("About a"));
  await settle();
  // A button with a reason for being unavailable carries the reason in its text.
  const edit = () => buttons().find((candidate) => candidate.textContent?.endsWith("Edit"))!;
  expect(edit().getAttribute("aria-disabled")).not.toBe("true");
  broadcast(deletedEvent(["a"]));
  expect(edit().getAttribute("aria-disabled")).toBe("true");
  expect(edit().textContent).toContain("This entry was deleted while it was open.");
  await click(edit());
  expect(document.querySelector("textarea")).toBeNull();
});

it("still edits an entry in a memory that holds 200", async () => {
  const { sent } = renderMemory({ entries: Array.from({ length: 200 }, (_, index) => entryOf(`e${index}`)) });
  await settle();
  await chooseFromMenu("About e0", "Edit");
  await settle();
  type(inputLabelled("Summary"), "Changed");
  await click(button("Save"));
  expect(sent()).toEqual([{ type: "save_memory", console: "c-1", memory: "e0", summary: "Changed", content: "Content of e0", local: false, version: 1 }]);
});

it("says an entry updated within the last minute was updated just now, by whom", async () => {
  renderMemory({
    entries: [
      entryOf("a", { updated_at: Date.now() }),
      entryOf("b", { updated_at: Date.now(), updated_by: { kind: "agent" } }),
      entryOf("c", { updated_at: Date.now(), updated_by: { kind: "agent", agent: other.id } }),
    ],
  });
  await settle();
  expect(row("About a").textContent).toContain("Updated just now by you");
  expect(row("About b").textContent).toContain("Updated just now by an agent");
  expect(row("About c").textContent).toContain(`Updated just now by ${nameOf(other.id)}`);
});

it("reads the entry again when the daemon refuses the edit as changed, so the form shows the entry as it now stands", async () => {
  let saved = false;
  const { request } = renderMemory({
    entries: [entryOf("a")],
    respond: (body) => {
      if (body.type === "save_memory") {
        saved = true;
        return Promise.reject(daemonError("memory_changed", { memory: "a", summary: "About a" }));
      }
      // A session saved the entry, and the window has not heard of it: only a read finds it.
      if (body.type === "read_memory" && saved) return Promise.resolve({ type: "memory_content", id: null, entry: entryOf("a", { content: "Theirs", version: 2 }) });
      return undefined;
    },
  });
  await settle();
  await chooseFromMenu("About a", "Edit");
  await settle();
  type(inputLabelled("Summary"), "Mine");
  await click(button("Save"));
  await settle();
  expect(request.mock.calls.filter((call) => call[0].type === "read_memory")).toHaveLength(2);
  expect(document.querySelector("textarea")!.value).toBe("Theirs");
  expect(inputLabelled("Summary").value).toBe("Mine");
});

it("says a memory filled while the form was open in an alert at the top of the form, not under a field", async () => {
  const { broadcast, sent } = renderMemory({ entries: Array.from({ length: 199 }, (_, index) => entryOf(`e${index}`)) });
  await settle();
  await click(button("Add entry"));
  await settle();
  broadcast(savedEvent(entryOf("e199")));
  type(inputLabelled("Summary"), "About z");
  type(document.querySelector("textarea")!, "Some content");
  await click(button("Add"));
  const form = Array.from(document.querySelectorAll('[role="dialog"]')).at(-1)!;
  const alert = Array.from(form.querySelectorAll("*")).find((element) => element.childElementCount === 0 && element.textContent?.includes("already holds 200 entries"))!;
  expect(form.textContent).toContain("The memory already holds 200 entries. Delete one to add another.");
  // The alert comes before the first field.
  expect(alert.compareDocumentPosition(inputLabelled("Summary")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(inputLabelled("Summary").getAttribute("aria-invalid")).not.toBe("true");
  expect(sent()).toEqual([]);
});
