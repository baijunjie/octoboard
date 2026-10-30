// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { PanelHeader } from "./components/PanelHeader";
import { mountForTooltip } from "./components/tooltipTestSupport";
import { consoleOf, pageOf, projectOf, sessionOf, snapshotState } from "./gallery/fixtures/builders";
import { defaultStack } from "./panels/stackState";
import { usePanelStacks } from "./panels/usePanelStacks";
import { PlatformProvider } from "./platform/react";
import type { Event, Project } from "./protocol";
import { applyDaemonEvent, createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => <PanelHeader title={project.name} />,
}));
vi.mock("./terminal/ShellTerminalPanel", () => ({ ShellTerminalPanel: () => <PanelHeader title="Terminal" /> }));
vi.mock("./terminal/shellTerminals", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./terminal/shellTerminals")>();
  return { ...actual, startShellTerminal: vi.fn(async () => undefined) };
});
// The panel states whose pages it is given, which is what a project session's Report panel is about.
vi.mock("./report/ReportPanel", () => ({
  ReportPanel: ({ sessionId }: { sessionId: string }) => (
    <>
      <PanelHeader title="Report" />
      <span data-report-session={sessionId} />
      <button type="button" data-in-report>inside</button>
    </>
  ),
}));
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    constructor() {
      return new Proxy(this, { get: (target, key) => (key === "then" ? undefined : key in target ? Reflect.get(target, key) : () => {}) });
    }
  },
}));

function mountApp(narrow = false) {
  // The panel stacks and the closed Report panels outlive a test's window. Each session's closed
  // Report panel is reset through the setter of the hook that has that session's panel.
  const SESSIONS = ["s-one", "s-two", "s-none"];
  const stacks: Record<string, ReturnType<typeof usePanelStacks>> = {};
  function StackReset({ session }: { session?: string }) {
    stacks[session ?? "plain"] = usePanelStacks(session);
    return null;
  }
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: !narrow && media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  document.documentElement.style.setProperty("--breakpoint-phone", "782px");
  onTestFinished(() => {
    act(() => {
      for (const session of SESSIONS) stacks[session].setStack("project", { ...defaultStack("project"), open: ["report", "files"] });
      stacks.plain.setStack("project", defaultStack("project"));
      stacks.plain.setStack("report", defaultStack("report"));
    });
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-docked");
    document.documentElement.style.removeProperty("--breakpoint-phone");
    try {
      localStorage.clear();
    } catch {
      // No storage: nothing was kept.
    }
  });
  const console_ = consoleOf("c-rep", "Main");
  const project = projectOf("p-site", console_.id, "Website");
  const sessions = [
    sessionOf("s-one", console_.id, project.id, "First work", "idle"),
    sessionOf("s-two", console_.id, project.id, "Second work", "idle"),
    sessionOf("s-none", console_.id, project.id, "No pages", "idle"),
  ];
  const request = vi.fn(async (_message: unknown) => ({ type: "ack", id: null })) as Daemon["request"] & ReturnType<typeof vi.fn>;
  const daemon: Daemon = {
    store: createStateStore(snapshotState({ consoles: [console_], projects: [project], sessions })),
    request,
    toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {},
    dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  mountForTooltip(
    <PlatformProvider value={{ kind: "browser" }}>
      <ThemeProvider><DaemonProvider value={daemon}><StackReset />{SESSIONS.map((session) => <StackReset key={session} session={session} />)}<App /></DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
  /** What the daemon broadcasts, as the window receives it. */
  const receive = (event: Event) => act(() => applyDaemonEvent(daemon.store, event));
  /** The daemon's `snapshot` of what the window already holds: it carries no pages. */
  const resnapshot = () => {
    const state = daemon.store.getState();
    return receive({
      type: "snapshot",
      hosts: [...(state.hosts?.values() ?? [])], consoles: [...state.consoles.values()], projects: [...state.projects.values()],
      sessions: [...state.sessions.values()], trusted_directories: [], settings: state.settings, git_statuses: [],
      agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], home_dir: null, shell_terminals: [],
    });
  };
  const push = (session: string, id: string) => receive({ type: "page_created", page: pageOf(id, session, `<p>${id}</p>`, 0) });
  return { request, receive, push, resnapshot };
}

const pressElement = (element: HTMLElement | null | undefined) => {
  expect(element).toBeTruthy();
  return act(async () => element!.click());
};
const byLabel = (label: string, within: ParentNode = document) => within.querySelector<HTMLElement>(`[aria-label="${label}"]`);
const toggle = (label: string) => byLabel(label, document.querySelector("[data-region=topbar]")!);
const row = (title: string) =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-region=sidebar] [role=button]")).filter((element) => element.textContent?.includes(title)).at(-1);
const reportCard = () => document.getElementById("panel-report");
const reportSession = () => reportCard()?.querySelector("[data-report-session]")?.getAttribute("data-report-session");
const filesCard = () => document.getElementById("panel-files");

