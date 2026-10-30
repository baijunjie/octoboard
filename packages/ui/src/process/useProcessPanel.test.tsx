// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { AsideOwner } from "../layout/asideOwner";
import type { Event, ProcessTree, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import type { ProcessPolling } from "./useProcessTrees";
import { useProcessPanel } from "./useProcessPanel";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const console_ = consoleOf("c", "Main");
const project = projectOf("p", console_.id, "App");
const first = sessionOf("a", console_.id, project.id, "First", "idle");
const second = sessionOf("b", console_.id, project.id, "Second", "idle");
const tree = (id: string, session_id?: string): ProcessTree => ({ id, session_id, command: "dev", cwd: "/app", cwd_missing: false, kept: false, started_at: 0, process_count: 1, memory_bytes: 0, ports: [] });

async function mount(request: (body: RequestBody) => Promise<Event>, initialSelection?: string) {
  vi.useFakeTimers();
  const daemon = {
    store: createStateStore({ projects: new Map([[project.id, project]]), sessions: new Map([[first.id, first], [second.id, second]]) }),
    request: vi.fn(request), toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  } as unknown as Daemon;
  let owner: AsideOwner = { kind: "project", project: project.id };
  let selected: string | undefined = initialSelection;
  let polling: ProcessPolling = "live";
  let data!: ReturnType<typeof useProcessPanel>;
  function Probe() { data = useProcessPanel(owner, selected, polling); return null; }
  const element = document.body.appendChild(document.createElement("div"));
  const root = createRoot(element);
  const render = async () => act(async () => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  await render();
  onTestFinished(() => { act(() => root.unmount()); element.remove(); vi.useRealTimers(); });
  return {
    daemon,
    data: () => data,
    select: async (id: string | undefined) => { selected = id; await render(); },
    owner: async (next: AsideOwner) => { owner = next; await render(); },
    polling: async (next: ProcessPolling) => { polling = next; await render(); },
  };
}

it("defaults to all project trees, follows sidebar changes, and preserves a manual filter until the next change", async () => {
  const panel = await mount(async () => ({ type: "processes_listed", trees: [tree("1", first.id), tree("2", second.id), tree("3")] }));
  expect(panel.data().trees).toHaveLength(3);
  await panel.select(first.id);
  expect(panel.data().trees?.map((item) => item.id)).toEqual(["1"]);
  await act(async () => panel.data().selectSession(second.id));
  expect(panel.data().trees?.map((item) => item.id)).toEqual(["2"]);
  await panel.select(first.id);
  expect(panel.data().sessionId).toBe(second.id);
  await panel.select(second.id);
  await act(async () => panel.data().selectSession(undefined));
  expect(panel.data().trees).toHaveLength(3);
  await panel.select(first.id);
  expect(panel.data().sessionId).toBe(first.id);
  expect(vi.mocked(panel.daemon.request).mock.calls).toHaveLength(1);
});

it("falls back to all when a manually selected session is deleted", async () => {
  const panel = await mount(async () => ({ type: "processes_listed", trees: [tree("1", first.id), tree("2")] }));
  await act(async () => panel.data().selectSession(first.id));
  await act(async () => panel.daemon.store.setState({ sessions: new Map([[second.id, second]]) }));
  expect(panel.data().sessionId).toBeUndefined();
  expect(panel.data().trees).toHaveLength(2);
});

it("stops polling when inactive, resumes when active, and ignores late answers from the previous owner", async () => {
  let finish!: (answer: Event) => void;
  const panel = await mount(async (body) => body.type === "list_processes" && "project_id" in body
    ? new Promise<Event>((resolve) => { finish = resolve; })
    : { type: "processes_listed", trees: [tree("console", "console")] });
  expect(panel.data().trees).toBeUndefined();
  await panel.owner({ kind: "report", consoleSession: "console" });
  expect(panel.data().trees?.map((item) => item.id)).toEqual(["console"]);
  await act(async () => finish({ type: "processes_listed", trees: [tree("stale", first.id)] }));
  expect(panel.data().trees?.map((item) => item.id)).toEqual(["console"]);
  await panel.polling("off");
  const calls = vi.mocked(panel.daemon.request).mock.calls.length;
  await act(async () => void vi.advanceTimersByTime(9000));
  expect(vi.mocked(panel.daemon.request).mock.calls).toHaveLength(calls);
  await panel.polling("live");
  expect(vi.mocked(panel.daemon.request).mock.calls).toHaveLength(calls + 1);
});

it("initializes the filter from an already selected session", async () => {
  const panel = await mount(async () => ({ type: "processes_listed", trees: [tree("1", first.id), tree("2", second.id)] }), first.id);
  expect(panel.data().sessionId).toBe(first.id);
  expect(panel.data().trees?.map((item) => item.id)).toEqual(["1"]);
});

it("lists every session of a project it is told to show all of, though the selected session belongs to it", async () => {
  const panel = await mount(async () => ({ type: "processes_listed", trees: [tree("1", first.id), tree("2", second.id)] }), first.id);
  expect(panel.data().sessionId).toBe(first.id);
  await act(async () => panel.data().showAllOf(project.id));
  expect(panel.data().sessionId).toBeUndefined();
  expect(panel.data().trees?.map((item) => item.id)).toEqual(["1", "2"]);
  await panel.select(second.id);
  expect(panel.data().sessionId).toBe(second.id);
});

it("polls every 15 seconds in the background and every 3 while live", async () => {
  const panel = await mount(async () => ({ type: "processes_listed", trees: [] }));
  await panel.polling("background");
  const calls = () => vi.mocked(panel.daemon.request).mock.calls.length;
  const before = calls();
  await act(async () => void vi.advanceTimersByTime(14000));
  expect(calls()).toBe(before);
  await act(async () => void vi.advanceTimersByTime(1000));
  expect(calls()).toBe(before + 1);
  await panel.polling("live");
  const live = calls();
  await act(async () => void vi.advanceTimersByTime(3000));
  expect(calls()).toBe(live + 1);
});

it("pauses while the document is hidden and asks at once when it is shown again", async () => {
  const panel = await mount(async () => ({ type: "processes_listed", trees: [] }));
  const calls = () => vi.mocked(panel.daemon.request).mock.calls.length;
  let hidden = true;
  vi.spyOn(document, "hidden", "get").mockImplementation(() => hidden);
  onTestFinished(() => void vi.restoreAllMocks());
  const before = calls();
  await act(async () => void vi.advanceTimersByTime(9000));
  expect(calls()).toBe(before);
  hidden = false;
  await act(async () => void document.dispatchEvent(new Event("visibilitychange")));
  expect(calls()).toBe(before + 1);
});
