// @vitest-environment jsdom
import { act, useEffect, useImperativeHandle } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { setGitModeWorktree } from "../browser/browserState";
import { DaemonRequestError } from "../daemon-client";
import { presence, stubListMotion } from "../components/listMotionTestSupport";
import { isolate } from "../i18n/isolate";
import { PanelFrameProvider, type PanelFrame } from "../components/PanelHeader";
import { PlatformProvider } from "../platform/react";
import type { Event, Project, RequestBody, ShellTerminal } from "../protocol";
import { applyDaemonEvent, createStateStore, DaemonProvider, seedShellTerminal, type Daemon } from "../store";
import { ShellTerminalPanel } from "./ShellTerminalPanel";
import { startShellTerminal, takeShellTerminalFocus } from "./shellTerminals";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const focused = vi.hoisted(() => [] as string[]);
const mountedViews = vi.hoisted(() => [] as string[]);

// The view stands in for the terminal itself, which needs a real browser: it says which terminal
// it is and whether its tab is the one shown, and records the focus it is given and each mount, where
// the real view attaches to its terminal.
vi.mock("./ShellTerminalView", () => ({
  ShellTerminalView: ({ terminal, shown, ref }: { terminal: ShellTerminal; shown: boolean; ref?: React.Ref<{ focus: () => void }> }) => {
    useEffect(() => void mountedViews.push(terminal.id), []);
    useImperativeHandle(ref, () => ({ focus: () => focused.push(terminal.id) }), [terminal.id]);
    return <div id={`shell-terminal-${terminal.id}`} tabIndex={-1} data-view={terminal.id} data-shown={String(shown)} />;
  },
}));

const project = { id: "p", name: "Website" } as Project;
const shell = (id: string, created: number, extra: Partial<ShellTerminal> = {}): ShellTerminal => ({
  id, project_id: "p", worktree: null, cwd: "/p", title: `shell ${id}`, renamed: false, state: "running", busy: false, created_at: created, ...extra,
});

let cleanup: (() => void) | undefined;
afterEach(() => {
  cleanup?.();
  cleanup = undefined;
  focused.length = 0;
  mountedViews.length = 0;
  localStorage.clear();
  setGitModeWorktree("p", undefined);
});

// `eventFirst`: the broadcast of the new record arrives before the reply that names it; otherwise
// it arrives after, as `lateEvent` releases it. The broadcast carries a newer title than the reply
// does, as one that came after a rename would.
function mount(terminals: ShellTerminal[], { holdCreate, eventFirst = true, frame }: { holdCreate?: Promise<void>; eventFirst?: boolean; frame?: PanelFrame } = {}) {
  const store = createStateStore({ shellTerminals: new Map(terminals.map((terminal) => [terminal.id, terminal])) });
  const broadcast = () => applyDaemonEvent(store, { type: "shell_terminal_upserted", shell_terminal: shell("new", 99, { title: "newer" }) });
  const request = vi.fn(async (body: RequestBody): Promise<Event> => {
    if (body.type === "create_shell_terminal") {
      await holdCreate;
      if (eventFirst) broadcast();
      return { type: "shell_terminal_created", id: null, shell_terminal: shell("new", 99) };
    }
    return { type: "ack", id: null } as Event;
  });
  const daemon: Daemon = {
    store,
    request,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <PlatformProvider value={{ kind: "browser" }}>
        <DaemonProvider value={daemon}>
          <PanelFrameProvider value={frame}>
            <ShellTerminalPanel project={project} switching={false} />
          </PanelFrameProvider>
        </DaemonProvider>
      </PlatformProvider>,
    ),
  );
  cleanup = () => {
    act(() => root.unmount());
    container.remove();
  };
  const tabs = () => Array.from(container.querySelectorAll<HTMLElement>("[role=tab]"));
  return {
    container,
    daemon,
    request,
    store,
    tabs,
    lateEvent: broadcast,
    shown: () => container.querySelector("[data-shown=true]")?.getAttribute("data-view"),
  };
}

it("keeps the New terminal button and its focus when another client adds or removes the last terminal", () => {
  const panel = mount([]);
  const button = panel.container.querySelector<HTMLButtonElement>('button[aria-label="New terminal"]')!;
  act(() => button.focus());
  for (const terminals of [[shell("remote", 1)], []]) {
    act(() => panel.store.setState({ shellTerminals: new Map(terminals.map((terminal) => [terminal.id, terminal])) }));
    expect(panel.container.querySelector('button[aria-label="New terminal"]')).toBe(button);
    expect(document.activeElement).toBe(button);
    expect(button.hasAttribute("data-region-landing")).toBe(terminals.length === 0);
  }
});

