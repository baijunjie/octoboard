// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import * as reactAria from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "./errors/codedError";
import { ErrorAlerts } from "./shell/ErrorAlerts";
import { errorAlerts } from "./components/alerts/errorAlerts";
import { App } from "./App";
import { stubWindow } from "./appTestSupport";
import { PANE_INSET_MS } from "./layout/paneDockMotion";
import { setProjectBrowserMode } from "./project-browser/browserPreferences";
import { useProjectBrowserState } from "./project-browser/browserState";
import { AFTER_FOCUS_MOVE_DELAY_MS, FIRST_WRITE_DELAY_MS } from "./components/announce/StatusAnnouncer";
import { ANNOUNCEMENT_MS } from "./app/useAnnouncement";
import { mountForTooltip } from "./components/hint/tooltipTestSupport";
import { windowAttention } from "./components/announce/windowTestSupport";
import { accountOf, consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { isolate } from "./i18n/isolate";
import { PlatformProvider } from "./platform/react";
import type { PlatformAdapter } from "./platform";
import type { Project, Session } from "./protocol";
import { DaemonProvider, type Daemon } from "./store";
import { testDaemon, withEmptyReplies } from "./test/testDaemon";
import { ThemeProvider } from "./preferences/theme";
import { FileViewer } from "./viewer/FileViewer";
import { BROWSER_HOST } from "./test/hosts";
import { fakeTimers } from "./test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("react-aria", async (importOriginal) => {
  const actual = await importOriginal<typeof import("react-aria")>();
  return { ...actual, setInteractionModality: vi.fn(actual.setInteractionModality) };
});
vi.mock("./project-browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => {
    const state = useProjectBrowserState(project.id);
    return <div data-browser={project.id} data-mode={state.mode} />;
  },
}));
// The terminal view needs a real browser; a project's shells are only listed as tabs here.
vi.mock("./terminal/shell/ShellTerminalView", () => ({ ShellTerminalView: () => null }));
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
    setFontSize() {}
    detach() {}
  },
}));

function mountApp(narrow = false, clipboard?: { writeText: (text: string) => Promise<void> }, withSession = false, links?: PlatformAdapter["links"]) {
  stubWindow(narrow ? 390 : 1400);
  const console_ = consoleOf("c-actions", "Main");
  const projects = [projectOf("p-actions", console_.id, "Website"), projectOf("p-other", console_.id, "API")];
  const sessions = withSession ? [sessionOf("s-archive", console_.id, "p-actions", "Fix bug", "idle")] : [];
  const daemon = testDaemon({
    state: snapshotState({ consoles: [console_], projects, sessions }),
    request: vi.fn(withEmptyReplies(async () => ({ type: "ack", id: null }))) as never,
    showNotice: vi.fn(),
  });
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST, clipboard, links }}>
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

it("keeps a closed docked panel hidden on hover and opens it when its toggle is pressed", async () => {
  fakeTimers();
  document.documentElement.style.setProperty("--peek-gap", "8px");
  onTestFinished(() => {
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
  });
  await act(async () => { await vi.advanceTimersByTimeAsync(PANE_INSET_MS); });
  expect(container.querySelector(`[data-browser="${project.id}"]`)).toBeNull();
  await press("Show files panel");
  expect(container.querySelector(`[data-browser="${project.id}"]`)).not.toBeNull();
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
  await openProjectMenu(project.name);
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("The system file manager could not be opened."));
  await press("Open in");
  await press("File manager");
  expect(daemon.request).toHaveBeenCalledWith({ type: "open_project_directory", project: project.id });
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ message: "The system file manager could not be opened.", title: "Could not open in the file manager" })]);
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
  const tree = (id: string, command: string, session_id: string | undefined) => ({ id, session_id, command, cwd: "/p", cwd_missing: false, kept: session_id === undefined, keepable: true, started_at: Date.now(), process_count: 1, memory_bytes: 0, ports: [] });
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
  const panel = document.getElementById("panel-processes")!;
  expect(panel.querySelector('button[aria-label="Filter by session"]')).not.toBeNull();
  expect(panel.querySelector('button[aria-haspopup="listbox"]')).toBeNull();
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
  await openProjectMenu(project.name);
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("The system terminal could not be opened."));
  await press("Open in");
  await press("System terminal");
  expect(daemon.request).toHaveBeenCalledWith({ type: "open_project_terminal", project: project.id });
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ message: "The system terminal could not be opened.", title: "Could not open in the system terminal" })]);
});

