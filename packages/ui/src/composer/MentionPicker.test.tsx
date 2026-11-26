// @vitest-environment jsdom
import { act } from "react";
import { flushSync } from "react-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { consoleOf, projectOf, sessionOf, snapshotState } from "../fixtures/builders";
import type { BrowseEntry, ClientRequestBody, Event, Session } from "../protocol";
import type { Daemon } from "../store";
import { fakeTimers } from "../test/clock";
import { daemonError } from "../test/daemonError";
import { MAC_HOST } from "../test/hosts";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { composerDraft } from "./composerDrafts";
import { sessionComposerEnabled } from "./composerSetting";
import { SessionComposer } from "./SessionComposer";
import { MENTION_DEBOUNCE_MS } from "./useMentionPaths";
import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS } from "../components/announce/StatusAnnouncer";

const entry = (name: string, kind: "file" | "directory"): BrowseEntry => ({ name, kind, size: kind === "file" ? 1 : null, version: null, target: null } as BrowseEntry);

const ROOT: BrowseEntry[] = [entry("README.md", "file"), entry("src", "directory")];
const SRC: BrowseEntry[] = [entry("app.ts", "file"), entry("apple.ts", "file"), entry("zoo.ts", "file")];

const me = sessionOf("me", "c1", "p1", "Me", "idle", { started_at: 50 });
const others = [
  sessionOf("far", "c2", "p9", "Far away", "idle", { started_at: 90 }),
  sessionOf("near", "c1", "p1", "Near work", "working", { started_at: 20 }),
  sessionOf("console", "c1", undefined, "Console helper", "idle", { started_at: 30 }),
  sessionOf("gone", "c1", "p1", "Gone", "archived", { started_at: 99 }),
];

function setUp({ session = me, failSearch = false, holdSearch = false, holdSend = false }: { session?: Session; failSearch?: boolean; holdSearch?: boolean; holdSend?: boolean } = {}) {
  const sent: ClientRequestBody[] = [];
  const daemon: Daemon = testDaemon({
    state: snapshotState({
      consoles: [consoleOf("c1", "Main"), consoleOf("c2", "Second")],
      projects: [projectOf("p1", "c1", "app"), projectOf("p9", "c2", "other")],
      sessions: [me, ...others, ...(session === me ? [] : [session])],
      messageQueues: {},
    }),
    request: async (body): Promise<Event> => {
      sent.push(body);
      if (body.type === "list_project_dir") {
        return { type: "project_dir", id: null, project: body.project, worktree: null, path: body.path, root_id: "r", entries: body.path === "" ? ROOT : body.path === "src" ? SRC : body.path === "notes" ? [entry("to do.md", "file")] : [], complete: true };
      }
      if (body.type === "search_project_files") {
        if (failSearch) throw daemonError("unknown_project", { project: "p1" });
        if (holdSearch) return new Promise<Event>(() => {});
        return { type: "project_file_matches", id: null, project: body.project, query: body.query, paths: body.query === "app" ? ["src/app.ts", "src/apple.ts"] : [], complete: true };
      }
      if (holdSend && body.type === "create_queued_message") return new Promise<Event>(() => {});
      return { type: "ack", id: null };
    },
  });
  const container = mountHost(<SessionComposer session={session} />, { daemon, platform: { kind: "browser", host: MAC_HOST } });
  const field = container.querySelector<HTMLTextAreaElement>("textarea")!;
  act(() => field.focus());
  return { daemon, sent, container, field };
}

/** Types `text` as the whole of the field, the caret at its end, and lets the picker's wait pass and its answer in. */
async function typeText(field: HTMLTextAreaElement, text: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(field, text);
    field.setSelectionRange(text.length, text.length);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => {
    vi.advanceTimersByTime(MENTION_DEBOUNCE_MS);
  });
  await act(async () => {});
}

function press(field: HTMLTextAreaElement, init: KeyboardEventInit) {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  act(() => {
    field.dispatchEvent(event);
  });
  return event;
}