it("lists the project's terminals oldest first, shows the newest, and names the box of terminals as that tab's panel", () => {
  const panel = mount([shell("b", 2), shell("a", 1, { state: "exited" }), { ...shell("x", 3), project_id: "other" }]);
  expect(panel.tabs().map((tab) => tab.textContent)).toEqual([`${isolate("shell a")} (exited)`, "shell b"]);
  expect(panel.container.querySelectorAll("[data-view]")).toHaveLength(2);
  expect(panel.shown()).toBe("b");
  const tabpanel = panel.container.querySelector("[role=tabpanel]")!;
  const selected = panel.tabs().find((tab) => tab.getAttribute("aria-selected") === "true")!;
  expect(tabpanel.getAttribute("aria-labelledby")).toBe(selected.id);
  expect(tabpanel.id).toBe(selected.getAttribute("aria-controls"));
});

it("mounts one view per terminal, so each terminal has one emulator and one connection", () => {
  mount([shell("a", 1), shell("b", 2)]);
  expect([...mountedViews].sort()).toEqual(["a", "b"]);
});

it("starts a terminal in the worktree the Git mode shows, and shows it once the daemon has it", async () => {
  const panel = mount([shell("a", 1)]);
  setGitModeWorktree("p", "w-side");
  await act(async () => panel.container.querySelector<HTMLButtonElement>("button[aria-label='New terminal']")!.click());
  expect(panel.request).toHaveBeenCalledWith({ type: "create_shell_terminal", project: "p", worktree: "w-side" });
  expect(panel.shown()).toBe("new");
});

it("closes an idle tab at once and asks before closing one that is running something", async () => {
  const panel = mount([shell("a", 1), shell("b", 2, { busy: true })]);
  const [idle, busy] = panel.tabs();
  await act(async () => idle.querySelector<HTMLElement>("span[aria-hidden=true]")!.click());
  expect(panel.request).toHaveBeenCalledWith({ type: "close_shell_terminal", shell_terminal: "a" });
  panel.request.mockClear();
  await act(async () => busy.querySelector<HTMLElement>("span[aria-hidden=true]")!.click());
  expect(panel.request).not.toHaveBeenCalled();
  expect(document.querySelector("[role=alertdialog], [role=dialog]")).not.toBeNull();
});

it("offers to start a terminal while the project has none", () => {
  const panel = mount([]);
  expect(panel.container.textContent).toContain("No terminals");
  expect(panel.container.querySelector("[role=tabpanel]")).toBeNull();
});

it("keeps a closed tab shown until the daemon has ended it, then shows the tab after it", async () => {
  const panel = mount([shell("a", 1), shell("b", 2), shell("c", 3)]);
  await act(async () => panel.tabs()[1].click());
  expect(panel.shown()).toBe("b");
  await act(async () => panel.tabs()[1].querySelector<HTMLElement>("span[aria-hidden=true]")!.click());
  expect(panel.shown()).toBe("b");
  act(() => panel.store.setState((state) => {
    const shellTerminals = new Map(state.shellTerminals);
    shellTerminals.delete("b");
    return { shellTerminals };
  }));
  expect(panel.shown()).toBe("c");
});

it("shows a start in flight instead of the empty state, which would offer a second one", async () => {
  let release!: () => void;
  const panel = mount([], { holdCreate: new Promise<void>((resolve) => (release = resolve)) });
  let started!: Promise<string | undefined>;
  act(() => {
    started = startShellTerminal(panel.daemon, "p", { ifNone: true });
  });
  expect(panel.container.textContent).toContain("Starting terminal");
  expect(panel.container.textContent).not.toContain("No terminals");
  await act(async () => {
    release();
    await started;
  });
  expect(panel.container.textContent).not.toContain("Starting terminal");
  expect(panel.shown()).toBe("new");
});

it.each([true, false])("ends the pending start only with the new record held, with the broadcast first: %s", async (eventFirst) => {
  const panel = mount([], { eventFirst });
  await act(async () => {
    await startShellTerminal(panel.daemon, "p", { ifNone: true });
  });
  // Whichever came first, the panel never shows the empty state between the reply and the record.
  expect(panel.container.textContent).not.toContain("No terminals");
  expect(panel.shown()).toBe("new");
  act(() => panel.lateEvent());
  expect(panel.tabs().map((tab) => tab.textContent)).toEqual(["newer"]);
  expect(panel.shown()).toBe("new");
});

