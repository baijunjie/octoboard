// @vitest-environment jsdom
import { act, useState } from "react";
import { getInteractionModality, setInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { mountForTooltip } from "../components/tooltipTestSupport";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Event, ProcessTree, RequestBody, Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ProcessesPanel } from "./ProcessesPanel";
import { ProcessPanelProvider, useProcessPanel } from "./useProcessPanel";
import type { Project } from "../protocol";

type ProcessesScope = { session: Session } | { project: Project } | { console: Session };

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const running = sessionOf("s-1", console_.id, project.id, "Fix search", "idle");
const second = sessionOf("s-2", console_.id, project.id, "Add filters", "working");
const NOW = Date.UTC(2026, 9, 28, 12, 0, 0);

const tree = (id: string, extra: Partial<ProcessTree> = {}): ProcessTree => ({
  id, session_id: running.id, command: "npm run dev", cwd: "/work/search", cwd_missing: false, kept: false,
  started_at: NOW - 5 * 60_000, process_count: 1, memory_bytes: 120_000_000, ports: [], ...extra,
});

/** Mounts the panel over a store holding `sessions`, with the daemon answering `request`. */
async function mount(scope: ProcessesScope, request: (body: RequestBody) => Promise<Event>, sessions: Session[] = [running, second]) {
  vi.useFakeTimers({ now: NOW });
  onTestFinished(() => {
    vi.useRealTimers();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  const daemon: Daemon = {
    store: createStateStore({ sessions: new Map(sessions.map((s) => [s.id, s])), projects: new Map([[project.id, project]]) }),
    request: vi.fn(request) as never,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  let switchScope!: (scope: ProcessesScope) => void;
  function Panel() {
    const [view, setView] = useState(scope);
    switchScope = setView;
    const owner = "console" in view ? { kind: "report" as const, consoleSession: view.console.id } : { kind: "project" as const, project: "project" in view ? view.project.id : project.id };
    const data = useProcessPanel(owner, "session" in view ? view.session.id : undefined, "live");
    return <ProcessPanelProvider value={data}><section data-region="panel-processes" tabIndex={-1}><ProcessesPanel /></section></ProcessPanelProvider>;
  }
  mountForTooltip(<DaemonProvider value={daemon}><Panel /></DaemonProvider>);
  await act(async () => {});
  return { daemon, switchScope };
}

const panel = () => document.querySelector('[data-region="panel-processes"]')!;
const rows = () => [...panel().querySelectorAll("li")];
const button = (label: string, row = rows()[0]) => [...row.querySelectorAll("button")].find((candidate) => candidate.textContent === label)!;
const confirmation = () => document.querySelector('[role="alertdialog"]')!;
const confirmStop = () => act(async () => confirmation().querySelector<HTMLButtonElement>('button[type="submit"]')!.click());
/** Confirms with the mouse: a press that leaves the interaction in the pointer's modality, which a script's `click()` does not. */
async function confirmWithPointer() {
  const submit = confirmation().querySelector<HTMLButtonElement>('button[type="submit"]')!;
  const before = getInteractionModality();
  onTestFinished(() => act(() => Reflect.apply(setInteractionModality, undefined, [before])));
  await act(async () => void document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true })));
  await act(async () => void submit.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, detail: 1 })));
}
const sent = (daemon: Daemon, type: string) => vi.mocked(daemon.request).mock.calls.map(([body]) => body).filter((body) => body.type === type);

it("lists a session's trees with their command, working directory, ports, memory, age and process count", async () => {
  await mount({ session: running }, async () => ({
    type: "processes_listed",
    id: null,
    trees: [tree("1:1", { ports: [3000, 5173], process_count: 3 }), tree("2:2", { command: "tail -f log", memory_bytes: 2_500_000 })],
  }));
  expect(rows()).toHaveLength(2);
  const text = rows()[0].textContent!.replace(/[\u2066-\u2069]/g, "");
  expect(text).toContain("npm run dev");
  expect(text).toContain("/work/search");
  expect(text).toContain("120 MB of memory · listening on ports 3000 and 5173 · started 5 minutes ago · 3 processes");
  expect(rows()[1].textContent).toContain("2.5 MB of memory · started 5 minutes ago");
  expect(rows()[1].textContent).not.toContain("process");
  expect(panel().querySelector("h2")?.textContent).toBe("Processes");
});

