// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import * as reactAria from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "./daemon-client";
import { ErrorAlerts } from "./dialogs/ErrorAlerts";
import { errorAlerts } from "./errorAlerts";
import { App } from "./App";
import { setProjectBrowserMode, useProjectBrowserState } from "./browser/browserState";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "./components/StatusAnnouncer";
import { mountForTooltip } from "./components/tooltipTestSupport";
import { windowAttention } from "./components/windowTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./gallery/fixtures/builders";
import { isolate } from "./i18n/isolate";
import { PlatformProvider } from "./platform/react";
import type { PlatformAdapter } from "./platform";
import type { Project } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";
import { FileViewer } from "./viewer/FileViewer";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("react-aria", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-aria")>();
  return { ...actual, setInteractionModality: vi.fn(actual.setInteractionModality) };
});
vi.mock("./browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => {
    const state = useProjectBrowserState(project.id);
    return <div data-browser={project.id} data-mode={state.mode} />;
  },
}));
// The terminal view needs a real browser; a project's shells are only listed as tabs here.
vi.mock("./terminal/ShellTerminalView", () => ({ ShellTerminalView: () => null }));
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    mount() {}
    dispose() {}
    fit() {}
    focus() {}
    attach() {}
    setColorTheme() {}
    paste() {}
    syncSize() {}
    setInputLabel() {}
    detach() {}
  },
}));