it.each([true, false])("focuses the terminal a start from the panel made with the broadcast first: %s", async (eventFirst) => {
  const panel = mount([shell("a", 1)], { eventFirst });
  await act(async () => panel.container.querySelector<HTMLButtonElement>("button[aria-label='New terminal']")!.click());
  act(() => panel.lateEvent());
  await vi.waitFor(() => expect(focused).toEqual(["new"]));
});

it("takes a focus request once its terminal is shown, so none is left to act on a later choice of that tab", async () => {
  const panel = mount([shell("a", 1)]);
  await act(async () => {
    await startShellTerminal(panel.daemon, "p", { focus: true });
  });
  await vi.waitFor(() => expect(focused).toEqual(["new"]));
  expect(takeShellTerminalFocus("new")).toBe(false);
});

it("starts a terminal in the place given, and focuses it once shown when asked from outside the panel", async () => {
  const panel = mount([shell("a", 1)]);
  await act(async () => {
    await startShellTerminal(panel.daemon, "p", { place: { worktree: "w", path: "src/ui" }, focus: true });
  });
  expect(panel.request).toHaveBeenCalledWith({ type: "create_shell_terminal", project: "p", worktree: "w", path: "src/ui" });
  expect(panel.shown()).toBe("new");
  await vi.waitFor(() => expect(focused).toEqual(["new"]));
});

it("starts a terminal in the project's own directory for an empty place, not in the Git mode's worktree", async () => {
  const panel = mount([shell("a", 1)]);
  await act(async () => {
    await startShellTerminal(panel.daemon, "p", { place: {} });
  });
  expect(panel.request).toHaveBeenCalledWith({ type: "create_shell_terminal", project: "p" });
});

it("words a refused start in a folder as that folder being gone, and a refused start elsewhere by its own code", async () => {
  const toastError = vi.fn();
  const gone = new DaemonRequestError("not a folder", "path_not_a_directory", { path: "/w/src" });
  const daemon = { request: vi.fn(async () => { throw gone; }), store: createStateStore(), toastError } as unknown as Pick<Daemon, "request" | "store" | "toastError">;
  await startShellTerminal(daemon, "p", { place: { path: "src" } });
  await startShellTerminal(daemon, "p", { place: {} });
  expect(toastError.mock.calls.map((call) => call[3])).toEqual(["hostPathGone", undefined]);
});

it("starts no second terminal from the header while the first, with none yet, is being started", async () => {
  let release!: () => void;
  const panel = mount([], { holdCreate: new Promise<void>((resolve) => (release = resolve)) });
  const create = () => panel.request.mock.calls.filter(([body]) => body.type === "create_shell_terminal");
  act(() => panel.container.querySelector<HTMLButtonElement>("button[aria-label='New terminal']")!.click());
  expect(panel.container.textContent).toContain("Starting terminal");
  await act(async () => panel.container.querySelector<HTMLButtonElement>("button[aria-label='New terminal']")!.click());
  expect(create()).toHaveLength(1);
  await act(async () => release());
  expect(panel.shown()).toBe("new");
  // The press that found a start in flight waits for it too, and focuses what it made.
  await vi.waitFor(() => expect(focused).toEqual(["new", "new"]));
});

it("keeps one status region mounted for the starting state and hides the visible copy", async () => {
  let release!: () => void;
  const panel = mount([], { holdCreate: new Promise<void>((resolve) => (release = resolve)) });
  const region = panel.container.querySelector<HTMLElement>("[role=status].sr-only")!;
  expect(region.textContent).toBe("");
  act(() => void startShellTerminal(panel.daemon, "p", { ifNone: true }));
  expect(panel.container.querySelector("[role=status].sr-only")).toBe(region);
  await vi.waitFor(() => expect(region.textContent).toBe("Starting terminal…"));
  expect(Array.from(panel.container.querySelectorAll("[aria-hidden=true]")).some((el) => el.textContent?.includes("Starting terminal"))).toBe(true);
  await act(async () => release());
});