it("words a command and a working directory that could not be read, and marks a deleted one", async () => {
  await mount({ session: running }, async () => ({
    type: "processes_listed",
    id: null,
    trees: [tree("1:1", { command: "", cwd: null }), tree("2:2", { cwd_missing: true })],
  }));
  expect(rows()[0].textContent).toContain("Unknown command");
  expect(rows()[0].textContent).toContain("Working directory unknown");
  expect(rows()[1].textContent).toContain("Directory deleted");
});

it("says there is nothing, not that it is loading, once the list comes back empty", async () => {
  await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: [] }));
  expect(panel().textContent).toContain("No processes");
  expect(panel().textContent).toContain("Nothing the session started is still running.");
  expect(panel().textContent).not.toContain("Loading processes");
});

it("reads a session whose process has ended as having none, raising nothing", async () => {
  await mount({ session: running }, async () => { throw new DaemonRequestError("gone", "session_not_running", {}); });
  expect(panel().textContent).toContain("No processes");
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
});

it("filters a project's trees by a session picked in a selector that starts empty, with no visible label, and goes back to all of them when cleared", async () => {
  await mount({ project }, async () => ({
    type: "processes_listed",
    id: null,
    trees: [tree("1:1"), tree("2:2", { session_id: second.id, command: "vite" }), tree("3:3", { session_id: undefined, command: "node kept.js" })],
  }));
  const trigger = panel().querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]')!;
  expect(trigger.textContent).toBe("All sessions");
  expect(trigger.getAttribute("aria-label")).toBe("Filter by session");
  expect(rows()).toHaveLength(3);
  const choose = (value: string) => act(() => {
    const native = panel().querySelector("select")!;
    native.value = value;
    native.dispatchEvent(new Event("change", { bubbles: true }));
  });
  choose(second.id);
  expect(trigger.textContent).toBe("Add filters");
  expect(rows().map((row) => row.textContent)).toEqual([expect.stringContaining("vite")]);
  expect(panel().querySelectorAll("h3")).toHaveLength(0);
  act(() => panel().querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
  expect(trigger.textContent).toBe("All sessions");
  expect(rows()).toHaveLength(3);
  choose(second.id);
  expect(rows()).toHaveLength(1);
  act(() => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Backspace", bubbles: true })); });
  expect(trigger.textContent).toBe("All sessions");
  expect(rows()).toHaveLength(3);
});

it("sets the panel's explanation and the Project processes note in one style", async () => {
  await mount({ project }, async () => ({ type: "processes_listed", id: null, trees: [tree("1:1"), tree("3:3", { session_id: undefined })] }));
  const notes = [...panel().querySelectorAll("p")].filter((p) => p.textContent === "Processes started by a session stop automatically when the session ends." || p.textContent?.startsWith("These processes belong to no session"));
  expect(notes).toHaveLength(2);
  expect(new Set(notes.map((note) => note.className)).size).toBe(1);
});

it("groups a project's trees under the sessions' titles and puts those no session owns under Project processes, kept or left behind", async () => {
  const { daemon } = await mount({ project }, async () => ({
    type: "processes_listed",
    id: null,
    trees: [
      tree("1:1"),
      tree("2:2", { session_id: second.id, command: "vite" }),
      tree("3:3", { session_id: undefined, kept: true, command: "node kept.js" }),
      tree("4:4", { session_id: undefined, command: "node old.js", cwd_missing: true }),
    ],
  }));
  expect(sent(daemon, "list_processes")).toEqual([{ type: "list_processes", project_id: project.id }]);
  const headings = [...panel().querySelectorAll("h3")].map((heading) => heading.textContent);
  expect(headings).toEqual(["Session processes", "Project processes"]);
  expect([...panel().querySelectorAll("h4")].map((heading) => heading.textContent)).toEqual(["Fix search", "Add filters"]);
  expect(panel().textContent).toContain("These processes belong to no session. They are never stopped automatically; only you can stop them.");
  const group = [...panel().querySelectorAll("section")].find((section) => section.querySelector(":scope > h3")?.textContent === "Project processes")!;
  expect([...group.querySelectorAll("li")].map((row) => row.textContent)).toEqual([expect.stringContaining("node kept.js"), expect.stringMatching(/node old\.js.*Directory deleted/)]);
});

it("stops a tree on request, drops it from the list and says so", async () => {
  const stopped = new Set<string>();
  const { daemon } = await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") {
      body.ids.forEach((id) => stopped.add(id));
      return { type: "processes_stopped", id: null, ids: body.ids };
    }
    return { type: "processes_listed", id: null, trees: [tree("1:1"), tree("2:2", { command: "tail -f log" })].filter((candidate) => !stopped.has(candidate.id)) };
  });
  await act(async () => button("Stop").click());
  expect(sent(daemon, "stop_processes")).toEqual([]);
  await confirmStop();
  expect(sent(daemon, "stop_processes")).toEqual([{ type: "stop_processes", ids: ["1:1"] }]);
  expect(daemon.toastNotice).toHaveBeenCalledWith("Stopped the process.");
  expect(rows().map((row) => row.textContent)).toEqual([expect.stringContaining("tail -f log")]);
});