function mountApp(narrow = false, clipboard?: { writeText: (text: string) => Promise<void> }, withSession = false, links?: PlatformAdapter["links"]) {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: !narrow && media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  document.documentElement.style.setProperty("--breakpoint-phone", "782px");
  onTestFinished(() => {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-docked");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
  const console_ = consoleOf("c-actions", "Main");
  const projects = [projectOf("p-actions", console_.id, "Website"), projectOf("p-other", console_.id, "API")];
  const sessions = withSession ? [sessionOf("s-archive", console_.id, "p-actions", "Fix bug", "idle")] : [];
  const daemon: Daemon = {
    store: createStateStore(snapshotState({ consoles: [console_], projects, sessions })),
    request: vi.fn(async () => ({ type: "ack", id: null })) as Daemon["request"],
    toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {},
    dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", clipboard, links }}>
      <ThemeProvider><DaemonProvider value={daemon}><App /><ErrorAlerts daemon={daemon} /></DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
  return { container, daemon, project: projects[0] };
}

const controlName = (element: Element) => element.getAttribute("aria-label")
  ?? element.getAttribute("aria-labelledby")?.split(/\s+/).map((id) => document.getElementById(id)?.textContent ?? "").join(" ")
  ?? (element.matches("[role=menuitem], [role=menuitemradio]") ? element.querySelector('[data-slot="label"]')?.textContent : undefined)
  ?? element.textContent;

async function press(label: string) {
  const target = Array.from(document.querySelectorAll<HTMLElement>("button, [role=menuitem]")).find((element) =>
    controlName(element) === label,
  );
  expect(target, label).toBeDefined();
  await act(async () => {
    if (target!.getAttribute("role") === "menuitem" && target!.hasAttribute("aria-haspopup")) {
      target!.focus();
      target!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    } else {
      target!.click();
    }
  });
}

const menuHas = (label: string) => Array.from(document.querySelectorAll("[role=menuitem]")).some((item) => controlName(item) === label);
const openProjectMenu = (name: string) => press(`Actions for project ${isolate(name)}`);
const openPanel = async (label: string) => {
  await press("Open panel");
  await press(label);
};

it.each([false, true])("reopens a hidden project's Files pane when narrow=%s", async (narrow) => {
  const { container, project } = mountApp(narrow);
  if (narrow) await press("Show sessions");
  await openProjectMenu(project.name);
  await openPanel("Files");
  expect(container.querySelector(`[data-browser="${project.id}"]`)?.getAttribute("data-mode")).toBe("files");
  expect(container.querySelector('[data-region="topbar"] [aria-label="Hide files panel"]')).not.toBeNull();
  if (!narrow) {
    await openProjectMenu(project.name);
    await press("Open panel");
    expect(menuHas("Files")).toBe(false);
    await press("Open in");
    await press("File manager");
    await openProjectMenu("API");
    await press("Open panel");
    expect(menuHas("Files")).toBe(true);
    await press("Open in");
    await press("File manager");
  }
  await press("Hide files panel");
  if (narrow) await press("Show sessions");
  await act(async () => setProjectBrowserMode(project.id, "git"));
  await openProjectMenu(project.name);
  await press("Open panel");
  expect(menuHas("Files")).toBe(true);
  await press("Files");
  expect(container.querySelector(`[data-browser="${project.id}"]`)?.getAttribute("data-mode")).toBe("files");
  expect(container.querySelector('[data-region="topbar"] [aria-label="Hide files panel"]')).not.toBeNull();
});

const toggle = (label: string) => document.querySelector<HTMLElement>(`[data-region="topbar"] [aria-label="${label}"]`);
const asidePane = () => document.querySelector<HTMLElement>("[data-pane=aside]")!;

it.each([false, true])("has no toggle for the pane itself, only a panel's, which hides and shows it; narrow=%s", async (narrow) => {
  const { project } = mountApp(narrow);
  if (narrow) await press("Show sessions");
  await openProjectMenu(project.name);
  await openPanel("Files");
  expect(document.querySelector('[data-region="topbar"] [aria-label$="project pane"]')).toBeNull();
  expect(toggle("Hide files panel")!.getAttribute("aria-expanded")).toBe("true");
  // Not the fixed float of a hover: docked as a row sibling, or, in a window too narrow to dock it,
  // over the terminal's area with no width handle and with the terminal keeping the whole width.
  expect(asidePane().classList.contains("fixed")).toBe(false);
  expect(asidePane().classList.contains("absolute")).toBe(narrow);
  expect(document.querySelector("[role=separator][data-pane=aside]") === null).toBe(narrow);
  expect(asidePane().previousElementSibling!.classList.contains("min-w-0")).toBe(narrow);
  await press("Hide files panel");
  expect(toggle("Show files panel")!.getAttribute("aria-expanded")).toBe("false");
  await press("Show files panel");
  expect(toggle("Hide files panel")).not.toBeNull();
});

it("floats the hidden pane in while the pointer rests on a toggle, and docks it when the toggle is pressed", async () => {
  vi.useFakeTimers();
  document.documentElement.style.setProperty("--peek-gap", "8px");
  onTestFinished(() => {
    vi.useRealTimers();
    document.documentElement.style.removeProperty("--peek-gap");
  });
  const { container, project } = mountApp();
  await openProjectMenu(project.name);
  await openPanel("Files");
  await press("Hide files panel");
  // It goes at once, with no ease out.
  expect(container.querySelector("[data-browser]")).toBeNull();
  const button = toggle("Show files panel")!;
  const pointer = (type: string) => new PointerEvent(type, { pointerType: "mouse", bubbles: type !== "pointerenter" });
  await act(async () => {
    button.dispatchEvent(pointer("pointerover"));
    button.dispatchEvent(pointer("pointerenter"));
    await vi.advanceTimersByTimeAsync(300);
  });
  expect(container.querySelector(`[data-browser="${project.id}"]`)).not.toBeNull();
  expect(asidePane().classList.contains("fixed")).toBe(true);
  await press("Show files panel");
  expect(asidePane().classList.contains("fixed")).toBe(false);
  expect(toggle("Hide files panel")).not.toBeNull();
});

it("hands focus on the pane's width handle to the terminal when the pane goes", async () => {
  const { project } = mountApp();
  await openProjectMenu(project.name);
  await openPanel("Files");
  const handle = document.querySelector<HTMLElement>("[role=separator][data-pane=aside]")!;
  handle.focus();
  expect(document.activeElement).toBe(handle);
  await press("Hide files panel");
  expect(document.querySelector("[role=separator][data-pane=aside]")).toBeNull();
  expect(document.activeElement).not.toBe(document.body);
});

it("sends the project directory request and reports a failure through the shared error channel", async () => {
  const { daemon, project } = mountApp();
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("The system file manager could not be opened."));
  await openProjectMenu(project.name);
  await press("Open in");
  await press("File manager");
  expect(daemon.request).toHaveBeenCalledWith({ type: "open_project_directory", project: project.id });
  expect(daemon.toastError).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ message: "The system file manager could not be opened." }), undefined, "Could not open in the file manager");
});

