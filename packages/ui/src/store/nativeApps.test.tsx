// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import type { Agent, ClientRequestBody } from "../protocol";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { setNativeApps } from "./actions";
import { createStateStore } from "./state";
import { useNativeAppInstalled, useNativeAppUnseen, useNativeAppsRefresh } from "./nativeApps";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("recording a probe", () => {
  it("holds the agents the daemon found installed, none before a probe", () => {
    const store = createStateStore();
    expect(store.getState().nativeApps).toEqual([]);
    expect(store.getState().nativeAppsUnseen.size).toBe(0);
    setNativeApps(store, ["claude", "codex"], [], []);
    expect(store.getState().nativeApps).toEqual(["claude", "codex"]);
  });

  it("marks the sessions asked about and not seen, and takes the mark off one a later probe finds seen", () => {
    const store = createStateStore();
    setNativeApps(store, ["claude"], ["a", "b", "c"], ["b"]);
    expect([...store.getState().nativeAppsUnseen].sort()).toEqual(["a", "c"]);
    setNativeApps(store, ["claude"], ["a", "b"], ["a"]);
    expect([...store.getState().nativeAppsUnseen].sort()).toEqual(["b", "c"]);
  });

  it("leaves a session a probe did not ask about as the last probe that asked left it", () => {
    const store = createStateStore();
    setNativeApps(store, [], ["a"], []);
    setNativeApps(store, [], ["b"], ["b"]);
    expect([...store.getState().nativeAppsUnseen]).toEqual(["a"]);
    setNativeApps(store, [], [], []);
    expect([...store.getState().nativeAppsUnseen]).toEqual(["a"]);
  });

  it("changes nothing, and tells no subscriber, when a probe says what the state already holds", () => {
    const store = createStateStore();
    setNativeApps(store, ["claude", "codex"], ["a", "b"], ["b"]);
    const before = store.getState();
    const notified = vi.fn();
    store.subscribe(notified);

    setNativeApps(store, ["claude", "codex"], ["a", "b"], ["b"]);

    expect(store.getState()).toBe(before);
    expect(store.getState().nativeApps).toBe(before.nativeApps);
    expect(store.getState().nativeAppsUnseen).toBe(before.nativeAppsUnseen);
    expect(notified).not.toHaveBeenCalled();
  });

  it("keeps the unseen set itself when only the installed agents change, and the agents when only the sessions do", () => {
    const store = createStateStore();
    setNativeApps(store, ["claude"], ["a"], []);
    const first = store.getState();
    setNativeApps(store, ["claude", "codex"], ["a"], []);
    expect(store.getState().nativeApps).toEqual(["claude", "codex"]);
    expect(store.getState().nativeAppsUnseen).toBe(first.nativeAppsUnseen);
    const second = store.getState();
    setNativeApps(store, ["claude", "codex"], ["a"], ["a"]);
    expect(store.getState().nativeApps).toBe(second.nativeApps);
    expect(store.getState().nativeAppsUnseen.size).toBe(0);
  });
});

/** Mounts `Subject` under a daemon whose requests are `request`, returning what it captured. */
function mountWith<T>(request: (body: ClientRequestBody) => Promise<unknown>, use: () => T, state = {}) {
  const daemon = testDaemon({ request: request as never, state });
  const captured = { current: undefined as T };
  function Subject() {
    captured.current = use();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<TestHost daemon={daemon}><Subject /></TestHost>));
  return { daemon, captured };
}

describe("the hooks that read the last probe", () => {
  it("say whether an agent's app is installed and whether a session's conversation is unseen", () => {
    const { daemon, captured } = mountWith(() => Promise.reject(new Error("no daemon")), () => [useNativeAppInstalled("claude"), useNativeAppInstalled("grok"), useNativeAppUnseen("a"), useNativeAppUnseen("b")]);
    expect(captured.current).toEqual([false, false, false, false]);

    act(() => setNativeApps(daemon.store, ["claude"] satisfies Agent[], ["a", "b"], ["b"]));

    expect(captured.current).toEqual([true, false, true, false]);
  });
});

describe("useNativeAppsRefresh", () => {
  const settle = async () => act(async () => { await vi.waitFor(() => expect(true).toBe(true)); });

  it("asks which apps are installed, and records the answer with the sessions asked about", async () => {
    const request = vi.fn().mockResolvedValue({ type: "native_apps", id: null, agents: ["codex"], sessions_seen: ["s-2"], sessions_unseen: ["s-1"] });
    const { daemon, captured } = mountWith(request, useNativeAppsRefresh);

    await act(async () => captured.current(["s-1", "s-2"]));

    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "probe_native_apps", sessions: ["s-1", "s-2"] });
    await vi.waitFor(() => expect(daemon.store.getState().nativeApps).toEqual(["codex"]));
    expect([...daemon.store.getState().nativeAppsUnseen]).toEqual(["s-1"]);
  });

  it("records only the sessions the daemon judged, not one it asked about and left out of both lists", async () => {
    const request = vi.fn().mockResolvedValue({ type: "native_apps", id: null, agents: ["claude"], sessions_seen: ["s-2"], sessions_unseen: ["s-1"] });
    const { daemon, captured } = mountWith(request, useNativeAppsRefresh);

    await act(async () => captured.current(["s-1", "s-2", "s-3"]));

    await vi.waitFor(() => expect(daemon.store.getState().nativeApps).toEqual(["claude"]));
    expect([...daemon.store.getState().nativeAppsUnseen]).toEqual(["s-1"]);
  });

  it("asks about no sessions when it is given none", async () => {
    const request = vi.fn().mockResolvedValue({ type: "native_apps", id: null, agents: ["claude"], sessions_seen: [] });
    const { daemon, captured } = mountWith(request, useNativeAppsRefresh);

    await act(async () => captured.current());

    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "probe_native_apps", sessions: [] });
    await vi.waitFor(() => expect(daemon.store.getState().nativeApps).toEqual(["claude"]));
  });

  it("ignores a reply that is not native_apps, as a stand-in daemon that answers every request alike gives", async () => {
    const request = vi.fn().mockResolvedValue({ type: "ack", id: null });
    const { daemon, captured } = mountWith(request, useNativeAppsRefresh, { nativeApps: ["claude"] });

    await act(async () => captured.current(["s-1"]));
    await settle();

    expect(request).toHaveBeenCalledTimes(1);
    expect(daemon.store.getState().nativeApps).toEqual(["claude"]);
    expect(daemon.store.getState().nativeAppsUnseen.size).toBe(0);
  });

  it("keeps the last answer and raises nothing when the probe fails", async () => {
    const request = vi.fn().mockRejectedValue(new Error("offline"));
    const { daemon, captured } = mountWith(request, useNativeAppsRefresh, { nativeApps: ["claude"] });

    await act(async () => captured.current(["s-1"]));
    await settle();

    expect(request).toHaveBeenCalledTimes(1);
    expect(daemon.store.getState().nativeApps).toEqual(["claude"]);
  });
});