it("copies the project name unchanged and reports success", async () => {
  const clipboard = { writeText: vi.fn(async () => {}) };
  const { daemon, project } = mountApp(false, clipboard);
  await openProjectMenu(project.name);
  await press("Copy project name");
  expect(clipboard.writeText).toHaveBeenCalledExactlyOnceWith(project.name);
  expect(daemon.showNotice).toHaveBeenCalledWith("Project name copied");
});

it("reports a clipboard failure", async () => {
  const clipboard = { writeText: vi.fn(async () => { throw new Error("Denied"); }) };
  const { daemon, project } = mountApp(false, clipboard);
  await openProjectMenu(project.name);
  await press("Copy project name");
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not copy the project name", message: "The clipboard could not be written to.", classification: expect.objectContaining({ kind: "actionable", recovery: "clipboardRetry" }) })]);
  expect(daemon.showNotice).not.toHaveBeenCalled();
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
  expect(daemon.showNotice).toHaveBeenCalledWith("Session ID copied");
});

it("reports a session ID clipboard failure", async () => {
  const clipboard = { writeText: vi.fn(async () => { throw new Error("Denied"); }) };
  const { daemon } = mountApp(false, clipboard, true);
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Copy session ID");
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not copy the session ID", message: "The clipboard could not be written to.", classification: expect.objectContaining({ kind: "actionable", recovery: "clipboardRetry" }) })]);
  expect(daemon.showNotice).not.toHaveBeenCalled();
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
  expect(daemon.showNotice).toHaveBeenCalledWith("File changes restored");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

async function openPullCommand(outcome: unknown) {
  const mounted = mountApp();
  const { daemon, project } = mounted;
  vi.mocked(daemon.request).mockImplementation((async (body: { type: string; merge?: boolean }) => {
    if (body.type !== "pull_project_branch") return { type: "ack", id: null };
    return { type: "project_branch_pulled", id: null, outcome: body.merge ? { kind: "merged" } : outcome };
  }) as never);
  await act(async () => daemon.store.setState((state) => ({
    gitStatuses: new Map(state.gitStatuses).set(project.id, {
      project: project.id, repository: true, branch: "main", detached: false, upstream: "origin/main",
      ahead: 2, behind: 3, activity: "idle", error: null,
    }),
  })));
  await openProjectMenu(project.name);
  await press("Git");
  await press("Pull current branch");
  return mounted;
}

it("reports a manual pull that found nothing to pull", async () => {
  const { daemon, project } = await openPullCommand({ kind: "up_to_date" });
  expect(daemon.request).toHaveBeenCalledWith({ type: "pull_project_branch", project: project.id });
  expect(daemon.showNotice).toHaveBeenCalledWith("The current branch is already up to date.");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("asks before a pull creates a merge commit, and merges only the confirmed project", async () => {
  const { daemon, project } = await openPullCommand({ kind: "needs_merge", ahead: 2, behind: 3 });
  const dialog = document.querySelector('[role="alertdialog"]')!;
  expect(dialog.textContent).toContain("has diverged from its upstream: 2 commits ahead and 3 commits behind.");
  expect(dialog.textContent).toContain("creates a merge commit");
  expect(daemon.request).not.toHaveBeenCalledWith({ type: "pull_project_branch", project: project.id, merge: true });
  expect(daemon.showNotice).not.toHaveBeenCalled();
  await act(async () => dialog.querySelector("form")!.requestSubmit());
  expect(daemon.request).toHaveBeenCalledWith({ type: "pull_project_branch", project: project.id, merge: true });
  expect(daemon.showNotice).toHaveBeenCalledWith("Pulled the current branch.");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("pulls nothing when the merge is cancelled", async () => {
  const { daemon, project } = await openPullCommand({ kind: "needs_merge", ahead: 2, behind: 3 });
  await press("Cancel");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(daemon.request).not.toHaveBeenCalledWith({ type: "pull_project_branch", project: project.id, merge: true });
});

it("turns the merge confirmation into a failure modal when the confirmed pull meets conflicts", async () => {
  const { daemon } = await openPullCommand({ kind: "needs_merge", ahead: 2, behind: 3 });
  vi.mocked(daemon.request).mockRejectedValueOnce(new CodedError("Merging the upstream into this branch causes conflicts, so the merge was cancelled and nothing changed: src/a.ts\nsrc/b.ts", "pull_merge_conflict", { detail: "src/a.ts\nsrc/b.ts" }));
  await act(async () => document.querySelector<HTMLFormElement>('[role="alertdialog"] form')!.requestSubmit());
  const dialogs = [...document.querySelectorAll('[role="alertdialog"]')];
  expect(dialogs).toHaveLength(1);
  expect(dialogs[0].textContent).toContain("the merge was cancelled and nothing changed");
  expect(dialogs[0].textContent).not.toContain("Pull and merge");
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(daemon.showNotice).not.toHaveBeenCalled();
});

it("keeps the merge confirmation open under an alert for any other refusal of the confirmed pull", async () => {
  const { daemon } = await openPullCommand({ kind: "needs_merge", ahead: 2, behind: 3 });
  vi.mocked(daemon.request).mockRejectedValueOnce(new CodedError("Git could not pull this branch: error: Your local changes would be overwritten", "git_pull_failed", { detail: "error: Your local changes would be overwritten" }));
  await act(async () => document.querySelector<HTMLFormElement>('[role="alertdialog"] form')!.requestSubmit());
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ title: "Could not pull the branch" })]);
  expect(daemon.showNotice).not.toHaveBeenCalled();
});

/** A confirmation that closed itself because its project was removed still has its confirmed pull in flight: when that
 * settles, it asks to close, which must not close the dialog that has been opened since. */
it("keeps the dialog opened since open when a merge confirmation closed by its project's removal settles its pull", async () => {
  const { daemon, project } = mountApp();
  const other = projectOf("p-other", project.console_id, "API");
  let release!: () => void;
  vi.mocked(daemon.request).mockImplementation((async (body: { type: string; merge?: boolean }) => {
    if (body.type !== "pull_project_branch") return { type: "ack", id: null };
    if (!body.merge) return { type: "project_branch_pulled", id: null, outcome: { kind: "needs_merge", ahead: 2, behind: 3 } };
    await new Promise<void>((done) => { release = done; });
    return { type: "project_branch_pulled", id: null, outcome: { kind: "merged" } };
  }) as never);
  await setGitStatus(daemon, project);
  await setGitStatus(daemon, other);
  await openProjectMenu(project.name);
  await press("Git");
  await press("Pull current branch");
  const first = document.querySelector('[role="alertdialog"]')!;
  expect(first.textContent).toContain(project.name);
  await act(async () => first.querySelector("form")!.requestSubmit());
  expect(daemon.request).toHaveBeenCalledWith({ type: "pull_project_branch", project: project.id, merge: true });

  await act(async () => daemon.store.setState((state) => ({ projects: new Map([...state.projects].filter(([id]) => id !== project.id)) })));
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  await openProjectMenu(other.name);
  await press("Git");
  await press("Pull current branch");
  const second = document.querySelector('[role="alertdialog"]')!;
  expect(second.textContent).toContain(other.name);

  await act(async () => release());
  expect(second.isConnected).toBe(true);
  expect(document.querySelector('[role="alertdialog"]')).toBe(second);
  expect(second.textContent).toContain("Pull and merge");
});

/** Gives each project the git status `status` says, with its upstream unless the test names another. */
const setGitStatus = (daemon: Daemon, project: Project, status: { branch?: string | null; detached?: boolean; upstream?: string | null } = {}) =>
  act(async () => daemon.store.setState((state) => ({
    gitStatuses: new Map(state.gitStatuses).set(project.id, {
      project: project.id, repository: true, branch: "main", detached: false, upstream: "origin/main",
      ahead: 0, behind: 0, activity: "idle", error: null, ...status,
    }),
  })));

const menuItem = (label: string) => Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem]")).find((item) => controlName(item) === label);

it.each([
  ["a branch with no upstream", { branch: "main", upstream: null }, "No upstream"],
  ["a detached HEAD", { branch: "abc1234", detached: true, upstream: null }, "Not on a branch"],
])("disables Pull current branch in the project's row menu with its reason for %s, and pulls nothing", async (_case, status, reason) => {
  const { daemon, project } = mountApp();
  await setGitStatus(daemon, project, status);
  await openProjectMenu(project.name);
  await press("Git");
  const pull = menuItem("Pull current branch")!;
  expect(pull.getAttribute("aria-disabled")).toBe("true");
  expect(pull.textContent).toContain(reason);
  await act(async () => pull.click());
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "pull_project_branch" }));
  // The other Git actions stay on offer.
  expect(menuItem("Push current branch")?.getAttribute("aria-disabled")).not.toBe("true");
});