it.each([false, true])("opens the project's Terminal panel from Open panel, starting a shell only while it has none; narrow=%s", async (narrow) => {
  const { daemon, project } = mountApp(narrow);
  if (narrow) await press("Show sessions");
  await openProjectMenu(project.name);
  await openPanel("Terminal");
  const creates = () => vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === "create_shell_terminal");
  expect(creates()).toEqual([[{ type: "create_shell_terminal", project: project.id }]]);
  expect(toggle("Hide terminal panel")).not.toBeNull();
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "open_project_terminal" }));
});

it.each([false, true])("opens the project's Processes panel from Open panel, listing every session's processes even with one selected; narrow=%s", async (narrow) => {
  const { daemon, project } = mountApp(narrow, undefined, true);
  const tree = (id: string, command: string, session_id: string | undefined) => ({ id, session_id, command, cwd: "/p", cwd_missing: false, kept: session_id === undefined, started_at: Date.now(), process_count: 1, memory_bytes: 0, ports: [] });
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "list_processes" ? {
    type: "processes_listed",
    id: null,
    trees: [tree("t1", "vite of the selected", "s-archive"), tree("t2", "vite of another", "s-other"), tree("t3", "vite of the project", undefined)],
  } : { type: "ack", id: null });
  if (narrow) await press("Show sessions");
  const row = Array.from(document.querySelectorAll<HTMLElement>("[aria-label]")).find((element) => element.getAttribute("aria-label")!.includes("Fix bug") && !element.getAttribute("aria-label")!.startsWith("Actions for"));
  expect(row, "the session's row").toBeDefined();
  await act(async () => row!.click());
  await openProjectMenu(project.name);
  await openPanel("Processes");
  expect(toggle("Hide processes panel")).not.toBeNull();
  expect(vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === "list_processes").at(-1)).toEqual([{ type: "list_processes", project_id: project.id }]);
  expect(document.body.textContent).toContain("All sessions");
  for (const command of ["vite of the selected", "vite of another", "vite of the project"]) expect(document.body.textContent).toContain(command);
  expect(vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === "create_shell_terminal")).toEqual([]);
});

it("starts no shell from Open panel when the project already has one", async () => {
  const { daemon, project } = mountApp();
  await act(async () => daemon.store.setState({ shellTerminals: new Map([["t1", { id: "t1", project_id: project.id, worktree: null, cwd: "/p", title: "zsh", renamed: false, state: "running", busy: false, created_at: 1 }]]) }));
  await openProjectMenu(project.name);
  await openPanel("Terminal");
  expect(vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === "create_shell_terminal")).toEqual([]);
  expect(toggle("Hide terminal panel")).not.toBeNull();
});

it("sends the system terminal request and reports a failure through the shared error channel", async () => {
  const { daemon, project } = mountApp();
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("The system terminal could not be opened."));
  await openProjectMenu(project.name);
  await press("Open in");
  await press("System terminal");
  expect(daemon.request).toHaveBeenCalledWith({ type: "open_project_terminal", project: project.id });
  expect(daemon.toastError).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ message: "The system terminal could not be opened." }), undefined, "Could not open in the system terminal");
});

it("copies the project name unchanged and reports success", async () => {
  const clipboard = { writeText: vi.fn(async () => {}) };
  const { daemon, project } = mountApp(false, clipboard);
  await openProjectMenu(project.name);
  await press("Copy project name");
  expect(clipboard.writeText).toHaveBeenCalledExactlyOnceWith(project.name);
  expect(daemon.toastNotice).toHaveBeenCalledWith("Project name copied");
});

it("reports a clipboard failure", async () => {
  const clipboard = { writeText: vi.fn(async () => { throw new Error("Denied"); }) };
  const { daemon, project } = mountApp(false, clipboard);
  await openProjectMenu(project.name);
  await press("Copy project name");
  expect(daemon.toastError).toHaveBeenCalledWith(expect.objectContaining({ message: "Could not copy the project name", recovery: "clipboard" }), undefined, "Could not copy");
  expect(daemon.toastNotice).not.toHaveBeenCalled();
});

