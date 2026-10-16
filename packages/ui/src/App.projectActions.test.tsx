// @vitest-environment jsdom
import { act } from "react";
import * as reactAria from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { setProjectBrowserMode, useProjectBrowserState } from "./browser/browserState";
import { mountForTooltip } from "./components/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./gallery/fixtures/builders";
import { isolate } from "./i18n/isolate";
import { PlatformProvider } from "./platform/react";
import type { Project } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

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
    disarmWake() {}
  },
}));

function mountApp(narrow = false, clipboard?: { writeText: (text: string) => Promise<void> }, withSession = false) {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: !narrow && media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1148px");
  onTestFinished(() => {
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
    <PlatformProvider value={{ kind: "browser", clipboard }}>
      <ThemeProvider><DaemonProvider value={daemon}><App /></DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
  return { container, daemon, project: projects[0] };
}

async function press(label: string) {
  const target = Array.from(document.querySelectorAll<HTMLElement>("button, [role=menuitem]")).find((element) =>
    element.getAttribute("aria-label") === label || element.textContent === label,
  );
  expect(target, label).toBeDefined();
  await act(async () => target!.click());
}

const menuHas = (label: string) => Array.from(document.querySelectorAll("[role=menuitem]")).some((item) => item.textContent === label);
const openProjectMenu = (name: string) => press(`Actions for project ${isolate(name)}`);

it.each([false, true])("reopens a hidden project's Files pane when narrow=%s", async (narrow) => {
  const { container, project } = mountApp(narrow);
  if (narrow) await press("Show sessions");
  await openProjectMenu(project.name);
  await press("Browse files");
  expect(container.querySelector(`[data-browser="${project.id}"]`)?.getAttribute("data-mode")).toBe("files");
  expect(container.querySelector('[data-region="topbar"] [aria-label="Hide project pane"]')).not.toBeNull();
  if (!narrow) {
    await openProjectMenu(project.name);
    expect(menuHas("Browse files")).toBe(false);
    await press("Open in file manager");
    await openProjectMenu("API");
    expect(menuHas("Browse files")).toBe(true);
    await press("Open in file manager");
  }
  await press("Hide project pane");
  if (narrow) await press("Show sessions");
  await act(async () => setProjectBrowserMode(project.id, "git"));
  await openProjectMenu(project.name);
  expect(menuHas("Browse files")).toBe(true);
  await press("Browse files");
  expect(container.querySelector(`[data-browser="${project.id}"]`)?.getAttribute("data-mode")).toBe("files");
  expect(container.querySelector('[data-region="topbar"] [aria-label="Hide project pane"]')).not.toBeNull();
});

it("sends the project directory request and reports a failure through the normal toast", async () => {
  const { daemon, project } = mountApp();
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("The system file manager could not be opened."));
  await openProjectMenu(project.name);
  await press("Open in file manager");
  expect(daemon.request).toHaveBeenCalledWith({ type: "open_project_directory", project: project.id });
  expect(daemon.toastError).toHaveBeenCalledExactlyOnceWith("The system file manager could not be opened.");
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
  expect(daemon.toastError).toHaveBeenCalledWith("Could not copy the project name");
  expect(daemon.toastNotice).not.toHaveBeenCalled();
});

it("omits copying the project name without clipboard support", async () => {
  const { project } = mountApp();
  await openProjectMenu(project.name);
  expect(menuHas("Copy project name")).toBe(false);
});

it("copies the session ID unchanged and reports success", async () => {
  const clipboard = { writeText: vi.fn(async () => {}) };
  const { daemon } = mountApp(false, clipboard, true);
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Copy session ID");
  expect(clipboard.writeText).toHaveBeenCalledExactlyOnceWith("s-archive");
  expect(daemon.toastNotice).toHaveBeenCalledWith("Session ID copied");
});

it("reports a session ID clipboard failure", async () => {
  const clipboard = { writeText: vi.fn(async () => { throw new Error("Denied"); }) };
  const { daemon } = mountApp(false, clipboard, true);
  await press(`Actions for session ${isolate("Fix bug")}`);
  await press("Copy session ID");
  expect(daemon.toastError).toHaveBeenCalledWith("Could not copy the session ID");
  expect(daemon.toastNotice).not.toHaveBeenCalled();
});

it("omits copying the session ID without clipboard support", async () => {
  mountApp(false, undefined, true);
  await press(`Actions for session ${isolate("Fix bug")}`);
  expect(menuHas("Copy session ID")).toBe(false);
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

it("keeps the restore confirmation open and displays a refusal", async () => {
  const { daemon } = await openRestoreConfirmation();
  vi.mocked(daemon.request).mockRejectedValueOnce(new Error("Finish the merge first."));
  await act(async () => document.querySelector('[role="alertdialog"] form')!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Finish the merge first.");
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