it("offers Pull current branch in the project's row menu while the git status names no branch yet", async () => {
  const { daemon, project } = mountApp();
  await setGitStatus(daemon, project, { branch: null, upstream: null });
  await openProjectMenu(project.name);
  await press("Git");
  expect(menuItem("Pull current branch")?.getAttribute("aria-disabled")).not.toBe("true");
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "pull_project_branch" }));
});

/** Asks Website and API to pull, each reply held until `resolve` is called with the project's name, then opens
 * Website's project settings (or the app's Settings, with `appSettings`), so a modal is on screen when the replies
 * come. */
async function pullWithSettingsOpen(appSettings = false) {
  const mounted = mountApp();
  const { daemon, project } = mounted;
  const other = projectOf("p-other", project.console_id, "API");
  const replies = new Map<string, () => void>();
  vi.mocked(daemon.request).mockImplementation((async (body: { type: string; project: string; merge?: boolean }) => {
    if (body.type !== "pull_project_branch") return { type: "ack", id: null };
    if (body.merge) return { type: "project_branch_pulled", id: null, outcome: { kind: "merged" } };
    return new Promise((resolve) => replies.set(body.project, () => resolve({ type: "project_branch_pulled", id: null, outcome: { kind: "needs_merge", ahead: 2, behind: 3 } })));
  }) as never);
  await setGitStatus(daemon, project);
  await setGitStatus(daemon, other);
  await openProjectMenu(project.name);
  await press("Git");
  await press("Pull current branch");
  await openProjectMenu(other.name);
  await press("Git");
  await press("Pull current branch");
  if (appSettings) {
    await press("Settings");
  } else {
    await openProjectMenu(project.name);
    await press("Project settings");
  }
  expect(document.querySelector("[role=dialog]")).not.toBeNull();
  const resolve = (id: string) => act(async () => replies.get(id)!());
  return { ...mounted, other, resolve };
}