const listbox = () => document.querySelector<HTMLElement>("[role=listbox]");
const options = () => [...document.querySelectorAll<HTMLElement>("[role=option]")];
// The location beside a session is isolated for its reading direction; the marks are left out to compare.
const optionTexts = () => options().map((option) => option.textContent?.replace(/[\u2068\u2069]/g, ""));
const groups = () => [...document.querySelectorAll<HTMLElement>("[role=listbox] [role=group]")].map((group) => document.getElementById(group.getAttribute("aria-labelledby")!)!.textContent);

beforeEach(() => {
  fakeTimers();
  sessionComposerEnabled.set(true);
});
afterEach(() => sessionComposerEnabled.set(false));

describe("the composer's @ picker", () => {
  it("opens on @ with the project's top entries, directories first, then the other unarchived sessions in order", async () => {
    const { field, sent } = setUp();
    await typeText(field, "look at @");
    expect(listbox()).not.toBeNull();
    expect(groups()).toEqual(["Files", "Sessions"]);
    expect(optionTexts()).toEqual(["src", "README.md", "Near workapp", "Console helperMain's console session", "Far awayother"]);
    expect(sent).toContainEqual({ type: "list_project_dir", project: "p1", path: "", slot: "mention:p1" });
    expect(field.hasAttribute("role")).toBe(false);
    expect(field.hasAttribute("aria-expanded")).toBe(false);
    expect(field.getAttribute("aria-controls")).toBe(listbox()!.id);
    expect(field.getAttribute("aria-autocomplete")).toBe("list");
    expect(field.getAttribute("aria-activedescendant")).toBe(options()[0].id);
    expect(options()[0].getAttribute("aria-selected")).toBe("true");
  });

  it("narrows the files by a project-wide name search and the sessions by title", async () => {
    const { field, sent } = setUp();
    await typeText(field, "@app");
    expect(sent).toContainEqual({ type: "search_project_files", project: "p1", query: "app", slot: "mention:p1" });
    expect(optionTexts()).toEqual(["src/app.ts", "src/apple.ts"]);
  });

  it("lists a typed folder's entries for a query with a slash", async () => {
    const { field, sent } = setUp();
    await typeText(field, "@src/ap");
    expect(sent).toContainEqual({ type: "list_project_dir", project: "p1", path: "src", slot: "mention:p1" });
    expect(sent.some((body) => body.type === "search_project_files")).toBe(false);
    expect(optionTexts()).toEqual(["src/app.ts", "src/apple.ts"]);
  });

  it("moves through the options with the arrow keys and inserts the picked file's path relative to the project root", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "see @src/");
    expect(press(field, { key: "ArrowDown" }).defaultPrevented).toBe(true);
    expect(field.getAttribute("aria-activedescendant")).toBe(options()[1].id);
    press(field, { key: "ArrowUp" });
    press(field, { key: "ArrowUp" });
    expect(field.getAttribute("aria-activedescendant")).toBe(options().at(-1)!.id);
    press(field, { key: "ArrowDown" });
    press(field, { key: "ArrowDown" });
    expect(press(field, { key: "Enter" }).defaultPrevented).toBe(true);
    expect(composerDraft(daemon.store, "me").text).toBe("see src/apple.ts ");
    expect(listbox()).toBeNull();
    expect(field.selectionStart).toBe("see src/apple.ts ".length);
  });

  it("inserts a picked session as its title and full id, with Tab as with Enter", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "ask @near");
    expect(optionTexts()).toEqual(["Near workapp"]);
    press(field, { key: "Tab" });
    expect(composerDraft(daemon.store, "me").text).toBe("ask @「Near work」(session near) ");
  });

  it("picks an option pressed with the pointer, keeping focus in the field", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "@");
    const target = options().find((option) => option.textContent?.startsWith("Far away"))!;
    const down = new MouseEvent("mousedown", { bubbles: true, cancelable: true });
    act(() => {
      target.dispatchEvent(down);
    });
    expect(down.defaultPrevented).toBe(true);
    act(() => target.click());
    expect(composerDraft(daemon.store, "me").text).toBe("@「Far away」(session far) ");
    expect(document.activeElement).toBe(field);
  });

  it("leaves Enter and the arrows to an input method composing, and Command-Enter to sending", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "@");
    expect(press(field, { key: "Enter", isComposing: true }).defaultPrevented).toBe(false);
    expect(press(field, { key: "ArrowDown", keyCode: 229 } as KeyboardEventInit).defaultPrevented).toBe(false);
    expect(field.getAttribute("aria-activedescendant")).toBe(options()[0].id);
    expect(composerDraft(daemon.store, "me").text).toBe("@");
  });

  it("follows text an input method commits into the query", async () => {
    const { field } = setUp();
    await typeText(field, "@");
    await typeText(field, "@近");
    expect(optionTexts()).toEqual([]);
    await typeText(field, "@Near");
    expect(optionTexts()).toEqual(["Near workapp"]);
  });

  it("closes with Escape until another mention is begun, leaving the text alone", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "@sr");
    const escape = press(field, { key: "Escape" });
    expect(escape.defaultPrevented).toBe(true);
    expect(listbox()).toBeNull();
    expect(composerDraft(daemon.store, "me").text).toBe("@sr");
    await typeText(field, "@src");
    expect(listbox()).toBeNull();
    await typeText(field, "@src @");
    expect(listbox()).not.toBeNull();
  });

  it("does not open for an @ inside a word, and closes once the mention is ended by a space", async () => {
    const { field } = setUp();
    await typeText(field, "me@example");
    expect(listbox()).toBeNull();
    await typeText(field, "@src");
    expect(listbox()).not.toBeNull();
    await typeText(field, "@src ");
    expect(listbox()).toBeNull();
  });

  it("closes when the field loses focus", async () => {
    const { field } = setUp();
    await typeText(field, "@");
    act(() => field.blur());
    expect(listbox()).toBeNull();
  });

  it("lists no files in a console session's composer, and says so when no session matches", async () => {
    const consoleSession = sessionOf("cs", "c1", undefined, "Console", "idle");
    const { field, sent } = setUp({ session: consoleSession });
    await typeText(field, "@");
    expect(groups()).toEqual(["Sessions"]);
    expect(sent.some((body) => body.type === "list_project_dir")).toBe(false);
    await typeText(field, "@nothing-like-it");
    expect(options()).toEqual([]);
    expect(document.body.textContent).toContain("No matching sessions");
  });

  it("says no files match while sessions still do, in the files' place above the sessions", async () => {
    const { field } = setUp();
    await typeText(field, "@near");
    expect(groups()).toEqual(["Sessions"]);
    const note = [...document.querySelectorAll<HTMLElement>("div")].find((element) => element.textContent === "No matching files");
    expect(note).toBeDefined();
    expect(note!.compareDocumentPosition(listbox()!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("does not open while a message is being sent", async () => {
    const { field } = setUp({ holdSend: true });
    await typeText(field, "hello");
    press(field, { key: "Enter", metaKey: true });
    await typeText(field, "hello @");
    expect(listbox()).toBeNull();
  });

  it("names no list or option while the picker is closed, staying a plain text box", async () => {
    const { field } = setUp();
    await typeText(field, "plain text");
    expect(field.getAttribute("aria-autocomplete")).toBe("list");
    expect(field.hasAttribute("role")).toBe(false);
    expect(field.hasAttribute("aria-controls")).toBe(false);
    expect(field.hasAttribute("aria-activedescendant")).toBe(false);
  });

  it("says the project's files are being searched while the search is out", async () => {
    const { field } = setUp({ holdSearch: true });
    await typeText(field, "@zz");
    expect(document.body.textContent).toContain("Searching project files");
  });

  describe("the note, spoken from a status since focus stays in the text area", () => {
    // The picker's own status, beside its list (the composer has one of its own).
    const status = () => listbox()!.parentElement!.querySelector<HTMLElement>(":scope > [role=status]")!;
    const afterFirstWrite = () => act(async () => {
      vi.advanceTimersByTime(FIRST_WRITE_DELAY_MS);
    });

    it("says the files are being searched, then that none match, the visible note hidden from it", async () => {
      const { field } = setUp({ holdSearch: true });
      await typeText(field, "@zz");
      await afterFirstWrite();
      expect(status().textContent).toBe("Searching project files");
      const visible = [...document.querySelectorAll<HTMLElement>("[aria-hidden]")].find((element) => element.textContent === "Searching project files");
      expect(visible).toBeDefined();
    });

    it("says no files match, in place, as the query changes while the picker shows", async () => {
      const { field } = setUp();
      await typeText(field, "@");
      await afterFirstWrite();
      expect(status().textContent).toBe("");
      await typeText(field, "@near");
      expect(status().textContent).toBe("No matching files");
    });

    it("says the files could not be searched as an alert, not from the status", async () => {
      const { field } = setUp({ failSearch: true });
      await typeText(field, "@near");
      await afterFirstWrite();
      expect(status().textContent).toBe("");
      const alert = () => listbox()!.parentElement!.querySelector<HTMLElement>(":scope > [role=alert]");
      expect(alert()).toBeNull();
      await act(async () => {
        vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS);
      });
      expect(alert()!.textContent).toBe("Could not search project files");
    });

    it("says no session matches in a console session's composer", async () => {
      const { field } = setUp({ session: sessionOf("cs", "c1", undefined, "Console", "idle") });
      await typeText(field, "@nothing-like-it");
      await afterFirstWrite();
      expect(status().textContent).toBe("No matching sessions");
    });
  });

  it("inserts a picked path with whitespace quoted, as a file dropped on the terminal is", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "read @notes/");
    expect(optionTexts()).toEqual(["notes/to do.md"]);
    press(field, { key: "Enter" });
    expect(composerDraft(daemon.store, "me").text).toBe("read 'notes/to do.md' ");
  });
});