it("offers Stop and Keep on a session's tree, each named by the command, and only Stop on one no session owns", async () => {
  await mount({ project }, async () => ({
    type: "processes_listed",
    id: null,
    trees: [tree("1:1"), tree("2:2", { session_id: undefined, command: "node old.js" }), tree("3:3", { command: "" })],
  }));
  expect([...rows()[0].querySelectorAll("button")].map((candidate) => candidate.textContent)).toEqual(["Keep", "Stop"]);
  expect(button("Stop").getAttribute("aria-label")).toBe("Stop \u2068npm run dev\u2069");
  expect(button("Keep").getAttribute("aria-label")).toBe("Keep \u2068npm run dev\u2069");
  expect(button("Keep", rows()[1]).getAttribute("aria-label")).toContain("Unknown command");
  expect(rows()[2].querySelectorAll("button")).toHaveLength(1);
  expect(button("Stop", rows()[2]).getAttribute("aria-label")).toContain("node old.js");
});

it("puts actions beside single-line facts while the command and directory keep the card's whole width", async () => {
  await mount({ project }, async () => ({ type: "processes_listed", id: null, trees: [tree("1:1")] }));
  const line = button("Stop").parentElement!.parentElement!;
  expect(line.textContent).toContain("120 MB");
  expect(line.textContent).not.toContain("npm run dev");
  expect(line.textContent).not.toContain("/work/search");
  expect(line.querySelector(".whitespace-nowrap")?.textContent).toContain("120 MB");
  expect(rows()[0].textContent).toContain("npm run dev");
  expect(rows()[0].textContent).toContain("/work/search");
});

it("offers a console session's trees only Stop", async () => {
  const consoleSession = sessionOf("s-console", console_.id, undefined, "Main", "idle");
  await mount({ console: consoleSession }, async () => ({ type: "processes_listed", id: null, trees: [tree("1:1", { session_id: consoleSession.id })] }), [consoleSession]);
  expect(rows()).toHaveLength(1);
  expect([...rows()[0].querySelectorAll("button")].map((candidate) => candidate.textContent)).toEqual(["Stop"]);
});

