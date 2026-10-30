// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { PanelHeader } from "./components/PanelHeader";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./gallery/fixtures/builders";
import { touchEvent } from "./layout/touchTestSupport";
import { defaultStack } from "./panels/stackState";
import { usePanelStacks } from "./panels/usePanelStacks";
import { PlatformProvider } from "./platform/react";
import type { ProcessTree, Project } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const renders = vi.hoisted(() => ({ app: 0, rows: 0 }));
vi.mock("./process/useProcessPanel", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./process/useProcessPanel")>();
  return { ...actual, useProcessPanel: (...args: Parameters<typeof actual.useProcessPanel>) => {
    renders.app++;
    return actual.useProcessPanel(...args);
  } };
});
vi.mock("./i18n/react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./i18n/react")>();
  return { ...actual, useT: () => {
    // Counting the hook's caller observes the real compiled row body even when the compiler
    // reuses its child elements. A leaf component or Profiler would miss those executions.
    if (/\bat TreeRow\b/.test(new Error().stack ?? "")) renders.rows++;
    return actual.useT();
  } };
});
vi.mock("./browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: ({ project }: { project: Project }) => <PanelHeader title={project.name} /> }));
vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => <PanelHeader title="Report" /> }));
vi.mock("./terminal/TerminalController", () => ({ TerminalController: class {
  constructor() { return new Proxy(this, { get: (target, key) => key === "then" ? undefined : key in target ? Reflect.get(target, key) : () => {} }); }
} }));

async function mountApp(phone: boolean) {
  renders.app = 0;
  renders.rows = 0;
  vi.useFakeTimers({ now: Date.UTC(2026, 9, 28, 12) });
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: !phone && media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  for (const [key, value] of Object.entries({ "--rail-width": "48px", "--breakpoint-docked": "1168px", "--breakpoint-phone": "782px" })) document.documentElement.style.setProperty(key, value);
  const console_ = consoleOf("c-poll", "Main");
  const project = projectOf("p-poll", console_.id, "Website");
  const session = sessionOf("s-poll", console_.id, project.id, "Fix header", "idle");
  let trees: ProcessTree[] = [{ id: "1:1", session_id: session.id, command: "npm run dev", cwd: "/work/site", cwd_missing: false, kept: false, started_at: Date.now(), process_count: 1, memory_bytes: 120_000_000, ports: [3000] }];
  const daemon: Daemon = {
    store: createStateStore(snapshotState({ consoles: [console_], projects: [project], sessions: [session] })),
    request: vi.fn(async (body) => body.type === "list_processes" ? { type: "processes_listed", id: null, trees: structuredClone(trees) } : { type: "ack", id: null }) as Daemon["request"],
    toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  let stacks!: ReturnType<typeof usePanelStacks>;
  function StackReset() { stacks = usePanelStacks(); return null; }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "browser" }}><ThemeProvider><DaemonProvider value={daemon}><StackReset /><App /></DaemonProvider></ThemeProvider></PlatformProvider>));
  onTestFinished(async () => {
    act(() => { stacks.setStack("project", defaultStack("project")); stacks.setStack("report", defaultStack("report")); root.unmount(); });
    container.remove();
    await act(async () => { await vi.runOnlyPendingTimersAsync(); });
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
    vi.unstubAllGlobals();
    for (const key of ["--rail-width", "--breakpoint-docked", "--breakpoint-phone"]) document.documentElement.style.removeProperty(key);
    localStorage.clear();
  });
  const sessionRow = [...document.querySelectorAll<HTMLElement>("[data-region=sidebar] [role=button]")].filter((item) => item.textContent?.includes(session.title!)).at(-1);
  expect(sessionRow).toBeDefined();
  await act(async () => sessionRow!.click());
  const toggle = (label: string) => document.querySelector<HTMLElement>(`[data-region=topbar] [aria-label="${label}"]`);
  if (!phone) {
    expect(toggle("Show processes panel")).not.toBeNull();
    await act(async () => toggle("Show processes panel")!.click());
    expect(document.querySelectorAll("#panel-processes li")).toHaveLength(1);
    expect(renders.rows).toBeGreaterThan(0);
  } else expect(document.getElementById("panel-processes")).toBeNull();
  await act(async () => await vi.advanceTimersByTimeAsync(500));
  return { daemon, setTrees: (value: ProcessTree[]) => { trees = value; }, trees: () => trees, toggle };
}

it.each([false, true])("does not render App or rows for identical process polls (phone closed: %s)", async (phone) => {
  const app = await mountApp(phone);
  const before = { ...renders };
  const calls = () => vi.mocked(app.daemon.request).mock.calls.filter(([body]) => body.type === "list_processes").length;
  const requested = calls();
  await act(async () => await vi.advanceTimersByTimeAsync(phone ? 15000 : 3000));
  expect(calls()).toBe(requested + 1);
  expect({ app: renders.app, rows: renders.rows }).toEqual({ app: before.app, rows: before.rows });
  app.setTrees(app.trees().map((tree) => ({ ...tree, command: "npm run preview" })));
  await act(async () => await vi.advanceTimersByTimeAsync(phone ? 15000 : 3000));
  expect(renders.app).toBeGreaterThan(before.app);
  if (!phone) {
    expect(renders.rows).toBeGreaterThan(before.rows);
    expect(document.getElementById("panel-processes")?.textContent).toContain("npm run preview");
  }
  const afterChange = { ...renders };
  await act(async () => await vi.advanceTimersByTimeAsync(phone ? 15000 : 3000));
  expect({ app: renders.app, rows: renders.rows }).toEqual({ app: afterChange.app, rows: afterChange.rows });
  app.setTrees([]);
  await act(async () => await vi.advanceTimersByTimeAsync(phone ? 15000 : 3000));
  expect(renders.app).toBeGreaterThan(afterChange.app);
  if (phone) {
    expect(app.toggle("Show processes panel")).not.toBeNull();
    // The App uses the latest empty list to skip Processes when cycling the floating panels.
    for (let swipe = 0; swipe < 2; swipe++) {
      for (const [type, x] of [["touchstart", 200], ["touchmove", 140], ["touchmove", 80], ["touchend", 80]] as const) {
        const finger = { identifier: 1, clientX: x, clientY: 300 };
        await act(async () => void document.body.dispatchEvent(touchEvent(type, type === "touchend" ? [] : [finger], [finger])));
      }
    }
    expect([...document.querySelectorAll("[data-panel-card]")].map((card) => card.id)).toEqual(["panel-files"]);
  } else expect(document.querySelectorAll("#panel-processes li")).toHaveLength(0);
});
