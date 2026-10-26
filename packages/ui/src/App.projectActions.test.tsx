// @vitest-environment jsdom
import { act } from "react";
import * as reactAria from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { ErrorAlerts } from "./dialogs/ErrorAlerts";
import { errorAlerts } from "./errorAlerts";
import { App } from "./App";
import { setProjectBrowserMode, useProjectBrowserState } from "./browser/browserState";
import { mountForTooltip } from "./components/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./gallery/fixtures/builders";
import { isolate } from "./i18n/isolate";
import { PlatformProvider } from "./platform/react";
import type { PlatformAdapter } from "./platform";
import type { Project } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";

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
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    mount() {}
    dispose() {}
    fit() {}
    syncSize() {}
    setInputLabel() {}
    detach() {}
  },
}));

function mountApp(narrow = false, clipboard?: { writeText: (text: string) => Promise<void> }, withSession = false, links?: PlatformAdapter["links"]) {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: !narrow && media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  const console_ = consoleOf("c-actions", "Main");
  const projects = [projectOf("p-actions", console_.id, "Website"), projectOf("p-other", console_.id, "API")];
  const sessions = withSession ? [sessionOf("s-archive", console_.id, "p-actions", "Fix bug", "idle")] : [];
  const daemon: Daemon = {
    store: createStateStore(snapshotState({ consoles: [console_], projects, sessions })),
    request: vi.fn(async () => ({ type: "ack" })) as Daemon["request"],
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

it.each([false, true])("reopens a hidden project's Files pane when narrow=%s", async (narrow) => {
  const { container, project } = mountApp(narrow);
  if (narrow) await press("Show sessions");
  await openProjectMenu(project.name);
  await press("Browse files");
  expect(container.querySelector(`[data-browser="${project.id}"]`)?.getAttribute("data-mode")).toBe("files");
  expect(container.querySelector('[data-region="topbar"] [aria-label="Hide files panel"]')).not.toBeNull();
  if (!narrow) {
    await openProjectMenu(project.name);
    expect(menuHas("Browse files")).toBe(false);
    await press("Open in");
    await press("File manager");
    await openProjectMenu("API");
    expect(menuHas("Browse files")).toBe(true);
    await press("Open in");
    await press("File manager");
  }
  await press("Hide files panel");
  if (narrow) await press("Show sessions");
  await act(async () => setProjectBrowserMode(project.id, "git"));
  await openProjectMenu(project.name);
  expect(menuHas("Browse files")).toBe(true);
  await press("Browse files");
  expect(container.querySelector(`[data-browser="${project.id}"]`)?.getAttribute("data-mode")).toBe("files");
  expect(container.querySelector('[data-region="topbar"] [aria-label="Hide files panel"]')).not.toBeNull();
});

const toggle = (label: string) => document.querySelector<HTMLElement>(`[data-region="topbar"] [aria-label="${label}"]`);
const asidePane = () => document.querySelector<HTMLElement>("[data-pane=aside]")!;

it.each([false, true])("has no toggle for the pane itself, only a panel's, which hides and shows it; narrow=%s", async (narrow) => {
  const { project } = mountApp(narrow);
  if (narrow) await press("Show sessions");
  await openProjectMenu(project.name);
  await press("Browse files");
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
  await press("Browse files");
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
  await press("Browse files");
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
      project: project.id, repository: true, branch: "main", detached: false,
      ahead: 0, behind: 0, activity: "idle",
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
      project: project.id, repository: true, branch: "main", detached: false,
      ahead: 0, behind: 0, activity: "idle",
    }),
  })));
  vi.mocked(daemon.request).mockImplementation((async (body: { type: string }) => body.type === "list_project_branches"
    ? { type: "project_branches", project: project.id, branches: [{ name: "alpha" }, { name: "beta" }], complete: false }
    : { type: "ack" }) as never);
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
    if (body.type !== "inspect_session_branches") return { type: "ack" };
    await answered;
    return { type: "session_branches_inspected", items } as never;
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
    return { type: "ack" };
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
      items: [{
        kind: "worktree", repository: "/r/.git", branch: "feature", path: "/r-feature", project: "p-actions",
        session: "s-archive", state: "uncommitted_changes", uncommitted: true, detail: null,
      }],
    }
    : { type: "ack" });
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Archive");
  expect(daemon.request).toHaveBeenCalledWith({ type: "inspect_session_branches", sessions: ["s-archive"] });
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "archive_session" }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("has uncommitted changes");
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