describe("the composer's @ picker, further", () => {
  it("inserts a picked directory as its path with no trailing slash, then a space", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "open @");
    expect(optionTexts()[0]).toBe("src");
    press(field, { key: "Enter" });
    expect(composerDraft(daemon.store, "me").text).toBe("open src ");
  });

  it("asks the daemon once for what was typed in a quick run, a short wait after the last key", async () => {
    const { field, sent } = setUp();
    await act(async () => {
      for (const text of ["@a", "@ap", "@app"]) {
        Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(field, text);
        field.setSelectionRange(text.length, text.length);
        field.dispatchEvent(new Event("input", { bubbles: true }));
      }
    });
    expect(sent.filter((body) => body.type === "search_project_files")).toEqual([]);
    await act(async () => {
      vi.advanceTimersByTime(MENTION_DEBOUNCE_MS);
    });
    await act(async () => {});
    expect(sent.filter((body) => body.type === "search_project_files")).toEqual([{ type: "search_project_files", project: "p1", query: "app", slot: "mention:p1" }]);
    expect(optionTexts()).toEqual(["src/app.ts", "src/apple.ts"]);
  });

  it("says the files could not be searched while the sessions still list", async () => {
    const { field } = setUp({ failSearch: true });
    await typeText(field, "@near");
    expect(document.body.textContent).toContain("Could not search project files");
    expect(optionTexts()).toEqual(["Near workapp"]);
  });

  it("leaves Tab and Escape to an input method composing, the list staying open", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "@");
    expect(press(field, { key: "Tab", isComposing: true }).defaultPrevented).toBe(false);
    expect(press(field, { key: "Escape", keyCode: 229 } as KeyboardEventInit).defaultPrevented).toBe(false);
    expect(listbox()).not.toBeNull();
    expect(composerDraft(daemon.store, "me").text).toBe("@");
  });

  it("leaves Enter alone, as a new line, where nothing matches", async () => {
    const { field } = setUp();
    await typeText(field, "@zzz-nothing");
    expect(options()).toEqual([]);
    expect(press(field, { key: "Enter" }).defaultPrevented).toBe(false);
  });

  it("does not take Shift-Tab or a modified Enter as a pick", async () => {
    const { field, daemon } = setUp();
    await typeText(field, "@");
    expect(press(field, { key: "Tab", shiftKey: true }).defaultPrevented).toBe(false);
    expect(press(field, { key: "Enter", ctrlKey: true }).defaultPrevented).toBe(false);
    expect(composerDraft(daemon.store, "me").text).toBe("@");
  });

  it("opens again for a mention begun on a new line", async () => {
    const { field } = setUp();
    await typeText(field, "first line\n@near");
    expect(optionTexts()).toEqual(["Near workapp"]);
  });
});