it("keeps a tree on request after a confirmation, drops it from the list and says so", async () => {
  const kept = new Set<string>();
  const { daemon } = await mount({ session: running }, async (body) => {
    if (body.type === "keep_processes") {
      body.ids.forEach((id) => kept.add(id));
      return { type: "processes_kept", id: null, ids: body.ids };
    }
    return { type: "processes_listed", id: null, trees: [tree("1:1"), tree("2:2", { command: "tail -f log" })].filter((candidate) => !kept.has(candidate.id)) };
  });
  await act(async () => button("Keep").click());
  expect(sent(daemon, "keep_processes")).toEqual([]);
  expect(confirmation().textContent).toContain("Keep process");
  expect(confirmation().textContent).toContain("no longer stop when the session ends");
  await confirmStop();
  expect(sent(daemon, "keep_processes")).toEqual([{ type: "keep_processes", ids: ["1:1"] }]);
  expect(daemon.toastNotice).toHaveBeenCalledWith("Kept the process.");
  expect(rows().map((row) => row.textContent)).toEqual([expect.stringContaining("tail -f log")]);
});

it("cancels a keep confirmation without sending a request", async () => {
  const { daemon } = await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: [tree("1:1")] }));
  await act(async () => button("Keep").click());
  await act(async () => [...confirmation().querySelectorAll("button")].find((element) => element.textContent === "Cancel")!.click());
  expect(sent(daemon, "keep_processes")).toEqual([]);
  expect(confirmation()).toBeNull();
});

it("raises a refused keep as an error alert and keeps the tree listed", async () => {
  await mount({ session: running }, async (body) => {
    if (body.type === "keep_processes") throw new DaemonRequestError("no", "internal_error", {});
    return { type: "processes_listed", id: null, trees: [tree("1:1")] };
  });
  await act(async () => button("Keep").click());
  await confirmStop();
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not keep the process"]);
  expect(rows()).toHaveLength(1);
});

it("shows the whole command in a confirmation, wrapped rather than cut", async () => {
  const command = "node ".concat("--flag=value ".repeat(30));
  await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: [tree("1:1", { command })] }));
  await act(async () => button("Stop").click());
  expect(confirmation().textContent).toContain(command.trim());
  expect(confirmation().querySelector(".line-clamp-2")).toBeNull();
});

it("cancels a stop confirmation without sending a request", async () => {
  const { daemon } = await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: [tree("1:1", { process_count: 4 })] }));
  await act(async () => button("Stop").click());
  expect(sent(daemon, "stop_processes")).toEqual([]);
  expect(confirmation().textContent).toContain("This process and its child processes will be stopped.");
  expect(confirmation().textContent).toContain("npm run dev");
  expect(confirmation().textContent).toContain("4 processes");
  await act(async () => [...confirmation().querySelectorAll("button")].find((element) => element.textContent === "Cancel")!.click());
  expect(sent(daemon, "stop_processes")).toEqual([]);
  expect(rows()).toHaveLength(1);
});

it("raises a refused stop as an error alert and keeps the tree listed", async () => {
  let failing = true;
  const { daemon } = await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") {
      if (failing) throw new DaemonRequestError("no", "internal_error", {});
      return { type: "processes_stopped", id: null, ids: body.ids };
    }
    return { type: "processes_listed", id: null, trees: [tree("1:1")] };
  });
  await act(async () => button("Stop").click());
  await confirmStop();
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not stop the process"]);
  expect(rows()).toHaveLength(1);
  expect(confirmation()).not.toBeNull();
  failing = false;
  await act(async () => errorAlerts.acknowledge());
  await confirmStop();
  expect(sent(daemon, "stop_processes")).toHaveLength(2);
});

