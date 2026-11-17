// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { useNativeAppsSync } from "./useNativeAppsSync";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Host() {
  useNativeAppsSync();
  return null;
}

function mount(connectionState: "connecting" | "open") {
  const request = vi.fn().mockResolvedValue({ type: "native_apps", id: null, agents: ["claude"], sessions_seen: [] });
  const daemon = testDaemon({ state: { connectionState, snapshotEpoch: 0 }, request });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<TestHost daemon={daemon}><Host /></TestHost>));
  return { daemon, request };
}

const probes = (request: ReturnType<typeof vi.fn>) => request.mock.calls.filter(([body]) => body.type === "probe_native_apps").length;

it("probes for the installed apps as the window connects, asking about no session, and records the answer", async () => {
  const { daemon, request } = mount("open");

  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "probe_native_apps", sessions: [] });
  await vi.waitFor(() => expect(daemon.store.getState().nativeApps).toEqual(["claude"]));
});

it("waits for the connection before it asks, and asks once it opens", async () => {
  const { daemon, request } = mount("connecting");
  expect(probes(request)).toBe(0);

  await act(async () => daemon.store.setState({ connectionState: "open" }));

  expect(probes(request)).toBe(1);
});

it("asks again when a new snapshot arrives over the open connection, as after a reconnect or a lag recovery", async () => {
  const { daemon, request } = mount("open");
  expect(probes(request)).toBe(1);

  await act(async () => daemon.store.setState({ snapshotEpoch: 1 }));
  expect(probes(request)).toBe(2);

  await act(async () => daemon.store.setState({ connectionState: "connecting" }));
  expect(probes(request)).toBe(2);
});