it("offers a project session's Report panel only while that session has pages, listing them when it is selected", async () => {
  const { request, push } = mountApp();
  await pressElement(row("First work"));
  expect(request).toHaveBeenCalledWith({ type: "list_pages", session: "s-one" });
  expect(toggle("Hide report panel")).toBeNull();
  expect(toggle("Show report panel")).toBeNull();
  expect(reportCard()).toBeNull();
  // Files is the project's, there with the session selected.
  expect(filesCard()).not.toBeNull();

  // A page arrives: the panel opens beside Files, like a console session's does by default.
  await push("s-one", "page-1");
  expect(toggle("Hide report panel")).not.toBeNull();
  expect(reportSession()).toBe("s-one");
  expect(filesCard()).not.toBeNull();
});

it("shows the selected session's own pages, and no Report panel for one with none", async () => {
  const { push } = mountApp();
  await push("s-one", "page-1");
  await push("s-two", "page-2");
  await pressElement(row("First work"));
  expect(reportSession()).toBe("s-one");
  await pressElement(row("Second work"));
  expect(reportSession()).toBe("s-two");
  await pressElement(row("No pages"));
  expect(reportCard()).toBeNull();
  expect(toggle("Hide report panel")).toBeNull();
  expect(toggle("Show report panel")).toBeNull();
  // The project's panels are still there.
  expect(filesCard()).not.toBeNull();
});

it("keeps Files and Terminal shared across a project's sessions and the Report panel's open state per session", async () => {
  const { push } = mountApp();
  await push("s-one", "page-1");
  await push("s-two", "page-2");
  await pressElement(row("First work"));
  expect(reportSession()).toBe("s-one");

  // Closing the Report panel of one session leaves the other's, and does not touch Files.
  await pressElement(toggle("Hide report panel"));
  expect(reportCard()).toBeNull();
  expect(toggle("Show report panel")).not.toBeNull();
  expect(filesCard()).not.toBeNull();
  await pressElement(row("Second work"));
  expect(reportSession()).toBe("s-two");

  // Opening Terminal and closing Files here is the project's: the first session sees both.
  await pressElement(toggle("Show terminal panel"));
  await pressElement(toggle("Hide files panel"));
  await pressElement(row("First work"));
  expect(filesCard()).toBeNull();
  expect(document.getElementById("panel-terminal")).not.toBeNull();
  expect(reportCard()).toBeNull();
  await pressElement(toggle("Show report panel"));
  expect(reportSession()).toBe("s-one");
});

it("follows a deletion live: the pager's pages go, and the panel and its toggle go with the last one", async () => {
  const { push, receive } = mountApp();
  await push("s-one", "page-1");
  await push("s-one", "page-2");
  await pressElement(row("First work"));
  expect(reportSession()).toBe("s-one");

  await receive({ type: "pages_deleted", session_id: "s-one", pages: ["page-1"] });
  expect(reportSession()).toBe("s-one");
  expect(toggle("Hide report panel")).not.toBeNull();

  await receive({ type: "pages_deleted", session_id: "s-one", pages: ["page-2"] });
  expect(reportCard()).toBeNull();
  expect(toggle("Hide report panel")).toBeNull();
  expect(toggle("Show report panel")).toBeNull();
  expect(filesCard()).not.toBeNull();

  // A page pushed afterwards brings it back, still open as it was.
  await push("s-one", "page-3");
  expect(reportSession()).toBe("s-one");
});