it("asks again every few seconds while open, and shows a failed refresh as an Alert over the last list, never as an error alert dialog", async () => {
  let failing = false;
  const { daemon } = await mount({ session: running }, async () => {
    if (failing) throw new DaemonRequestError("down", "internal_error", {});
    return { type: "processes_listed", id: null, trees: [tree("1:1", { ports: [3000] })] };
  });
  expect(sent(daemon, "list_processes")).toHaveLength(1);
  expect(panel().textContent).not.toContain("Could not refresh the list");
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(sent(daemon, "list_processes")).toHaveLength(2);
  failing = true;
  await act(async () => void vi.advanceTimersByTime(3000));
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(sent(daemon, "list_processes")).toHaveLength(4);
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(panel().textContent).toContain("Could not refresh the list");
  expect(panel().textContent).toContain("The processes shown may be out of date.");
  expect(rows()).toHaveLength(1);
  failing = false;
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(panel().textContent).not.toContain("Could not refresh the list");
});

it("keeps showing the refresh failure after a stop, which asks again at once", async () => {
  let failing = false;
  await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") return { type: "processes_stopped", id: null, ids: body.ids };
    if (failing) throw new DaemonRequestError("down", "internal_error", {});
    return { type: "processes_listed", id: null, trees: [tree("1:1"), tree("2:2")] };
  });
  failing = true;
  await act(async () => void vi.advanceTimersByTime(3000));
  await act(async () => button("Stop").click());
  await confirmStop();
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(panel().textContent).toContain("Could not refresh the list");
});

it("shows a first-list failure in the panel and waits for explicit retry", async () => {
  let failing = true;
  const { daemon } = await mount({ session: running }, async () => {
    if (failing) throw new Error("socket closed");
    return { type: "processes_listed", id: null, trees: [tree("1:1")] };
  });
  expect(panel().textContent).toContain("Could not list the processes");
  expect(panel().querySelector("h2")?.textContent).toBe("Processes");
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  await act(async () => void vi.advanceTimersByTime(9000));
  expect(sent(daemon, "list_processes")).toHaveLength(1);
  failing = false;
  await act(async () => [...panel().querySelectorAll("button")].find((candidate) => candidate.textContent === "Try again")!.click());
  expect(rows()).toHaveLength(1);
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(sent(daemon, "list_processes")).toHaveLength(3);
});

it("makes row controls and Cancel unavailable while the confirmed stop is pending", async () => {
  await mount({ session: running }, async (body) => body.type === "stop_processes"
    ? new Promise<never>(() => {})
    : { type: "processes_listed", id: null, trees: [tree("1:1"), tree("2:2", { command: "tail -f log" })] });
  await act(async () => button("Stop").click());
  await confirmStop();
  expect([...confirmation().querySelectorAll("button")].find((element) => element.textContent === "Stop")!.hasAttribute("data-pending")).toBe(true);
  expect([...confirmation().querySelectorAll("button")].find((element) => element.textContent === "Cancel")!.disabled).toBe(true);
  expect(button("Stop", rows()[1]).disabled).toBe(true);

});


it("ignores a late action failure after the panel passes to another owner", async () => {
  let refuse!: (error: Error) => void;
  const { switchScope } = await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") return new Promise<Event>((_resolve, reject) => { refuse = reject; });
    return { type: "processes_listed", id: null, trees: [tree("1:1")] };
  });
  await act(async () => button("Stop").click());
  await confirmStop();
  await act(async () => switchScope({ project: { ...project, id: "other" } }));
  expect(button("Stop").disabled).toBe(false);
  await act(async () => refuse(new Error("late refusal")));
  expect(errorAlerts.getSnapshot()).toHaveLength(0);
  expect(rows()).toHaveLength(1);
});

it("hands focus back into the panel when a focused process row disappears during refresh", async () => {
  let gone = false;
  await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: gone ? [] : [tree("1:1")] }));
  await act(async () => button("Stop").focus());
  gone = true;
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(panel().contains(document.activeElement)).toBe(true);
  expect(rows()).toHaveLength(0);
});

it("dismisses the confirmation if its process disappears before confirmation", async () => {
  let gone = false;
  const { daemon } = await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: gone ? [] : [tree("1:1")] }));
  await act(async () => button("Stop").click());
  gone = true;
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(confirmation()).toBeNull();
  expect(sent(daemon, "stop_processes")).toEqual([]);
});