it("hands focus from the empty state's button to the header's before the start begins", async () => {
  let release!: () => void;
  const panel = mount([], { holdCreate: new Promise<void>((resolve) => (release = resolve)) });
  const empty = Array.from(panel.container.querySelectorAll<HTMLButtonElement>("button")).find((button) => !button.hasAttribute("aria-label") && button.textContent?.includes("New terminal"))!;
  act(() => empty.focus());
  act(() => empty.click());
  expect(document.activeElement).toBe(panel.container.querySelector("button[aria-label='New terminal']"));
  await act(async () => release());
});

it("keeps a record the broadcast brought when the reply names the same terminal", () => {
  const store = createStateStore({});
  applyDaemonEvent(store, { type: "shell_terminal_upserted", shell_terminal: shell("x", 1, { title: "newer" }) });
  seedShellTerminal(store, shell("x", 1, { title: "older" }));
  expect(store.getState().shellTerminals.get("x")?.title).toBe("newer");
});

it("joins the earlier start still pending when the later one has already settled", async () => {
  const store = createStateStore({});
  const holds: Array<(reply: Event) => void> = [];
  const request = vi.fn((body: RequestBody) => new Promise<Event>((resolve) => {
    if (body.type === "create_shell_terminal") holds.push(resolve);
  }));
  const daemon = { store, request, toastError: vi.fn() } as unknown as Daemon;
  const created = (id: string) => ({ type: "shell_terminal_created", id: null, shell_terminal: shell(id, 1) }) as Event;
  const first = startShellTerminal(daemon, "p");
  const second = startShellTerminal(daemon, "p");
  holds[1]!(created("second"));
  expect(await second).toBe("second");
  const joined = startShellTerminal(daemon, "p", { ifNone: true });
  expect(request).toHaveBeenCalledTimes(2);
  holds[0]!(created("first"));
  expect(await joined).toBe("first");
  expect(await first).toBe("first");
});

it("moves its tabs with the collection motion: none as it opens, a tab arriving enters, and one gone leaves only an inert copy", async () => {
  const motion = stubListMotion();
  const panel = mount([shell("a", 1), shell("b", 2)]);
  expect(motion.played()).toEqual([]);
  act(() => applyDaemonEvent(panel.store, { type: "shell_terminal_upserted", shell_terminal: shell("c", 3) }));
  const arrived = panel.tabs().find((tab) => tab.textContent === "shell c")!;
  expect(motion.played().map(({ element, frames }) => [element, presence(frames)])).toEqual([[arrived, "enter"]]);
  motion.clear();
  await act(async () => panel.tabs()[1].click());
  act(() => panel.container.querySelector<HTMLElement>("[data-view=b]")!.focus());
  focused.length = 0;
  act(() => applyDaemonEvent(panel.store, { type: "shell_terminal_deleted", shell_terminal: "b" }));
  expect(motion.played().map(({ frames }) => presence(frames))).toEqual(["exit"]);
  const copy = panel.container.querySelector<HTMLElement>("[data-list-motion-exit]")!;
  expect(copy.getAttribute("aria-hidden")).toBe("true");
  expect(copy.hasAttribute("inert")).toBe(true);
  expect(copy.querySelector("[id]")).toBeNull();
  // The strip names only the tabs that are left, and the terminal shown in place of the closed one takes focus.
  expect(panel.tabs().filter((tab) => !copy.contains(tab)).map((tab) => tab.textContent)).toEqual(["shell a", "shell c"]);
  expect(panel.shown()).toBe("c");
  await vi.waitFor(() => expect(focused).toContain("c"));
  expect(new Set(focused)).toEqual(new Set(["c"]));
});

it("drops a closed tab at once, with no copy, while reduced motion is asked for", () => {
  const motion = stubListMotion();
  const panel = mount([shell("a", 1), shell("b", 2)]);
  motion.reduce();
  act(() => applyDaemonEvent(panel.store, { type: "shell_terminal_deleted", shell_terminal: "a" }));
  act(() => applyDaemonEvent(panel.store, { type: "shell_terminal_upserted", shell_terminal: shell("c", 3) }));
  expect(motion.played()).toEqual([]);
  expect(panel.container.querySelector("[data-list-motion-exit]")).toBeNull();
});

const remove = (panel: ReturnType<typeof mount>, id: string) =>
  act(() => panel.store.setState((state) => ({ shellTerminals: new Map([...state.shellTerminals].filter(([key]) => key !== id)) })));

/** Puts the interaction in the pointer's modality, as a press just before the change under test leaves it. */
function pointerModality() {
  const before = getInteractionModality();
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
  act(() => setInteractionModality("pointer"));
  onTestFinished(() => act(() => Reflect.apply(setInteractionModality, undefined, [before])));
}

