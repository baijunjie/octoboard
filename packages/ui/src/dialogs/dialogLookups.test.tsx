// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import type { Event, RequestBody } from "../protocol";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { useDetectedAgent, type Detection, type DetectionQuery } from "./useDetectedAgent";
import { useResponsibleBranches } from "./useBranchInspections";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const DELAY_MS = 300;
const detected = (agent: "claude" | "codex"): Event => ({ type: "agent_detected", id: null, agent });
const connectionClosed = () => new CodedError("connection_closed", "connection_closed", {});

/** Renders `use` under a daemon whose requests the test answers itself, one pending request per call. */
function mount<T>(use: (request: (body: RequestBody) => Promise<Event>) => T) {
  const out: { body: RequestBody; resolve: (event: Event) => void; reject: (err: unknown) => void }[] = [];
  const request = vi.fn((body: RequestBody) => new Promise<Event>((resolve, reject) => void out.push({ body, resolve, reject })));
  const daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 1 }, request: request as never });
  let result!: T;
  const rendered: T[] = [];
  const Probe = () => {
    result = use(daemon.request as never);
    rendered.push(result);
    return null;
  };
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  const render = () => act(() => root.render(<DaemonProvider value={daemon}><Probe /></DaemonProvider>));
  render();
  onTestFinished(() => act(() => root.unmount()));
  return {
    out,
    result: () => result,
    rendered,
    render,
    reconnect: () => act(() => daemon.store.setState((state) => ({ snapshotEpoch: state.snapshotEpoch + 1 }))),
  };
}

function mountDetected() {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  let query: DetectionQuery | undefined = { type: "detect_directory_agent", path: "/a" };
  const onDetected = vi.fn();
  const hook = mount((request) => useDetectedAgent(request, query, DELAY_MS, onDetected));
  return {
    ...hook,
    detection: (): Detection => hook.result(),
    ask: (path: string) => {
      query = { type: "detect_directory_agent", path };
      hook.render();
    },
    clear: () => {
      query = undefined;
      hook.render();
    },
    wait: () => act(() => void vi.advanceTimersByTime(DELAY_MS)),
    onDetected,
  };
}

it("asks a detection again once the connection is back when its request was lost", async () => {
  const hook = mountDetected();
  hook.wait();
  expect(hook.out).toHaveLength(1);
  await act(async () => hook.out[0]!.reject(connectionClosed()));
  expect(hook.detection()).toEqual({ status: "checking" });
  hook.reconnect();
  hook.wait();
  expect(hook.out).toHaveLength(2);
  await act(async () => hook.out[1]!.resolve(detected("claude")));
  expect(hook.detection()).toEqual({ status: "ready" });
  expect(hook.onDetected).toHaveBeenCalledWith("claude");
});

it("shows a detection as checking from the first render after the query comes back from nothing to ask", async () => {
  const hook = mountDetected();
  hook.wait();
  await act(async () => hook.out[0]!.resolve(detected("claude")));
  expect(hook.detection()).toEqual({ status: "ready" });
  hook.clear();
  expect(hook.detection()).toEqual({ status: "idle" });
  const from = hook.rendered.length;
  hook.ask("/a");
  expect(hook.rendered.slice(from)).not.toContainEqual({ status: "ready" });
  expect(hook.detection()).toEqual({ status: "checking" });
});

it("does not ask a detection again after a reconnect when it was answered", async () => {
  const hook = mountDetected();
  hook.wait();
  await act(async () => hook.out[0]!.resolve(detected("claude")));
  hook.reconnect();
  hook.wait();
  expect(hook.out).toHaveLength(1);
});

it("shows checking again, and drops the first reply, when the query goes back within the delay", async () => {
  const hook = mountDetected();
  hook.wait();
  await act(async () => hook.out[0]!.resolve(detected("claude")));
  expect(hook.detection()).toEqual({ status: "ready" });
  hook.ask("/b");
  hook.ask("/a");
  expect(hook.detection()).toEqual({ status: "checking" });
  hook.wait();
  expect(hook.out).toHaveLength(2);
  await act(async () => hook.out[1]!.resolve(detected("codex")));
  expect(hook.onDetected).toHaveBeenLastCalledWith("codex");
});

it("drops a reply still out for the query when it comes back within the delay", async () => {
  const hook = mountDetected();
  hook.wait();
  hook.ask("/b");
  hook.ask("/a");
  await act(async () => hook.out[0]!.resolve(detected("claude")));
  expect(hook.detection()).toEqual({ status: "checking" });
  expect(hook.onDetected).not.toHaveBeenCalled();
});

it("asks the branches a handoff is responsible for again once the connection is back", async () => {
  const hook = mount(() => useResponsibleBranches("s-1", true));
  expect(hook.out).toHaveLength(1);
  await act(async () => hook.out[0]!.reject(connectionClosed()));
  expect(hook.result().pending).toBe(true);
  hook.reconnect();
  expect(hook.out).toHaveLength(2);
  await act(async () => hook.out[1]!.resolve({ type: "session_branches_inspected", id: null, items: [] }));
  expect(hook.result().pending).toBe(false);
  expect(hook.result().error).toBeUndefined();
});
