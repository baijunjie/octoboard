// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import type { ClientRequestBody, Console, Project } from "../protocol";
import type { Daemon } from "../store";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { hasNewProjectId, useGitStatusSchedule } from "./useGitStatusSchedule";
import { fakeTimers } from "../test/clock";

describe("hasNewProjectId", () => {
  it("is false when every current id is already known", () => {
    expect(hasNewProjectId(new Set(["a", "b"]), ["a", "b"])).toBe(false);
  });

  it("is true when a current id is not in known", () => {
    expect(hasNewProjectId(new Set(["a"]), ["a", "b"])).toBe(true);
  });

  it("is true when one project is removed and another added in the same tick, even though the count is unchanged", () => {
    expect(hasNewProjectId(new Set(["a"]), ["b"])).toBe(true);
  });

  it("is false when a project is only removed", () => {
    expect(hasNewProjectId(new Set(["a", "b"]), ["a"])).toBe(false);
  });

  it("is false for an empty current list", () => {
    expect(hasNewProjectId(new Set(["a"]), [])).toBe(false);
  });
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn((_body: ClientRequestBody) => Promise.resolve({ type: "ack", id: null }) as Promise<never>);

let daemon: Daemon;
const consolesOf = (...ids: string[]) => new Map(ids.map((id) => [id, { id } as Console]));
const projectsOf = (...ids: string[]) => new Map(ids.map((id) => [id, { id, console_id: "c1" } as Project]));

function Host() {
  useGitStatusSchedule();
  return null;
}

function mount() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = () => act(() => root.render(<TestHost daemon={daemon}><Host /></TestHost>));
  render();
  let mounted = true;
  const unmount = () => {
    if (!mounted) return;
    mounted = false;
    act(() => root.unmount());
    container.remove();
  };
  onTestFinished(unmount);
  return {
    rerender: render,
    unmount,
  };
}

const REFRESH_INTERVAL_MS = 5 * 60_000;

describe("useGitStatusSchedule", () => {
  beforeEach(() => {
    request.mockClear();
    fakeTimers();
    daemon = testDaemon({ state: { connectionState: "open", snapshotEpoch: 0, consoles: consolesOf("c1"), projects: projectsOf("p1") }, request });
  });

  it("sends one request on mount and none again on a re-render with unchanged inputs", () => {
    const host = mount();
    expect(request).toHaveBeenCalledTimes(1);

    host.rerender();
    host.rerender();
    expect(request).toHaveBeenCalledTimes(1);

    host.unmount();
  });

  it("fires the interval once per period, rather than a re-render restarting it", () => {
    const host = mount();
    expect(request).toHaveBeenCalledTimes(1);

    // Advance partway through the period before the re-renders below, so a restart would tick at
    // a *different* time than the original interval does (2 min + REFRESH_INTERVAL_MS, instead of
    // REFRESH_INTERVAL_MS from mount) and the assertions below would catch it; perturbing at t = 0
    // instead would not, since a torn-down-and-recreated interval ticks at the same instant the
    // original one would have.
    act(() => vi.advanceTimersByTime(2 * 60_000));

    // Re-renders that change nothing must not tear the effect down and recreate the interval.
    host.rerender();
    host.rerender();

    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS - 2 * 60_000 - 1));
    expect(request).toHaveBeenCalledTimes(1);

    // The tick lands on the original schedule (REFRESH_INTERVAL_MS from mount), not 2 min late.
    act(() => vi.advanceTimersByTime(1));
    expect(request).toHaveBeenCalledTimes(2);

    host.unmount();
  });

  it("refreshes once more when a project appears in a console, without resetting the interval", () => {
    const host = mount();
    expect(request).toHaveBeenCalledTimes(1);

    // As above, perturb partway through the period rather than at t = 0, so a restarted interval
    // would tick 2 min later than the original schedule and the assertions below would catch it.
    act(() => vi.advanceTimersByTime(2 * 60_000));

    act(() => daemon.store.setState({ projects: projectsOf("p1", "p2") }));
    host.rerender();
    expect(request).toHaveBeenCalledTimes(2);

    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS - 2 * 60_000 - 1));
    expect(request).toHaveBeenCalledTimes(2);

    // The interval still fires on its original schedule (REFRESH_INTERVAL_MS from mount), unmoved
    // by the roster change above.
    act(() => vi.advanceTimersByTime(1));
    expect(request).toHaveBeenCalledTimes(3);

    host.unmount();
  });

  it("sends nothing for a console with no project list change and no elapsed interval", () => {
    const host = mount();
    request.mockClear();

    host.rerender();
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS - 1));
    expect(request).not.toHaveBeenCalled();

    host.unmount();
  });
});