it("dismisses an unconfirmed stop when the owner changes without sending it", async () => {
  const { daemon, switchScope } = await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: [tree("1:1")] }));
  await act(async () => button("Stop").click());
  await act(async () => switchScope({ project: { ...project, id: "other" } }));
  expect(confirmation()).toBeNull();
  expect(sent(daemon, "stop_processes")).toEqual([]);
});

const settleFocus = () => act(async () => void vi.advanceTimersByTime(100));

it("hands focus to the next row's first action after a confirmed stop, with the ring", async () => {
  const stopped = new Set<string>();
  await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") {
      body.ids.forEach((id) => stopped.add(id));
      return { type: "processes_stopped", id: null, ids: body.ids };
    }
    return { type: "processes_listed", id: null, trees: [tree("1:1"), tree("2:2", { command: "tail -f log" })].filter((candidate) => !stopped.has(candidate.id)) };
  });
  await act(async () => button("Stop").focus());
  await act(async () => button("Stop").click());
  await confirmStop();
  await settleFocus();
  expect(rows()).toHaveLength(1);
  expect(document.activeElement).toBe(button("Keep"));
  expect(getInteractionModality()).toBe("keyboard");
});

it("hands focus on after a stop opened from the keyboard but confirmed with the pointer", async () => {
  const stopped = new Set<string>();
  await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") {
      body.ids.forEach((id) => stopped.add(id));
      return { type: "processes_stopped", id: null, ids: body.ids };
    }
    return { type: "processes_listed", id: null, trees: [tree("1:1"), tree("2:2", { command: "tail -f log" })].filter((candidate) => !stopped.has(candidate.id)) };
  });
  await act(async () => button("Stop").focus());
  await act(async () => button("Stop").click());
  await confirmWithPointer();
  await settleFocus();
  expect(rows()).toHaveLength(1);
  expect(document.activeElement).toBe(button("Keep"));
  expect(getInteractionModality()).toBe("keyboard");
});

it("leaves focus alone after a stop opened and confirmed with the pointer, with no ring", async () => {
  const stopped = new Set<string>();
  await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") {
      body.ids.forEach((id) => stopped.add(id));
      return { type: "processes_stopped", id: null, ids: body.ids };
    }
    return { type: "processes_listed", id: null, trees: [tree("1:1"), tree("2:2", { command: "tail -f log" })].filter((candidate) => !stopped.has(candidate.id)) };
  });
  await act(async () => button("Stop").click());
  await confirmWithPointer();
  await settleFocus();
  expect(rows()).toHaveLength(1);
  expect(panel().contains(document.activeElement)).toBe(false);
  expect(getInteractionModality()).toBe("pointer");
});

it("hands focus to the panel's landing, the session filter of a project, after the last row is kept", async () => {
  const kept = new Set<string>();
  await mount({ project }, async (body) => {
    if (body.type === "keep_processes") {
      body.ids.forEach((id) => kept.add(id));
      return { type: "processes_kept", id: null, ids: body.ids };
    }
    return { type: "processes_listed", id: null, trees: [tree("1:1")].filter((candidate) => !kept.has(candidate.id)) };
  });
  await act(async () => button("Keep").focus());
  await act(async () => button("Keep").click());
  await confirmStop();
  await settleFocus();
  expect(rows()).toHaveLength(0);
  expect(document.activeElement).toBe(panel().querySelector('button[aria-haspopup="listbox"]'));
});