it.each([true, false])("focuses New terminal after the last tab goes with canMaximize=%s", async (canMaximize) => {
  const panel = mount([shell("a", 1)], {
    frame: { canMaximize, maximized: false, onToggleMaximized: vi.fn(), onClose: vi.fn() },
  });
  act(() => panel.tabs()[0].focus());
  remove(panel, "a");
  await vi.waitFor(() => expect(document.activeElement).toBe(panel.container.querySelector('button[aria-label="New terminal"]')));
});

it("hands focus on a tab that goes to the shown tab, with the focus ring, never to the departing copy", async () => {
  stubListMotion();
  const panel = mount([shell("a", 1), shell("b", 2), shell("c", 3)]);
  await act(async () => panel.tabs()[1].click());
  act(() => panel.tabs()[1].focus());
  pointerModality();
  remove(panel, "b");
  expect(panel.container.querySelector("[data-list-motion-exit]")).not.toBeNull();
  await vi.waitFor(() => expect(document.activeElement).toBe(panel.tabs().find((tab) => tab.textContent === "shell c")));
  expect(document.activeElement!.closest("[data-list-motion-exit]")).toBeNull();
  expect(getInteractionModality()).toBe("keyboard");
});

it("hands focus to the shown tab, with the focus ring, when the tab goes with its context menu open", async () => {
  const panel = mount([shell("a", 1), shell("b", 2)]);
  act(() => panel.tabs()[0].dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 20, clientY: 30 })));
  await vi.waitFor(() => expect(document.activeElement?.closest("[role=menu]")).not.toBeNull());
  pointerModality();
  remove(panel, "a");
  await vi.waitFor(() => expect(document.activeElement).toBe(panel.tabs()[0]));
  expect(panel.tabs()[0].textContent).toBe("shell b");
  expect(document.querySelector("[role=menu]")).toBeNull();
  expect(getInteractionModality()).toBe("keyboard");
});

it("hands focus to the New terminal button when the last tab goes while it holds focus", async () => {
  const panel = mount([shell("a", 1)]);
  act(() => panel.tabs()[0].focus());
  remove(panel, "a");
  await vi.waitFor(() => expect(document.activeElement).toBe(panel.container.querySelector("button[aria-label='New terminal']")));
});

it("leaves focus alone when a tab goes while the header's menu, not that tab's, holds it", async () => {
  const panel = mount([shell("a", 1), shell("b", 2)]);
  const header = panel.container.querySelector<HTMLElement>("button[aria-label^='Actions for terminal']:not([hidden])")!;
  act(() => header.focus());
  act(() => header.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
  await vi.waitFor(() => expect(document.activeElement?.closest("[role=menu]")).not.toBeNull());
  const menu = document.activeElement!.closest("[role=menu]")!;
  pointerModality();
  remove(panel, "a");
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  expect(document.activeElement?.closest("[role=menu]")).toBe(menu);
  expect(getInteractionModality()).toBe("pointer");
});

it("leaves focus where the user put it after the tab that held it has gone", async () => {
  const panel = mount([shell("a", 1), shell("b", 2)]);
  act(() => panel.tabs()[0].focus());
  const elsewhere = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => elsewhere.remove());
  remove(panel, "a");
  act(() => elsewhere.focus());
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  expect(document.activeElement).toBe(elsewhere);
});

/** Has the daemon end a terminal as it answers its close, as it does for one already idle at the prompt. */
function closesAtOnce(panel: ReturnType<typeof mount>) {
  panel.request.mockImplementation(async (body: RequestBody): Promise<Event> => {
    if (body.type === "close_shell_terminal") remove(panel, body.shell_terminal);
    return { type: "ack", id: null } as Event;
  });
}

/** Opens the context menu of `tab` and chooses Close terminal, which asks about a busy terminal first. */
async function closeFromContextMenu(tab: HTMLElement) {
  act(() => tab.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 20, clientY: 30 })));
  await vi.waitFor(() => expect(document.activeElement?.closest("[role=menu]")).not.toBeNull());
  const item = Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem]")).find((each) => each.textContent === "Close terminal")!;
  await act(async () => item.click());
  await vi.waitFor(() => expect(document.querySelector("[role=alertdialog]")).not.toBeNull());
}

const dialogButton = (name: string) =>
  Array.from(document.querySelectorAll<HTMLButtonElement>("[role=alertdialog] button")).find((button) => button.textContent === name)!;