it("holds the merge confirmation behind the dialog that is open when the pull answers, and opens it once that closes", async () => {
  const { project, resolve } = await pullWithSettingsOpen();
  await resolve(project.id);
  // The dialog the person is in stays, and no confirmation takes its place or lies over it.
  expect(document.querySelector("[role=dialog]")).not.toBeNull();
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  await press("Cancel");
  expect(document.querySelector("[role=dialog]")).toBeNull();
  const confirmation = document.querySelector('[role="alertdialog"]')!;
  expect(confirmation.querySelector("h2")?.textContent).toBe("Pull and merge");
  expect(confirmation.textContent).toContain(project.name);
});

it("holds the merge confirmation behind the app's Settings, and opens it once Settings closes", async () => {
  const { project, resolve } = await pullWithSettingsOpen(true);
  await resolve(project.id);
  expect(document.querySelector("[role=dialog]")).not.toBeNull();
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  await press("Close");
  expect(document.querySelector("[role=dialog]")).toBeNull();
  const confirmation = document.querySelector('[role="alertdialog"]')!;
  expect(confirmation.querySelector("h2")?.textContent).toBe("Pull and merge");
  expect(confirmation.textContent).toContain(project.name);
});

it("opens several held merge confirmations one after another in the order their pulls answered", async () => {
  const { project, other, resolve } = await pullWithSettingsOpen();
  await resolve(other.id);
  await resolve(project.id);
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  await press("Cancel");
  const first = document.querySelector('[role="alertdialog"]')!;
  expect(first.textContent).toContain(other.name);
  expect(first.textContent).not.toContain(project.name);
  await press("Cancel");
  const second = document.querySelector('[role="alertdialog"]')!;
  expect(second.textContent).toContain(project.name);
  expect(first.isConnected).toBe(false);
  await press("Cancel");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.querySelector("[role=dialog]")).toBeNull();
});