describe("all-console scheduling", () => {
  beforeEach(() => {
    request.mockClear();
    fakeTimers();
    daemon = testDaemon({ state: {
      connectionState: "open", snapshotEpoch: 0, consoles: consolesOf("c1", "c2"),
      projects: new Map([
        ["p1", { id: "p1", console_id: "c1" } as Project],
        ["p2", { id: "p2", console_id: "c2" } as Project],
      ]),
    }, request });
  });

  const requestedConsoles = () => request.mock.calls.map(([body]) => (body as { console: string }).console);

  it("refreshes every console on mount and every five minutes, and stops on unmount", () => {
    const host = mount();
    expect(requestedConsoles()).toEqual(["c1", "c2"]);
    request.mockClear();
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS));
    expect(requestedConsoles()).toEqual(["c1", "c2"]);
    host.unmount();
    request.mockClear();
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS));
    expect(request).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("refreshes a new or moved project in its destination console without shifting the clock", () => {
    mount();
    act(() => vi.advanceTimersByTime(2 * 60_000));
    request.mockClear();
    act(() => daemon.store.setState({ projects: new Map([
      ...daemon.store.getState().projects,
      ["p3", { id: "p3", console_id: "c2" } as Project],
    ]) }));
    expect(requestedConsoles()).toEqual(["c2"]);
    request.mockClear();
    act(() => daemon.store.setState({ projects: new Map([
      ...daemon.store.getState().projects,
      ["p1", { id: "p1", console_id: "c2" } as Project],
    ]) }));
    expect(requestedConsoles()).toEqual(["c2"]);
    request.mockClear();
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS - 2 * 60_000));
    expect(requestedConsoles()).toEqual(["c1", "c2"]);
  });

  it("checks a newly added console immediately and drops a removed console from later rounds", () => {
    mount();
    request.mockClear();
    act(() => daemon.store.setState({ consoles: consolesOf("c1", "c3") }));
    expect(requestedConsoles()).toEqual(["c3"]);
    request.mockClear();
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS));
    expect(requestedConsoles()).toEqual(["c1", "c3"]);
  });

  it("pauses while disconnected and refreshes all consoles once on reconnect or a new snapshot", () => {
    act(() => daemon.store.setState({ connectionState: "connecting" }));
    mount();
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS));
    expect(request).not.toHaveBeenCalled();
    act(() => daemon.store.setState({ connectionState: "open", snapshotEpoch: 1 }));
    expect(requestedConsoles()).toEqual(["c1", "c2"]);
    request.mockClear();
    act(() => daemon.store.setState({ snapshotEpoch: 2, projects: new Map([
      ...daemon.store.getState().projects,
      ["p3", { id: "p3", console_id: "c2" } as Project],
    ]) }));
    expect(requestedConsoles()).toEqual(["c1", "c2"]);
    request.mockClear();
    act(() => daemon.store.setState({ connectionState: "connecting" }));
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS));
    expect(request).not.toHaveBeenCalled();
    act(() => daemon.store.setState({ connectionState: "open" }));
    expect(requestedConsoles()).toEqual(["c1", "c2"]);
  });
});
