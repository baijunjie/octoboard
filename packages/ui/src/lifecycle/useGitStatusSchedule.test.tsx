// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { hasNewProjectId, useGitStatusSchedule } from "./useGitStatusSchedule";

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

const request = vi.fn(() => Promise.resolve({}) as Promise<never>);

/** A stand-in for the store slice `useGitStatusSchedule` reads, mutable per test. */
let fakeState: {
  connectionState: string;
  snapshotEpoch: number;
  projects: Map<string, { id: string; console_id: string }>;
};

vi.mock("../store", () => ({
  useDaemon: () => ({ request }),
  useDaemonStore: (selector: (state: typeof fakeState) => unknown) => selector(fakeState),
}));

function Host({ consoleId }: { consoleId: string | undefined }) {
  useGitStatusSchedule(consoleId);
  return null;
}

function mount(consoleId: string | undefined) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Host consoleId={consoleId} />));
  return {
    rerender: (id: string | undefined) => act(() => root.render(<Host consoleId={id} />)),
    unmount: () => {
      act(() => root.unmount());
      container.remove();
    },
  };
}

const REFRESH_INTERVAL_MS = 5 * 60_000;

describe("useGitStatusSchedule", () => {
  beforeEach(() => {
    request.mockClear();
    vi.useFakeTimers();
    fakeState = {
      connectionState: "open",
      snapshotEpoch: 0,
      projects: new Map([["p1", { id: "p1", console_id: "c1" }]]),
    };
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("sends one request on mount and none again on a re-render with unchanged inputs", () => {
    const host = mount("c1");
    expect(request).toHaveBeenCalledTimes(1);

    host.rerender("c1");
    host.rerender("c1");
    expect(request).toHaveBeenCalledTimes(1);

    host.unmount();
  });

  it("fires the interval once per period, rather than a re-render restarting it", () => {
    const host = mount("c1");
    expect(request).toHaveBeenCalledTimes(1);

    // Advance partway through the period before the re-renders below, so a restart would tick at
    // a *different* time than the original interval does (2 min + REFRESH_INTERVAL_MS, instead of
    // REFRESH_INTERVAL_MS from mount) and the assertions below would catch it; perturbing at t = 0
    // instead would not, since a torn-down-and-recreated interval ticks at the same instant the
    // original one would have.
    act(() => vi.advanceTimersByTime(2 * 60_000));

    // Re-renders that change nothing must not tear the effect down and recreate the interval.
    host.rerender("c1");
    host.rerender("c1");

    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS - 2 * 60_000 - 1));
    expect(request).toHaveBeenCalledTimes(1);

    // The tick lands on the original schedule (REFRESH_INTERVAL_MS from mount), not 2 min late.
    act(() => vi.advanceTimersByTime(1));
    expect(request).toHaveBeenCalledTimes(2);

    host.unmount();
  });

  it("refreshes once more when a project appears in the shown console, without resetting the interval", () => {
    const host = mount("c1");
    expect(request).toHaveBeenCalledTimes(1);

    // As above, perturb partway through the period rather than at t = 0, so a restarted interval
    // would tick 2 min later than the original schedule and the assertions below would catch it.
    act(() => vi.advanceTimersByTime(2 * 60_000));

    fakeState = { ...fakeState, projects: new Map(fakeState.projects).set("p2", { id: "p2", console_id: "c1" }) };
    host.rerender("c1");
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
    const host = mount("c1");
    request.mockClear();

    host.rerender("c1");
    act(() => vi.advanceTimersByTime(REFRESH_INTERVAL_MS - 1));
    expect(request).not.toHaveBeenCalled();

    host.unmount();
  });
});