it("shows a warning on Switch branch for a truncated list and still lets the user choose a returned branch", async () => {
  const { daemon, project } = mountApp();
  await act(async () => daemon.store.setState((state) => ({
    gitStatuses: new Map(state.gitStatuses).set(project.id, {
      project: project.id, repository: true, branch: "main", detached: false, upstream: null,
      ahead: 0, behind: 0, activity: "idle", error: null,
    }),
  })));
  vi.mocked(daemon.request).mockImplementation((async (body: { type: string }) => {
    if (body.type === "list_project_branches") return { type: "project_branches", id: null, project: project.id, branches: [{ name: "alpha" }, { name: "beta" }], complete: false };
    if (body.type === "get_project_source") return { type: "project_source", id: null, source: { project: project.id, root: "/r", resolved_root: "/r", root_id: "r", git_error: null } };
    return { type: "ack", id: null };
  }) as never);
  await openProjectMenu(project.name);
  await press("Git");
  await press("Switch branch");
  const parent = [...document.querySelectorAll("[role=menuitem]")].find((item) => controlName(item) === "Switch branch");
  expect(parent?.querySelector("svg.lucide-triangle-alert")).not.toBeNull();
  expect(parent?.querySelector("svg.lucide-circle-alert")).toBeNull();
  const warning = "There are more branches than this list can show. The others are not listed.";
  expect(parent!.getAttribute("aria-describedby")?.split(/\s+/).some((id) => document.getElementById(id)?.textContent === warning)).toBe(true);
  const alpha = [...document.querySelectorAll<HTMLElement>("[role=menuitemradio]")].find((item) => controlName(item) === "alpha");
  expect(alpha).toBeDefined();
  const branchMenu = alpha!.closest("[role=menu]")!;
  expect([...branchMenu.querySelectorAll("[role^=menuitem]")].map(controlName)).toEqual(["alpha", "beta"]);
  await act(async () => alpha!.click());
  expect(daemon.request).toHaveBeenCalledWith({ type: "switch_project_branch", project: project.id, branch: "alpha" });
  // Asked by the person opening the menu, so the daemon looks again for a Git it found missing.
  for (const type of ["list_project_branches", "get_project_source"]) {
    expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type, asked_by_person: true }));
  }
});

it("keeps a native operation rejection string as the error diagnostic", async () => {
  const { daemon } = mountApp();
  await openProjectMenu("Website");
  vi.mocked(daemon.request).mockRejectedValueOnce("Native file manager rejected");
  await press("Open in");
  await press("File manager");
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ message: "Native file manager rejected", title: "Could not open in the file manager", error: "Native file manager rejected" })]);
});