it("keeps the confirmation, the toast and the focus hand-over when a refresh takes the tree out while the stop is in flight", async () => {
  let gone = false;
  let answer!: (event: Event) => void;
  const { daemon } = await mount({ project }, async (body) => {
    if (body.type === "stop_processes") return new Promise<Event>((resolve) => { answer = resolve; });
    return { type: "processes_listed", id: null, trees: gone ? [] : [tree("1:1")] };
  });
  await act(async () => button("Stop").focus());
  await act(async () => button("Stop").click());
  await confirmStop();
  gone = true;
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(confirmation()).not.toBeNull();
  await act(async () => answer({ type: "processes_stopped", id: null, ids: ["1:1"] }));
  await settleFocus();
  expect(daemon.toastNotice).toHaveBeenCalledWith("Stopped the process.");
  expect(confirmation()).toBeNull();
  expect(document.activeElement).toBe(panel().querySelector('button[aria-haspopup="listbox"]'));
});

const tryAgain = () => [...panel().querySelectorAll("button")].find((candidate) => candidate.textContent === "Try again")!;

it("gives the failed first list's place to the loading line when Try again starts, with focus going to the panel's landing first, and lands there once the list arrives", async () => {
  let failing = true;
  let answer!: (event: Event) => void;
  await mount({ project }, async () => {
    if (failing) throw new Error("socket closed");
    return new Promise<Event>((resolve) => { answer = resolve; });
  });
  await act(async () => tryAgain().focus());
  failing = false;
  await act(async () => tryAgain().click());
  expect(panel().textContent).toContain("Loading processes");
  expect(panel().textContent).not.toContain("Could not list the processes");
  expect(document.activeElement).toBe(panel().querySelector('button[aria-haspopup="listbox"]'));
  await act(async () => answer({ type: "processes_listed", id: null, trees: [tree("1:1")] }));
  await settleFocus();
  expect(rows()).toHaveLength(1);
  expect(document.activeElement).toBe(panel().querySelector('button[aria-haspopup="listbox"]'));
});

it("announces a retry that fails again as a new failure, once the focus move it came with has been spoken", async () => {
  await mount({ session: running }, async () => { throw new Error("socket closed"); });
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  const first = panel().querySelector("[role=alert]");
  expect(first).not.toBeNull();
  await act(async () => tryAgain().click());
  expect(panel().querySelector("[role=alert]")).toBeNull();
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  const again = panel().querySelector("[role=alert]");
  expect(again).not.toBeNull();
  expect(again).not.toBe(first);
});

const threeTrees = (without?: string) => [tree("1:1"), tree("2:2", { command: "tail -f log" }), tree("3:3", { command: "node third.js" })].filter((candidate) => candidate.id !== without);

it("hands focus to the card that followed the tree when a refresh took it out while its stop was in flight", async () => {
  let gone = false;
  let answer!: (event: Event) => void;
  await mount({ session: running }, async (body) => {
    if (body.type === "stop_processes") return new Promise<Event>((resolve) => { answer = resolve; });
    return { type: "processes_listed", id: null, trees: threeTrees(gone ? "2:2" : undefined) };
  });
  await act(async () => button("Stop", rows()[1]).focus());
  await act(async () => button("Stop", rows()[1]).click());
  await confirmStop();
  gone = true;
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(rows()).toHaveLength(2);
  await act(async () => answer({ type: "processes_stopped", id: null, ids: ["2:2"] }));
  await settleFocus();
  expect(confirmation()).toBeNull();
  expect(document.activeElement).toBe(button("Keep", rows()[1]));
  expect(rows()[1].textContent).toContain("node third.js");
});

it("hands focus to the next row's first action when a refresh takes the tree out from under its open confirmation", async () => {
  let gone = false;
  await mount({ session: running }, async () => ({ type: "processes_listed", id: null, trees: threeTrees(gone ? "2:2" : undefined) }));
  await act(async () => button("Stop", rows()[1]).focus());
  await act(async () => button("Stop", rows()[1]).click());
  gone = true;
  await act(async () => void vi.advanceTimersByTime(3000));
  await settleFocus();
  expect(confirmation()).toBeNull();
  expect(rows()).toHaveLength(2);
  expect(document.activeElement).toBe(button("Keep", rows()[1]));
  expect(rows()[1].textContent).toContain("node third.js");
});