it("keeps a project session's Report panel and its toggle through a snapshot, until the re-list answers", async () => {
  const { push, resnapshot, request } = mountApp();
  await push("s-one", "page-1");
  await pressElement(row("First work"));
  const card = reportCard();
  expect(card).not.toBeNull();

  // The snapshot empties the store's pages, and the re-list is not answered here.
  request.mockClear();
  await resnapshot();
  expect(reportCard()).toBe(card);
  expect(toggle("Hide report panel")).not.toBeNull();
  expect(filesCard()).not.toBeNull();
  // One listing for the new snapshot, not one from the window and one from the panel.
  expect(request.mock.calls.filter(([message]) => (message as { type: string }).type === "list_pages")).toHaveLength(1);
});

it("keeps a session known to have no pages out of the pane through a snapshot", async () => {
  const { resnapshot } = mountApp();
  await pressElement(row("No pages"));
  await resnapshot();
  expect(reportCard()).toBeNull();
  expect(toggle("Hide report panel")).toBeNull();
  expect(toggle("Show report panel")).toBeNull();
});

it("offers the Report panel when listing a project session's pages fails, so its own failure card can say why", async () => {
  const { request } = mountApp();
  request.mockImplementation(async (message: unknown) => {
    if ((message as { type: string }).type === "list_pages") throw new Error("offline");
    return { type: "ack", id: null };
  });
  await pressElement(row("First work"));
  expect(toggle("Hide report panel")).not.toBeNull();
  expect(reportSession()).toBe("s-one");
});

it("clears a failed listing once the store holds a list, so a later snapshot does not bring back an emptied panel", async () => {
  const { request, receive, resnapshot } = mountApp();
  request.mockImplementation(async (message: unknown) => {
    if ((message as { type: string }).type === "list_pages") throw new Error("offline");
    return { type: "ack", id: null };
  });
  await pressElement(row("First work"));
  expect(toggle("Hide report panel")).not.toBeNull();

  // Try again answers, and then every page is deleted; the daemon answers listings from here on.
  request.mockImplementation(async () => ({ type: "ack", id: null }));
  await receive({ type: "page_list", id: null, session_id: "s-one", pages: [pageOf("page-1", "s-one", "<p>1</p>", 0)] });
  await receive({ type: "pages_deleted", session_id: "s-one", pages: ["page-1"] });
  expect(reportCard()).toBeNull();

  await resnapshot();
  expect(reportCard()).toBeNull();
  expect(toggle("Hide report panel")).toBeNull();
  expect(toggle("Show report panel")).toBeNull();
});

it("takes back the card a hovered Report toggle floated in when the last page is deleted, handing focus on", async () => {
  vi.useFakeTimers();
  document.documentElement.style.setProperty("--peek-gap", "8px");
  onTestFinished(() => {
    vi.useRealTimers();
    document.documentElement.style.removeProperty("--peek-gap");
  });
  const { push, receive } = mountApp();
  await push("s-one", "page-1");
  await pressElement(row("First work"));
  await pressElement(toggle("Hide report panel"));
  await pressElement(toggle("Hide files panel"));
  expect(reportCard()).toBeNull();

  const button = toggle("Show report panel")!;
  const pointer = (type: string) => new PointerEvent(type, { pointerType: "mouse", bubbles: type !== "pointerenter" });
  await act(async () => {
    button.dispatchEvent(pointer("pointerover"));
    button.dispatchEvent(pointer("pointerenter"));
    await vi.advanceTimersByTimeAsync(300);
  });
  expect(reportCard()).not.toBeNull();
  act(() => reportCard()!.querySelector<HTMLElement>("[data-in-report]")!.focus());
  act(() => setInteractionModality("pointer"));

  await receive({ type: "pages_deleted", session_id: "s-one", pages: ["page-1"] });
  await act(async () => void (await vi.advanceTimersByTimeAsync(500)));
  expect(reportCard()).toBeNull();
  // The card's focus was handed on, with the ring (the mocked terminal cannot take it itself).
  expect(getInteractionModality()).toBe("keyboard");
});

it("takes back a floating Report panel at once when its last page is deleted, handing its focus on", async () => {
  const { push, receive } = mountApp(true);
  await push("s-one", "page-1");
  await pressElement(row("First work"));
  await pressElement(toggle("Show report panel"));
  expect(reportCard()).not.toBeNull();
  act(() => reportCard()!.querySelector<HTMLElement>("[data-in-report]")!.focus());
  act(() => setInteractionModality("pointer"));

  await receive({ type: "pages_deleted", session_id: "s-one", pages: ["page-1"] });
  // Not left fading away as a blank card.
  expect(reportCard()).toBeNull();
  expect(getInteractionModality()).toBe("keyboard");
});