it("copies the session ID unchanged and reports success", async () => {
  const clipboard = { writeText: vi.fn(async () => {}) };
  const { daemon } = mountApp(false, clipboard, true);
  await press(`Actions for session ${isolate("Fix bug")}`);
  const copyItem = document.querySelector('[role=menuitem][aria-label="Copy session ID"]')!;
  expect(copyItem.querySelector('[dir="ltr"]')?.textContent).toBe("s-archive");
  expect(document.getElementById(copyItem.getAttribute("aria-describedby")!)?.textContent).toBe("s-archive");
  await press("Copy session ID");
  expect(clipboard.writeText).toHaveBeenCalledExactlyOnceWith("s-archive");
  expect(daemon.toastNotice).toHaveBeenCalledWith("Session ID copied");
});

it("reports a session ID clipboard failure", async () => {
  const clipboard = { writeText: vi.fn(async () => { throw new Error("Denied"); }) };
  const { daemon } = mountApp(false, clipboard, true);
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Copy session ID");
  expect(daemon.toastError).toHaveBeenCalledWith(expect.objectContaining({ message: "Could not copy the session ID", recovery: "clipboard" }), undefined, "Could not copy");
  expect(daemon.toastNotice).not.toHaveBeenCalled();
});

async function openRestoreConfirmation() {
  const mounted = mountApp();
  const { daemon, project } = mounted;
  await act(async () => daemon.store.setState((state) => ({
    gitStatuses: new Map(state.gitStatuses).set(project.id, {
      project: project.id, repository: true, branch: "main", detached: false, upstream: null,
      ahead: 0, behind: 0, activity: "idle", error: null,
    }),
  })));
  await openProjectMenu(project.name);
  await press("Git");
  await press("Restore file changes");
  return mounted;
}

it("requires confirmation to restore file changes and permits cancelling", async () => {
  const { daemon } = await openRestoreConfirmation();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Staged new files will be deleted");
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "restore_project_changes" }));
  await press("Cancel");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "restore_project_changes" }));
});