describe("the composer's text around an @, as a keyboard edits it", () => {
  // A browser runs the microtasks after each listener of an event the user caused, React's flush of state set in one
  // among them; jsdom, dispatching from a script, runs none until the dispatch returns. Each listener a text area takes
  // is followed by that flush here, so a state update made in one renders before React's own listener reads the edit,
  // as it does in a browser.
  beforeEach(() => {
    const add = HTMLTextAreaElement.prototype.addEventListener;
    const spy = vi.spyOn(HTMLTextAreaElement.prototype, "addEventListener").mockImplementation(function (this: HTMLTextAreaElement, type, listener, options) {
      if (typeof listener !== "function") return add.call(this, type, listener, options);
      const wrapped = (event: globalThis.Event) => {
        listener.call(this, event);
        flushSync(() => {});
      };
      wrappedListeners.set(listener, wrapped);
      return add.call(this, type, wrapped, options);
    });
    const remove = HTMLTextAreaElement.prototype.removeEventListener;
    const removeSpy = vi.spyOn(HTMLTextAreaElement.prototype, "removeEventListener").mockImplementation(function (this: HTMLTextAreaElement, type, listener, options) {
      const wrapped = typeof listener === "function" ? wrappedListeners.get(listener) : undefined;
      return remove.call(this, type, wrapped ?? listener, options);
    });
    return () => {
      spy.mockRestore();
      removeSpy.mockRestore();
    };
  });
  const wrappedListeners = new WeakMap<object, (event: globalThis.Event) => void>();

  /** One edit as a browser makes it: the value changes and `input` fires, its listeners each run with their own
   * microtasks after them (outside `act`, which would batch what a real event does not), then React's work settles. */
  async function edit(field: HTMLTextAreaElement, text: string) {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(field, text);
    field.setSelectionRange(text.length, text.length);
    field.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: text.length > 0 ? "insertText" : "deleteContentBackward" }));
    await act(async () => {
      vi.advanceTimersByTime(MENTION_DEBOUNCE_MS);
    });
  }

  it("keeps a typed @, opens the picker, and lets Backspace take it away again", async () => {
    const { field, daemon } = setUp();
    await edit(field, "hi ");
    await edit(field, "hi @");
    expect(field.value).toBe("hi @");
    expect(composerDraft(daemon.store, "me").text).toBe("hi @");
    expect(listbox()).not.toBeNull();
    await edit(field, "hi @@");
    expect(composerDraft(daemon.store, "me").text).toBe("hi @@");
    await edit(field, "hi @");
    await edit(field, "hi ");
    expect(field.value).toBe("hi ");
    expect(composerDraft(daemon.store, "me").text).toBe("hi ");
    expect(listbox()).toBeNull();
  });

  it("keeps a filter typed after the @", async () => {
    const { field, daemon } = setUp();
    await edit(field, "@");
    await edit(field, "@s");
    await edit(field, "@sr");
    expect(field.value).toBe("@sr");
    expect(composerDraft(daemon.store, "me").text).toBe("@sr");
  });
});
