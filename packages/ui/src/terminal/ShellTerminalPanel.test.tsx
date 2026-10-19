// @vitest-environment jsdom
import { act, useImperativeHandle } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import { setGitModeWorktree } from "../browser/browserState";
import { isolate } from "../i18n/isolate";
import { PlatformProvider } from "../platform/react";
import type { Event, Project, RequestBody, ShellTerminal } from "../protocol";
import { applyDaemonEvent, createStateStore, DaemonProvider, seedShellTerminal, type Daemon } from "../store";
import { ShellTerminalPanel } from "./ShellTerminalPanel";
import { startShellTerminal } from "./shellTerminals";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const focused = vi.hoisted(() => [] as string[]);

// The view stands in for the terminal itself, which needs a real browser: it says which terminal
// it is and whether its tab is the one shown, and records the focus it is given.
vi.mock("./ShellTerminalView", () => ({
  ShellTerminalView: ({ terminal, shown, ref }: { terminal: ShellTerminal; shown: boolean; ref?: React.Ref<{ focus: () => void }> }) => {
    useImperativeHandle(ref, () => ({ focus: () => focused.push(terminal.id) }), [terminal.id]);
    return <div id={`shell-terminal-${terminal.id}`} data-view={terminal.id} data-shown={String(shown)} />;
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
  localStorage.clear();
  setGitModeWorktree("p", undefined);
});

// `eventFirst`: the broadcast of the new record arrives before the reply that names it; otherwise
// it arrives after, as `lateEvent` releases it. The broadcast carries a newer title than the reply
// does, as one that came after a rename would.
function mount(terminals: ShellTerminal[], { holdCreate, eventFirst = true }: { holdCreate?: Promise<void>; eventFirst?: boolean } = {}) {
  const store = createStateStore({ shellTerminals: new Map(terminals.map((terminal) => [terminal.id, terminal])) });
  const broadcast = () => applyDaemonEvent(store, { type: "shell_terminal_upserted", shell_terminal: shell("new", 99, { title: "newer" }) });
  const request = vi.fn(async (body: RequestBody): Promise<Event> => {
    if (body.type === "create_shell_terminal") {
      await holdCreate;
      if (eventFirst) broadcast();
      return { type: "shell_terminal_created", shell_terminal: shell("new", 99) };
    }
    return { type: "ack" } as Event;
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
          <ShellTerminalPanel project={project} switching={false} />
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

it("focuses the terminal a start from the panel made, once it is shown", async () => {
  const panel = mount([shell("a", 1)]);
  await act(async () => panel.container.querySelector<HTMLButtonElement>("button[aria-label='New terminal']")!.click());
  await vi.waitFor(() => expect(focused).toEqual(["new"]));
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
  const created = (id: string) => ({ type: "shell_terminal_created", shell_terminal: shell(id, 1) }) as Event;
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