it("restores only the confirmed project and reports success", async () => {
  const { daemon, project } = await openRestoreConfirmation();
  const dialog = document.querySelector('[role="alertdialog"]')!;
  await act(async () => dialog.querySelector("form")!.requestSubmit());
  expect(daemon.request).toHaveBeenCalledWith({ type: "restore_project_changes", project: project.id });
  expect(daemon.toastNotice).toHaveBeenCalledWith("File changes restored");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("says a truncated branch list is limited, as a muted note after the branches and not a failure", async () => {
  const { daemon, project } = mountApp();
  await act(async () => daemon.store.setState((state) => ({
    gitStatuses: new Map(state.gitStatuses).set(project.id, {
      project: project.id, repository: true, branch: "main", detached: false, upstream: null,
      ahead: 0, behind: 0, activity: "idle", error: null,
    }),
  })));
  vi.mocked(daemon.request).mockImplementation((async (body: { type: string }) => body.type === "list_project_branches"
    ? { type: "project_branches", id: null, project: project.id, branches: [{ name: "alpha" }, { name: "beta" }], complete: false }
    : { type: "ack", id: null }) as never);
  await openProjectMenu(project.name);
  await press("Git");
  await press("Switch branch");
  const notice = Array.from(document.querySelectorAll("[role=menuitem]")).find((item) => controlName(item).startsWith("There are more branches"))!;
  expect(controlName(notice)).toBe("There are more branches than this list can show. The others are not listed.");
  expect(notice.getAttribute("aria-disabled")).toBe("true");
  // The last item, with no glyph of the branches' kind.
  expect(notice.querySelector("svg")).toBeNull();
  expect([...notice.closest("[role=menu]")!.querySelectorAll("[role^=menuitem]")].at(-1)).toBe(notice);
  expect(menuHas("Some branches could not be listed")).toBe(false);
});

it("keeps a native operation rejection string as the error diagnostic", async () => {
  const { daemon } = mountApp();
  vi.mocked(daemon.request).mockRejectedValueOnce("Native file manager rejected");
  await openProjectMenu("Website");
  await press("Open in");
  await press("File manager");
  expect(daemon.toastError).toHaveBeenCalledWith(expect.objectContaining({ message: "Native file manager rejected", cause: "Native file manager rejected" }), undefined, "Could not open in the file manager");
});

it("keeps the restore confirmation open and displays a refusal", async () => {
  const { daemon } = await openRestoreConfirmation();
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("Finish the merge first."));
  await act(async () => document.querySelector('[role="alertdialog"] form')!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect([...document.querySelectorAll('[role="alertdialog"]')].some((dialog) => dialog.textContent?.includes("Finish the merge first."))).toBe(true);
  expect(daemon.toastNotice).not.toHaveBeenCalled();
});

it("hands focus on once an unselected session archived from its menu reads archived", async () => {
  const { daemon } = mountApp(false, undefined, true);
  await press(`Actions for session ${isolate("Fix bug")}`);
  vi.mocked(reactAria.setInteractionModality).mockClear();
  await press("Archive");
  expect(daemon.request).toHaveBeenCalledWith({ type: "archive_session", session: "s-archive" });
  expect(reactAria.setInteractionModality).not.toHaveBeenCalled();
  (document.activeElement as HTMLElement | null)?.blur();
  await act(async () => daemon.store.setState((state) => ({
    sessions: new Map(state.sessions).set("s-archive", { ...state.sessions.get("s-archive")!, status: "archived" }),
  })));
  expect(reactAria.setInteractionModality).toHaveBeenCalledWith("keyboard");
});

/** Mounts the app with the idle session `s-archive`, answering its branch inspection only once
 * `answer` is called, with `items`. */
function mountWithPendingInspection(items: unknown[] = []) {
  const mounted = mountApp(false, undefined, true);
  let answer!: () => void;
  const answered = new Promise<void>((done) => { answer = done; });
  vi.mocked(mounted.daemon.request).mockImplementation(async (body) => {
    if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
    await answered;
    return { type: "session_branches_inspected", id: null, items } as never;
  });
  return { ...mounted, answer: () => act(async () => answer()) };
}

const archiveFromMenu = async () => {
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Archive");
};
const sentOf = (daemon: Daemon, type: string) => vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === type);

it("ignores a repeated archive press while the first one's inspection is out", async () => {
  const { daemon, answer } = mountWithPendingInspection();
  await archiveFromMenu();
  await archiveFromMenu();
  await answer();
  expect(sentOf(daemon, "inspect_session_branches")).toHaveLength(1);
  expect(sentOf(daemon, "archive_session")).toHaveLength(1);
});

it("asks after all when the session turned busy while its inspection was out", async () => {
  const { daemon, answer } = mountWithPendingInspection();
  await archiveFromMenu();
  await act(async () => daemon.store.setState((state) => ({
    sessions: new Map(state.sessions).set("s-archive", { ...state.sessions.get("s-archive")!, status: "working" }),
  })));
  await answer();
  expect(sentOf(daemon, "archive_session")).toHaveLength(0);
  expect(document.querySelector('[role="alertdialog"]')).not.toBeNull();
});

it("asks after all when a session was bound to it while its inspection was out", async () => {
  const { daemon, answer } = mountWithPendingInspection();
  await archiveFromMenu();
  await act(async () => daemon.store.setState((state) => ({
    sessions: new Map(state.sessions).set("s-bound", sessionOf("s-bound", "c-actions", "p-actions", "Bound", "working", { bound_to: "s-archive" })),
  })));
  await answer();
  expect(sentOf(daemon, "archive_session")).toHaveLength(0);
  expect(document.querySelector('[role="alertdialog"]')).not.toBeNull();
});

it("reinspects the expanded archive scope when a child binds during the preflight, holding confirmation until its choices are ready", async () => {
  const { daemon } = mountApp(false, undefined, true);
  let answerOriginal!: () => void;
  let answerExpanded!: () => void;
  const original = new Promise<void>((resolve) => { answerOriginal = resolve; });
  const expanded = new Promise<void>((resolve) => { answerExpanded = resolve; });
  vi.mocked(daemon.request).mockImplementation(async (body) => {
    if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
    if (body.sessions.length === 1) {
      await original;
      return { type: "session_branches_inspected", id: null, items: [], processes: [TREE] };
    }
    await expanded;
    return {
      type: "session_branches_inspected",
      id: null,
      items: [{
        kind: "branch", repository: "/r/.git", branch: "child-feature", path: null, project: "p-actions",
        session: "s-bound", state: "unmerged", uncommitted: false, detail: null,
      }],
      processes: [TREE, { ...TREE, id: "4300:2", session_id: "s-bound", command: "node child-server.js" }],
    };
  });
  await archiveFromMenu();
  await act(async () => daemon.store.setState((state) => ({
    sessions: new Map(state.sessions).set("s-bound", sessionOf("s-bound", "c-actions", "p-actions", "Bound", "working", { bound_to: "s-archive" })),
  })));
  await act(async () => answerOriginal());
  expect(sentOf(daemon, "inspect_session_branches").map(([body]) => body)).toEqual([
    { type: "inspect_session_branches", sessions: ["s-archive"], processes: true },
    { type: "inspect_session_branches", sessions: ["s-archive", "s-bound"], processes: true },
  ]);
  const dialog = document.querySelector('[role="alertdialog"]')!;
  expect(dialog.querySelector('[data-pending="true"]')).not.toBeNull();
  await act(async () => dialog.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(sentOf(daemon, "archive_session")).toHaveLength(0);
  await act(async () => answerExpanded());
  expect(dialog.querySelector('[data-pending="true"]')).toBeNull();
  expect(dialog.textContent).toContain("child-feature");
  expect(dialog.textContent).toContain("node child-server.js");
  const boxes = [...dialog.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
  expect(boxes.map((box) => box.checked)).toEqual([false, true, true]);
  await act(async () => boxes[2].click());
  await act(async () => dialog.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(daemon.request).toHaveBeenCalledWith({
    type: "archive_session", session: "s-archive", keep_process_ids: ["4300:2"],
    keep: [{ kind: "branch", repository: "/r/.git", branch: "child-feature", path: null, state: "unmerged", uncommitted: false }],
  });
});

it("leaves a dialog opened while the inspection was out in place", async () => {
  const { daemon, answer } = mountWithPendingInspection([{
    kind: "branch", repository: "/r/.git", branch: "feature", path: null, project: "p-actions",
    session: "s-archive", state: "unmerged", uncommitted: false, detail: null,
  }]);
  await archiveFromMenu();
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Rename");
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  await answer();
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(sentOf(daemon, "archive_session")).toHaveLength(0);
});

it("archives an idle session at once when what is bound to it cannot be inspected", async () => {
  const { daemon } = mountApp(false, undefined, true);
  vi.mocked(daemon.request).mockImplementation(async (body) => {
    if (body.type === "inspect_session_branches") throw new Error("Connection lost");
    return { type: "ack", id: null };
  });
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Archive");
  expect(daemon.request).toHaveBeenCalledWith({ type: "archive_session", session: "s-archive" });
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("asks before archiving an idle session that has a bound worktree git refuses to remove", async () => {
  const { daemon } = mountApp(false, undefined, true);
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "inspect_session_branches"
    ? {
      type: "session_branches_inspected",
      id: null,
      items: [{
        kind: "worktree", repository: "/r/.git", branch: "feature", path: "/r-feature", project: "p-actions",
        session: "s-archive", state: "uncommitted_changes", uncommitted: true, detail: null,
      }],
    }
    : { type: "ack", id: null });
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Archive");
  expect(daemon.request).toHaveBeenCalledWith({ type: "inspect_session_branches", sessions: ["s-archive"], processes: true });
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "archive_session" }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("has uncommitted changes");
});

/** A process tree the archive's inspection found, running for a while. */
const TREE = {
  id: "4242:1", session_id: "s-archive", command: "npm run dev", cwd: "/r-feature", cwd_missing: false, kept: false,
  started_at: 1_000, process_count: 3, memory_bytes: 120_000_000, ports: [5173],
};

it("asks before archiving an idle session that has left a process running, and sends the ones kept", async () => {
  const { daemon } = mountApp(false, undefined, true);
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "inspect_session_branches"
    ? { type: "session_branches_inspected", id: null, items: [], processes: [TREE, { ...TREE, id: "4300:2", command: "tail -f log" }] }
    : { type: "ack", id: null });
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Archive");
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "archive_session" }));
  const dialog = document.querySelector('[role="alertdialog"]')!;
  expect(dialog.textContent).toContain("npm run dev");
  expect(dialog.textContent).toContain("2 processes are stopped.");
  const boxes = [...dialog.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
  expect(boxes.map((box) => box.checked)).toEqual([true, true]);
  await act(async () => boxes[0].click());
  expect(dialog.textContent).toContain("1 process is stopped.");
  await act(async () => dialog.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(daemon.request).toHaveBeenCalledWith({ type: "archive_session", session: "s-archive", keep_process_ids: ["4242:1"] });
});

it("inspects again for the confirmation the session's turning busy opened over a failed inspection, rather than reusing it", async () => {
  const { daemon } = mountApp(false, undefined, true);
  let inspections = 0;
  let fail!: () => void;
  const failed = new Promise<void>((_, reject) => { fail = () => reject(new Error("Connection lost")); });
  failed.catch(() => {});
  vi.mocked(daemon.request).mockImplementation(async (body) => {
    if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
    if (++inspections === 1) await failed;
    return { type: "session_branches_inspected", id: null, items: [], processes: [TREE] } as never;
  });
  await archiveFromMenu();
  await act(async () => daemon.store.setState((state) => ({
    sessions: new Map(state.sessions).set("s-archive", { ...state.sessions.get("s-archive")!, status: "working" }),
  })));
  await act(async () => fail());
  expect(sentOf(daemon, "inspect_session_branches")).toHaveLength(2);
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("npm run dev");
});

it("archives an idle session at once when no process it left running has run long enough to ask about", async () => {
  const { daemon } = mountApp(false, undefined, true);
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "inspect_session_branches"
    ? { type: "session_branches_inspected", id: null, items: [] }
    : { type: "ack", id: null });
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Archive");
  expect(daemon.request).toHaveBeenCalledWith({ type: "archive_session", session: "s-archive" });
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("opens the project repository through the platform and reports an opening failure", async () => {
  const links = { open: vi.fn(async () => {}) };
  const { daemon, project } = mountApp(false, undefined, false, links);
  await act(async () => daemon.store.setState((state) => ({
    projects: new Map(state.projects).set(project.id, { ...project, remote_url: "git@gitlab.com:team/app.git" }),
  })));
  await openProjectMenu(project.name);
  await press("Open in");
  await press("GitLab");
  expect(links.open).toHaveBeenCalledExactlyOnceWith("https://gitlab.com/team/app");
  vi.mocked(links.open).mockRejectedValueOnce(new Error("Browser unavailable"));
  await openProjectMenu(project.name);
  await press("Open in");
  await press("GitLab");
  expect(errorAlerts.getSnapshot().some((failure) => failure.message === "The link did not open in a browser. Its address is shown below.")).toBe(true);
  expect(document.querySelector('[role="alertdialog"] p[dir=ltr]')?.textContent).toBe("https://gitlab.com/team/app");
});

/** Opens Change model on the idle session `s-archive`, which the daemon lists two models for, and picks Sonnet. */
async function pickSonnetInChangeModel(answer: (body: { type: string }) => unknown) {
  const mounted = mountApp(false, undefined, true);
  vi.mocked(mounted.daemon.request).mockImplementation((async (body: { type: string; agent?: string }) => {
    if (body.type === "list_agent_models") {
      return { type: "agent_models", id: null, agent: body.agent, account: null, unavailable: null, models: [
        { id: "opus", name: "Opus", efforts: ["low", "high"] },
        { id: "sonnet", name: "Sonnet", efforts: ["low", "high"] },
      ] };
    }
    return answer(body);
  }) as Daemon["request"]);
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Change model");
  const model = Array.from(document.querySelectorAll<HTMLButtonElement>("button")).find((button) =>
    button.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.textContent === "Model"))!;
  await act(async () => model.click());
  await act(async () => Array.from(document.querySelectorAll<HTMLElement>('[role="option"]')).find((option) => option.textContent === "Sonnet")!.click());
  // From here only the announcement's waits are timed by the test.
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  onTestFinished(() => { vi.useRealTimers(); });
  await act(async () => Array.from(document.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "Change model")!.click());
  return mounted;
}

const said = () => document.querySelector('[role="status"]:not(:empty)')?.textContent;
const passTime = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

it("sends only the edited values when a model is changed, and announces the restart", async () => {
  windowAttention();
  const { daemon } = await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "restarted", continued: true }));
  expect(sentOf(daemon, "set_session_model")).toEqual([[
    { type: "set_session_model", session: "s-archive", model: "sonnet", terminal_appearance: expect.stringMatching(/^(light|dark)$/) },
  ]]);
  expect(said()).toBeUndefined();
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS + 100);
  expect(said()).toContain("restarted with its new model settings");
  expect(daemon.toastError).not.toHaveBeenCalled();
});