it("keeps the restore confirmation open and displays a refusal", async () => {
  const { daemon } = await openRestoreConfirmation();
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("Finish the merge first."));
  await act(async () => document.querySelector('[role="alertdialog"] form')!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect([...document.querySelectorAll('[role="alertdialog"]')].some((dialog) => dialog.textContent?.includes("Finish the merge first."))).toBe(true);
  expect(daemon.showNotice).not.toHaveBeenCalled();
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
    keep: [{ kind: "branch", repository: "/r/.git", branch: "child-feature", path: null, state: "unmerged", uncommitted: false, tip: null }],
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
  id: "4242:1", session_id: "s-archive", command: "npm run dev", cwd: "/r-feature", cwd_missing: false, kept: false, keepable: true,
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
  expect(dialog.textContent).toContain("2 processes will be stopped.");
  const boxes = [...dialog.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
  expect(boxes.map((box) => box.checked)).toEqual([true, true]);
  await act(async () => boxes[0].click());
  expect(dialog.textContent).toContain("1 process will be stopped.");
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
async function pickSonnetInChangeModel(answer: (body: { type: string }) => unknown, sessionRecord: Partial<Session> = {}) {
  // The status region's waits are on the clock from before the window mounts and arms them.
  fakeTimers();
  const mounted = mountApp(false, undefined, true);
  await act(async () => mounted.daemon.store.setState((state) => ({
    sessions: new Map(state.sessions).set("s-archive", { ...state.sessions.get("s-archive")!, ...sessionRecord }),
  })));
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
  // The status region writes nothing until its first write delay (`StatusAnnouncer`) has passed.
  await passTime(FIRST_WRITE_DELAY_MS);
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
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toContain("restarted with its new launch settings");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("asks for the values to be applied again when the pickers showed what the records say the session runs", async () => {
  windowAttention();
  const { daemon } = await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "restarted", continued: false }), { model: "opus", effort: "high", observed_effort: "low" });
  expect(sentOf(daemon, "set_session_model")).toEqual([[
    { type: "set_session_model", session: "s-archive", model: "sonnet", effort: "low", reapply: true, terminal_appearance: expect.stringMatching(/^(light|dark)$/) },
  ]]);
});

it("announces values recorded for the next resume after the focus move", async () => {
  windowAttention();
  await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBeUndefined();
  await passTime(1);
  expect(said()).toContain("will use its new launch settings when it is resumed");
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
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toContain("Renamed");
});

it("holds an announcement while the window is not attended, then says it after the focus-move wait", async () => {
  const attend = windowAttention();
  attend(false);
  await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(said()).toBeUndefined();
  attend(true);
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBeUndefined();
  await passTime(1);
  expect(said()).toContain("will use its new launch settings when it is resumed");
});

it("holds an announcement while a modal is open over the window", async () => {
  windowAttention();
  await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "recorded" }));
  await act(async () => errorAlerts.report({ message: "Something else failed.", recovery: "none" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(said()).toBeUndefined();
  await act(async () => errorAlerts.acknowledge());
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toContain("will use its new launch settings when it is resumed");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("announces a batch of account switches once its last switch is done", async () => {
  windowAttention();
  // The status region's waits are on the clock from before the window mounts and arms them.
  fakeTimers();
  const { daemon } = mountApp(false, undefined, true);
  await act(async () => daemon.store.setState((state) => ({ settings: { ...state.settings, accounts: [accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work")] } })));
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Switch account");
  const work = Array.from(document.querySelectorAll<HTMLElement>("[role=menuitemradio], [role=menuitem]")).find((item) => controlName(item) === "Work");
  expect(work).toBeDefined();
  await act(async () => work!.click());
  await passTime(FIRST_WRITE_DELAY_MS);
  await act(async () => Array.from(document.querySelectorAll<HTMLButtonElement>("[role=alertdialog] button, [role=dialog] button")).find((button) => button.textContent === "Switch")!.click());
  expect(sentOf(daemon, "switch_session_account")).toEqual([[
    { type: "switch_session_account", session: "s-archive", account: "a-work", terminal_appearance: expect.stringMatching(/^(light|dark)$/) },
  ]]);
  expect(said()).toBeUndefined();
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe(`1 session switched to ${isolate("Work")}`);
});

it("announces nothing when the session had the model settings already", async () => {
  windowAttention();
  await pickSonnetInChangeModel(() => ({ type: "session_model_set", id: null, session: "s-archive", outcome: "unchanged" }));
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(document.querySelector('[role="status"]:not(:empty)')).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("points a launch setting the agent does not offer back to Change model", async () => {
  const error = new CodedError("No.", "invalid_launch_setting", { field: "model", value: "sonnet", agent: "Codex" });
  await pickSonnetInChangeModel((body) => {
    if (body.type === "set_session_model") throw error;
    return { type: "ack", id: null };
  });
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ error, title: "Could not change the model", session: "s-archive", recovery: "changeModelSetting" })]);
});

it("opens Change model for the session from the alert of a launch setting the agent does not offer", async () => {
  mountApp(false, undefined, true);
  await act(async () => errorAlerts.report({
    message: "No.", session: "s-archive", title: "Could not change the model", recovery: "changeModelSetting",
  }));
  const open = () => Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((button) => button.textContent === "Change model")!;
  await act(async () => open().click());
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Change model");
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("The session Fix bug will run");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("reports a refused model change under its own title for the session", async () => {
  const error = new CodedError("Did not stop.", "model_change_did_not_stop", { session: "s-archive" });
  await pickSonnetInChangeModel((body) => {
    if (body.type === "set_session_model") throw error;
    return { type: "ack", id: null };
  });
  expect(errorAlerts.getSnapshot()).toEqual([expect.objectContaining({ error, title: "Could not change the model", session: "s-archive", recovery: undefined })]);
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
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}><ThemeProvider><DaemonProvider value={daemon}>
      <FileViewer subject={{ key: "file", path: "test.bin", content: { state: "file", body: { kind: "binary", size: 1 } } }} onClose={close} />
    </DaemonProvider></ThemeProvider></PlatformProvider>,
  ));
  const status = Array.from(app.querySelectorAll('[role="status"]')).at(-1)!;
  expect(status.closest('[inert], [aria-hidden="true"]')).not.toBeNull();
  await act(async () => reply());
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS + ANNOUNCEMENT_MS);
  expect(status.textContent).toBe("");
  await act(async () => close());
  await passTime(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(status.textContent).toBe("");
  await passTime(1);
  expect(status.textContent).toContain("will use its new launch settings when it is resumed");
});