it("hands focus to the shown tab, with the focus ring, when a busy tab closed from its context menu goes on confirming", async () => {
  const panel = mount([shell("a", 1, { busy: true }), shell("b", 2)]);
  closesAtOnce(panel);
  await closeFromContextMenu(panel.tabs()[0]);
  act(() => dialogButton("Close terminal").focus());
  pointerModality();
  await act(async () => dialogButton("Close terminal").click());
  expect(panel.request).toHaveBeenCalledWith({ type: "close_shell_terminal", shell_terminal: "a" });
  await vi.waitFor(() => expect(document.activeElement).toBe(panel.tabs()[0]));
  expect(panel.tabs().map((tab) => tab.textContent)).toEqual(["shell b"]);
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
  expect(getInteractionModality()).toBe("keyboard");
});

it("hands focus to the New terminal button when the last tab, busy, goes on confirming its close", async () => {
  const panel = mount([shell("a", 1, { busy: true })]);
  closesAtOnce(panel);
  await closeFromContextMenu(panel.tabs()[0]);
  act(() => dialogButton("Close terminal").focus());
  await act(async () => dialogButton("Close terminal").click());
  await vi.waitFor(() => expect(document.activeElement).toBe(panel.container.querySelector("button[aria-label='New terminal']")));
});

it("hands focus to the shown terminal when the shown tab, busy, closed from its close mark goes on confirming", async () => {
  const panel = mount([shell("a", 1), shell("b", 2, { busy: true })]);
  closesAtOnce(panel);
  act(() => panel.container.querySelector<HTMLElement>("[data-view=b]")!.focus());
  await act(async () => panel.tabs()[1].querySelector<HTMLElement>("span[aria-hidden=true]")!.click());
  await vi.waitFor(() => expect(document.querySelector("[role=alertdialog]")).not.toBeNull());
  focused.length = 0;
  act(() => dialogButton("Close terminal").focus());
  await act(async () => dialogButton("Close terminal").click());
  await vi.waitFor(() => expect(focused).toEqual(["a"]));
  expect(panel.shown()).toBe("a");
});

// Where focus goes back to after Cancel is the dialog's own return, which needs a browser (the gallery's
// shell-panel-tab-menu-focused); here only that the panel moves it nowhere itself.
it("hands nothing on when the close of a busy tab is declined", async () => {
  const panel = mount([shell("a", 1, { busy: true }), shell("b", 2)]);
  await closeFromContextMenu(panel.tabs()[0]);
  act(() => dialogButton("Cancel").focus());
  await act(async () => dialogButton("Cancel").click());
  await vi.waitFor(() => expect(document.querySelector("[role=alertdialog]")).toBeNull());
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  expect(document.activeElement).not.toBe(panel.tabs()[1]);
  // The click's own modality: no hand-off asked for the focus ring.
  expect(getInteractionModality()).toBe("virtual");
  expect(panel.request).not.toHaveBeenCalledWith({ type: "close_shell_terminal", shell_terminal: "a" });
});

// jsdom runs no restore of focus as a dialog closes, so the test puts focus back where React Aria would: on the
// shown terminal that held it when the close mark was pressed.
it("leaves focus where the confirmation put it back, with no ring, when the daemon refuses a busy tab's close", async () => {
  const panel = mount([shell("a", 1, { busy: true }), shell("b", 2)]);
  panel.request.mockImplementation(async (body: RequestBody): Promise<Event> => {
    if (body.type === "close_shell_terminal") throw new Error("refused");
    return { type: "ack", id: null } as Event;
  });
  const view = panel.container.querySelector<HTMLElement>("[data-view=b]")!;
  act(() => view.focus());
  await act(async () => panel.tabs()[0].querySelector<HTMLElement>("span[aria-hidden=true]")!.click());
  await vi.waitFor(() => expect(document.querySelector("[role=alertdialog]")).not.toBeNull());
  act(() => dialogButton("Close terminal").focus());
  pointerModality();
  act(() => dialogButton("Close terminal").click());
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
  act(() => view.focus());
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    await new Promise((resolve) => requestAnimationFrame(resolve));
  });
  expect(panel.request).toHaveBeenCalledWith({ type: "close_shell_terminal", shell_terminal: "a" });
  expect(document.activeElement).toBe(view);
  expect(getInteractionModality()).not.toBe("keyboard");
  expect(panel.tabs()).toHaveLength(2);
});