it("announces values recorded for the next resume after the focus move", async () => {
  windowAttention();
  await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS - 100);
  expect(said()).toBeUndefined();
  await passTime(200);
  expect(said()).toContain("will use its new model settings when it is resumed");
});

it("names the session by its title at the time of the reply", async () => {
  windowAttention();
  let reply!: () => void;
  const { daemon } = await pickSonnetInChangeModel(() => new Promise((resolve) => {
    reply = () => resolve({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" });
  }));
  await act(async () => daemon.store.setState((state) => ({
    sessions: new Map(state.sessions).set("s-archive", { ...state.sessions.get("s-archive")!, title: "Renamed" }),
  })));
  await act(async () => reply());
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS + 100);
  expect(said()).toContain("Renamed");
});

it("holds an announcement while the window is not attended, then says it after the focus-move wait", async () => {
  const attend = windowAttention();
  attend(false);
  await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(said()).toBeUndefined();
  attend(true);
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS - 100);
  expect(said()).toBeUndefined();
  await passTime(200);
  expect(said()).toContain("will use its new model settings when it is resumed");
});

it("holds an announcement while a modal is open over the window", async () => {
  windowAttention();
  const { daemon } = await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" }));
  await act(async () => errorAlerts.report({ message: "Something else failed.", recovery: "none" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(said()).toBeUndefined();
  await act(async () => errorAlerts.acknowledge());
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS + 100);
  expect(said()).toContain("will use its new model settings when it is resumed");
  expect(daemon.toastError).not.toHaveBeenCalled();
});

it("announces nothing when the session had the model settings already", async () => {
  windowAttention();
  const { daemon } = await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "unchanged" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS + 100);
  expect(document.querySelector('[role="status"]:not(:empty)')).toBeNull();
  expect(daemon.toastError).not.toHaveBeenCalled();
});

it("points a launch setting the agent does not offer back to Change model", async () => {
  const error = new DaemonRequestError("No.", "invalid_launch_setting", { field: "model", value: "sonnet", agent: "Codex" });
  const { daemon } = await pickSonnetInChangeModel((body) => {
    if (body.type === "set_session_model") throw error;
    return { type: "ack", id: null };
  });
  expect(daemon.toastError).toHaveBeenCalledExactlyOnceWith(error, "s-archive", "Could not change the model", "changeModelSetting");
});

it("opens Change model for the session from the alert of a launch setting the agent does not offer", async () => {
  const { daemon } = mountApp(false, undefined, true);
  await act(async () => errorAlerts.report({
    message: "No.", session: "s-archive", title: "Could not change the model", recovery: "changeModelSetting",
  }));
  const open = () => Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "Change model")!;
  await act(async () => open().click());
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Change model");
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("The session Fix bug will run");
  expect(daemon.toastError).not.toHaveBeenCalled();
});

