// @vitest-environment jsdom
// Listing a task's runs: when the page asks for them, again after a snapshot and for another task, and what a failure does.
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { setLanguage, t } from "../i18n/language";
import type { ClientRequestBody, ScheduledTaskRun } from "../protocol";
import { applyDaemonEvent, type State } from "../store";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { daemonError } from "../test/daemonError";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useScheduledTaskRuns } from "./useScheduledTaskRuns";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  setLanguage("en");
  acknowledgeAllAlerts();
});

const OPEN: Partial<State> = { connectionState: "open", snapshotEpoch: 1 };

function runOf(id: string, task: string): ScheduledTaskRun {
  return { id, task_id: task, ran_at: 1, due_at: null, trigger: "run_now", outcome: { kind: "started", session: "s-1", started_independent: false } };
}

function mount(state: Partial<State>, fail?: () => Error) {
  const seen: (ScheduledTaskRun[] | undefined)[] = [];
  const failures: unknown[] = [];
  const request = vi.fn(async (body: ClientRequestBody) => {
    if (fail) throw fail();
    if (body.type !== "list_scheduled_task_runs") throw new Error(`unexpected ${body.type}`);
    return { type: "scheduled_task_runs_listed" as const, id: null, task: body.task, runs: [runOf(`r-${body.task}`, body.task)] };
  });
  const daemon = testDaemon({ state, request });
  function Probe({ task }: { task: string }): null {
    const listing = useScheduledTaskRuns(task);
    seen.push(listing.runs);
    failures.push(listing.failure);
    return null;
  }
  const container = mountHost(<Probe task="t-1" />, { daemon });
  return { daemon, container, request, seen, failures, Probe, set: (patch: Partial<State>) => act(async () => daemon.store.setState(patch)) };
}

const listed = (request: ReturnType<typeof vi.fn>) => request.mock.calls.map(([body]) => (body as { task: string }).task);

it("has no runs until they are listed, then the task's runs as the store holds them", async () => {
  const { daemon, request, seen } = mount(OPEN);
  expect(seen[0]).toBeUndefined();
  await act(async () => {});
  expect(listed(request)).toEqual(["t-1"]);
  // The reply lands in the store through the connection; the stand-in's request does not, so the test applies it.
  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_runs_listed", id: null, task: "t-1", runs: [runOf("r-1", "t-1")] }));
  expect(seen.at(-1)?.map((run) => run.id)).toEqual(["r-1"]);
});

it("lists again after a snapshot and for another task, and not while the window is not connected", async () => {
  const { container, request, Probe, set } = mount({ connectionState: "connecting", snapshotEpoch: 0 });
  await act(async () => {});
  expect(request).not.toHaveBeenCalled();
  await set({ connectionState: "open" });
  expect(listed(request)).toEqual([]);
  await set({ snapshotEpoch: 1 });
  expect(listed(request)).toEqual(["t-1"]);
  await set({ snapshotEpoch: 2 });
  expect(listed(request)).toEqual(["t-1", "t-1"]);
  container.render(<Probe task="t-2" />);
  await act(async () => {});
  expect(listed(request)).toEqual(["t-1", "t-1", "t-2"]);
});

it("holds a failure for the page to draw, raising no alert, but not the connection going or the task being gone", async () => {
  const quiet = [
    () => new CodedError(t("connection.error.connectionIsClosed"), "connection_closed", {}),
    () => daemonError("unknown_scheduled_task", {}),
  ];
  for (const fail of quiet) {
    const { failures } = mount(OPEN, fail);
    await act(async () => {});
    expect(failures.at(-1)).toBeUndefined();
  }
  const { failures } = mount(OPEN, () => daemonError("internal_error", { detail: "database is locked" }));
  await act(async () => {});
  expect(failures.at(-1)).toBeDefined();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("lists once, on the new snapshot, after a reconnect", async () => {
  const { request, set } = mount(OPEN);
  await act(async () => {});
  expect(listed(request)).toEqual(["t-1"]);
  await set({ connectionState: "closed" });
  await set({ connectionState: "open" });
  expect(listed(request)).toEqual(["t-1"]);
  await set({ snapshotEpoch: 2 });
  expect(listed(request)).toEqual(["t-1", "t-1"]);
});

it("lists again on Try again after a failure, and holds nothing of the failure once the runs are listed", async () => {
  let failing = true;
  const seen: { failure: unknown; retry: () => void }[] = [];
  const request = vi.fn(async (body: ClientRequestBody) => {
    if (failing) throw daemonError("internal_error", { detail: "database is locked" });
    if (body.type !== "list_scheduled_task_runs") throw new Error(`unexpected ${body.type}`);
    return { type: "scheduled_task_runs_listed" as const, id: null, task: body.task, runs: [] };
  });
  function Probe(): null {
    const listing = useScheduledTaskRuns("t-1");
    seen.push(listing);
    return null;
  }
  mountHost(<Probe />, { daemon: testDaemon({ state: OPEN, request }) });
  await act(async () => {});
  expect(seen.at(-1)!.failure).toBeDefined();
  failing = false;
  await act(async () => seen.at(-1)!.retry());
  expect(request).toHaveBeenCalledTimes(2);
  expect(seen.at(-1)!.failure).toBeUndefined();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});
