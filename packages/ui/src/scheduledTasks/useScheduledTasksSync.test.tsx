// @vitest-environment jsdom
// Listing the daemon's scheduled tasks: when the window asks for them, again after a snapshot, and what a failure does.
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { setLanguage, t } from "../i18n/language";
import type { ClientRequestBody } from "../protocol";
import type { State } from "../store";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { daemonError } from "../test/daemonError";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { useScheduledTasksSync } from "./useScheduledTasksSync";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  setLanguage("en");
  acknowledgeAllAlerts();
  seen.length = 0;
});

const seen: ReturnType<typeof useScheduledTasksSync>[] = [];

function Probe({ enabled }: { enabled: boolean }): null {
  seen.push(useScheduledTasksSync(enabled));
  return null;
}

const OPEN: Partial<State> = { connectionState: "open", snapshotEpoch: 1 };

function mount(state: Partial<State>, fail?: () => Error) {
  const request = vi.fn(async (body: ClientRequestBody) => {
    if (fail) throw fail();
    return { type: "scheduled_tasks_listed" as const, id: null, tasks: [], time_zone: null, body };
  });
  const daemon = testDaemon({ state, request });
  const container = mountHost(<Probe enabled />, { daemon });
  return { daemon, container, request, set: (patch: Partial<State>) => act(async () => daemon.store.setState(patch)) };
}

const listings = (request: ReturnType<typeof vi.fn>) => request.mock.calls.filter(([body]) => (body as ClientRequestBody).type === "list_scheduled_tasks").length;

it("lists the tasks once the window is connected and has its snapshot, and again after each later snapshot", async () => {
  const { request, set } = mount({ connectionState: "connecting", snapshotEpoch: 0 });
  await act(async () => {});
  expect(listings(request)).toBe(0);
  await set({ connectionState: "open" });
  expect(listings(request)).toBe(0);
  await set({ snapshotEpoch: 1 });
  expect(listings(request)).toBe(1);
  await set({ snapshotEpoch: 2 });
  expect(listings(request)).toBe(2);
  await set({ connectionState: "closed" });
  await set({ snapshotEpoch: 3 });
  expect(listings(request)).toBe(2);
});

it("lists nothing while the daemon cannot be used", async () => {
  const request = vi.fn(async () => ({ type: "scheduled_tasks_listed" as const, id: null, tasks: [], time_zone: null }));
  const daemon = testDaemon({ state: OPEN, request });
  const container = mountHost(<Probe enabled={false} />, { daemon });
  await act(async () => {});
  expect(request).not.toHaveBeenCalled();
  container.render(<Probe enabled />);
  await act(async () => {});
  expect(request).toHaveBeenCalledTimes(1);
});

it("holds a failure for the page to draw, raising no alert, but not the connection going", async () => {
  mount(OPEN, () => new CodedError(t("connection.error.connectionIsClosed"), "connection_closed", {}));
  await act(async () => {});
  expect(seen.at(-1)!.failure).toBeUndefined();

  mount(OPEN, () => daemonError("internal_error", { detail: "database is locked" }));
  await act(async () => {});
  expect(seen.at(-1)!.failure).toBeDefined();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("lists once, on the new snapshot, after a reconnect", async () => {
  const { request, set } = mount(OPEN);
  await act(async () => {});
  expect(listings(request)).toBe(1);
  await set({ connectionState: "closed" });
  await set({ connectionState: "open" });
  expect(listings(request)).toBe(1);
  await set({ snapshotEpoch: 2 });
  expect(listings(request)).toBe(2);
});

it("lists again on Try again, pending while that is out, and drops the failure once a listing succeeds", async () => {
  let failing = true;
  let answer: (() => void) | undefined;
  const request = vi.fn(async (body: ClientRequestBody) => {
    if (failing) throw daemonError("internal_error", { detail: "database is locked" });
    await new Promise<void>((resolve) => (answer = resolve));
    return { type: "scheduled_tasks_listed" as const, id: null, tasks: [], time_zone: null, body };
  });
  const daemon = testDaemon({ state: OPEN, request });
  mountHost(<Probe enabled />, { daemon });
  await act(async () => {});
  expect(listings(request)).toBe(1);
  expect(seen.at(-1)!.failure).toBeDefined();
  expect(seen.at(-1)!.pending).toBe(false);

  failing = false;
  await act(async () => seen.at(-1)!.retry());
  expect(listings(request)).toBe(2);
  expect(seen.at(-1)!.pending).toBe(true);
  await act(async () => answer!());
  expect(seen.at(-1)!.failure).toBeUndefined();
  expect(seen.at(-1)!.pending).toBe(false);
  expect(errorAlerts.getSnapshot()).toEqual([]);
});