it("reports a refused model change under its own title for the session", async () => {
  const error = new DaemonRequestError("Did not stop.", "model_change_did_not_stop", { session: "s-archive" });
  const { daemon } = await pickSonnetInChangeModel((body) => {
    if (body.type === "set_session_model") throw error;
    return { type: "ack", id: null };
  });
  expect(daemon.toastError).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ code: "model_change_did_not_stop" }), "s-archive", "Could not change the model", undefined);
  expect(document.querySelector('[role="status"]:not(:empty)')).toBeNull();
});

it("keeps a reply unheard and unexpired while a FileViewer covers the status region", async () => {
  windowAttention();
  let reply!: () => void;
  const { daemon, container: app } = await pickSonnetInChangeModel(() => new Promise((resolve) => {
    reply = () => resolve({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" });
  }));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const close = () => root.render(null);
  await act(async () => root.render(
    <PlatformProvider value={{ kind: "browser" }}><ThemeProvider><DaemonProvider value={daemon}>
      <FileViewer subject={{ key: "file", path: "test.bin", content: { state: "file", body: { kind: "binary", size: 1 } } }} onClose={close} />
    </DaemonProvider></ThemeProvider></PlatformProvider>,
  ));
  const status = Array.from(app.querySelectorAll('[role="status"]')).at(-1)!;
  expect(status.closest('[inert], [aria-hidden="true"]')).not.toBeNull();
  await act(async () => reply());
  await passTime(20_000);
  expect(status.textContent).toBe("");
  await act(async () => close());
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS - 100);
  expect(status.textContent).toBe("");
  await passTime(200);
  expect(status.textContent).toContain("will use its new model settings when it is resumed");
});
